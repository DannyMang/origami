mod error;
mod grade;
mod tool;

use anyhow::{Context as _, Result, anyhow, bail};
use boa_engine::{
    Context, JsArgs, JsError, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction, Source, js_string,
    object::builtins::JsArray, property::Attribute, script::Script,
};
pub use error::Error;
pub use grade::{Curves, Grade, Secondary, Vignette, Wheels};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::{self, Read, Seek, SeekFrom},
    mem,
    path::{Path, PathBuf},
    pin::pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc,
    },
    task::{self, Poll, Waker},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};
pub use tool::{ffmpeg, ffprobe, missing, output, start, status};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Sound,
    Image,
    Text,
    Color,
    Rect,
    Line,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Val {
    Fixed(f64),
    Frames(u32, #[serde(deserialize_with = "frames")] Vec<f32>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Run {
    Kept(usize),
    Given(Vec<f32>),
}

fn frames<'a, D: Deserializer<'a>>(d: D) -> Result<Vec<f32>, D::Error> {
    match Run::deserialize(d)? {
        Run::Kept(i) => KEPT
            .with_borrow_mut(|k| k.get_mut(i).map(mem::take))
            .ok_or_else(|| D::Error::custom("no such run")),
        Run::Given(s) => Ok(s),
    }
}

impl Val {
    pub fn at(&self, i: usize) -> f64 {
        match self {
            Self::Fixed(v) => *v,
            Self::Frames(_, s) => s[i.min(s.len() - 1)] as f64,
        }
    }

    pub fn max(&self) -> f64 {
        match self {
            Self::Fixed(v) => *v,
            Self::Frames(_, s) => s.iter().fold(f32::MIN, |a, b| a.max(*b)) as f64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub from: usize,
    pub to: usize,
    pub opacity: Val,
    pub x: Val,
    pub y: Val,
    pub scale: Val,
    pub rotate: Val,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub kind: Kind,
    pub src: String,
    pub w: u32,
    pub h: u32,
    pub start: f64,
    pub end: f64,
    pub src_in: f64,
    pub speed: f64,
    pub z: u32,
    pub scale: Val,
    pub pos: (Option<Val>, Option<Val>),
    pub gain: f64,
    pub fade: (f64, f64),
    pub label: String,
    pub line: u32,
    #[serde(default)]
    pub home: u32,
    #[serde(default)]
    pub total: f64,
    pub video: bool,
    pub audio: bool,
    pub size: f64,
    pub color: String,
    #[serde(default)]
    pub font: String,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub ends: [f64; 4],
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub align: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<Grade>,
    pub opacity: Val,
    pub rotate: Val,
    pub shown: Val,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
}

impl Clip {
    pub fn vals(&self) -> impl Iterator<Item = (&'static str, &Val)> {
        [
            ("scale", Some(&self.scale)),
            ("x", self.pos.0.as_ref()),
            ("y", self.pos.1.as_ref()),
            ("opacity", Some(&self.opacity)),
            ("rotate", Some(&self.rotate)),
            ("shown", Some(&self.shown)),
        ]
        .into_iter()
        .filter_map(|(n, v)| Some((n, v?)))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub fps: u32,
    pub size: (u32, u32),
    pub duration: f64,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub proxy: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub clips: Vec<Clip>,
    #[serde(skip)]
    pub files: Vec<(u32, PathBuf)>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Media {
    size: u64,
    mtime: u64,
    sha: String,
    dur: f64,
    w: u32,
    h: u32,
    video: bool,
    audio: bool,
}

type Lock = BTreeMap<String, Media>;

pub type Measure = fn(&Clip, (f64, f64)) -> (f64, f64);

thread_local! {
    static ST: RefCell<(PathBuf, Lock, Lock, bool, Option<Measure>)> = RefCell::default();
    static FAIL: RefCell<Option<Error>> = RefCell::default();
    static READ: RefCell<Vec<(u32, PathBuf)>> = RefCell::default();
    static COLD: RefCell<(bool, Vec<(String, PathBuf)>, Lock)> = RefCell::default();
    static KEPT: RefCell<Vec<Vec<f32>>> = RefCell::default();
}

fn find(dir: &Path, rel: &str) -> Option<PathBuf> {
    let path = dir.join(rel);
    if path.exists() {
        return Some(path);
    }
    let named = |p: &PathBuf| p.is_file() && p.file_stem() == path.file_name();
    let mut near = path
        .parent()
        .and_then(|p| fs::read_dir(p).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path());
    near.find(named)
}

fn cube(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.contains('/') || Path::new(name).extension().is_some() {
        return find(dir, name);
    }
    let named = |p: &PathBuf, end| {
        let n = p.file_name().unwrap_or_default().to_string_lossy();
        n.starts_with("Adobe Premiere Pro ") && n.ends_with(end)
    };
    let list = |p: &Path| fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path());
    let apps = list("/Applications".as_ref())
        .filter(|p| named(p, ""))
        .flat_map(|p| list(&p))
        .filter(|p| named(p, ".app"));
    let file = format!("{name}.cube");
    let luts = apps.flat_map(|p| ["Creative", "Technical"].map(|f| p.join("Contents/Lumetri/LUTs").join(f)));
    [dir.to_path_buf()]
        .into_iter()
        .chain(luts)
        .map(|p| p.join(&file))
        .find(|p| p.is_file())
}

const FACES: [&str; 3] = ["ttf", "otf", "ttc"];

fn bare(name: &str) -> bool {
    !name.contains('/') && Path::new(name).extension().is_none()
}

fn installed() -> impl Iterator<Item = PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Fonts");
    let system = ["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"].map(PathBuf::from);
    [home].into_iter().chain(system)
}

fn face(dir: &Path, name: &str) -> Option<PathBuf> {
    if !bare(name) {
        return find(dir, name);
    }
    let places = [dir.to_path_buf(), dir.join("fonts")].into_iter().chain(installed());
    let files = places.flat_map(|p| FACES.map(|e| p.join(format!("{name}.{e}"))));
    files.into_iter().find(|p| p.is_file())
}

pub fn fonts() -> Vec<String> {
    let files = installed().flat_map(|p| fs::read_dir(p).into_iter().flatten().flatten().map(|e| e.path()));
    let typed = files.filter(|p| p.extension().is_some_and(|e| FACES.iter().any(|f| e == *f)));
    let stems = typed.filter_map(|p| Some(p.file_stem()?.to_str()?.to_string()));
    let mut out: Vec<String> = stems.filter(|n| bare(n) && !n.starts_with('.')).collect();
    out.sort_by_key(|n| (n.to_lowercase(), n.clone()));
    out.dedup();
    out
}

fn stat(path: &Path) -> Result<(u64, u64)> {
    let meta = fs::metadata(path)?;
    Ok((meta.len(), meta.modified()?.duration_since(UNIX_EPOCH)?.as_secs()))
}

fn scan(path: &Path, prev: Option<&Media>, relock: bool, rel: &str) -> Result<Media> {
    let ((size, mtime), mut hash, mut file) = (stat(path)?, Sha256::new(), fs::File::open(path)?);
    BUSY.store(true, Relaxed);
    hash.update(size.to_le_bytes());
    for at in [0, size.saturating_sub(1 << 20)] {
        file.seek(SeekFrom::Start(at))?;
        io::copy(&mut (&file).take(1 << 20), &mut hash)?;
    }
    let sha = format!("{:x}", hash.finalize());
    if prev.is_some_and(|p| p.sha != sha) && !relock {
        bail!("{rel} changed since origami.lock (use --relock)");
    }
    let out = output(
        ffprobe()
            .args(["-v", "error", "-of", "json", "-show_entries"])
            .arg("format=duration:stream=codec_type,width,height:stream_side_data=rotation")
            .arg(path),
    )?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let has = |t| streams.iter().any(|s| s["codec_type"] == t);
    let dim = |k| streams.iter().find_map(|s| s[k].as_u64()).unwrap_or(0) as u32;
    let sides = streams.iter().flat_map(|s| s["side_data_list"].as_array().into_iter().flatten());
    let turn = sides.filter_map(|d| d["rotation"].as_f64()).any(|r| r.abs() % 180. == 90.);
    Ok(Media {
        size,
        mtime,
        sha,
        dur: v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.),
        w: dim(if turn { "height" } else { "width" }),
        h: dim(if turn { "width" } else { "height" }),
        video: has("video"),
        audio: has("audio"),
    })
}

fn media(path: &Path, old: &Lock, new: &mut Lock, relock: bool, rel: &str) -> Result<String> {
    let (size, mtime) = stat(path)?;
    let same = |m: &Media| m.size == size && m.mtime == mtime;
    let m = match old.get(rel) {
        Some(m) if same(m) => m.clone(),
        prev => {
            let known = COLD.with_borrow_mut(|(first, missed, probed)| match first {
                true => {
                    missed.push((rel.into(), path.into()));
                    Some(Media {
                        size,
                        mtime,
                        sha: String::new(),
                        dur: 0.,
                        w: 0,
                        h: 0,
                        video: true,
                        audio: true,
                    })
                }
                false => probed.remove(rel).filter(same),
            });
            known.map_or_else(|| scan(path, prev, relock, rel), Ok)?
        }
    };
    new.insert(rel.into(), m.clone());
    Ok(serde_json::json!({ "path": path, "dur": m.dur, "w": m.w, "h": m.h, "video": m.video, "audio": m.audio }).to_string())
}

type Find = fn(&Path, &str) -> Option<PathBuf>;

fn host(args: &[JsValue], cx: &mut Context, find: Find, f: impl FnOnce(&str, &Path) -> Result<String>) -> JsResult<JsValue> {
    let mut text = |i: usize| -> JsResult<String> { Ok(args.get(i).cloned().unwrap_or_default().to_string(cx)?.to_std_string_escaped()) };
    let (path, call) = (text(0)?, text(1)?);
    let line = args.get(2).and_then(JsValue::as_number).unwrap_or(0.) as u32;
    let out = match ST.with_borrow(|st| find(&st.0, &path)) {
        Some(file) => {
            READ.with_borrow_mut(|r| r.push((line, file.clone())));
            f(&path, &file).map_err(|e| anyhow!("line {line}: {call}(\"{path}\"): {e}"))
        }
        None => Err(Error::Missing { call, path, line }.into()),
    };
    let fail = |e: anyhow::Error| {
        let message = e.to_string();
        FAIL.set(e.downcast().ok());
        JsNativeError::error().with_message(message)
    };
    Ok(JsString::from(out.map_err(fail)?).into())
}

fn probe(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, find, |rel, path| {
        ST.with_borrow_mut(|(_, old, new, relock, _)| media(path, old, new, *relock, rel))
    })
}

fn read(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, find, |_, path| Ok(fs::read_to_string(path)?))
}

