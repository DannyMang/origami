use crate::{
    export::{alpha, place},
    grade, text,
};
use lru::LruCache;
use origami_lang::{Clip, Kind, Timeline};
use std::{
    collections::HashMap,
    io::Read,
    num::NonZeroUsize,
    process::{Child, Command, Stdio},
    sync::{Arc, LazyLock, Mutex},
};

type Key = (String, usize, usize);
type Px = Arc<Vec<u8>>;

static ACCEL: LazyLock<Mutex<HashMap<String, &[&str]>>> = LazyLock::new(Mutex::default);

fn accel(path: &str) -> &'static [&'static str] {
    let probe = || {
        let show = [
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ];
        let out = Command::new("ffprobe")
            .args(show)
            .arg(path)
            .output()
            .map(|o| o.stdout)
            .unwrap_or_default();
        match out.trim_ascii() {
            b"h264" => &[][..],
            b"hevc" => &["-hwaccel", "videotoolbox", "-threads", "1"],
            _ => &["-hwaccel", "videotoolbox"],
        }
    };
    ACCEL.lock().unwrap().entry(path.into()).or_insert_with(probe)
}

fn raw(args: &[&str], len: usize) -> Option<Vec<u8>> {
    let tail = ["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgra", "-"];
    let out = Command::new("ffmpeg").args(["-v", "error"]).args(args).args(tail).output().ok()?;
    (out.stdout.len() == len).then_some(out.stdout)
}

fn still(path: &str, t: f64, w: usize, h: usize) -> Option<Vec<u8>> {
    raw(&["-ss", &t.to_string(), "-i", path, "-vf", &format!("scale={w}:{h}")], w * h * 4)
}

pub fn thumb(path: &str, t: f64, w: usize, h: usize) -> Option<Vec<u8>> {
    let fast = ["-skip_frame", "nokey", "-ss", &t.to_string(), "-i", path];
    raw(&[accel(path), &fast, &["-vf", &format!("scale={w}:{h}")]].concat(), w * h * 4)
}

pub fn peaks(path: &str) -> Vec<f32> {
    let args = ["-v", "error", "-i", path, "-ac", "1", "-ar", "4000", "-f", "u8", "-"];
    let pcm = Command::new("ffmpeg").args(args).output().map(|o| o.stdout).unwrap_or_default();
    let peaks: Vec<f32> = pcm
        .chunks(200)
        .map(|c| c.iter().map(|&s| (s as f32 - 128.).abs()).fold(0., f32::max))
        .collect();
    let top = peaks.iter().fold(1., |m: f32, &p| m.max(p));
    peaks.into_iter().map(|p| p / top).collect()
}

struct Decoder {
    child: Child,
    next: i64,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Preview {
    decoders: Vec<(Key, Decoder)>,
    frames: LruCache<(Key, i64), Px>,
    stills: HashMap<String, Option<Px>>,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            decoders: vec![],
            frames: LruCache::new(NonZeroUsize::new(600).unwrap()),
            stills: HashMap::new(),
        }
    }
}

impl Preview {
    pub fn size(tl: &Timeline) -> (usize, usize) {
        let k = (960. / tl.size.0 as f64).min(1.);
        ((tl.size.0 as f64 * k) as usize, (tl.size.1 as f64 * k) as usize)
    }

    fn decoder(&mut self, key: &Key, k: i64, from: i64, fps: u32) -> Option<&mut Decoder> {
        let near = |(o, d): &(Key, Decoder)| o == key && d.next <= k && k <= d.next + 60;
        let found = match self.decoders.iter().position(near) {
            Some(i) => self.decoders.remove(i),
            None => {
                let args = ["-ss", &(from as f64 / fps as f64).to_string(), "-i", &key.0];
                let out = [
                    "-vf",
                    &format!("scale={}:{}", key.1, key.2),
                    "-r",
                    &fps.to_string(),
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "bgra",
                    "-",
                ];
                let child = Command::new("ffmpeg")
                    .args(["-v", "error"])
                    .args(accel(&key.0))
                    .args(args)
                    .args(out)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok()?;
                self.decoders.drain(..self.decoders.len().saturating_sub(5));
                (key.clone(), Decoder { child, next: from })
            }
        };
        self.decoders.push(found);
        self.decoders.last_mut().map(|d| &mut d.1)
    }

