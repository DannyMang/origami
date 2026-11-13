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
    pub scale: f64,
    pub pos: (Option<f64>, Option<f64>),
    pub gain: f64,
    pub fade: (f64, f64),
    pub label: String,
    pub line: u32,
    #[serde(default)]
    pub total: f64,
    pub video: bool,
    pub audio: bool,
    pub size: f64,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<Grade>,
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

thread_local! {
    static ST: RefCell<(PathBuf, Lock, Lock, bool)> = RefCell::default();
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
        ST.with_borrow_mut(|(_, old, new, relock)| media(path, old, new, *relock, rel))
    })
}

fn read(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, find, |_, path| Ok(fs::read_to_string(path)?))
}

fn lut(_: &JsValue, args: &[JsValue], cx: &mut Context) -> JsResult<JsValue> {
    host(args, cx, cube, |_, path| Ok(path.to_string_lossy().into()))
}

pub fn eval(script: &Path, relock: bool) -> Result<Timeline> {
    FAIL.take();
    build(script, relock).map_err(|e| FAIL.take().map_or(e, Into::into))
}

fn build(script: &Path, relock: bool) -> Result<Timeline> {
    let script = script
        .canonicalize()
        .with_context(|| format!("missing script: {}", script.display()))?;
    let dir = script.parent().unwrap().to_path_buf();
    let lock = dir.join("origami.lock");
    let old: Lock = fs::read(&lock)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    ST.set((dir, old.clone(), Lock::new(), relock));
    let js = |e: JsError| anyhow!("{e}");
    let cx = &mut Context::default();
    let run = |cx: &mut Context, code: &str| cx.eval(Source::from_bytes(code)).map_err(js);
    let set = |cx: &mut Context, k: JsString, v: JsValue| cx.register_global_property(k, v, Attribute::all()).map_err(js);
    for (name, f) in [
        (
            js_string!("__probe"),
            probe as fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>,
        ),
        (js_string!("__read"), read),
        (js_string!("__lut"), lut),
    ] {
        cx.register_global_builtin_callable(name, 1, NativeFunction::from_fn_ptr(f))
            .map_err(js)?;
    }
    run(cx, include_str!("prelude.js"))?;
    set(cx, js_string!("__src"), JsString::from(fs::read_to_string(&script)?).into())?;
    let code = run(cx, "__prep(__src)")?.to_string(cx).map_err(js)?.to_std_string_escaped();
    let root = run(cx, &code)?;
    set(cx, js_string!("__root"), root)?;
    let out = run(cx, "__out(__root)")?.to_string(cx).map_err(js)?.to_std_string_escaped();
    let new = ST.take().2;
    if new != old {
        fs::write(lock, serde_json::to_string_pretty(&new)?)?;
    }
    Ok(serde_json::from_str(&out)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(src: &str) -> Timeline {
        let dir = std::env::temp_dir().join(format!("origami-{}", src.len()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edit.js"), src).unwrap();
        eval(&dir.join("edit.js"), true).unwrap()
    }

    #[test]
    fn times_and_nesting() {
        let t = tl("const a = text('a').for('0:02')\ncat(a, stack(color('#000').for(3), text('b').at('0:01')), text('c'))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.start, c.end)).collect();
        assert_eq!(span, [(0., 2.), (2., 5.), (3., 5.), (5., 10.)]);
        assert_eq!((t.duration, t.clips[0].label.as_str(), t.clips[3].line), (10., "a", 2));
    }

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
        let e = eval(&dir.join("edit.js"), true).unwrap_err();
        assert_eq!(e.downcast_ref::<Error>().map(Error::line), Some(2));
    }

    #[test]
    fn shift_loop_speed() {
        let t = tl("stack(text('x').for(2).loop(2).shift(1), color('red'))");
        let span: Vec<_> = t.clips.iter().map(|c| (c.start, c.end)).collect();
        assert_eq!(span, [(1., 3.), (3., 5.), (0., 5.)]);
    }
}
