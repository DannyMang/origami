use crate::{
    export::{alpha, place, plain},
    grade,
    text::{self, Glyph},
};
use lru::LruCache;
use origami_lang::{Clip, Kind, Part, Timeline, Val};
use std::{
    collections::HashMap,
    io::Read,
    num::NonZeroUsize,
    process::{Child, Command, Stdio},
    sync::{Arc, LazyLock, Mutex},
    thread,
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

fn spin((x, y): (f64, f64), k: f64, deg: f64) -> (f64, f64) {
    let (s, c) = deg.to_radians().sin_cos();
    (k * (x * c - y * s), k * (x * s + y * c))
}

fn blit(out: &mut [u8], (pw, ph): (usize, usize), src: &[u8], (sw, sh): (usize, usize), k: f64, deg: f64, (cx, cy): (f64, f64), a: f64) {
    if !(k > 0. && a > 0.) {
        return;
    }
    let ((sin, cos), (hw, hh)) = (deg.to_radians().sin_cos(), (sw as f64 / 2., sh as f64 / 2.));
    let (rx, ry) = (
        k * (hw * cos.abs() + hh * sin.abs()) + 1.,
        k * (hw * sin.abs() + hh * cos.abs()) + 1.,
    );
    let span = |c: f64, r: f64, n: usize| ((c - r).clamp(0., n as f64) as usize, (c + r).ceil().clamp(0., n as f64) as usize);
    let ((x0, x1), (y0, y1)) = (span(cx, rx, pw), span(cy, ry, ph));
    let band = if (x1 - x0) * (y1 - y0) < 1 << 16 {
        ph
    } else {
        (y1 - y0).div_ceil(thread::available_parallelism().map_or(1, usize::from))
    };
    let paint = |(n, rows): (usize, &mut [u8])| {
        for (y, row) in (y0 + n * band..).zip(rows.chunks_exact_mut(pw * 4)) {
            for (x, d) in (x0..x1).zip(row[x0 * 4..].chunks_exact_mut(4)) {
                let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                let (u, v) = ((dx * cos + dy * sin) / k + hw - 0.5, (dy * cos - dx * sin) / k + hh - 0.5);
                let ((ux, vy), mut sum) = ((u.floor(), v.floor()), [0.; 4]);
                let (fx, fy) = (u - ux, v - vy);
                for (ox, oy, w) in [
                    (0, 0, (1. - fx) * (1. - fy)),
                    (1, 0, fx * (1. - fy)),
                    (0, 1, (1. - fx) * fy),
                    (1, 1, fx * fy),
                ] {
                    let (sx, sy) = (ux as i64 + ox, vy as i64 + oy);
                    if sx < 0 || sy < 0 || sx >= sw as i64 || sy >= sh as i64 {
                        continue;
                    }
                    let s = &src[(sy as usize * sw + sx as usize) * 4..][..4];
                    let w = w * s[3] as f64 / 255.;
                    for i in 0..3 {
                        sum[i] += s[i] as f64 * w;
                    }
                    sum[3] += w;
                }
                for i in 0..3 {
                    d[i] = (sum[i] * a + d[i] as f64 * (1. - sum[3] * a)) as u8;
                }
            }
        }
    };
    let mut bands = out[y0 * pw * 4..y1 * pw * 4].chunks_mut(band * pw * 4).enumerate();
    let first = bands.next();
    thread::scope(|s| {
        bands.for_each(|b| drop(s.spawn(|| paint(b))));
        first.map(paint)
    });
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
    texts: HashMap<String, Arc<(Vec<Glyph>, usize, usize)>>,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            decoders: vec![],
            frames: LruCache::new(NonZeroUsize::new(600).unwrap()),
            stills: HashMap::new(),
            texts: HashMap::new(),
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
            while self.frames.len() * px.len() > 1 << 30 {
                self.frames.pop_lru();
            }
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

    fn moved(&mut self, out: &mut [u8], c: &Clip, t: f64, fps: u32, (pw, ph): (usize, usize), k: f64, i: usize, a: f64) {
        let ((w, h), q, turn) = ((pw as f64, ph as f64), c.scale.at(i) / c.scale.max(), c.rotate.at(i));
        let at = |p: &Option<Val>, w: f64, tw: usize| p.as_ref().map_or(w / 2., |x| x.at(i) * w + tw as f64 * q / 2.);
        let mid = |tw, th| (at(&c.pos.0, w, tw), at(&c.pos.1, h, th));
        if c.parts.is_empty() && c.shown == Val::Fixed(1.) {
            if let Some((px, (sw, sh), _)) = self.layer(c, t, fps, (pw, ph), k) {
                blit(out, (pw, ph), &px, (sw, sh), q, turn, mid(sw, sh), a);
            }
            return;
        }
        let key = format!("{}|{}|{}|{:?}", c.src, c.color, c.size * c.scale.max() * k, c.grade);
        let tint = |mut g: Glyph| {
            g.px = c.grade.as_ref().map_or(g.px.clone(), |x| grade::paint(x, g.px, (g.w, g.h)));
            g
        };
        let laid = || {
            let (glyphs, tw, th) = text::glyphs(c, k);
            Arc::new((glyphs.into_iter().map(tint).collect(), tw, th))
        };
        let text = self.texts.entry(key).or_insert_with(laid).clone();
        let (glyphs, tw, th) = &*text;
        let (mid, n) = (mid(*tw, *th), (c.shown.at(i) * c.src.chars().count() as f64).round() as usize);
        let own = |p: &Part| {
            let of = glyphs.iter().filter(|g| (p.from..p.to).contains(&g.i) && g.w > 0);
            let b = of.fold((usize::MAX, usize::MAX, 0, 0), |b, g| {
                (b.0.min(g.x), b.1.min(g.y), b.2.max(g.x + g.w), b.3.max(g.y + g.h))
            });
            ((b.0 + b.2) as f64 / 2., (b.1 + b.3) as f64 / 2.)
        };
        for g in glyphs.iter().filter(|g| g.i < n && g.w > 0) {
            let (ps, pr, pa, (dx, dy), pc) = match c.parts.iter().find(|p| (p.from..p.to).contains(&g.i)) {
                Some(p) => (
                    p.scale.at(i),
                    p.rotate.at(i),
                    p.opacity.at(i),
                    (p.x.at(i) * w, p.y.at(i) * h),
                    own(p),
                ),
                None => (1., 0., 1., (0., 0.), (0., 0.)),
            };
            let v = spin((g.x as f64 + g.w as f64 / 2. - pc.0, g.y as f64 + g.h as f64 / 2. - pc.1), ps, pr);
            let u = spin((pc.0 + v.0 - *tw as f64 / 2., pc.1 + v.1 - *th as f64 / 2.), q, turn);
            blit(
                out,
                (pw, ph),
                &g.px,
                (g.w, g.h),
                q * ps,
                turn + pr,
                (mid.0 + u.0 + dx, mid.1 + u.1 + dy),
                (a * pa).min(1.),
            );
        }
    }

    pub fn frame(&mut self, tl: &Timeline, t: f64, (pw, ph): (usize, usize)) -> Vec<u8> {
        let k = pw as f64 / tl.size.0 as f64;
        let mut out = [0, 0, 0, 255].repeat(pw * ph);
        for c in tl.clips.iter().filter(|c| c.video && c.start <= t && t < c.end) {
            let i = ((t - c.start) * tl.fps as f64).round() as usize;
            let (a, solid) = ((alpha(c, t) * c.opacity.at(i)).clamp(0., 1.), c.kind == Kind::Video);
            if !plain(c) {
                self.moved(&mut out, c, t, tl.fps, (pw, ph), k, i, a);
                continue;
            }
            let Some((px, (sw, sh), (x, y))) = self.layer(c, t, tl.fps, (pw, ph), k) else {
                continue;
            };
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