    fn video(&mut self, key: Key, k: i64, fps: u32) -> Option<Px> {
        if let Some(px) = self.frames.get(&(key.clone(), k)) {
            return Some(px.clone());
        }
        let back = self.frames.contains(&(key.clone(), k + 1));
        let mut read = vec![];
        let d = self.decoder(&key, k, if back { (k - 45).max(0) } else { k }, fps)?;
        while d.next <= k {
            let mut px = vec![0; key.1 * key.2 * 4];
            d.child.stdout.as_mut()?.read_exact(&mut px).ok()?;
            read.push((d.next, Arc::new(px)));
            d.next += 1;
        }
        for (n, px) in read {
            self.frames.put((key.clone(), n), px);
        }
        self.frames.get(&(key, k)).cloned()
    }

    fn layer(&mut self, c: &Clip, t: f64, fps: u32, size: (usize, usize), k: f64) -> Option<(Px, (usize, usize), (i64, i64))> {
        let (tw, th, x, y) = place(c, (size.0 as f64, size.1 as f64), k);
        let (tw, th) = (tw as usize, th as usize);
        let px = match c.kind {
            Kind::Video => {
                let i = ((c.src_in + (t - c.start) * c.speed) * fps as f64).round() as i64;
                self.video((c.src.clone(), tw, th), i, fps)?
            }
            Kind::Image => self
                .stills
                .entry(format!("{}|{tw}x{th}", c.src))
                .or_insert_with(|| still(&c.src, 0., tw, th).map(Arc::new))
                .clone()?,
            Kind::Text => self
                .stills
                .entry(format!("{}|{}|{th}", c.src, c.color))
                .or_insert_with(|| Some(Arc::new(text::raster(c, k).0)))
                .clone()?,
            _ => {
                let [r, g, b] = text::rgb(&c.src);
                Arc::new([b, g, r, 255].repeat(tw * th))
            }
        };
        let px = c.grade.as_ref().map_or(px.clone(), |g| grade::paint(g, px, (tw, th)));
        Some((px, (tw, th), (x as i64, y as i64)))
    }

    pub fn frame(&mut self, tl: &Timeline, t: f64) -> Vec<u8> {
        let (pw, ph) = Self::size(tl);
        let mut out = [0, 0, 0, 255].repeat(pw * ph);
        for c in tl.clips.iter().filter(|c| c.video && c.start <= t && t < c.end) {
            let Some((px, (sw, sh), (x, y))) = self.layer(c, t, tl.fps, (pw, ph), pw as f64 / tl.size.0 as f64) else {
                continue;
            };
            let (a, solid) = (alpha(c, t), c.kind == Kind::Video);
            let (x0, x1) = ((-x).max(0) as usize, (pw as i64 - x).clamp(0, sw as i64) as usize);
            for sy in (0..sh).filter(|sy| (0..ph as i64).contains(&(y + *sy as i64))) {
                let dst = &mut out[((y + sy as i64) as usize * pw + (x + x0 as i64) as usize) * 4..][..(x1.saturating_sub(x0)) * 4];
                let src = &px[(sy * sw + x0) * 4..][..dst.len()];
                if solid && a >= 1. {
                    dst.copy_from_slice(src);
                    continue;
                }
                for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    let sa = a * if solid { 1. } else { s[3] as f64 / 255. };
                    for i in 0..3 {
                        d[i] = (s[i] as f64 * sa + d[i] as f64 * (1. - sa)) as u8;
                    }
                }
            }
        }
        let k = pw as f64 / tl.size.0 as f64;
        for c in tl.clips.iter().filter(|c| c.kind == Kind::Video && t < c.start && c.start < t + 1.) {
            let (tw, th, ..) = place(c, (pw as f64, ph as f64), k);
            let (key, first) = ((c.src.clone(), tw as usize, th as usize), (c.src_in * tl.fps as f64).round() as i64);
            if !self.frames.contains(&(key.clone(), first)) {
                self.decoder(&key, first, first, tl.fps);
            }
        }
        out
    }
}
