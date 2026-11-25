mod disk;
mod error;
mod grade;
mod ir;
mod picture;
mod tool;

use anyhow::{Context as _, Result, anyhow, bail};
use boa_engine::{
    Context, JsArgs, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source, js_string, object::builtins::JsArray,
    property::Attribute, script::Script,
};
pub use disk::{load, save};
pub use error::Error;
pub use grade::{Curves, Grade, Secondary, Vignette, Wheels};
pub use ir::{Clip, Fx, Kind, Part, Timeline, Val};
pub use picture::picture;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, Read, Seek, SeekFrom, Write},
    mem,
    os::unix::{fs::MetadataExt, process::ExitStatusExt},
    panic,
    path::{Path, PathBuf},
    pin::pin,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock, PoisonError,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc::{self, RecvTimeoutError},
    },
    task::{self, Poll, Waker},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};
pub use tool::{ffmpeg, ffprobe, literal, missing, output, start, status, stop, watch};

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
    native: bool,
}

type Lock = BTreeMap<String, Media>;

pub type Measure = fn(&Clip, (f64, f64)) -> (f64, f64);

thread_local! {
    static ST: RefCell<(PathBuf, Lock, Lock, bool, Option<Measure>)> = RefCell::default();
    static FAIL: RefCell<Option<Error>> = RefCell::default();
    static READ: RefCell<Vec<(u32, PathBuf)>> = RefCell::default();
    static COLD: RefCell<(bool, Vec<(String, PathBuf)>, Lock)> = RefCell::default();
    static KEPT: RefCell<Vec<Vec<f32>>> = RefCell::default();
    static LINE: RefCell<u32> = RefCell::default();
}

fn find(dir: &Path, rel: &str) -> Option<PathBuf> {
    let path = dir.join(rel);
    if path.is_file() {
        return Some(path);
    }
    let named = |p: &PathBuf| p.is_file() && p.file_stem() == path.file_name();
    path.parent().into_iter().flat_map(inside).find(named)
}

fn inside(dir: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path())
}

fn cube(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.contains('/') || Path::new(name).extension().is_some() {
        return find(dir, name);
    }
    let named = |p: &PathBuf, end| {
        let n = p.file_name().unwrap_or_default().to_string_lossy();
        n.starts_with("Adobe Premiere Pro ") && n.ends_with(end)
    };
    let apps = inside("/Applications".as_ref())
        .filter(|p| named(p, ""))
        .flat_map(|p| inside(&p))
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
    let files = installed().flat_map(|p| inside(&p));
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
    if SERVING.load(Relaxed) {
        let _ = send(&mut io::stdout().lock(), &[]);
    }
    hash.update(size.to_le_bytes());
    for at in [0, size.saturating_sub(1 << 20)] {
        file.seek(SeekFrom::Start(at))?;
        io::copy(&mut (&file).take(1 << 20), &mut hash)?;
    }
    let sha = format!("{:x}", hash.finalize());
    if prev.is_some_and(|p| p.sha != sha) && !relock {
        bail!("{rel} changed since origami.lock (use --relock)");
    }
    if let Some((w, h)) = picture::size(path) {
        return Ok(Media {
            size,
            mtime,
            sha,
            dur: 0.,
            w,
            h,
            video: true,
            audio: false,
            native: true,
        });
    }
    let named = literal(&path.to_string_lossy());
    let out = output(
        ffprobe()
            .args(["-v", "error", "-of", "json", "-show_entries"])
            .arg("format=duration:stream=codec_type,codec_name,width,height:stream_side_data=rotation")
            .args(named)
            .arg(path),
    )?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let has = |t| streams.iter().any(|s| s["codec_type"] == t);
    let dim = |k| streams.iter().find_map(|s| s[k].as_u64()).unwrap_or(0) as u32;
    let sides = streams.iter().flat_map(|s| s["side_data_list"].as_array().into_iter().flatten());
    let turn = sides.filter_map(|d| d["rotation"].as_f64()).any(|r| r.abs() % 180. == 90.);
    let dur = v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.);
    let kind = if has("video") { "video" } else { "audio" };
    if let Some(s) = streams.iter().find(|s| s["codec_type"] == kind) {
        let (map, frames) = (format!("0:{}:0", &kind[..1]), format!("-frames:{}", &kind[..1]));
        let tail = ["-map", &map, &frames, "1", "-f", "null", "-"];
        let seen = output(ffmpeg().args(["-v", "error"]).args(named).arg("-i").arg(path).args(tail))?;
        if !seen.status.success() {
            let what = match (kind, dur <= 0.04) {
                ("video", true) => "picture",
                ("video", false) => "video",
                _ => "sound",
            };
            let name = match what {
                "picture" => path.extension().and_then(|e| e.to_str()),
                _ => s["codec_name"].as_str(),
            };
            bail!("{}", unread(name, what, &String::from_utf8_lossy(&seen.stderr)));
        }
    }
    Ok(Media {
        size,
        mtime,
        sha,
        dur,
        w: dim(if turn { "height" } else { "width" }),
        h: dim(if turn { "width" } else { "height" }),
        video: has("video"),
        audio: has("audio"),
        native: false,
    })
}

fn unread(name: Option<&str>, what: &str, said: &str) -> String {
    let name = name.map_or("unrecognised".into(), str::to_uppercase);
    match what {
        _ if !said.contains("no decoder found") => format!("this {name} {what} cannot be read: the file may be damaged"),
        "picture" => format!("{name} pictures are not supported: export it as PNG first"),
        "video" => format!("this is {name} video, which Origami cannot read: convert it to H.264, HEVC or ProRes first"),
        _ => format!("this is {name} sound, which Origami cannot read: convert it to WAV, MP3 or AAC first"),
    }
}

fn media(path: &Path, old: &Lock, new: &mut Lock, relock: bool, rel: &str) -> Result<String> {
    if !new.contains_key(rel) {
        tool::playable(path)?;
    }
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
                        native: false,
                    })
                }
                false => probed.get(rel).filter(|m| same(m)).cloned(),
            });
            known.map_or_else(|| scan(path, prev, relock, rel), Ok)?
        }
    };
    new.insert(rel.into(), m.clone());
    Ok(
        serde_json::json!({ "path": path, "dur": m.dur, "w": m.w, "h": m.h, "video": m.video, "audio": m.audio, "native": m.native })
            .to_string(),
    )
}

type Find = fn(&Path, &str) -> Option<PathBuf>;

fn arg(args: &[JsValue], i: usize, cx: &mut Context) -> JsResult<String> {
    Ok(args.get_or_undefined(i).to_string(cx)?.to_std_string_escaped())
}

fn host(args: &[JsValue], cx: &mut Context, name: &str, find: Find, f: impl FnOnce(&str, &Path) -> Result<String>) -> JsResult<JsValue> {
    let path = arg(args, 0, cx)?;
    let call = args
        .get(1)
        .and_then(JsValue::as_string)
        .map_or(name.into(), |c| c.to_std_string_escaped());
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
        let message = message.strip_prefix("line 0: ").unwrap_or(&message).to_string();
        FAIL.set(e.downcast().ok().filter(|_| line > 0));
        JsNativeError::error().with_message(message)
    };
    Ok(JsString::from(out.map_err(fail)?).into())
}

fn probe(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, "__probe", find, |rel, path| {
        ST.with_borrow_mut(|(_, old, new, relock, _)| media(path, old, new, *relock, rel))
    })
}

fn read(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, "__read", find, |_, path| {
        let text = Some(load(path)?)
            .filter(|t| t.lines().count() <= 1 << 18)
            .ok_or_else(|| anyhow!("the file has over {} lines", 1 << 18))?
            .replace('\r', "");
        let rows = text.split("\n\n").scan(1, |row, b| {
            let at = *row;
            *row += b.matches('\n').count() + 2;
            Some((at, b))
        });
        Ok(serde_json::to_string(&rows.collect::<Vec<_>>())?)
    })
}

pub type Table = (usize, bool, Vec<[f32; 3]>);