fn lut(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, cube, |_, path| Ok(path.to_string_lossy().into()))
}

fn font(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, face, |_, path| Ok(path.to_string_lossy().into()))
}

fn extent(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let text = args.first().cloned().unwrap_or_default().to_string(cx)?.to_std_string_escaped();
    let num = |i: usize| args.get(i).and_then(JsValue::as_number).unwrap_or(0.);
    let clip: Clip = serde_json::from_str(&text).map_err(|e| JsNativeError::error().with_message(e.to_string()))?;
    let (w, h) = ST.with_borrow(|st| st.4).map_or((0., 0.), |f| f(&clip, (num(1), num(2))));
    Ok(if num(3) == 0. { w } else { h }.into())
}

fn assigns(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let src = args.get_or_undefined(0).to_string(cx)?.to_std_string_escaped();
    let rows: Vec<JsValue> = src
        .split('\n')
        .enumerate()
        .filter(|(_, s)| s.contains('='))
        .map(|(i, s)| JsArray::from_iter([JsValue::from(i as u32 + 1), JsString::from(s).into()], cx).into())
        .collect();
    Ok(JsArray::from_iter(rows, cx).into())
}

fn tag(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let mut text = |i: usize| -> JsResult<String> { Ok(args.get(i).cloned().unwrap_or_default().to_string(cx)?.to_std_string_escaped()) };
    let (src, made, bare, called) = (text(0)?, text(1)?, text(2)?, text(3)?);
    let (mut out, mut line, mut from, mut quote) = (String::new(), 1, 0, None);
    for (i, c) in src.match_indices(['(', '\n', '"', '\'', '`']) {
        match (quote, c) {
            (_, "\n") => (line, quote) = (line + 1, quote.filter(|q| *q == "`")),
            (Some(q), _) if q == c && !src[..i].ends_with('\\') => quote = None,
            (None, "\"" | "'" | "`") => quote = Some(c),
            _ => {}
        }
        if quote.is_some() || c != "(" {
            continue;
        }
        let end = src[..i].trim_end().len();
        let at = src[..end].trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_').len();
        let list = if src[..at].ends_with('.') { &called } else { &bare };
        if [&made, list].iter().any(|l| l.split(' ').any(|n| n == &src[at..end])) {
            out += &src[from..i];
            out += &format!("[{line}]");
            from = i;
        }
    }
    Ok(JsString::from(out + &src[from..]).into())
}

fn pairs(keys: &JsValue, d: f64, cx: &mut Context) -> JsResult<Option<Vec<(f64, f64, bool)>>> {
    let all = JsArray::from_object(keys.to_object(cx)?)?;
    let pair = |j, cx: &mut Context| {
        let Some(p) = all.get(j, cx)?.as_object().filter(JsObject::is_array) else {
            return Ok(None);
        };
        let (s, v, e) = (p.get(0, cx)?, p.get(1, cx)?, p.get(2, cx)?);
        let share = s.as_string().map(|s| s.to_std_string_escaped()).and_then(|s| {
            let part = |c: char| c.is_ascii_digit() || c == '.';
            let body = s.strip_suffix('%').filter(|b| !b.is_empty() && b.chars().all(part))?;
            let lead = body.match_indices('.').nth(1).map_or(body, |(i, _)| &body[..i]);
            Some(lead.parse().unwrap_or(f64::NAN) / 100. * d)
        });
        let whole = |v: &JsValue| v.as_number().filter(|v| v.is_finite());
        let t = whole(&s).map(|s| if s < 0. { d + s } else { s }).or(share);
        Ok(t.zip(whole(&v)).map(|(t, v)| (t, v, !e.is_undefined())))
    };
    (0..all.length(cx)?).map(|j| pair(j, cx)).collect()
}

fn plain(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let keys = pairs(args.get_or_undefined(0), 0., cx)?;
    Ok(keys.is_some_and(|k| k.iter().all(|k| !k.2)).into())
}

fn dots(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let list = |v: &JsValue| v.as_object().filter(JsObject::is_array).map(JsArray::from_object);
    let Some(all) = list(args.get_or_undefined(0)) else {
        return Ok(false.into());
    };
    let (all, mut fine) = (all?, true);
    for j in 0..all.length(cx)? {
        let Some(p) = list(&all.get(j, cx)?) else {
            return Ok(false.into());
        };
        let (p, whole) = (p?, |v: JsValue| v.as_number().is_some_and(f64::is_finite));
        fine &= p.length(cx)? == 2 && whole(p.get(0, cx)?) && whole(p.get(1, cx)?);
    }
    Ok(fine.into())
}

