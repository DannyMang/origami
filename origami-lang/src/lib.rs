mod error;
mod grade;

use anyhow::{Context as _, Result, anyhow, bail};
use boa_engine::{Context, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source, js_string, property::Attribute};
pub use error::Error;
pub use grade::{Curves, Grade, Secondary, Vignette, Wheels};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, mpsc},
    thread,
    time::UNIX_EPOCH,
};

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
    Frames(u32, Vec<f32>),
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
    pub clips: Vec<Clip>,
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

fn face(dir: &Path, name: &str) -> Option<PathBuf> {
    if name.contains('/') || Path::new(name).extension().is_some() {
        return find(dir, name);
    }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Fonts");
    let system = ["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"].map(PathBuf::from);
    let places = [dir.to_path_buf(), dir.join("fonts"), home].into_iter().chain(system);
    let files = places.flat_map(|p| ["ttf", "otf", "ttc"].map(|e| p.join(format!("{name}.{e}"))));
    files.into_iter().find(|p| p.is_file())
}

fn media(path: &Path, old: &Lock, new: &mut Lock, relock: bool, rel: &str) -> Result<String> {
    let meta = fs::metadata(path)?;
    let (size, mtime) = (meta.len(), meta.modified()?.duration_since(UNIX_EPOCH)?.as_secs());
    let m = match old.get(rel) {
        Some(m) if m.size == size && m.mtime == mtime => m.clone(),
        prev => {
            let (mut hash, mut file) = (Sha256::new(), fs::File::open(path)?);
            hash.update(size.to_le_bytes());
            for at in [0, size.saturating_sub(1 << 20)] {
                file.seek(SeekFrom::Start(at))?;
                io::copy(&mut (&file).take(1 << 20), &mut hash)?;
            }
            let sha = format!("{:x}", hash.finalize());
            if prev.is_some_and(|p| p.sha != sha) && !relock {
                bail!("{rel} changed since origami.lock (use --relock)");
            }
            let out = Command::new("ffprobe")
                .args(["-v", "error", "-of", "json", "-show_entries"])
                .arg("format=duration:stream=codec_type,width,height")
                .arg(path)
                .output()
                .context("ffprobe not found")?;
            let v: serde_json::Value = serde_json::from_slice(&out.stdout)?;
            let streams = v["streams"].as_array().cloned().unwrap_or_default();
            let has = |t| streams.iter().any(|s| s["codec_type"] == t);
            let dim = |k| streams.iter().find_map(|s| s[k].as_u64()).unwrap_or(0) as u32;
            Media {
                size,
                mtime,
                sha,
                dur: v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.),
                w: dim("width"),
                h: dim("height"),
                video: has("video"),
                audio: has("audio"),
            }
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
        Some(file) => f(&path, &file),
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

fn tag(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    let mut text = |i: usize| -> JsResult<String> { Ok(args.get(i).cloned().unwrap_or_default().to_string(cx)?.to_std_string_escaped()) };
    let (src, made, bare, called) = (text(0)?, text(1)?, text(2)?, text(3)?);
    let (mut out, mut line, mut from) = (String::new(), 1, 0);
    for (i, c) in src.match_indices(['(', '\n']) {
        if c == "\n" {
            line += 1;
            continue;
        }
        let at = src[..i].trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_').len();
        let list = if src[..at].ends_with('.') { &called } else { &bare };
        if [&made, list].iter().any(|l| l.split(' ').any(|n| n == &src[at..i])) {
            out += &src[from..i];
            out += &format!("[{line}]");
            from = i;
        }
    }
    Ok(JsString::from(out + &src[from..]).into())
}

type Job = Box<dyn FnOnce(&mut Context) + Send>;

static VM: LazyLock<mpsc::Sender<Job>> = LazyLock::new(|| {
    let (jobs, queue) = mpsc::channel::<Job>();
    thread::spawn(move || {
        queue.iter().fold(boot(), |mut cx, job| {
            job(&mut cx);
            boot()
        });
    });
    jobs
});

fn js(e: JsError) -> anyhow::Error {
    anyhow!("{e}")
}

fn boot() -> Context {
    let mut cx = Context::default();
    for (name, f) in [
        (
            js_string!("__probe"),
            probe as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        (js_string!("__read"), read),
        (js_string!("__lut"), lut),
        (js_string!("__font"), font),
        (js_string!("__measure"), extent),
        (js_string!("__tag"), tag),
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
        FAIL.take();
        let tl = build(cx, &script, relock, src.as_deref(), measure);
        drop(back.send(tl.map_err(|e| FAIL.take().filter(|_| src.is_none()).map_or(e, Into::into))));
    };
    let _ = VM.send(Box::new(job));
    got.recv()?
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
    ST.set((dir, old.clone(), Lock::new(), relock, Some(measure)));
    let run = |cx: &mut Context, code: &str| cx.eval(Source::from_bytes(code)).map_err(js);
    let set = |cx: &mut Context, k: JsString, v: JsValue| cx.register_global_property(k, v, Attribute::all()).map_err(js);
    set(
        cx,
        js_string!("__src"),
        JsString::from(src.map_or_else(|| fs::read_to_string(&script), |s| Ok(s.into()))?).into(),
    )?;
    let code = run(cx, "__prep(__src)")?.to_string(cx).map_err(js)?.to_std_string_escaped();
    let root = match run(cx, &code) {
        Ok(root) => root,
        Err(e) => {
            set(cx, js_string!("__err"), JsString::from(e.to_string()).into())?;
            bail!("{}", run(cx, "__late(__err)")?.to_string(cx).map_err(js)?.to_std_string_escaped());
        }
    };
    set(cx, js_string!("__root"), root)?;
    let out = run(cx, "__out(__root)")?.to_string(cx).map_err(js)?.to_std_string_escaped();
    let new = ST.take().2;
    if new != old && src.is_none() {
        fs::write(lock, serde_json::to_string_pretty(&new)?)?;
    }
    Ok(serde_json::from_str(&out)?)
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
            "const a = color('red')\nstack(a.exposure(1), text('b'))\n  .saturation(50)\n  .curve('red', [[1, 1], [0, 0]])\n  .curve('luma', [[0, 0]])\n  .saturation(80)",
        );
        let g = t.clips[1].grade.clone().unwrap();
        assert_eq!(
            (g.saturation, g.exposure, &g.curve.red, g.curve.luma.len()),
            (Some(80.), 0., &vec![(0., 0.), (1., 1.)], 1)
        );
        assert_eq!((t.clips[0].grade.as_ref().unwrap().exposure, t.clips[1].line), (1., 2));
        assert_eq!(tl("color('red').sharpen(5)").clips[0].grade, None);
        let dir = std::env::temp_dir().join("origami-look");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edit.js"), "color('red')\n  .look('none')").unwrap();
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
        let plain = serde_json::to_value(tl("color('red').scale(0.5).pos(0.1)")).unwrap();
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
            ("color('red')\n  .shown(1)", 2),
            ("text('a')\n\n  .opacity(t => 'x')", 3),
            ("color('red')\n\n\n  .for(1)\nrect(10)", 5),
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
stack(text('b').for(2).pos(a.pos).rotate(a.rotate).opacity(a.opacity).shown(a.scale), color('red').pos(l.pos.x, l.pos.y), c, d)");
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
            (Kind::Line, [0.1, 0.5, 0.9, 0.5], 4., "white")
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

    // A looped clip should repeat back to back, and shift should move all of its repeats later by the
    // same amount.
    #[test]
    fn shift_loop_speed() {
        let t = tl("stack(text('x').for(2).loop(2).shift(1), color('red'))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.start, c.end)).collect();
        assert_eq!(span, [(1., 3.), (3., 5.), (0., 5.)]);
    }
}