pub fn table(text: &str) -> Option<Table> {
    let (mut n, mut cube, mut t) = (0, true, vec![]);
    for line in text.lines().map(str::trim) {
        let mut words = line.split_whitespace();
        let (Some(head), rest) = (words.next(), words.flat_map(str::parse::<f32>).collect::<Vec<_>>()) else {
            continue;
        };
        match (head, &rest[..]) {
            ("LUT_3D_SIZE", [size]) => n = *size as usize,
            ("LUT_1D_SIZE", [size]) => (n, cube) = (*size as usize, false),
            ("DOMAIN_MIN", lo) if lo != [0.; 3] => return None,
            ("DOMAIN_MAX", hi) if hi != [1.; 3] => return None,
            (r, [g, b]) => t.extend(r.parse().ok().map(|r| [r, *g, *b])),
            _ => {}
        }
    }
    (n > 1 && Some(t.len()) == if cube { n.checked_pow(3) } else { Some(n) }).then_some((n, cube, t))
}

static TABLES: Mutex<BTreeMap<PathBuf, (u64, i64, i64)>> = Mutex::new(BTreeMap::new());

fn lut(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, "__lut", cube, |_, path| {
        let m = fs::metadata(path)?;
        let stamp = (m.len(), m.mtime(), m.mtime_nsec());
        let mut whole = TABLES.lock().unwrap_or_else(PoisonError::into_inner);
        if whole.get(path) != Some(&stamp) {
            table(&load(path)?).ok_or_else(|| anyhow!("not a .cube file with a 0 to 1 domain"))?;
            whole.insert(path.into(), stamp);
        }
        Ok(path.to_string_lossy().into())
    })
}

fn font(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, "__font", face, |_, path| Ok(path.to_string_lossy().into()))
}

fn extent(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let text = arg(args, 0, cx)?;
    let num = |i: usize| args.get(i).and_then(JsValue::as_number).unwrap_or(0.);
    let clip: Clip = serde_json::from_str(&text).map_err(|_| {
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        JsNativeError::error().with_message(match (v["line"].as_u64(), v["label"].as_str()) {
            (Some(n), Some(l)) => format!("line {n}: {l} has a value out of range"),
            _ => "__measure takes a clip written by Origami".into(),
        })
    })?;
    let read = |f: &str| READ.with_borrow(|r| r.iter().any(|(_, p)| p.as_os_str() == f));
    if let Some(e) = stray(&clip, read) {
        return Err(JsNativeError::error().with_message(e).into());
    }
    let (w, h) = ST.with_borrow(|st| st.4).map_or((0., 0.), |f| f(&clip, (num(1), num(2))));
    Ok(if num(3) == 0. { w } else { h }.into())
}

fn stray(c: &Clip, opened: impl Fn(&str) -> bool) -> Option<String> {
    let media = matches!(c.kind, Kind::Video | Kind::Sound | Kind::Image).then_some(&c.src);
    let looks = c.grade.iter().flat_map(|g| g.input_lut.iter().chain(g.look.iter().map(|l| &l.0)));
    let f = media
        .into_iter()
        .chain([&c.font])
        .chain(looks)
        .find(|f| !f.is_empty() && !opened(f.as_str()))?;
    Some(format!(
        "line {}: {} uses {f}, a file the script did not open with video, sound, image, font or look",
        c.line, c.label
    ))
}

fn assigns(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let src = arg(args, 0, cx)?;
    let rows: Vec<JsValue> = src
        .split('\n')
        .enumerate()
        .filter(|(_, s)| s.contains('='))
        .map(|(i, s)| JsArray::from_iter([JsValue::from(i as u32 + 1), JsString::from(s).into()], cx).into())
        .collect();
    Ok(JsArray::from_iter(rows, cx).into())
}