fn ramp(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let mut num = |i: usize| args.get_or_undefined(i).to_number(cx);
    let (d, n, fps, mut i) = (num(1)?, num(2)?, num(3)?, 0);
    let Some(keys) = pairs(args.get_or_undefined(0), d, cx)? else {
        return Ok(JsValue::undefined());
    };
    let (t, v): (Vec<f64>, Vec<f64>) = keys.iter().map(|k| (k.0, k.1)).unzip();
    let at = |f: usize| {
        let x = f as f64 / fps;
        while i < t.len() && !(x < t[i]) {
            i += 1;
        }
        JsValue::from(match i {
            0 => v[0],
            i if i == t.len() => v[i - 1],
            i => v[i - 1] + (v[i] - v[i - 1]) * ((x - t[i - 1]) / (t[i] - t[i - 1])),
        })
    };
    Ok(JsArray::from_iter((0..n as usize).map(at).collect::<Vec<_>>(), cx).into())
}

fn single(v: f64) -> f32 {
    let (f, near) = ((v + 0.) as f32, |k: f64| (v * k) as f32);
    if near(1. + 1e-15) == f && near(1. - 1e-15) == f {
        return f;
    }
    let text = if v.abs() < 1e21 { format!("{v}") } else { format!("{v:e}") };
    serde_json::from_str(&text).unwrap_or(f)
}

fn keep(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let all = JsArray::from_object(args.get_or_undefined(0).to_object(cx)?)?;
    let mut s = vec![];
    for j in 0..all.length(cx)? {
        match all.get(j, cx)?.as_number().filter(|v| v.is_finite()) {
            Some(v) => s.push(v),
            None => return Ok(JsValue::undefined()),
        }
    }
    if s.iter().all(|v| *v == s[0]) {
        return Ok(false.into());
    }
    Ok(KEPT
        .with_borrow_mut(|k| {
            k.push(s.into_iter().map(single).collect());
            k.len() - 1
        })
        .into())
}

type Job = Box<dyn FnOnce(&mut Context) + Send>;

static VM: Mutex<Option<mpsc::Sender<Job>>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);
const LIMIT: Duration = Duration::from_secs(if cfg!(test) { 2 } else { 10 });

fn spawn() -> mpsc::Sender<Job> {
    let (jobs, queue) = mpsc::channel::<Job>();
    let queue = Arc::new(Mutex::new(queue));
    for _ in 0..2 {
        let queue = queue.clone();
        let work = move || loop {
            let mut cx = boot();
            let Ok(job) = queue.lock().unwrap().recv() else { break };
            job(&mut cx);
        };
        thread::Builder::new().stack_size(256 << 20).spawn(work).unwrap();
    }
    jobs
}

fn js(e: JsError) -> anyhow::Error {
    anyhow!("{e}")
}

fn timed(cx: &mut Context, code: &str, path: Option<&Path>) -> Result<JsResult<JsValue>> {
    let from = Instant::now();
    let script = match Script::parse(Source::from_reader(code.as_bytes(), path), None, cx) {
        Ok(script) => script,
        Err(e) => return Ok(Err(e)),
    };
    let (mut run, mut poll) = (
        pin!(script.evaluate_async_with_budget(cx, 1 << 16)),
        task::Context::from_waker(Waker::noop()),
    );
    loop {
        match run.as_mut().poll(&mut poll) {
            Poll::Ready(v) => return Ok(v),
            Poll::Pending if from.elapsed() > LIMIT && !BUSY.swap(false, Relaxed) => {
                bail!("script took too long: stopped after {} seconds", LIMIT.as_secs())
            }
            Poll::Pending => {}
        }
    }
}

fn boot() -> Context {
    let mut cx = Context::default();
    cx.runtime_limits_mut().set_loop_iteration_limit(10_000_000);
    for (name, f) in [
        (
            js_string!("__probe"),
            probe as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        (js_string!("__read"), read),
        (js_string!("__lut"), lut),
        (js_string!("__font"), font),
        (js_string!("__measure"), extent),
        (js_string!("__assigns"), assigns),
        (js_string!("__tag"), tag),
        (js_string!("__plain"), plain),
        (js_string!("__dots"), dots),
        (js_string!("__ramp"), ramp),
        (js_string!("__keep"), keep),
    ] {
        cx.register_global_builtin_callable(name, 1, NativeFunction::from_fn_ptr(f))
            .unwrap();
    }
    cx.eval(Source::from_bytes(include_str!("prelude.js"))).unwrap();
    cx
}

fn ask(script: &Path, relock: bool, src: Option<&str>, measure: Measure) -> Result<Timeline> {
    let (script, src, (back, got)) = (script.to_path_buf(), src.map(String::from), mpsc::channel());
    let job = move |cx: &mut Context| {
        let tl = build(cx, &script, relock, src.as_deref(), measure);
        drop(back.send(tl.map_err(|e| FAIL.take().filter(|_| src.is_none()).map_or(e, Into::into))));
    };
    let mut vm = VM.lock().unwrap();
    let _ = vm.get_or_insert_with(spawn).send(Box::new(job));
    loop {
        match got.recv_timeout(LIMIT * 3) {
            Ok(tl) => return tl,
            Err(mpsc::RecvTimeoutError::Timeout) if BUSY.swap(false, Relaxed) => {}
            Err(e) => {
                vm.take();
                match e {
                    mpsc::RecvTimeoutError::Timeout => bail!("script took too long: stopped after {} seconds", LIMIT.as_secs() * 3),
                    mpsc::RecvTimeoutError::Disconnected => bail!("script evaluation crashed: edit the script and try again"),
                }
            }
        }
    }
}

pub fn eval(script: &Path, relock: bool, measure: Measure) -> Result<Timeline> {
    ask(script, relock, None, measure)
}

pub fn peek(script: &Path, src: &str, measure: Measure) -> Result<Timeline> {
    ask(script, false, Some(src), measure)
}

fn build(cx: &mut Context, script: &Path, relock: bool, src: Option<&str>, measure: Measure) -> Result<Timeline> {
    let script = script
        .canonicalize()
        .with_context(|| format!("missing script: {}", script.display()))?;
    let dir = script.parent().unwrap().to_path_buf();
    let lock = dir.join("origami.lock");
    let old: Lock = fs::read(&lock)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let text = src.map_or_else(|| fs::read_to_string(&script), |s| Ok(s.into()))?;
    let mut depth = 0;
    let nest = |b| {
        depth += matches!(b, b'(' | b'[' | b'{') as i32 - matches!(b, b')' | b']' | b'}') as i32;
        depth > 100
    };
    if let Some(i) = text.bytes().position(nest) {
        bail!(
            "line {}: brackets are nested more than 100 deep",
            text[..i].matches('\n').count() + 1
        );
    }
    let run = |cx: &mut Context, code: &str| cx.eval(Source::from_bytes(code)).map_err(js);
    let set = |cx: &mut Context, k: JsString, v: JsValue| cx.register_global_property(k, v, Attribute::all()).map_err(js);
    let say = |cx: &mut Context, v: JsValue| Ok::<_, anyhow::Error>(v.to_string(cx).map_err(js)?.to_std_string_escaped());
    let fault = |cx: &mut Context, e: JsError| -> Result<JsValue> {
        set(cx, js_string!("__err"), JsString::from(e.to_string()).into())?;
        let told = run(cx, "__late(__err)")?;
        bail!("{}", say(cx, told)?);
    };
    let load = |cx: &mut Context| {
        FAIL.take();
        READ.take();
        ST.set((dir.clone(), old.clone(), Lock::new(), relock, Some(measure)));
        set(cx, js_string!("__src"), JsString::from(text.as_str()).into())?;
        let code = run(cx, "__prep(__src)")?;
        let code = say(cx, code)?;
        timed(cx, &code, Some("script".as_ref()))?.or_else(|e| fault(cx, e))
    };
    COLD.set((true, vec![], Lock::new()));
    let mut root = load(cx);
    let mut missed = COLD.take().1;
    if !missed.is_empty() {
        missed.sort();
        missed.dedup();
        let probe = |part: &[(String, PathBuf)]| {
            let known = |(rel, path): &(String, PathBuf)| Some((rel.clone(), scan(path, old.get(rel), relock, rel).ok()?));
            part.iter().filter_map(known).collect::<Vec<_>>()
        };
        let probed = thread::scope(|s| {
            let jobs: Vec<_> = missed
                .chunks(missed.len().div_ceil(32))
                .map(|part| s.spawn(move || probe(part)))
                .collect();
            jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
        });
        COLD.set((false, vec![], probed));
        *cx = boot();
        root = load(cx);
    }
    set(cx, js_string!("__root"), root?)?;
    KEPT.take();
    let out = timed(cx, "__out(__root)", None)?.or_else(|e| fault(cx, e))?;
    let out = say(cx, out)?;
    let new = ST.take().2;
    if new != old && src.is_none() {
        let _ = fs::write(lock, serde_json::to_string_pretty(&new)?);
    }
    let mut tl: Timeline = serde_json::from_str(&out).or_else(|_| {
        let all: serde_json::Value = serde_json::from_str(&out)?;
        for c in all["clips"].as_array().into_iter().flatten() {
            if let Err(e) = Clip::deserialize(c) {
                bail!("line {}: {e}", c["line"]);
            }
        }
        Ok::<_, anyhow::Error>(serde_json::from_value(all)?)
    })?;
    let frame = (tl.size.0 as f64, tl.size.1 as f64);
    if let Some((c, (w, h))) = tl
        .clips
        .iter()
        .map(|c| (c, measure(c, frame)))
        .find(|(_, (w, h))| w.max(*h) > 16384.)
    {
        bail!(
            "line {}: {} would be drawn {w:.0}x{h:.0} pixels, over 16384 on a side: lower its size or scale",
            c.line,
            c.label
        );
    }
    tl.files = READ.take();
    Ok(tl)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(c: &Clip, _: (f64, f64)) -> (f64, f64) {
        (c.size * c.scale.max(), c.size * c.scale.max() / 2.)
    }

    fn tl(src: &str) -> Timeline {
        let dir = std::env::temp_dir().join(format!("origami-{}", src.len()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edit.js"), src).unwrap();
        eval(&dir.join("edit.js"), true, size).unwrap()
    }

    // Clips placed one after another (cat) or on top of each other (stack) should start and end at the
    // right times. A time written as '0:02' means two seconds, and each clip remembers its name and
    // the script line it came from.
    #[test]
    fn times_and_nesting() {
        let t = tl("const a = text('a').for('0:02')\ncat(a, stack(color('#000').for(3), text('b').at('0:01')), text('c'))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.start, c.end)).collect();
        assert_eq!(span, [(0., 2.), (2., 5.), (3., 5.), (5., 10.)]);
        assert_eq!((t.duration, t.clips[0].label.as_str(), t.clips[3].line), (10., "a", 2));
    }

    // A colour setting applied to a whole group should reach every clip inside it, and setting the
    // same thing twice keeps the last value. A look file that does not exist should point at the line
    // that asked for it.
    #[test]
    fn grade_reaches_leaves() {
        let t = tl(
            "const a = color('#f00')\nstack(a.exposure(1), text('b'))\n  .saturation(50)\n  .curve('red', [[1, 1], [0, 0]])\n  .curve('luma', [[0, 0]])\n  .saturation(80)",
        );
        let g = t.clips[1].grade.clone().unwrap();
        assert_eq!(
            (g.saturation, g.exposure, &g.curve.red, g.curve.luma.len()),
            (Some(80.), 0., &vec![(0., 0.), (1., 1.)], 1)
        );
        assert_eq!((t.clips[0].grade.as_ref().unwrap().exposure, t.clips[1].line), (1., 2));
        assert_eq!(tl("color('#f00').sharpen(5)").clips[0].grade, None);
        let dir = std::env::temp_dir().join("origami-look");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edit.js"), "color('#f00')\n  .look('none')").unwrap();
        let e = eval(&dir.join("edit.js"), true, size).unwrap_err();
        assert_eq!(e.downcast_ref::<Error>().map(Error::line), Some(2));
    }

    // Keyframes should give the right value on every frame: between keys, before the first and after
    // the last, with times counted from the clip's end or as a percentage, and with each easing style.
    #[test]
    fn keys_sample_per_frame() {
        let t = tl("settings({ fps: 10 })
const k = (...a) => text('a').for(1).opacity(keys(...a))
const e = n => k([0, 0], [1, 1, n])
stack(
  k([0.2, 1], [0.6, 3]),
  k([0, 0], [-0.5, 1]),
  k(['20%', 0], ['60%', 4]),
  k([0, 0], [0.5, 8, 'step']),
  e('slowIn'), e('slowOut'), e('slowInSlowOut'), e(bezier(0.3, 0.3, 0.6, 0.6)),
  e('overshootOut'), e('bounceOut'), e('springOut'),
  text('a').for(1).rotate(delay(keys([0, 0], [0.4, 4]), 0.2)).opacity((t, d) => d),
)");
        let at = |c: usize, i| (t.clips[c].opacity.at(i) * 1e4).round() / 1e4;
        assert_eq!((at(0, 0), at(0, 4), at(0, 6), at(0, 9), at(0, 99)), (1., 2., 3., 3., 3.));
        assert_eq!((at(1, 0), at(1, 2), at(1, 5), at(1, 8)), (0., 0.4, 1., 1.));
        assert_eq!((at(2, 1), at(2, 4), at(2, 7)), (0., 2., 4.));
        assert_eq!((at(3, 4), at(3, 5)), (0., 8.));
        assert_eq!((at(4, 5), at(5, 5), at(6, 5), at(6, 2), at(7, 3)), (0.125, 0.875, 0.5, 0.032, 0.3));
        assert!(at(8, 6) > 1. && at(9, 4) > at(9, 5) && at(10, 2) > 1.);
        let turn = |i| t.clips[11].rotate.at(i);
        assert_eq!((turn(1), turn(4), turn(8), &t.clips[11].opacity), (0., 2., 4., &Val::Fixed(1.)));
        assert!(matches!(t.clips[0].opacity, Val::Frames(2, ref s) if s.len() == 10));
    }

    // Keys given straight to a clip are worked out for the whole clip at once. They should give exactly
    // the values the same keys give when asked one frame at a time: keys out of order, two keys at one
    // time, times counted from the end or written as a percentage in any accepted form, eases, and a
    // second opacity multiplied in.
    #[test]
    fn keys_at_once_match_frame_by_frame() {
        let t = tl("settings({ fps: 24 })
const all = [keys([0.5, 1], [0.2, 0.25], [0.2, 0.75], [-0.3, 0.1]), keys([0, 0], ['40%', 1, 'slowOut'], [2, 0.5]), keys([0.3, 0.7]),
  keys(['12.5%', 3], ['5.%', 1], ['.5%', 2], ['61.8.9%', 4], ['100%', 0])]
const slow = k => (t, d) => k(t, d)
stack(all.flatMap(k => [k, slow(k)].map(v => text('a').for(1.3).opacity(0.5).opacity(v).rotate(v))))");
        for pair in t.clips.chunks(2) {
            assert_eq!((&pair[0].opacity, &pair[0].rotate), (&pair[1].opacity, &pair[1].rotate));
        }
        assert!(matches!((&t.clips[0].rotate, &t.clips[4].rotate), (Val::Frames(_, s), Val::Fixed(_)) if s.len() == 32));
        assert!(matches!(&t.clips[6].rotate, Val::Frames(_, s) if s[0] == 3. && s[31] < 0.1));
    }

    // A value worked out for every frame should reach the timeline as the same number that writing
    // it out as text and reading it back gives, zero below zero and values half-way between two
    // steps included, and a value that never changes should stay one plain number.
    #[test]
    fn frame_values_arrive_as_written() {
        let odd = [
            0.9311594069004059,
            1.1078019738197327,
            0.11360756680369377,
            2.2057770490646362,
            1e-9,
            12345678.9,
        ];
        let t = tl(&format!(
            "settings({{ fps: 6 }})\nconst v = {odd:?}\ntext('a').for(1).rotate(t => v[Math.round(t * 6)]).opacity(t => -0).pos(t => t ? -0 : 1, t => -t * 1e-320)"
        ));
        let read: Vec<f32> = odd.iter().map(|v| serde_json::from_str(&v.to_string()).unwrap()).collect();
        let c = &t.clips[0];
        assert!(matches!(&c.rotate, Val::Frames(3, s) if *s == read && s[0] != odd[0] as f32));
        assert!(matches!(&c.pos, (Some(Val::Frames(_, x)), Some(Val::Frames(_, y))) if x[1].is_sign_positive() && y[1].is_sign_negative()));
        assert!(matches!(c.opacity, Val::Fixed(v) if v == 0. && v.is_sign_positive()));
    }

    // A clip that follows a keyframed property, straight or through add and mul, should get exactly
    // the values it gets when the same keys are asked one frame at a time: with built-in eases, a
    // curve and an ease written as a function, a leader that starts later and one never placed.
    #[test]
    fn follows_at_once_match_frame_by_frame() {
        let t = tl("settings({ fps: 24 })
const slow = k => (t, d) => k(t, d)
const all = [keys([0, 0.1], [1, 0.5], ['90%', 0.9]), keys([0.2, 1], [1.5, 0, 'bounceOut'], [3, 0.4, bezier(0.2, 0.1, 0.3, 0.9)]), keys([0, 0], [2, 1, x => x * x])]
stack(...all.flatMap((k, i) => [k, slow(k)].map(v => {
  const lead = text('lead').for(3).pos(v, 0.5).opacity(0.5).opacity(v).rotate(v)
  const tag = text('tag').for(4).pos(lead.pos.x, add(lead.pos.x, 0.1)).opacity(mul(lead.opacity, 0.5)).rotate(lead.rotate).scale(add(lead.opacity, 1))
  return i ? [lead.at(1), tag, text('far').for(2).pos(tag.pos).at(2)] : [tag]
})))");
        let tags: Vec<_> = t.clips.iter().filter(|c| c.src != "lead").collect();
        let moves = |c: &Clip| (c.pos.clone(), c.opacity.clone(), c.rotate.clone(), c.scale.clone());
        for (a, b) in [(0, 1), (2, 4), (3, 5), (6, 8), (7, 9)] {
            assert_eq!(moves(tags[a]), moves(tags[b]));
        }
        assert!(matches!(&tags[2].pos.0, Some(Val::Frames(_, s)) if s.len() == 96 && s[0] == 1. && s[95] != s[40]));
    }

    // Animated text (per-word motion, typewriter reveal, scale, rotation) should survive being saved
    // to the timeline file and loaded back unchanged. A clip with no animation stays as plain numbers.
    #[test]
    fn motion_round_trip() {
        let t = tl("text('ab cd\\nef').for(1).pos(null, t => t)
  .each('word', (i, n) => ({ opacity: delay(t => t, i), x: n }))
  .shown(0.5).scale(2).scale(t => t).rotate(90)");
        let c = &t.clips[0];
        let names: Vec<_> = c.vals().map(|v| v.0).collect();
        assert_eq!(names, ["scale", "y", "opacity", "rotate", "shown"]);
        assert!(matches!((&c.scale, &c.pos.1), (Val::Frames(3, s), Some(Val::Frames(1, _))) if s[15] == 1.));
        let span: Vec<_> = c.parts.iter().map(|p| (p.from, p.to, p.x.at(0))).collect();
        assert_eq!(span, [(0, 2, 3.), (3, 5, 3.), (6, 8, 3.)]);
        assert!(matches!(c.parts[1].opacity, Val::Frames(2, _)));
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"rotate\":90.0,\"shown\":0.5,\"parts\":[{\"from\":0,\"to\":2,\"opacity\":[2,["));
        assert_eq!(serde_json::from_str::<Timeline>(&json).unwrap(), t);
        let plain = serde_json::to_value(tl("color('#f00').scale(0.5).pos(0.1)")).unwrap();
        assert_eq!(plain["clips"][0]["scale"], 0.5);
        assert_eq!(plain["clips"][0]["pos"], serde_json::json!([0.1, null]));
        assert!(plain["clips"][0].get("parts").is_none());
    }

    // When an animation or shape call is written wrongly, the error should name the script line where
    // the mistake is, so the author can find it.
    #[test]
    fn motion_errors_name_the_line() {
        for (src, line) in [
            ("text('a')\n  .scale('big')", 2),
            ("const k =\n  keys([0, 0], [1, 1, 'slow'])\ntext('a')", 2),
            ("color('#f00')\n  .shown(1)", 2),
            ("text('a')\n\n  .opacity(t => 'x')", 3),
            ("color('#f00')\n\n\n  .for(1)\nrect(10)", 5),
            ("stack(\n  line(0, 0, 1, '1'))", 2),
            ("text('a', {\n  align: 'middle' })", 1),
            ("rect(10, 10)\n  .shown(1)", 2),
        ] {
            let dir = std::env::temp_dir().join(format!("origami-motion-{line}-{}", src.len()));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("edit.js"), src).unwrap();
            let e = eval(&dir.join("edit.js"), true, size).unwrap_err().to_string();
            assert!(e.contains(&format!("line {line}: ")), "{e}");
        }
    }

    // A clip that follows another clip's position or opacity should read it at the same moment on the
    // timeline, even when the two clips start at different times, and with an offset or multiplier
    // applied.
    #[test]
    fn follows_on_the_timeline_clock() {
        let t = tl("settings({ fps: 10 })
const title = text('t').for(2).pos(0.5, keys([0, 0], [1, 1])).opacity(keys([0, 0], [2, 1]))
const tag = text('g').for(4).pos(title.pos.x, add(title.pos.y, -0.25)).opacity(mul(title.opacity, 0.5))
const both = text('b').for(1).pos(title.pos).scale(title.scale).rotate(tag.pos.y)
const far = text('f').for(1).opacity(tag.opacity)
stack(title.at(1), tag, both.at(1.5), far.at(2), title.at(9))");
        let c = &t.clips;
        let near = |v: f64| (v * 1e4).round() / 1e4;
        let y = |n: usize, i| near(c[n].pos.1.as_ref().unwrap().at(i));
        assert_eq!((c[0].start, y(0, 5), y(0, 15)), (1., 0.5, 1.));
        assert_eq!((y(1, 0), y(1, 10), y(1, 15), y(1, 20), y(1, 39)), (-0.25, -0.25, 0.25, 0.75, 0.75));
        assert_eq!(
            (c[1].pos.0.clone(), c[2].pos.0.clone(), c[2].scale.clone()),
            (Some(Val::Fixed(0.5)), Some(Val::Fixed(0.5)), Val::Fixed(1.))
        );
        let dim = |n: usize, i| near(c[n].opacity.at(i));
        assert_eq!((dim(1, 5), dim(1, 20), dim(1, 39)), (0., 0.25, 0.475));
        assert_eq!(
            (y(2, 0), y(2, 5), near(c[2].rotate.at(0)), near(c[2].rotate.at(9))),
            (0.5, 1., 0.25, 0.75)
        );
        assert_eq!((dim(3, 0), dim(3, 5)), (0.25, 0.375));
        assert!(matches!((&c[1].pos.1, &c[3].opacity), (Some(Val::Frames(3, _)), Val::Frames(5, _))));
    }

    // Following a property the other clip never set should use where that clip sits by default
    // (centred in the frame). Following a clip that was never placed on the timeline should still
    // work.
    #[test]
    fn follows_defaults_and_unplaced_clips() {
        let t = tl("settings({ fps: 10 })
const a = text('a', { size: 100 }).for(1).scale(keys([0, 1], [1, 2]))
const l = line(0.2, 0.4, 0.6, 0.4, { width: 10 })
let c = text('c')
const d = text('d').opacity(c.opacity)
c = c.opacity(mul(d.opacity, 0.5))
stack(text('b').for(2).pos(a.pos).rotate(a.rotate).opacity(a.opacity).shown(a.scale), color('#f00').pos(l.pos.x, l.pos.y), c, d)");
        let (b, near) = (&t.clips[0], |v: f64| (v * 1e6).round() / 1e6);
        let at = |v: &Option<Val>, i| near(v.as_ref().unwrap().at(i));
        assert_eq!(
            (at(&b.pos.0, 0), at(&b.pos.1, 0)),
            (near(0.5 - 50. / 1920.), near(0.5 - 25. / 1080.))
        );
        assert_eq!(
            (at(&b.pos.0, 5), at(&b.pos.0, 15)),
            (near(0.5 - 75. / 1920.), near(0.5 - 95. / 1920.))
        );
        assert_eq!(
            (b.rotate.clone(), b.opacity.clone(), near(b.shown.at(19))),
            (Val::Fixed(0.), Val::Fixed(1.), 1.9)
        );

        assert_eq!(
            t.clips[1].pos,
            (Some(Val::Fixed(0.4 - 5. / 1920.)), Some(Val::Fixed(0.4 - 2.5 / 1080.)))
        );
        assert_eq!(
            (t.clips[2].opacity.clone(), t.clips[3].opacity.clone()),
            (Val::Fixed(0.5), Val::Fixed(1.))
        );
    }

    // Trying to follow something that cannot be followed (the wrong kind of property, a group, a clip
    // defined further down) should give a clear message that names the line.
    #[test]
    fn follow_errors_name_the_line() {
        for (src, line, says) in [
            (
                "const a = text('a')\ntext('b')\n  .opacity(a.pos)",
                3,
                "opacity cannot follow a.pos ",
            ),
            ("const a = text('a')\ntext('b')\n  .pos(a.pos, 0.5)", 3, "pos cannot follow a.pos "),
            ("const a = text('a')\ntext('b').rotate(a.for)", 2, "rotate cannot follow a.for "),
            (
                "const a = text('a')\n\ntext('b').scale(stack(a).scale)",
                3,
                "scale cannot follow a group.scale ",
            ),
            (
                "const a = text('a')\ntext('b')\n  .each('char', () => ({ x: a.pos.x }))",
                3,
                "each x cannot follow a.pos.x ",
            ),
            (
                "const a = text('a')\ntext('b').pos(\n  add(a.pos, 0.1))",
                3,
                "add cannot follow a.pos ",
            ),
            (
                "const a = text('a')\ntext('b').pos(\n  mul(a.pos.x, a.pos.y))",
                3,
                "mul is written mul(",
            ),
            ("const a = text('a')\ntext('b').opacity('a.opacity')", 2, "opacity takes a number"),
            (
                "const b = text('b')\n  .pos(a.pos)\n\nconst a = text('a')",
                2,
                "a is defined below, on line 4",
            ),
        ] {
            let dir = std::env::temp_dir().join(format!("origami-follow-{line}-{}", src.len()));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("edit.js"), src).unwrap();
            let e = eval(&dir.join("edit.js"), true, size).unwrap_err().to_string();
            assert!(e.contains(&format!("line {line}: {says}")), "{e}");
        }
    }

    // Rectangles, lines, and text with a background plate or alignment should carry their settings
    // into the timeline. Timeline files saved before these existed should still load.
    #[test]
    fn shapes_and_plates() {
        let t = tl("stack(
  rect(400.4, 120, { color: '#0008', radius: 12 }).for(2).pos(0.1, 0.2),
  line(0.1, 0.5, 0.9, 0.5),
  line(0, 0, 1, 1, { color: '#f00', width: 9 }).rotate(keys([0, 0], [1, 90])),
  text('a', { background: '#00000080', align: 'left' }),
  caption('b'),
)");
        let c = &t.clips;
        assert_eq!(
            (c[0].kind, c[0].w, c[0].h, c[0].radius, c[0].color.as_str()),
            (Kind::Rect, 400, 120, 12., "#0008")
        );
        assert_eq!(
            (c[1].kind, c[1].ends, c[1].size, c[1].color.as_str()),
            (Kind::Line, [0.1, 0.5, 0.9, 0.5], 4., "#ffffff")
        );
        assert!(matches!((c[2].size, &c[2].rotate), (9., Val::Frames(4, _))));
        assert_eq!((c[3].background.as_str(), c[3].align.as_str()), ("#00000080", "left"));
        assert_eq!((c[4].background.as_str(), c[4].align.as_str(), c[4].end), ("", "", 2.));
        let json = serde_json::to_value(&t).unwrap();
        let mut old = json.clone();
        for c in old["clips"].as_array_mut().unwrap() {
            for k in ["radius", "ends", "background", "align"] {
                c.as_object_mut().unwrap().remove(k);
            }
        }
        let old: Timeline = serde_json::from_value(old).unwrap();
        assert_eq!((old.clips[4].clone(), old.clips[0].radius), (c[4].clone(), 0.));
        assert_eq!(serde_json::from_value::<Timeline>(json).unwrap(), t);
    }

    // Preview quality and sidecar scrubbing are set in settings and reach the timeline; a value that
    // is not one of the choices, null included, names the line it is on, also when settings is called
    // as globalThis.settings or under another name.
    #[test]
    fn settings_check_their_values() {
        let t = tl("settings({ fps: 24 })\ntext('a')");
        assert_eq!((t.fps, t.preview.as_str(), t.proxy), (24, "auto", true));
        let t = tl("settings({ preview: 'half', proxy: false })\ntext('half')");
        assert_eq!((t.preview.as_str(), t.proxy), ("half", false));
        for (src, says) in [
            (
                "text('a')\nsettings({ preview: 'big' })",
                "line 2: preview is \"auto\", \"full\", \"half\" or \"quarter\"",
            ),
            (
                "settings({ fps: 30 })\n\nsettings({ proxy: 'no' })\ntext('a')",
                "line 3: proxy is true or false",
            ),
            (
                "text('a')\nsettings({ preview: null })",
                "line 2: preview is \"auto\", \"full\", \"half\" or \"quarter\"",
            ),
            ("settings({ proxy: null })\ntext('a')", "line 1: proxy is true or false"),
            (
                "\nsettings ({ preview: 'big' })\ntext('a')",
                "line 2: preview is \"auto\", \"full\", \"half\" or \"quarter\"",
            ),
            (
                "globalThis.settings({ fps: 0 })\ntext('a')",
                "line 1: fps takes a whole number from 1 to 240",
            ),
            ("text('a')\n\nglobalThis.settings({ proxy: 1 })", "line 3: proxy is true or false"),
            (
                "text('a')\nconst s = settings\nconst f = () =>\n  s({ fps: 0.5 })\nf()",
                "line 4: fps takes a whole number from 1 to 240",
            ),
        ] {
            let dir = std::env::temp_dir().join(format!("origami-settings-{}", src.len()));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("edit.js"), src).unwrap();
            let e = eval(&dir.join("edit.js"), true, size).unwrap_err().to_string();
            assert!(e.contains(says), "{e}");
        }
    }

    // A looped clip should repeat back to back, and shift should move all of its repeats later by the
    // same amount.
    #[test]
    fn shift_loop_speed() {
        let t = tl("stack(text('x').for(2).loop(2).shift(1), color('#f00'))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.start, c.end)).collect();
        assert_eq!(span, [(1., 3.), (3., 5.), (0., 5.)]);
    }

    fn fails(dir: &Path, src: &str) -> String {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("edit.js"), src).unwrap();
        eval(&dir.join("edit.js"), false, size).unwrap_err().to_string()
    }

    // Values that make no sense (a negative length, zero speed, a frame rate of 0, a loop without end,
    // a day-long timeline) and common slips (a number among clips, an unclosed string or bracket, an
    // error thrown by a function of time) should be refused with a message that names the line.
    #[test]
    fn slips_name_the_line() {
        let deep = format!("{}text('a'){}", "\nstack(".repeat(101), ")".repeat(101));
        for (src, line, says) in [
            (
                "text('a')\n  .for(NaN)",
                2,
                "for takes a time above 0, in seconds or \"m:ss\", up to 24 hours",
            ),
            ("text('a').for(-3)", 1, "for takes a time above 0"),
            ("text('a')\n  .for(1e9)", 2, "for takes a time above 0"),
            ("text('a')\n  .at('x')", 2, "at takes a time, in seconds"),
            ("const a = text('a')\n  .speed(0)", 2, "speed takes a number above 0"),
            ("text('a').speed(-1)", 1, "speed takes a number above 0"),
            ("text('a')\n  .loop(Infinity)", 2, "loop takes a number"),
            ("text('a')\n  .loop(1e9)", 2, "loop takes a whole number from 1 to 1000"),
            ("text('a')\n  .fade('a')", 2, "fade takes seconds, 0 or more"),
            ("text('a')\ntext('c', { size: 'big' })", 2, "size takes a number, 0 or more"),
            ("\nsettings({ fps: 0 })\ntext('a')", 2, "fps takes a whole number from 1 to 240"),
            ("settings({ fps: 1e9 })\ntext('a')", 1, "fps takes a whole number from 1 to 240"),
            ("settings({\n  size: [0, 0] })\ntext('a')", 1, "size is written [1920, 1080]"),
            (
                "const a = text('a')\nstack(a,\n  5)",
                2,
                "stack takes clips, but item 2 is not a clip",
            ),
            ("const a = text('a')\n\n42\n", 3, "script must end with a clip"),
            ("const a = text('a')\nstack(a,\n  text('c'.for(1))\n", 3, "SyntaxError: abrupt end"),
            (
                "const a = text('a')\nconst b = text(\"b).for(1)\nstack(a, b)",
                2,
                "SyntaxError: unterminated string literal",
            ),
            ("text('a')\ncolor()", 2, "color is written color(\"#203040\")"),
            ("stack(text('a'),\n  text('c').scale(t => { throw new Error('x') }))", 2, "x"),
            ("const a = text('a')\n\nstack(a, bb)", 3, "ReferenceError: bb is not defined"),
            ("text('a')\n  .opacity(t => NaN)", 2, "a function of time must return a number"),
            (
                "cat(text('a').for(80000),\n  text('b').for(80000))",
                2,
                "b would end more than 24 hours in",
            ),
            (
                "const a = text('a')\nconst f = () => f()\nstack(a, f())",
                2,
                "script took too long: a loop, or a function",
            ),
            (deep.as_str(), 102, "brackets are nested more than 100 deep"),
        ] {
            let e = fails(&std::env::temp_dir().join(format!("origami-slip-{line}-{}", src.len())), src);
            assert!(e.starts_with(&format!("line {line}: {says}")), "{e}");
        }
        let fine = format!("{}text('a'){}", "stack(".repeat(99), ")".repeat(99));
        assert_eq!(tl(&fine).clips.len(), 1);
    }

    // A number that is NaN, Infinity or not a number at all should be refused wherever a method, an
    // option or a key takes one, with a message that names the line and what takes the number.
    #[test]
    fn numbers_must_be_finite() {
        let dir = std::env::temp_dir().join("origami-finite");
        for (call, says) in [
            ("text('a').scale(NaN)", "scale takes a number"),
            ("text('a').scale(1e200).scale(1e200)", "scale takes a number"),
            ("text('a').rotate(Infinity)", "rotate takes a number"),
            ("text('a').opacity(NaN)", "opacity takes a number"),
            ("text('a').shown(-Infinity)", "shown takes a number"),
            ("text('a').pos(Infinity, NaN)", "pos takes a number"),
            ("text('a').pos(0.5, NaN)", "pos takes a number"),
            ("stack(text('a'), text('b')).scale(NaN)", "scale takes a number"),
            ("text('a').each('char', () => ({ x: Infinity }))", "each x takes a number"),
            ("text('a').volume(NaN)", "volume takes a number"),
            ("text('a').speed(Infinity)", "speed takes a number"),
            ("text('a').sharpen(NaN)", "sharpen takes a number"),
            ("text('a').exposure(NaN)", "exposure takes a number"),
            ("text('a').curve('red', [[0, NaN]])", "curve takes points of two numbers"),
            ("text('a').hueVsSat([[0, 0], 1])", "hueVsSat takes points of two numbers"),
            ("text('a').wheel('shadows', { hue: NaN })", "wheel hue takes a number"),
            ("text('a').shadowTint({ amount: '3' })", "shadowTint amount takes a number"),
            ("text('a').vignette({ feather: Infinity })", "vignette feather takes a number"),
            (
                "text('a').hslSecondary({ key: { color: '#f00', range: NaN } })",
                "hslSecondary range takes a number",
            ),
            (
                "text('a').hslSecondary({ key: { color: '#f00' }, correction: { tint: NaN } })",
                "hslSecondary tint takes a number",
            ),
            ("rect(10, 10, { radius: NaN })", "radius takes a number"),
            ("line(0, 0, 1, 1, { width: Infinity })", "width takes a number"),
            ("rect(NaN, 10)", "rect is written rect(400, 120"),
            ("text('a').opacity(keys([0, NaN], [1, 1]))", "keys are written keys("),
            ("text('a').opacity(keys([0, 0], [Infinity, 1]))", "keys are written keys("),
            ("text('a').opacity(mul(2, Infinity))", "mul is written mul("),
            ("text('a').opacity(add(NaN, 1))", "add takes a number"),
            ("text('a').opacity(\n  delay(t => t, NaN))", "delay takes a number"),
            ("const e =\n  bezier(NaN, 0, 1, 1)", "bezier is written bezier(0.3, 0, 0.6, 1)"),
        ] {
            let line = 2 + call.matches('\n').count();
            let e = fails(&dir, &format!("text('z')\n{call}\ntext('y')"));
            assert!(e.starts_with(&format!("line {line}: {says}")), "{call}: {e}");
        }
        let fine = "text('a').scale(2).pos(0.1, null).opacity(mul(delay(keys([0, 0], [1, 1, bezier(0.3, 0, 0.6, 1)]), 0.5), 2)).sharpen(5).vignette({ amount: 10 }).wheel('midtones', { hue: 20 }).curve('luma', [[0, 0], [1, 1]])";
        assert_eq!(tl(fine).clips.len(), 1);
    }

    // Words such as cat( or text( inside a quoted piece of text should be shown exactly as written.
    #[test]
    fn quoted_words_stay_as_written() {
        let t = tl("stack(text('my cat(s), see text(1)').for(1), text(\"it's a stack(2)\").for(1), text(`line(3)`))");
        let said: Vec<_> = t.clips.iter().map(|c| c.src.as_str()).collect();
        assert_eq!(said, ["my cat(s), see text(1)", "it's a stack(2)", "line(3)"]);
    }

    fn boom(_: &Clip, _: (f64, f64)) -> (f64, f64) {
        panic!("boom")
    }

    // A script that never finishes should be stopped with a message instead of hanging, a crash while
    // evaluating should be reported, and the next script should evaluate normally either way.
    #[test]
    fn runaway_scripts_stop() {
        let dir = std::env::temp_dir().join("origami-runaway");
        let spin = "const spin = () => { for (let i = 0; i < 1e6; i++); }\nfor (let i = 0; i < 1e6; i++) spin()";
        assert_eq!(fails(&dir, spin), "script took too long: stopped after 2 seconds");
        assert_eq!(tl("text('a').for(3)").duration, 3.);
        fs::write(dir.join("edit.js"), "const a = text('a')\ntext('b').pos(a.pos)").unwrap();
        let e = eval(&dir.join("edit.js"), false, boom).unwrap_err().to_string();
        assert_eq!(e, "script evaluation crashed: edit the script and try again");
        assert_eq!(tl("text('a').for(4)").duration, 4.);
    }

    fn media(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("origami-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let make = |args: &[&str]| {
            let made = status(ffmpeg().args(["-v", "error", "-y"]).args(args).current_dir(&dir));
            assert!(made.unwrap().success());
        };
        make(&["-f", "lavfi", "-i", "color=black:s=64x36:r=30:d=2", "v.mp4"]);
        make(&["-display_rotation", "90", "-i", "v.mp4", "-c", "copy", "tall.mp4"]);
        make(&["-f", "lavfi", "-i", "sine=d=2", "a.wav"]);
        make(&["-f", "lavfi", "-i", "color=red:s=8x8", "-frames:v", "1", "i.png"]);
        fs::write(dir.join("bad.mp4"), "not a film").unwrap();
        fs::write(dir.join("bad.srt"), "1\n00:00:01,000 --> 00:00:02,000\nfine\n\n2\nxx --> yy\nodd\n").unwrap();
        dir
    }

    // Opening a project for the first time should read every media file, give the same timeline as
    // the next open and remember what it read, even when the folder cannot be written to. Footage
    // turned on its side keeps its turned size, a trim past the end of a file stops at the end, and a
    // file that changed since it was remembered is refused until asked to read it again.
    #[test]
    fn media_is_read_and_remembered() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, at) = (media("probe"), |c: &Clip| (c.src_in, c.end, c.w, c.h));
        let (script, lock) = (dir.join("edit.js"), dir.join("origami.lock"));
        let src = "const at = '.'\nconst v = video(at + '/v.mp4')\nstack(v.trim(1.5, 9), video('tall.mp4'), sound('a.wav').trim(-1, 1),\n  image('i.png').for(1))";
        fs::write(&script, src).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let shut = eval(&script, false, size);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!lock.exists());
        let cold = eval(&script, false, size).unwrap();
        let seen = fs::read_to_string(&lock).unwrap();
        assert_eq!((shut.unwrap(), eval(&script, false, size).unwrap()), (cold.clone(), cold.clone()));
        assert_eq!(fs::read_to_string(&lock).unwrap(), seen);
        let c = &cold.clips;
        assert_eq!((cold.size, at(&c[0]), at(&c[1])), ((64, 36), (1.5, 0.5, 64, 36), (0., 2., 36, 64)));
        assert_eq!((at(&c[2]), at(&c[3])), ((0., 1., 0, 0), (0., 1., 8, 8)));
        let seen: Lock = serde_json::from_str(&seen).unwrap();
        assert_eq!(seen.keys().collect::<Vec<_>>(), ["./v.mp4", "a.wav", "i.png", "tall.mp4"]);
        assert_eq!((seen["tall.mp4"].w, seen["tall.mp4"].dur, seen["a.wav"].audio), (36, 2., true));
        fs::write(dir.join("i.png"), "changed").unwrap();
        let e = eval(&script, false, size).unwrap_err().to_string();
        assert_eq!(e, "line 4: image(\"i.png\"): i.png changed since origami.lock (use --relock)");
        assert_eq!(
            fails(&dir, src.replace("i.png", "none.png").as_str()),
            "line 4: image(\"none.png\") points to a file that does not exist"
        );
    }

    // A file that is not what the script says it is (a broken film, a film used as a picture, a
    // picture used as sound, a subtitle file with a bad time) should be refused with a message that
    // names the line and the file.
    #[test]
    fn odd_files_name_the_line() {
        let dir = media("odd");
        for (src, says) in [
            ("video('bad.mp4')", "line 1: video(\"bad.mp4\") cannot be read as video"),
            (
                "text('a')\nimage('v.mp4')",
                "line 2: image(\"v.mp4\") cannot be read as a still image",
            ),
            ("sound('i.png')", "line 1: sound(\"i.png\") cannot be read as sound"),
            (
                "stack(text('a'),\n  captions('bad.srt'))",
                "line 2: captions(\"bad.srt\") line 6: xx --> yy is not a time range like 00:00:01,000 --> 00:00:04,000",
            ),
            (
                "text('a')\n\ncaptions('i.png')",
                "line 3: captions(\"i.png\"): stream did not contain valid UTF-8",
            ),
        ] {
            assert_eq!(fails(&dir, src), says);
        }
    }

    // A colour that does not parse and a layer too large to draw should each stop the script with an
    // error on its line, while every colour form the engine draws is accepted.
    #[test]
    fn refuses_unusable_values() {
        let dir = std::env::temp_dir().join("origami-refuse");
        fs::create_dir_all(&dir).unwrap();
        let run = |src: &str| {
            fs::write(dir.join("edit.js"), src).unwrap();
            eval(&dir.join("edit.js"), true, size).map_err(|e| e.to_string())
        };
        let ok = "stack(color('#ffffff').for(1), color('#abc'), color('#abcd'), color('ABCDEF'), color('#aabbcc80'), text('a', { color: '#000000', background: '#0008' }), rect(9, 9, { color: '#fff' }), line(0, 0, 1, 1, { color: '#000' }), text('b').hslSecondary({ key: { color: '#f00' } }))";
        assert!(run(ok).is_ok());
        for (src, want) in [
            ("text('a')\nrect(9, 9, { color: 'bleu' })", "line 2: \"bleu\" is not a colour"),
            ("text('a', { color: '#ff000' })", "line 1: \"#ff000\" is not a colour"),
            ("text('a',\n  { background: 'nope' })", "line 1: \"nope\" is not a colour"),
            ("text('a')\ncolor('red')", "line 2: \"red\" is not a colour"),
            ("text('a', { color: 'white' })", "line 1: \"white\" is not a colour: write hex"),
            (
                "text('a').hslSecondary({ key: { color: '+fffff' } })",
                "line 1: \"+fffff\" is not a colour",
            ),
            (
                "text('a')\ntext('b', { size: 1e9 })",
                "line 2: b would be drawn 1000000000x500000000 pixels",
            ),
            (
                "text('a')\ntext('b').scale(keys([0, 1], [1, 1e6]))",
                "line 2: b would be drawn 72000000x36000000 pixels",
            ),
        ] {
            let e = run(src).unwrap_err();
            assert!(e.starts_with(want), "{src}: {e}");
        }
    }

    // An odd frame size should be rounded to even numbers, with a note naming its line when the line
    // is known.
    #[test]
    fn rounds_odd_sizes() {
        let dir = std::env::temp_dir().join("origami-odd-size");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edit.js"), "settings({ size: [641, 360] })\ntext('a')").unwrap();
        let tl = eval(&dir.join("edit.js"), true, size).unwrap();
        assert_eq!(tl.size, (642, 360));
        assert_eq!(
            tl.note,
            "line 1: size [641, 360] is rounded to [642, 360], video files need even numbers"
        );
        fs::write(dir.join("edit.js"), "globalThis.settings({ size: [641, 360] })\ntext('a')").unwrap();
        assert_eq!(
            eval(&dir.join("edit.js"), true, size).unwrap().note,
            "size [641, 360] is rounded to [642, 360], video files need even numbers"
        );
    }

    // Every installed font the app lists by name should be one a script can ask for by that name, and
    // the list holds each name once.
    #[test]
    fn listed_fonts_resolve() {
        let all = fonts();
        assert!(all.contains(&"Menlo".to_string()) && all.iter().all(|n| face(Path::new("/nowhere"), n).is_some()));
        assert!(all.windows(2).all(|w| w[0] != w[1]));
    }
}