fn tag(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let mut text = |i| arg(args, i, cx);
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

fn list(v: &JsValue) -> Option<JsArray> {
    v.as_object().and_then(|o| JsArray::from_object(o).ok())
}

fn pairs(keys: &JsValue, d: f64, cx: &mut Context) -> JsResult<Option<Vec<(f64, f64, bool)>>> {
    let Some(all) = list(keys) else {
        return Ok(None);
    };
    let pair = |j, cx: &mut Context| {
        let Some(p) = list(&all.get(j, cx)?) else {
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
    let Some(all) = list(args.get_or_undefined(0)) else {
        return Ok(false.into());
    };
    let mut fine = true;
    for j in 0..all.length(cx)? {
        let Some(p) = list(&all.get(j, cx)?) else {
            return Ok(false.into());
        };
        let whole = |v: JsValue| v.as_number().is_some_and(f64::is_finite);
        fine &= p.length(cx)? == 2 && whole(p.get(0, cx)?) && whole(p.get(1, cx)?);
    }
    Ok(fine.into())
}

fn ramp(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let mut num = |i: usize| args.get_or_undefined(i).to_number(cx);
    let (d, n, fps, mut i) = (num(1)?, num(2)?, num(3)?, 0);
    let Some(keys) = pairs(args.get_or_undefined(0), d, cx)?.filter(|k| !k.is_empty()) else {
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
    Ok(JsArray::from_iter((0..(n as usize).min(1 << 25)).map(at).collect::<Vec<_>>(), cx).into())
}

fn single(v: f64) -> f32 {
    let (f, near) = ((v + 0.) as f32, |k: f64| (v * k) as f32);
    if near(1. + 1e-15) == f && near(1. - 1e-15) == f {
        return f;
    }
    let text = if v.abs() < 1e21 { format!("{v}") } else { format!("{v:e}") };
    serde_json::from_str(&text).unwrap_or(f)
}

const MANY: usize = if cfg!(test) { 1 << 16 } else { 1 << 20 };

fn keep(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let Some(all) = list(args.get_or_undefined(0)) else {
        return Ok(JsValue::undefined());
    };
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
    if KEPT.with_borrow(Vec::len) >= MANY {
        return Err(JsNativeError::error()
            .with_message(format!("the script has over {MANY} animated values"))
            .into());
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
    cx.runtime_limits_mut().set_stack_size_limit(1 << 21);
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

const HELP: &str = "--origami-evaluate";
const HEAVY: u64 = if cfg!(test) { 1 << 28 } else { 2 << 30 };
static FAR: AtomicBool = AtomicBool::new(false);
static SERVING: AtomicBool = AtomicBool::new(false);
type Helper = (Child, ChildStdin, mpsc::Receiver<Vec<u8>>);
static HELPER: Mutex<Option<Helper>> = Mutex::new(None);

unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, info: *mut [u64; 12]) -> i32;
    fn _exit(code: i32) -> !;
}

fn heft(pid: u32) -> u64 {
    let mut info = [0; 12];
    match unsafe { proc_pid_rusage(pid as i32, 0, &mut info) } {
        0 => info[9],
        _ => 0,
    }
}

fn send(w: &mut impl Write, data: &[u8]) -> io::Result<()> {
    w.write_all(&(data.len() as u64).to_le_bytes())?;
    w.write_all(data)?;
    w.flush()
}

fn frame(r: &mut impl Read) -> Option<Vec<u8>> {
    let mut n = [0; 8];
    r.read_exact(&mut n).ok()?;
    let n = Some(u64::from_le_bytes(n)).filter(|n| *n <= 1 << 28)?;
    let mut v = vec![];
    r.take(n).read_to_end(&mut v).ok()?;
    (v.len() as u64 == n).then_some(v)
}

pub fn serve(measure: Measure) {
    picture::serve();
    if env::args_os().nth(1).is_none_or(|a| a != HELP) {
        FAR.store(true, Relaxed);
        BUILT.get_or_init(stamp);
        thread::spawn(|| {
            let mut slot = HELPER.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.is_none() {
                *slot = fresh().ok();
            }
        });
        return;
    }
    SERVING.store(true, Relaxed);
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |p| {
        hook(p);
        unsafe { _exit(101) }
    }));
    let vm = spawn();
    while let Some(job) = frame(&mut io::stdin().lock()) {
        let _ = vm.send(Box::new(move |cx: &mut Context| {
            let reply = answer(cx, &job, measure);
            if send(&mut io::stdout().lock(), &reply).is_err() {
                unsafe { _exit(0) }
            }
        }));
    }
    unsafe { _exit(0) }
}

type Head = (Result<(usize, Vec<usize>, Vec<(u32, PathBuf)>), Vec<String>>, Option<Error>);

fn answer(cx: &mut Context, job: &[u8], measure: Measure) -> Vec<u8> {
    let (script, relock, src): (PathBuf, bool, Option<String>) = serde_json::from_slice(job).unwrap_or_default();
    BUSY.store(false, Relaxed);
    let got = half(cx, &script, relock, src.as_deref(), measure);
    let fail = got.as_ref().err().and_then(|e| uncaught(e, src.is_none()));
    let (head, out, kept): (Head, _, _) = match got {
        Ok((out, kept, files)) => ((Ok((out.len(), kept.iter().map(Vec::len).collect(), files)), fail), out, kept),
        Err(e) => ((Err(e.chain().map(|c| c.to_string()).collect()), fail), String::new(), vec![]),
    };
    let head = serde_json::to_vec(&head).unwrap_or_default();
    let mut reply = [&(head.len() as u64).to_le_bytes()[..], &head, out.as_bytes()].concat();
    reply.extend(kept.iter().flatten().flat_map(|f| f.to_le_bytes()));
    reply
}

fn hire(cmd: &mut Command) -> io::Result<Helper> {
    let mut child = start(cmd.stdin(Stdio::piped()).stdout(Stdio::piped()))?;
    let (stdin, mut stdout) = child.stdin.take().zip(child.stdout.take()).ok_or(io::ErrorKind::BrokenPipe)?;
    let (tell, heard) = mpsc::channel();
    thread::spawn(move || {
        while let Some(f) = frame(&mut stdout) {
            if tell.send(f).is_err() {
                break;
            }
        }
    });
    Ok((child, stdin, heard))
}

type Stamp = Option<(u64, u64, i64, i64)>;
static BUILT: OnceLock<Stamp> = OnceLock::new();

fn stamp() -> Stamp {
    let m = fs::metadata(env::current_exe().ok()?).ok()?;
    Some((m.dev(), m.ino(), m.mtime(), m.mtime_nsec()))
}

fn fresh() -> Result<Helper> {
    if BUILT.get().is_some_and(|b| *b != stamp()) {
        bail!("Origami was updated: restart it to run scripts");
    }
    let made = env::current_exe().and_then(|me| hire(Command::new(me).arg(HELP)));
    made.map_err(|e| anyhow!("could not start evaluating the script: {e}"))
}

fn crashed() -> anyhow::Error {
    anyhow!("script evaluation crashed: edit the script and try again")
}

fn wait(h: &Helper) -> Result<Vec<u8>> {
    let (from, mut since, mut busy) = (Instant::now(), Instant::now(), false);
    loop {
        match h.2.recv_timeout(Duration::from_millis(100)) {
            Ok(f) if !f.is_empty() => return Ok(f),
            Ok(_) => busy = true,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(crashed()),
        }
        if heft(h.0.id()) > HEAVY {
            bail!("script used too much memory: stopped at {} MB", HEAVY >> 20)
        }
        let over = since.elapsed() > LIMIT * 3;
        if over && !mem::take(&mut busy) || from.elapsed() > LIMIT * 30 {
            bail!("script took too long: stopped after {} seconds", from.elapsed().as_secs())
        }
        if over {
            since = Instant::now();
        }
    }
}

fn open(f: &[u8], measure: Measure) -> Result<Timeline> {
    let (n, rest) = f.split_first_chunk::<8>().ok_or_else(crashed)?;
    let (head, rest) = rest.split_at_checked(u64::from_le_bytes(*n) as usize).ok_or_else(crashed)?;
    let (got, fail): Head = serde_json::from_slice(head).map_err(|_| crashed())?;
    let fall = |e: anyhow::Error| fail.map_or(e, Into::into);
    match got {
        Err(chain) => {
            let e = chain.into_iter().rev().fold(None, |e: Option<anyhow::Error>, m| match e {
                Some(e) => Some(e.context(m)),
                None => Some(anyhow!(m)),
            });
            Err(fall(e.unwrap_or_else(crashed)))
        }
        Ok((n, lens, files)) => {
            let (out, rest) = rest.split_at_checked(n).ok_or_else(crashed)?;
            let mut floats = rest.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            let kept = lens.iter().map(|n| floats.by_ref().take(*n).collect()).collect();
            finish(std::str::from_utf8(out)?, kept, files, measure).map_err(fall)
        }
    }
}

fn uncaught(e: &anyhow::Error, saved: bool) -> Option<Error> {
    FAIL.take().filter(|f| saved && f.to_string() == e.to_string())
}

fn nested(text: &str, plain: bool) -> Result<()> {
    let (mut depth, mut mode, mut last, mut skip, mut holes) = (0, 0, 0, false, vec![]);
    let nest = |b| {
        let was = mem::replace(&mut last, b);
        let turn = matches!(b, b'(' | b'[' | b'{') as i32 - matches!(b, b')' | b']' | b'}') as i32;
        if plain {
            depth += turn;
        } else if mem::take(&mut skip) {
            last = 0;
        } else if b == b'\\' && !matches!(mode, b'/' | b'*') {
            skip = true;
        } else {
            match (mode, was, b) {
                (0, b'/', b'/' | b'*') => (mode, last) = (b, 0),
                (0, _, b'\'' | b'"' | b'`') => mode = b,
                (0, _, b'}') if holes.last() == Some(&depth) => {
                    holes.pop();
                    (depth, mode) = (depth - 1, b'`');
                }
                (0, ..) => depth += turn,
                (b'`', b'$', b'{') => {
                    depth += 1;
                    holes.push(depth);
                    mode = 0;
                }
                (b'*', b'*', b'/') => (mode, last) = (0, 0),
                (b'/' | b'\'' | b'"', _, b'\n') => mode = 0,
                (b'\'' | b'"' | b'`', ..) if b == mode => mode = 0,
                _ => {}
            }
        }
        depth > 100
    };
    match text.bytes().position(nest) {
        Some(i) => bail!(
            "line {}: brackets are nested more than 100 deep",
            text[..i].matches('\n').count() + 1
        ),
        None => Ok(()),
    }
}

fn remote(script: &Path, relock: bool, src: Option<&str>, measure: Measure) -> Result<Timeline> {
    let job = serde_json::to_vec(&(script, relock, src))?;
    let mut slot = HELPER.lock().unwrap_or_else(PoisonError::into_inner);
    let alive = |mut h: Helper| matches!(h.0.try_wait(), Ok(None)).then_some(h);
    let mut h = match slot.take().and_then(alive) {
        Some(h) => h,
        None => fresh()?,
    };
    let _ = send(&mut h.1, &job);
    let (got, mut sunk) = (wait(&h), false);
    *slot = match got {
        Ok(_) => Some(h),
        Err(_) => {
            let _ = h.0.kill();
            sunk = h.0.wait().is_ok_and(|s| s.signal() == Some(6));
            fresh().ok()
        }
    };
    drop(slot);
    if sunk && let Some(text) = src.map(String::from).or_else(|| load(script).ok()) {
        nested(&text, true)?;
    }
    open(&got?, measure)
}

fn ask(script: &Path, relock: bool, src: Option<&str>, measure: Measure) -> Result<Timeline> {
    if FAR.load(Relaxed) {
        return remote(script, relock, src, measure);
    }
    let (script, src, (back, got)) = (script.to_path_buf(), src.map(String::from), mpsc::channel());
    let job = move |cx: &mut Context| {
        let tl = half(cx, &script, relock, src.as_deref(), measure).and_then(|(out, kept, files)| finish(&out, kept, files, measure));
        drop(back.send(tl.map_err(|e| uncaught(&e, src.is_none()).map_or(e, Into::into))));
    };
    let mut vm = VM.lock().unwrap();
    BUSY.store(false, Relaxed);
    let _ = vm.get_or_insert_with(spawn).send(Box::new(job));
    loop {
        match got.recv_timeout(LIMIT * 3) {
            Ok(tl) => return tl,
            Err(mpsc::RecvTimeoutError::Timeout) if BUSY.swap(false, Relaxed) => {}
            Err(e) => {
                vm.take();
                match e {
                    mpsc::RecvTimeoutError::Timeout => bail!("script took too long: stopped after {} seconds", LIMIT.as_secs() * 3),
                    mpsc::RecvTimeoutError::Disconnected => return Err(crashed()),
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

type Half = (String, Vec<Vec<f32>>, Vec<(u32, PathBuf)>);

fn half(cx: &mut Context, script: &Path, relock: bool, src: Option<&str>, measure: Measure) -> Result<Half> {
    let script = script
        .canonicalize()
        .with_context(|| format!("missing script: {}", script.display()))?;
    let dir = script.parent().unwrap_or(&script).to_path_buf();
    let lock = dir.join("origami.lock");
    let old: Lock = Some(&lock)
        .filter(|l| fs::symlink_metadata(l).is_ok_and(|m| m.is_file()))
        .and_then(|l| load(l).ok())
        .and_then(|b| serde_json::from_str(&b).ok())
        .unwrap_or_default();
    let text = src.map_or_else(|| load(&script).map_err(|e| anyhow!("{}: {e}", script.display())), |s| Ok(s.into()))?;
    nested(&text, false)?;
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
                .chunks(missed.len().div_ceil(8))
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
        let _ = save(&lock, serde_json::to_string_pretty(&new)?);
    }
    if out.len() > 1 << 24 {
        bail!("the timeline is over 16 MB as text: use fewer clips or shorter texts");
    }
    Ok((out, KEPT.take(), READ.take()))
}

fn finish(out: &str, kept: Vec<Vec<f32>>, files: Vec<(u32, PathBuf)>, measure: Measure) -> Result<Timeline> {
    KEPT.set(kept);
    LINE.take();
    let tl = serde_json::from_str(out).map_err(|e| {
        let said = e.to_string();
        let said = said
            .strip_suffix(&format!(" at line {} column {}", e.line(), e.column()))
            .unwrap_or(&said);
        match LINE.take() {
            0 => anyhow!("{said}"),
            n => anyhow!("line {n}: {said}"),
        }
    });
    KEPT.take();
    let mut tl: Timeline = tl?;
    const DAY: f64 = 86400.;
    let fair = |v: u32, top: u32| (1..=top).contains(&v);
    if !(fair(tl.fps, 240) && fair(tl.size.0, 16384) && fair(tl.size.1, 16384) && tl.duration <= DAY) {
        bail!("fps {}, size {:?} or length {} is out of range", tl.fps, tl.size, tl.duration);
    }
    if let Some(c) = tl
        .clips
        .iter()
        .find(|c| !(c.start >= -DAY && c.end <= DAY && c.end >= c.start && c.speed > 0. && c.speed <= 1e3))
    {
        bail!("line {}: {} has a time or speed out of range", c.line, c.label);
    }
    if let Some(c) = tl.clips.iter().find(|c| c.kind == Kind::Sound && c.video) {
        bail!("line {}: {} is a sound, which has no picture to draw", c.line, c.label);
    }
    let mut held: Vec<(u32, &Clip)> = tl
        .clips
        .iter()
        .flat_map(|c| {
            let group = (c.kind == Kind::Group).then_some(c.id);
            group.into_iter().chain(c.matte.as_ref().map(|m| m.0)).map(move |id| (id, c))
        })
        .collect();
    held.sort_by_key(|h| h.0);
    let mut deep = BTreeMap::new();
    for (id, c) in held {
        let d = deep.get(&c.into).map_or(1, |d| d + 1);
        if id <= c.into || d > 16 || deep.insert(id, d).is_some() {
            bail!(
                "line {}: {} is inside itself or inside more than 16 groups and mattes",
                c.line,
                c.label
            );
        }
    }
    let opened: BTreeSet<&Path> = files.iter().map(|(_, f)| f.as_path()).collect();
    if let Some(e) = tl.clips.iter().find_map(|c| stray(c, |f| opened.contains(Path::new(f)))) {
        bail!("{e}");
    }
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
    for f in tl.clips.iter_mut().flat_map(|c| &mut c.fx) {
        let (v, lo, hi, said) = match f {
            Fx::Lens(v) => (
                v,
                -170.,
                170.,
                "lens goes past 170 either way and is held at 170, the most it takes",
            ),
            Fx::Blur(v) => (
                v,
                f32::MIN,
                16384.,
                "blur goes past 16384 and is held at 16384, the widest a picture can be",
            ),
            _ => continue,
        };
        if let Val::Frames(n, s) = v
            && s.iter().any(|x| !(lo..=hi).contains(x))
        {
            s.iter_mut().for_each(|x| *x = x.clamp(lo, hi));
            if tl.note.is_empty() {
                tl.note = if *n > 0 { format!("line {n}: {said}") } else { said.into() };
            }
        }
    }
    tl.files = files;
    Ok(tl)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(c: &Clip, _: (f64, f64)) -> (f64, f64) {
        (c.size * c.scale.max(), c.size * c.scale.max() / 2.)
    }

    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("origami-lang-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run(dir: &Path, src: &str) -> Result<Timeline> {
        fs::write(dir.join("edit.js"), src).unwrap();
        eval(&dir.join("edit.js"), false, size)
    }

    fn fails(dir: &Path, src: &str) -> String {
        run(dir, src).unwrap_err().to_string()
    }

    fn tl(src: &str) -> Timeline {
        static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        run(&scratch(&RUNS.fetch_add(1, Relaxed).to_string()), src).unwrap()
    }

    // Clips placed with cat and stack start and end at the right times, a time written as '0:02' means
    // two seconds, and each clip keeps its name and the script line it came from.
    #[test]
    fn times_and_nesting() {
        let t = tl("const a = text('a').for('0:02')\ncat(a, stack(color('#000').for(3), text('b').at('0:01')), text('c'))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.start, c.end)).collect();
        assert_eq!(span, [(0., 2.), (2., 5.), (3., 5.), (5., 10.)]);
        assert_eq!((t.duration, t.clips[0].label.as_str(), t.clips[3].line), (10., "a", 2));
    }

    // A colour setting on a group reaches every clip inside it with the last value winning, and a look
    // file that does not exist points at the line that asked for it.
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
        let e = run(&scratch("look"), "color('#f00')\n  .look('none')").unwrap_err();
        assert_eq!(e.downcast_ref::<Error>().map(Error::line), Some(2));
    }

    // A look or input table file that is empty, is not a colour table, has too few rows or reaches past
    // 0 to 1 stops the script with an error on its line, also when it replaces a table that was read.
    #[test]
    fn broken_tables_name_the_line() {
        let (dir, rows) = (scratch("tables"), "0 0 0\n".repeat(8));
        let whole = format!("LUT_3D_SIZE 2\n{rows}");
        let ranged = format!("LUT_3D_SIZE 2\nDOMAIN_MAX 2 2 2\n{rows}");
        let look = |text: &str, call: &str| {
            fs::write(dir.join("a.cube"), text).unwrap();
            let got = run(&dir, &format!("color('#f00')\n  .{call}('a.cube')"));
            got.map(|_| ()).map_err(|e| e.to_string())
        };
        assert_eq!(look(&whole, "look"), Ok(()));
        for (text, call) in [
            ("", "look"),
            ("hello this is not a lut", "inputLut"),
            ("LUT_3D_SIZE 2\n0 0 0\n", "look"),
            (ranged.as_str(), "inputLut"),
        ] {
            let want = format!("line 2: {call}(\"a.cube\"): not a .cube file with a 0 to 1 domain");
            assert_eq!(look(text, call), Err(want));
        }
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

    // Keys given straight to a clip, worked out for the whole clip at once, give exactly the values the
    // same keys give one frame at a time, whatever their order, times, percentages, eases or a second
    // opacity multiplied in.
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

    // Animated text survives being saved to the timeline file and loaded back unchanged, while a clip
    // with no animation stays as plain numbers.
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
            let e = fails(&scratch("motion"), src);
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

    // Following a property the other clip never set uses where that clip sits by default, and following
    // a clip never placed on the timeline still works.
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
            let e = fails(&scratch("follow"), src);
            assert!(e.contains(&format!("line {line}: {says}")), "{e}");
        }
    }

    // Rectangles, lines, and text with a background plate or alignment carry their settings into the
    // timeline, and timeline files saved before these existed still load.
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
            let e = fails(&scratch("settings"), src);
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

    // A stack or merged group given a length with for cuts the clips inside at its end, loop repeats a
    // whole stack or group, and trim on a stack or speed on a merged group stop with an error on its line.
    #[test]
    fn groups_cut_and_repeat() {
        let t = tl("cat(stack(color('#000').for(4), text('a').at(3)).merge().for(2), stack(rect(9, 9).for(1), text('b')).loop(2))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.start, c.end)).collect();
        assert_eq!(span, [(0., 2.), (0., 2.), (2., 3.), (2., 3.), (3., 4.), (3., 4.)]);
        assert_eq!((t.clips[0].kind, t.clips[4].kind, t.duration), (Kind::Group, Kind::Rect, 4.));
        let dir = scratch("groups");
        assert_eq!(
            fails(&dir, "stack(text('a'))\n  .trim(0, 1)"),
            "line 2: trim works on one clip, not a stack or cat: trim the clips inside"
        );
        assert_eq!(
            fails(&dir, "stack(text('a')).merge()\n  .speed(2)"),
            "line 2: speed goes before merge, like stack(a, b).speed(2).merge(), and speeds up each clip inside"
        );
    }

    // Every clip of a matte is silent, and the matte's own at and shift move it from the start of the
    // clip it cuts.
    #[test]
    fn mattes_are_silent_and_movable() {
        let t = tl("stack(text('a').for(3).at(1).volume(2).matte(stack(rect(9, 9).for(1).volume(3)).shift(0.5)))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.kind, c.start, c.end, c.gain)).collect();
        assert_eq!(span, [(Kind::Rect, 1.5, 2.5, 0.), (Kind::Text, 1., 4., 2.)]);
    }

    // An effect value outside what the effect can draw, whether written as a number, as keys or as a
    // function of time, is brought to the nearest value it takes with a note naming its line, the first
    // such value when there are several, while values inside are kept and NaN or Infinity stop the script.
    #[test]
    fn effects_stay_in_range() {
        let t = tl(
            "rect(9, 9).lens(180).levels(0.6, 0.4).levels(1, 1).noise(-0.5).fractal({ scale: 0 })
  .roughen({ border: -1, scale: 0, sharpness: -1 }).displace({ size: 0 }).lens(-1e9)",
        );
        let want = r#"[{"lens":170.0},{"levels":[0.6,0.61,false]},{"levels":[0.99,1.0,false]},{"noise":[0.0,0.0]},{"fractal":{"scale":1.0,"seed":0.0}},{"roughen":{"border":0.0,"scale":1.0,"sharp":0.0,"seed":0.0}},{"displace":{"amount":50.0,"size":1.0,"seed":0.0}},{"lens":-170.0}]"#;
        assert_eq!(serde_json::to_string(&t.clips[0].fx).unwrap(), want);
        for (src, note) in [
            ("lens(180)", "lens 180 is lowered to 170, the most it takes"),
            ("lens(-1e9)", "lens -1000000000 is raised to -170, the least it takes"),
            ("levels(0.6, 0.4)", "levels white 0.4 is raised to 0.61, the least it takes"),
            (
                "roughen({ sharpness: -1 })",
                "roughen sharpness -1 is raised to 0, the least it takes",
            ),
            ("displace({ size: 0 })", "displace size 0 is raised to 1, the least it takes"),
            (
                "lens(keys([0, 0], [1, -200]))",
                "lens goes past 170 either way and is held at 170, the most it takes",
            ),
            ("lens(t => 500)", "lens 500 is lowered to 170, the most it takes"),
            ("lens(() => -500)", "lens -500 is raised to -170, the least it takes"),
            (
                "lens(t => 500 + t)",
                "lens goes past 170 either way and is held at 170, the most it takes",
            ),
            ("blur(t => 1e9)", "blur 1000000000 is lowered to 16384, the widest a picture can be"),
            (
                "blur(t => 20000 + 1000 * t)",
                "blur goes past 16384 and is held at 16384, the widest a picture can be",
            ),
        ] {
            assert_eq!(tl(&format!("rect(9, 9).for(1)\n  .{src}")).note, format!("line 2: {note}"));
        }
        for (src, note) in [
            (
                "rect(9, 9).for(1).roughen({ border: -3, scale: 0 })\n  .blur(1e9).lens(keys([0, 0], [1, -200]))",
                "line 1: roughen border -3 is raised to 0, the least it takes",
            ),
            (
                "settings({ size: [641, 360] })\nrect(9, 9).for(1).blur(1e9).lens(200)",
                "line 1: size [641, 360] is rounded to [642, 360], video files need even numbers",
            ),
            (
                "rect(9, 9).for(1).blur(1e9)\n  .lens(200)",
                "line 1: blur 1000000000 is lowered to 16384, the widest a picture can be",
            ),
        ] {
            assert_eq!(tl(src).note, note);
        }
        let Fx::Lens(v) = &tl("rect(9, 9).for(1).lens(keys([0, 0], [1, -200]))").clips[0].fx[0] else {
            panic!()
        };
        assert_eq!((v.at(15), v.at(29)), (-100., -170.));
        let t = tl("rect(9, 9).for(1).lens(t => 500).blur(() => 1e9).blur(t => 20000 + 1000 * t).blur(t => 9 + t)");
        let [Fx::Lens(a), Fx::Blur(b), Fx::Blur(c), Fx::Blur(d)] = &t.clips[0].fx[..] else {
            panic!()
        };
        assert_eq!((a.max(), b.max(), c.max(), d.at(0)), (170., 16384., 16384., 9.));
        let t = tl("rect(9, 9).lens(30).levels(0.2, 0.8).roughen({ border: 0, sharpness: 0 }).noise(1)");
        assert_eq!((t.clips[0].fx[1].clone(), t.note.as_str()), (Fx::Levels(0.2, 0.8, false), ""));
        let dir = scratch("effects");
        assert_eq!(
            fails(&dir, "rect(9, 9)\n  .roughen({ scale: NaN })"),
            "line 2: roughen scale takes a number"
        );
        assert_eq!(
            fails(&dir, "rect(9, 9)\n  .lens(Infinity)"),
            "line 2: lens takes a number or a function of time"
        );
    }

    // Values that make no sense (a negative length, zero speed, a frame rate of 0, a loop without end,
    // a day-long timeline) and common slips (a number among clips, an unclosed string or bracket, a
    // misspelt name or method, a curve or wheel without its name, a font that is not installed, an
    // error thrown by a function of time) should be refused with a message that names the line, and a
    // name or loop that could be on several lines names no line.
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
            (
                "text('a')\n  .curve([[0, 0], [1, 1]])",
                2,
                "curve takes \"luma\", \"red\", \"green\" or \"blue\" first",
            ),
            (
                "text('a')\n  .wheel({ hue: 20 })",
                2,
                "wheel takes \"shadows\", \"midtones\" or \"highlights\" first",
            ),
            (
                "text('a')\ntext('x', { font: 'NoSuchFontName' })",
                2,
                "font(\"NoSuchFontName\") is not a font installed on this Mac, or a font file next to the script",
            ),
            (
                "text('x', { font: 'fonts/none.ttf' })",
                1,
                "font(\"fonts/none.ttf\") points to a file that does not exist",
            ),
            ("const a = text('a')\n\n\n\nvidoe('x')", 5, "ReferenceError: vidoe is not defined"),
            ("vidoe('x')\ntext('a')", 1, "ReferenceError: vidoe is not defined"),
            ("const f = () => {\n  return nope\n}\nf()", 2, "ReferenceError: nope is not defined"),
            ("const a = text('a')\n\nb\nconst b = a", 3, "b is defined below, on line 4"),
            (
                "const a = text('a')\n\nwhile (true) {}\ntext('b')",
                3,
                "script took too long: a loop, or a function",
            ),
            (
                "const a = text('a')\nconst b = = 3",
                2,
                "SyntaxError: unexpected token '=', primary expression",
            ),
            ("const a = text('😀').scael(2).rotaet(3)", 1, "scael is not a clip method"),
            ("text('a')\n  .scael(text('b').rotaet(3))", 2, "rotaet is not a clip method"),
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
            let e = fails(&scratch("slip"), src);
            assert!(e.starts_with(&format!("line {line}: {says}")), "{e}");
        }
        for (src, says) in [
            (
                "const a = text('a')\n// nope\nnope\ntext('b')",
                "ReferenceError: nope is not defined",
            ),
            (
                "const a = text('a')\nfor (const c of [a]) c\nwhile (true) {}\ntext('b')",
                "script took too long: a loop, or a function that calls itself, never ends",
            ),
        ] {
            assert_eq!(fails(&scratch("slip"), src), says);
        }
        let fine = format!("{}text('a'){}", "stack(".repeat(99), ")".repeat(99));
        assert_eq!(tl(&fine).clips.len(), 1);
    }

    // A script that reaches past the clip functions to name a web address or a file it never opened, to
    // read a device, a pipe, a huge file (the script itself included) or captions with too many lines,
    // to hand over values that would hang or crash the app or a timeline too big to read safely, or to
    // replace a function Origami calls after the script, is refused, and a long captions file with a
    // bad time deep inside is read quickly and names that time's line.
    #[test]
    fn reaching_past_the_functions_is_refused() {
        let dir = scratch("reach");
        status(std::process::Command::new("mkfifo").arg(dir.join("pipe.srt"))).unwrap();
        fs::File::create(dir.join("big.srt")).unwrap().set_len((1 << 24) + 1).unwrap();
        fs::write(dir.join("long.srt"), "\n".repeat((1 << 18) + 1)).unwrap();
        fs::write(dir.join("many.srt"), (" ".repeat(500) + "\n\n").repeat(8000) + "1\nxx --> yy\nodd").unwrap();
        let away = "Object.assign(v.$, { kind: 'video', src: 'http://127.0.0.1:9/a.mp4' })";
        for (src, says) in [
            (
                format!("const v = color('#000')\n{away}\nv"),
                "line 1: v uses http://127.0.0.1:9/a.mp4, a file the script did not open",
            ),
            (
                "const t = text('a')\nt.$.font = '/etc/hosts'\nt".into(),
                "line 1: t uses /etc/hosts, a file",
            ),
            (
                "const t = text('a')\nt.$.font = '/dev/zero'\ntext('b').pos(t.pos)".into(),
                "line 1: t uses /dev/zero, a file",
            ),
            (
                "const t = text('a')\nt.$.grade = { look: ['/dev/zero', 100] }\nt".into(),
                "line 1: t uses /dev/zero, a file",
            ),
            (
                "text('a')\ncaptions('/dev/zero')".into(),
                "line 2: captions(\"/dev/zero\") points to a file that does not exist",
            ),
            (
                "captions('pipe.srt')".into(),
                "line 1: captions(\"pipe.srt\") points to a file that does not exist",
            ),
            (
                "captions('big.srt')".into(),
                "line 1: captions(\"big.srt\"): the file is over 16 MB",
            ),
            (
                "text('a')\ncaptions('long.srt')".into(),
                "line 2: captions(\"long.srt\"): the file has over 262144 lines",
            ),
            (
                "captions('many.srt')".into(),
                "line 1: captions(\"many.srt\") line 16002: xx --> yy is not a time range",
            ),
            ("const t = text('a').for(1)\nt.$.opacity = [1, []]\nt".into(), "line 1: "),
            (
                "const t = text('a')\nt.$.label = 'a'.repeat(1 << 12).repeat(1 << 12)\nt".into(),
                "the timeline is over 16 MB as text",
            ),
            (
                "text('a').for(1).speed(1e-200).speed(1e-200)".into(),
                "line 1: a has a time or speed out of range",
            ),
            ("text('a').for(1).speed(1e12)".into(), "line 1: a has a time or speed out of range"),
            (
                "const t = text('a')\nt.$.dur = -998\nt".into(),
                "line 1: t has a time or speed out of range",
            ),
            (
                "const t = text('a')\nObject.assign(t.$, { kind: 'sound', src: '', w: 4294967295, h: 1 })\nt".into(),
                "line 1: t is a sound, which has no picture to draw",
            ),
            (
                "const j = JSON.stringify\nJSON.stringify = v => j({ ...v, size: [4e8, 4e8] })\ntext('a')".into(),
                "fps 30, size (400000000, 400000000) or length 5 is out of range",
            ),
            (
                "const __out = () => '{}'\ntext('a')".into(),
                "SyntaxError: cannot redefine non-configurable global property",
            ),
        ] {
            let e = fails(&dir, &src);
            assert!(e.starts_with(says), "{src}: {e}");
        }
        std::os::unix::fs::symlink("/dev/zero", dir.join("zero.js")).unwrap();
        assert_eq!(
            eval(&dir.join("zero.js"), false, size).unwrap_err().to_string(),
            "/dev/zero: not a plain file"
        );
    }

    // A number that is NaN, Infinity or not a number at all should be refused wherever a method, an
    // option or a key takes one, with a message that names the line and what takes the number.
    #[test]
    fn numbers_must_be_finite() {
        let dir = scratch("finite");
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

    // A name every object inherits, such as valueOf, toString or constructor, is not a clip method:
    // calling one fails like any unknown method, which the error names even with a second unknown call
    // on its line, instead of silently dropping or copying the clip.
    #[test]
    fn inherited_names_are_not_clip_methods() {
        let dir = scratch("inherited");
        let unknown =
            |name: &str| format!("line 2: {name} is not a clip method: check the spelling, or the script may need a newer Origami");
        assert_eq!(fails(&dir, "text('z')\ntext('a').for(2).nope(1)"), unknown("nope"));
        assert_eq!(fails(&dir, "text('z')\ntext('a').nope().nada()"), unknown("nope"));
        for name in [
            "valueOf",
            "toString",
            "toLocaleString",
            "constructor",
            "hasOwnProperty",
            "isPrototypeOf",
            "__proto__",
        ] {
            assert_eq!(fails(&dir, &format!("text('z')\ntext('a').{name}()")), unknown(name));
        }
    }

    // A clip used where text or a number belongs is refused with a plain message on its line.
    #[test]
    fn clips_are_not_text_or_numbers() {
        let dir = scratch("opaque");
        for src in [
            "const a = text('a')\ntext(`${a}`)",
            "const a = text('a')\ntext(a)",
            "const a = text('a')\ntext('b' + a)",
            "const a = text('a')\ntext('b').for(a)",
            "const a = stack(text('a'))\ntext('b').opacity(a * 2)",
        ] {
            assert_eq!(fails(&dir, src), "line 2: a clip cannot be used as text or a number", "{src}");
        }
    }

    // A name every object inherits is not a way to split text, an ease or a setting, and a setting
    // given as undefined keeps the value it had.
    #[test]
    fn inherited_names_are_not_choices() {
        let dir = scratch("choices");
        for (src, says) in [
            ("text('a b').each('__proto__', () => ({}))", "each is written each(\"char\""),
            ("text('a').each('constructor', () => ({}))", "each is written each(\"char\""),
            (
                "text('a').opacity(keys([0, 0], [1, 1, 'constructor']))",
                "\"constructor\" is not an ease (linear,",
            ),
            ("text('a').opacity(keys([0, 0], [1, 1, 'toString']))", "\"toString\" is not an ease"),
            (
                "text('a').opacity(keys([0, 0], [1, 1, '__proto__']))",
                "\"__proto__\" is not an ease",
            ),
        ] {
            let e = fails(&dir, &format!("text('z')\n{src}"));
            assert!(e.starts_with(&format!("line 2: {says}")), "{src}: {e}");
        }
        let t = tl(
            "settings({ fps: 24 })\nsettings({ fps: undefined, preview: undefined, proxy: undefined, ['__proto__']: { fps: 5 }, constructor: 1 })\ntext('a')",
        );
        assert_eq!((t.fps, t.preview.as_str(), t.proxy), (24, "auto", true));
    }

    // Origami's own functions called straight from a script with missing, odd or huge arguments, or too
    // many times, fail with a plain message on the script's line or not at all, never with a JSON
    // position, a JavaScript type error or line 0, and a followed clip holding a value out of range
    // names its own line.
    #[test]
    fn natives_fail_plainly() {
        let dir = scratch("natives");
        for (call, line, says) in [
            ("__measure('x')", 2, "__measure takes a clip written by Origami"),
            ("__measure()", 2, "__measure takes a clip written by Origami"),
            ("__measure({}, NaN)", 2, "__measure takes a clip written by Origami"),
            ("__probe()", 2, "__probe(\"undefined\") points to a file that does not exist"),
            ("__read(5)", 2, "__read(\"5\") points to a file that does not exist"),
            (
                "__lut({}, NaN, NaN)",
                2,
                "__lut(\"[object Object]\") points to a file that does not exist",
            ),
            ("__font('x', 'font', -1)", 2, "font(\"x\") is not a font installed on this Mac"),
            ("__read('x'.repeat(5e5))", 2, "__read(\"xxxxxxxx"),
            ("const v = video\nv('x')", 3, "video(\"x\") points to a file that does not exist"),
            (
                "const a = text('a').speed(1e200).speed(1e200)\ntext('b').pos(a.pos)",
                2,
                "a has a value out of range",
            ),
            ("const r = rect(5e9, 10)\ntext('b').pos(r.pos)", 2, "r has a value out of range"),
            (
                "const s = [0, 1]\nArray.from({ length: 65537 }, () => __keep(s))",
                3,
                "the script has over 65536 animated values",
            ),
        ] {
            let e = fails(&dir, &format!("text('z')\n{call}"));
            assert!(e.starts_with(&format!("line {line}: {says}")), "{call}: {e}");
        }
        for call in [
            "__plain()",
            "__plain(5)",
            "__ramp(null, 1, 1, 1)",
            "__ramp([], 1, 1, 1)",
            "__keep()",
            "__keep({ length: 1e9 })",
            "__dots('x')",
        ] {
            assert!(run(&dir, &format!("text('z')\n{call}\ntext('y')")).is_ok(), "{call}");
        }
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
        let dir = scratch("runaway");
        let spin = "const spin = () => { for (let i = 0; i < 1e6; i++); }\nfor (let i = 0; i < 1e6; i++) spin()";
        assert_eq!(fails(&dir, spin), "script took too long: stopped after 2 seconds");
        assert_eq!(tl("text('a').for(3)").duration, 3.);
        fs::write(dir.join("edit.js"), "const a = text('a')\ntext('b').pos(a.pos)").unwrap();
        let e = eval(&dir.join("edit.js"), false, boom).unwrap_err().to_string();
        assert_eq!(e, "script evaluation crashed: edit the script and try again");
        assert_eq!(tl("text('a').for(4)").duration, 4.);
    }

    // A helper that grows past the memory limit is stopped at once, one that dies or answers with a
    // damaged reply reports a crash instead of panicking, one that never answers is stopped after three
    // times the time limit, and no new helper starts from an Origami binary that changed since launch.
    #[test]
    fn stuck_helpers_are_stopped() {
        let run = |sh: &str| {
            let mut h = hire(Command::new("/bin/sh").args(["-c", sh])).unwrap();
            let got = wait(&h).and_then(|f| open(&f, size));
            let _ = h.0.kill();
            let _ = h.0.wait();
            got.unwrap_err().to_string()
        };
        let fat = "exec /usr/bin/perl -e '$x = \"a\" x 4e8; sleep 60'";
        assert_eq!(run(fat), "script used too much memory: stopped at 256 MB");
        assert_eq!(run("exit 3"), crashed().to_string());
        let lie = "printf '\\012\\0\\0\\0\\0\\0\\0\\0\\377\\0\\0\\0\\0\\0\\0\\0{}'; exec sleep 60";
        assert_eq!(run(lie), crashed().to_string());
        let from = Instant::now();
        assert_eq!(run("exec sleep 60"), "script took too long: stopped after 6 seconds");
        assert!(from.elapsed() < Duration::from_secs(8));
        BUILT.set(None).unwrap();
        assert_eq!(fresh().err().unwrap().to_string(), "Origami was updated: restart it to run scripts");
    }

    fn media(name: &str) -> PathBuf {
        let dir = scratch(name);
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

    // A first open reads every media file and gives the same timeline as the next open even in a folder
    // it cannot write to, keeping turned footage's turned size, stopping a trim at the file's end, and
    // refusing a file that changed since it was remembered.
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

    // A long generated script with more than ten thousand names defined at its top level evaluates like a
    // short one instead of stopping with a stack error.
    #[test]
    fn many_names_evaluate() {
        let src: String = (0..12_000).map(|i| format!("const c{i} = {i}\n")).collect();
        assert_eq!(tl(&(src + "text('x').for(c11999 / 1000)")).duration, 11.999);
    }

    // A first open reads each new media file once however many clips use it, so a montage cut from a few
    // new files opens about as fast as one clip of them.
    #[test]
    fn new_media_is_read_once() {
        let dir = media("once");
        let open = |n: usize| {
            let src = format!("cat(Array.from({{ length: {n} }}, (_, i) => video('v.mp4').trim(i % 10 / 10, 1)))");
            fs::write(dir.join("edit.js"), src).unwrap();
            let _ = fs::remove_file(dir.join("origami.lock"));
            let from = Instant::now();
            assert_eq!(eval(&dir.join("edit.js"), false, size).unwrap().clips.len(), n);
            from.elapsed()
        };
        let (one, many) = (open(1), open(200));
        assert!(many < one * 10, "{one:?} {many:?}");
    }

    // A playlist used as media is refused on its line at once, before any program reads it, even when a
    // shipped origami.lock already lists it.
    #[test]
    fn playlists_are_refused_at_once() {
        let dir = scratch("playlists");
        fs::write(dir.join("a.ts"), "").unwrap();
        fs::write(dir.join("live.m3u8"), "#EXTM3U\n#EXT-X-TARGETDURATION:100000\n#EXTINF:1,\na.ts\n").unwrap();
        fs::write(dir.join("nest.mp4"), "ffconcat version 1.0\nfile live.m3u8\n").unwrap();
        let (size, mtime) = stat(&dir.join("nest.mp4")).unwrap();
        let entry = serde_json::json!({ "size": size, "mtime": mtime, "sha": "", "dur": 5, "w": 64, "h": 36, "video": true, "audio": false, "native": false });
        fs::write(dir.join("origami.lock"), serde_json::json!({ "nest.mp4": entry }).to_string()).unwrap();
        for (src, says) in [
            ("video('live')", "line 1: video(\"live\")"),
            ("text('a')\nvideo('nest.mp4')", "line 2: video(\"nest.mp4\")"),
            ("sound('live.m3u8')", "line 1: sound(\"live.m3u8\")"),
        ] {
            assert_eq!(
                fails(&dir, src),
                format!("{says}: playlist files are not supported: use a video or sound file")
            );
        }
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

    // A picture, film or sound file that ffprobe lists but ffmpeg cannot decode is refused on its line with
    // its format and what to do about it, a file origami.lock already vouches for is not decoded again,
    // and one that changed since is.
    #[test]
    fn unreadable_media_names_its_format() {
        let dir = media("unread");
        let swap = |file: &str, from: &[u8], to: &[u8]| {
            let mut data = fs::read(dir.join(file)).unwrap();
            while let Some(i) = data.windows(from.len()).position(|w| w == from) {
                data[i..i + to.len()].copy_from_slice(to);
            }
            fs::write(dir.join(file), data).unwrap();
        };
        fs::write(
            dir.join("a.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"/>",
        )
        .unwrap();
        let copy = ["-v", "error", "-i", "v.mp4", "-c", "copy", "v.mkv"];
        assert!(status(ffmpeg().args(copy).current_dir(&dir)).unwrap().success());
        swap("v.mkv", b"V_MPEG4", b"V_ZZZZZ");
        swap("a.wav", b"fmt \x10\0\0\0\x01\0", b"fmt \x10\0\0\0\x99\x99");
        for (src, says) in [
            (
                "image('a.svg')",
                "line 1: image(\"a.svg\"): SVG pictures are not supported: export it as PNG first",
            ),
            (
                "text('a')\nvideo('v.mkv')",
                "line 2: video(\"v.mkv\"): this is unrecognised video, which Origami cannot read: convert it to H.264, HEVC or ProRes first",
            ),
            (
                "sound('a.wav')",
                "line 1: sound(\"a.wav\"): this is unrecognised sound, which Origami cannot read: convert it to WAV, MP3 or AAC first",
            ),
        ] {
            assert_eq!(fails(&dir, src), says);
        }
        assert_eq!(
            unread(Some("h264"), "video", "Invalid NAL unit size"),
            "this H264 video cannot be read: the file may be damaged"
        );
        let (png, src) = (dir.join("i.png"), "image('i.png').for(1)");
        run(&dir, src).unwrap();
        let (len, at) = (fs::metadata(&png).unwrap().len(), fs::metadata(&png).unwrap().modified().unwrap());
        let dated = |at| fs::File::options().write(true).open(&png).unwrap().set_modified(at).unwrap();
        fs::write(&png, vec![b'x'; len as usize]).unwrap();
        dated(at);
        run(&dir, src).unwrap();
        dated(at + Duration::from_secs(10));
        let e = eval(&dir.join("edit.js"), true, size).unwrap_err().to_string();
        assert_eq!(
            e,
            "line 1: image(\"i.png\"): this PNG picture cannot be read: the file may be damaged"
        );
    }

    // A colour that does not parse and a layer too large to draw should each stop the script with an
    // error on its line, while every colour form the engine draws is accepted.
    #[test]
    fn refuses_unusable_values() {
        let dir = scratch("refuse");
        let ok = "stack(color('#ffffff').for(1), color('#abc'), color('#abcd'), color('ABCDEF'), color('#aabbcc80'), text('a', { color: '#000000', background: '#0008' }), rect(9, 9, { color: '#fff' }), line(0, 0, 1, 1, { color: '#000' }), text('b').hslSecondary({ key: { color: '#f00' } }))";
        assert!(run(&dir, ok).is_ok());
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
            let e = fails(&dir, src);
            assert!(e.starts_with(want), "{src}: {e}");
        }
    }

    // A matte holds its whole clip, so a merged group used as a matte keeps its own scale and a matte
    // that has its own matte is drawn inside the first.
    #[test]
    fn mattes_hold_their_whole_clip() {
        let t = tl("rect(64, 36).matte(stack(rect(20, 20)).merge().scale(0.5))");
        let g = t.clips.iter().find(|c| c.kind == Kind::Group).unwrap();
        assert_eq!(t.clips.iter().find_map(|c| c.matte.clone()), Some((g.into, "alpha".into())));
        let t = tl("rect(64, 36).matte(rect(20, 20).matte(rect(9, 9)))");
        let mid = t.clips.iter().find(|c| c.w == 20).unwrap();
        assert_eq!(t.clips.iter().find(|c| c.w == 64).unwrap().matte.as_ref().unwrap().0, mid.into);
    }

    // A function called through another name, or on a whole group, still names its own line when
    // something is wrong, never line 0 or the line of the call before it, and so does a function of
    // time handed to one, whose mistake is only found after the script has ended.
    #[test]
    fn renamed_functions_name_their_own_line() {
        let dir = scratch("renamed");
        for (src, says) in [
            ("const r = rect\nr(1, 1, { radius: NaN })", "line 2: radius takes a number"),
            ("text('a')\nconst b = bezier\nb(NaN, 0, 1, 1)", "line 3: bezier is written"),
            ("text('a')\nconst s = stack\ns(1)", "line 3: stack takes clips"),
            ("text('a')\nconst d = delay\nd(t => t, NaN)", "line 3: delay takes a number"),
            ("const a = text('a')\nconst f = a.fade\nf(-1)", "line 3: fade takes seconds"),
            ("const a = text('a')\nconst s = a.speed\ns('x')", "line 3: speed takes a number"),
            ("stack(text('a'),\n  text('b'))\n  .fade(-1)", "line 3: fade takes seconds"),
            (
                "const a = text('a')\nconst o = a.opacity\no(t => 'x')",
                "line 3: a function of time must return a number",
            ),
            (
                "const a = text('a')\nconst o = a.opacity\nconst dim = f => o(\n  f)\n\ndim(t => NaN)",
                "line 3: a function of time must return a number",
            ),
            (
                "const p = path\n\np([[t => NaN, 0], [1, 0], [1, 1]])",
                "line 3: a function of time must return a number",
            ),
            (
                "const e = text('abc').each\n\n\ne('char', i => ({ opacity: t => NaN }))",
                "line 4: a function of time must return a number",
            ),
            (
                "const o = text('a').opacity\nconst b = [t => NaN].map(o)\nb[0]",
                "line 2: a function of time must return a number",
            ),
        ] {
            let e = fails(&dir, src);
            assert!(e.starts_with(says), "{src}: {e}");
        }
        let t = run(&dir, "const a = text('a').for(1)\nconst l = a.lens\nl(t => 500 + t)").unwrap();
        assert_eq!(
            t.note,
            "line 3: lens goes past 170 either way and is held at 170, the most it takes"
        );
        let t = run(&dir, "const a = text('a').for(1)\nconst l = a.lens\n\nl(t => 500)").unwrap();
        assert_eq!(t.note, "line 4: lens 500 is lowered to 170, the most it takes");
    }

    // A missing file the script caught and moved past is not blamed for a later mistake, while one it
    // did not catch is still reported as the missing file.
    #[test]
    fn caught_errors_are_forgotten() {
        let dir = scratch("caught");
        assert_eq!(
            fails(&dir, "try { video('nope') } catch {}\nfoo"),
            "line 2: ReferenceError: foo is not defined"
        );
        let e = run(&dir, "text('a')\nvideo('nope')").unwrap_err();
        assert_eq!(e.downcast_ref::<Error>().map(Error::line), Some(2));
    }

    // Brackets inside quoted text, backtick text and comments are not counted as nesting, while real
    // nesting after a stray or escaped quote mark or inside ${} is still refused, nesting hidden behind
    // a quote mark in a pattern is still found by the plain count used after a crash, and a script path
    // with no folder is an error, not a crash.
    #[test]
    fn quoted_brackets_are_not_nesting() {
        let (open, shut) = ("(".repeat(101), ")".repeat(101));
        let t = tl(&format!("stack(text('{open}').for(1),\n  text(\"it's {open}\"))"));
        assert_eq!(t.clips[0].src, open);
        let t = tl(&format!(
            "// don't {open}\n/* it's\n{open} */\nstack(text(`it's {open}`).for(1), text(`a${{'b'}}c{open}`))"
        ));
        assert_eq!((&t.clips[0].src, &t.clips[1].src), (&format!("it's {open}"), &format!("abc{open}")));
        let dir = scratch("quoted");
        for src in [
            format!("// don't\n{open}text('a'){shut}"),
            format!("text('\\'')\n{open}text('a'){shut}"),
            format!("text(`\\${{`)\n{open}text('a'){shut}"),
            format!("/* a */ /* b */\ntext(`${{{open}1{shut}}}`)"),
        ] {
            assert!(
                fails(&dir, &src).starts_with("line 2: brackets are nested more than 100 deep"),
                "{src}"
            );
        }
        let slip = format!("text('a')\nconst r = /'/, x = {open}1{shut}");
        assert!(nested(&slip, false).is_ok());
        assert_eq!(
            nested(&slip, true).unwrap_err().to_string(),
            "line 2: brackets are nested more than 100 deep"
        );
        let e = eval(Path::new("/"), false, size).unwrap_err().to_string();
        assert!(!e.contains("crashed"), "{e}");
    }

    // Groups and mattes nest up to 16 deep; one more, a group that holds itself or two groups with
    // the same number stop the script with an error on the line of the clip that holds them.
    #[test]
    fn nesting_is_bounded() {
        let dir = scratch("nest");
        let deep = |n: usize, wrap: &str| format!("let g = rect(9, 9)\nfor (let i = 0; i < {n}; i++)\n  g = {wrap}\ng");
        let (group, matte) = ("stack(g).merge()", "rect(9, 9).matte(g)");
        assert!(run(&dir, &deep(16, group)).is_ok() && run(&dir, &deep(16, matte)).is_ok());
        let made = "const g = { kind: 'group', src: '', in: 0, out: 1, speed: 1, gain: 1, fade: [0, 0], scale: 1, opacity: 1, rotate: 0, shown: 1, shift: 0, loop: 1, video: true, audio: false, label: 'g', line: 3 }\n";
        for (src, n, label) in [
            (deep(17, group), 3, "group"),
            (deep(17, matte), 3, "rect"),
            (format!("{made}\n;({{ $: {{ ...g, id: 0 }} }})"), 3, "g"),
            (
                format!("{made}\nstack({{ $: {{ ...g, id: 7 }} }}, {{ $: {{ ...g, id: 7 }} }})"),
                3,
                "g",
            ),
        ] {
            let want = format!("line {n}: {label} is inside itself or inside more than 16 groups and mattes");
            assert_eq!(fails(&dir, &src), want);
        }
    }

    // A blur wider than the widest picture is lowered to 16384 with a note naming its line, while a
    // smaller one is kept as written.
    #[test]
    fn lowers_huge_blurs() {
        let t = tl("text('a').blur(4)\n  .blur(1e9)");
        assert_eq!(t.clips[0].fx, [Fx::Blur(Val::Fixed(4.)), Fx::Blur(Val::Fixed(16384.))]);
        assert_eq!(t.note, "line 2: blur 1000000000 is lowered to 16384, the widest a picture can be");
    }

    // An odd frame size should be rounded to even numbers, with a note naming its line when the line
    // is known.
    #[test]
    fn rounds_odd_sizes() {
        let t = tl("settings({ size: [641, 360] })\ntext('a')");
        assert_eq!(t.size, (642, 360));
        assert_eq!(
            t.note,
            "line 1: size [641, 360] is rounded to [642, 360], video files need even numbers"
        );
        assert_eq!(
            tl("globalThis.settings({ size: [641, 360] })\ntext('a')").note,
            "size [641, 360] is rounded to [642, 360], video files need even numbers"
        );
    }

    // Every installed font the app lists is one a script can ask for by that name, and each name is
    // listed once.
    #[test]
    fn listed_fonts_resolve() {
        let all = fonts();
        assert!(all.contains(&"Menlo".to_string()) && all.iter().all(|n| face(Path::new("/nowhere"), n).is_some()));
        assert!(all.windows(2).all(|w| w[0] != w[1]));
    }
}
