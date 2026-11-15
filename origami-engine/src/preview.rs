use crate::{
    export::{alpha, place, plain, seat},
    grade, shape,
    text::{self, Glyph},
};
use lru::LruCache;
use origami_lang::{Clip, Kind, Part, Timeline, Val, ffmpeg, ffprobe};
use std::{
    collections::HashMap,
    io::Read,
    num::NonZeroUsize,
    path::Path,
    process::{Child, Stdio},
    sync::{Arc, LazyLock, Mutex},
    thread,
};

type Key = (String, usize, usize);
type Px = Arc<Vec<u8>>;

type Info = (&'static [&'static str], f64, f64);

static KNOWN: LazyLock<Mutex<HashMap<String, Info>>> = LazyLock::new(Mutex::default);

fn probe(path: &str) -> Info {
    let ask = || {
        let show = [
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,avg_frame_rate",
            "-of",
            "csv=p=0",
        ];
        let out = ffprobe().args(show).arg(path).output().map(|o| o.stdout).unwrap_or_default();
        let text = String::from_utf8_lossy(&out);
        let (codec, rate) = text.trim().split_once(',').unwrap_or_default();
        let (n, d) = rate.split_once('/').unwrap_or_default();
        let lead: &[&str] = match codec {
            "h264" => &[],
            "hevc" => &["-hwaccel", "videotoolbox", "-threads", "1"],
            _ => &["-hwaccel", "videotoolbox"],
        };
        (lead, n.parse().unwrap_or(0.), d.parse().unwrap_or(0.))
    };
    *KNOWN.lock().unwrap().entry(path.into()).or_insert_with(ask)
}

fn raw(args: &[&str], len: usize) -> Option<Vec<u8>> {
    let tail = ["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgra", "-"];
    let out = ffmpeg().args(["-v", "error"]).args(args).args(tail).output().ok()?;
    (out.stdout.len() == len).then_some(out.stdout)
}

fn still(path: &str, t: f64, w: usize, h: usize) -> Option<Vec<u8>> {
    raw(&["-ss", &t.to_string(), "-i", path, "-vf", &format!("scale={w}:{h}")], w * h * 4)
}

pub fn thumb(path: &str, t: f64, w: usize, h: usize) -> Option<Vec<u8>> {
    let fast = ["-skip_frame", "nokey", "-ss", &t.to_string(), "-i", path];
    raw(&[probe(path).0, &fast, &["-vf", &format!("scale={w}:{h}")]].concat(), w * h * 4)
}

pub fn peaks(path: &str) -> Vec<f32> {
    let args = ["-v", "error", "-i", path, "-ac", "1", "-ar", "4000", "-f", "u8", "-"];
    let pcm = ffmpeg().args(args).output().map(|o| o.stdout).unwrap_or_default();
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

fn nudge(px: &[u8], w: usize, (fx, fy): (f64, f64)) -> Vec<u8> {
    let ((a, b), mut out, n) = (((fx * 256.) as u32, (fy * 256.) as u32), px.to_vec(), w * 4);
    let band = px
        .len()
        .div_ceil(n)
        .div_ceil(thread::available_parallelism().map_or(1, usize::from))
        * n;
    thread::scope(|s| {
        for (j, rows) in out.chunks_mut(band).enumerate() {
            s.spawn(move || {
                for (y, row) in rows.chunks_exact_mut(n).enumerate() {
                    let at = j * band + y * n;
                    let (top, low) = (&px[at..at + n], px.get(at + n..at + 2 * n).unwrap_or(&px[at..at + n]));
                    for i in 0..n - 4 {
                        let l = top[i] as u32 * (256 - a) + top[i + 4] as u32 * a;
                        let r = low[i] as u32 * (256 - a) + low[i + 4] as u32 * a;
                        row[i] = ((l * (256 - b) + r * b) >> 16) as u8;
                    }
                }
            });
        }
    });
    out
}

fn index(c: &Clip, t: f64, fps: f64, rate: f64) -> i64 {
    let (step, at) = (fps / rate / c.speed, (t - c.start) * fps);
    let last = ((c.end - c.start) * fps / step - 1e-6).ceil() - 1.;
    let i = ((at - 0.5) / step - 1e-6).ceil().max(0.);
    let i = if i * step < at + 0.5 - 1e-6 { i } else { i - 1. };
    ((c.src_in * rate - 1e-6).ceil() + i.min(last)) as i64
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
    held: usize,
    stills: HashMap<String, Option<Px>>,
    texts: HashMap<String, Arc<(Vec<Glyph>, usize, usize)>>,
    pub room: usize,
    pub fast: bool,
    pub rough: bool,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            decoders: vec![],
            frames: LruCache::new(NonZeroUsize::new(600).unwrap()),
            held: 0,
            stills: HashMap::new(),
            texts: HashMap::new(),
            room: 1 << 30,
            fast: false,
            rough: false,
        }
    }
}

impl Preview {
    pub fn size(tl: &Timeline, wide: f64, still: bool) -> (usize, usize) {
        let (w, h) = (tl.size.0 as f64, tl.size.1 as f64);
        let n = match tl.preview.as_str() {
            "full" => 8.,
            "half" => 4.,
            "quarter" => 2.,
            _ => (8. * wide.min(if still { w } else { 1920. }) / w).ceil().clamp(1., 8.),
        };
        ((w * n / 8.) as usize, (h * n / 8.) as usize)
    }

    fn cell(c: &Clip, t: f64, fps: u32, (tw, th): (usize, usize), fast: bool) -> (Key, i64, (f64, f64)) {
        let (lrf, fps) = (Path::new(&c.src).with_extension("LRF"), fps as f64);
        let src = if fast && lrf.exists() {
            lrf.to_string_lossy().into()
        } else {
            c.src.clone()
        };
        let (n, d) = match probe(&src) {
            (_, n, d) if n > 0. && d > 0. => (n, d),
            _ => (fps, 1.),
        };
        ((src, tw, th), index(c, t, fps, n / d), (n, d))
    }

    fn near(&self, key: &Key, k: i64) -> Option<usize> {
        self.decoders.iter().position(|(o, d)| o == key && d.next <= k && k <= d.next + 30)
    }

    pub fn ready(&self, tl: &Timeline, t: f64, (pw, ph): (usize, usize)) -> bool {
        let (k, shown) = (pw as f64 / tl.size.0 as f64, |c: &&Clip| {
            c.kind == Kind::Video && c.video && c.start <= t && t < c.end
        });
        tl.clips.iter().filter(shown).all(|c| {
            let (tw, th, ..) = place(c, (pw as f64, ph as f64), k);
            let (key, i, _) = Self::cell(c, t, tl.fps, (tw as usize, th as usize), false);
            self.frames.contains(&(key.clone(), i)) || self.near(&key, i).is_some()
        })
    }

    fn decoder(&mut self, key: &Key, k: i64, from: i64, (n, d): (f64, f64)) -> Option<()> {
        let found = match self.near(key, k) {
            Some(i) => self.decoders.remove(i),
            None => {
                let seek = (from - 1).max(0);
                let args = ["-ss", &(seek as f64 * d / n).to_string(), "-i", &key.0];
                let out = [
                    "-vf",
                    &format!("fps={n}/{d}:start_time={},scale={}:{}", (from - seek) as f64 * d / n, key.1, key.2),
                    "-fps_mode",
                    "passthrough",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "bgra",
                    "-",
                ];
                let child = ffmpeg()
                    .args(["-v", "error"])
                    .args(probe(&key.0).0)
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
        Some(())
    }

    fn video(&mut self, key: Key, k: i64, rate: (f64, f64)) -> Option<Px> {
        if let Some(px) = self.frames.get(&(key.clone(), k)) {
            return Some(px.clone());
        }
        let len = key.1 * key.2 * 4;
        let back = self.decoders.iter().any(|(o, d)| *o == key && k < d.next && d.next <= k + 60);
        let lap = if back { (self.room / 4 / len.max(1)).min(45) as i64 } else { 0 };
        self.decoder(&key, k, (k - lap).max(0), rate)?;
        let (_, mut d) = self.decoders.pop()?;
        while d.next <= k {
            let mut px = vec![0; len];
            if d.child.stdout.as_mut().and_then(|o| o.read_exact(&mut px).ok()).is_none() {
                break;
            }
            while self.held + len > self.room
                && let Some(old) = self.frames.pop_lru()
            {
                self.held -= old.1.len();
            }
            self.held += len;
            self.held -= self.frames.push((key.clone(), d.next), Arc::new(px)).map_or(0, |f| f.1.len());
            d.next += 1;
        }
        self.decoders.push((key.clone(), d));
        self.frames.get(&(key, k)).cloned()
    }

    fn layer(&mut self, c: &Clip, t: f64, fps: u32, size: (usize, usize), k: f64) -> Option<(Px, (usize, usize), (i64, i64))> {
        let (tw, th, x, y) = place(c, (size.0 as f64, size.1 as f64), k);
        let (tw, th) = (tw as usize, th as usize);
        let px = match c.kind {
            Kind::Video => {
                let (key, i, rate) = Self::cell(c, t, fps, (tw, th), self.fast);
                let lean = if key.0 == c.src {
                    (0., 0.)
                } else {
                    (tw as f64 / c.w as f64, th as f64 / c.h as f64)
                };
                self.rough |= lean.0 > 0.;
                let px = self.video(key, i, rate)?;
                if lean.0 > 0. { Arc::new(nudge(&px, tw, lean)) } else { px }
            }
            Kind::Image => self
                .stills
                .entry(format!("{}|{tw}x{th}", c.src))
                .or_insert_with(|| still(&c.src, 0., tw, th).map(Arc::new))
                .clone()?,
            _ => self
                .stills
                .entry(format!(
                    "{}|{}|{}|{}|{}|{}|{:?}|{tw}x{th}",
                    c.src, c.color, c.font, c.background, c.align, c.radius, c.ends
                ))
                .or_insert_with(|| Some(Arc::new(shape::image(c, (size.0 as f64, size.1 as f64), k).0)))
                .clone()?,
        };
        let px = c.grade.as_ref().map_or(px.clone(), |g| grade::paint(g, px, (tw, th)));
        Some((px, (tw, th), (x as i64, y as i64)))
    }

    fn moved(&mut self, out: &mut [u8], c: &Clip, t: f64, fps: u32, (pw, ph): (usize, usize), k: f64, i: usize, a: f64) {
        let ((w, h), q, turn) = ((pw as f64, ph as f64), c.scale.at(i) / c.scale.max(), c.rotate.at(i));
        let mid = |tw, th| seat(c, (w, h), (tw as f64, th as f64), i);
        if c.parts.is_empty() && c.shown == Val::Fixed(1.) {
            if let Some((px, (sw, sh), _)) = self.layer(c, t, fps, (pw, ph), k) {
                blit(out, (pw, ph), &px, (sw, sh), q, turn, mid(sw, sh), a);
            }
            return;
        }
        let key = format!(
            "{}|{}|{}|{}|{}|{}|{:?}",
            c.src,
            c.color,
            c.font,
            c.background,
            c.align,
            c.size * c.scale.max() * k,
            c.grade
        );
        let tint = |mut g: Glyph| {
            g.px = c.grade.as_ref().map_or(g.px.clone(), |x| grade::paint(x, g.px, (g.w, g.h)));
            g
        };
        let laid = || {
            let (glyphs, tw, th) = text::glyphs(c, k, true);
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
        if !c.background.is_empty() {
            let plate = Arc::new(text::flat(&c.background, tw * th));
            let plate = c.grade.as_ref().map_or(plate.clone(), |g| grade::paint(g, plate, (*tw, *th)));
            blit(out, (pw, ph), &plate, (*tw, *th), q, turn, mid, a);
        }
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
        let (k, drawn) = (pw as f64 / tl.size.0 as f64, |c: &&Clip| {
            c.scale.max() > 0. && (c.kind != Kind::Text || c.size > 0.)
        });
        let mut out = [0, 0, 0, 255].repeat(pw * ph);
        self.rough = false;
        for c in tl.clips.iter().filter(|c| c.video && c.start <= t && t < c.end).filter(drawn) {
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
            for sy in (0..sh).filter(|sy| x0 < x1 && (0..ph as i64).contains(&(y + *sy as i64))) {
                let dst = &mut out[((y + sy as i64) as usize * pw + (x + x0 as i64) as usize) * 4..][..(x1 - x0) * 4];
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
            let (key, first, rate) = Self::cell(c, c.start, tl.fps, (tw as usize, th as usize), self.fast);
            if !self.frames.contains(&(key.clone(), first)) {
                self.decoder(&key, first, first, rate);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::tests::clip;

    // Each timeline frame should show one fixed source frame, the one a plain export shows. Footage
    // at 29.97 frames a second on a 30 fps timeline repeats one frame in every 1001; faster, slower
    // and trimmed clips step evenly from the first frame at or after the trim point; the last frame
    // of a clip is held to its end.
    #[test]
    fn frames_match_export() {
        let see = |(start, end, src_in, speed): (f64, f64, f64, f64), fps: f64, rate: f64, frames: &[i64]| {
            let c = Clip {
                start,
                end,
                src_in,
                speed,
                ..clip("video", "")
            };
            frames.iter().map(|n| index(&c, *n as f64 / fps, fps, rate)).collect::<Vec<_>>()
        };
        let film = 30000. / 1001.;
        assert_eq!(
            see((0., 40., 0., 1.), 30., film, &[0, 1, 499, 500, 501, 1199]),
            [0, 1, 499, 499, 500, 1198]
        );
        assert_eq!(see((2., 3.666, 4.334, 1.), 30., film, &[60, 61, 109]), [130, 131, 179]);
        assert_eq!(see((3.666, 4.166, 1.5, 2.), 30., film, &[110, 111, 124]), [45, 47, 73]);
        assert_eq!(
            see((4.166, 6.166, 3., 0.5), 30., film, &[125, 126, 127, 183, 184]),
            [90, 90, 91, 119, 119]
        );
        assert_eq!(
            see((9.426, 13.526, 7.77, 0.3), 30., film, &[400, 402, 403, 405]),
            [268, 268, 269, 269]
        );
        assert_eq!(
            see((13.526, 15.7193, 30.01, 1.5), 30., film, &[406, 407, 408, 409]),
            [900, 902, 903, 905]
        );
        assert_eq!(see((0., 3., 1., 1.), 24., film, &[0, 1, 2, 3, 70, 71]), [30, 31, 32, 34, 117, 119]);
        assert_eq!(see((0., 2., 0.5, 1.), 30., 30., &[0, 1, 59]), [15, 16, 74]);
    }

    // Auto quality should render the picture at the size it has on screen, in steps of an eighth of
    // the timeline and never larger than the timeline, with at most 1920 across while playing or
    // scrubbing. A quality written in the script wins over the screen size.
    #[test]
    fn sizes_follow_the_screen() {
        let size = |w, h, preview: &str, wide, still| {
            let tl = Timeline {
                size: (w, h),
                preview: preview.into(),
                ..Default::default()
            };
            Preview::size(&tl, wide, still)
        };
        let hd = |wide| size(1920, 1080, "auto", wide, false);
        assert_eq!(
            [hd(1900.), hd(3024.), hd(950.), hd(1300.), hd(1.)],
            [(1920, 1080), (1920, 1080), (960, 540), (1440, 810), (240, 135)]
        );
        assert_eq!(size(3840, 2160, "auto", 3024., false), (1920, 1080));
        assert_eq!(size(3840, 2160, "auto", 3024., true), (3360, 1890));
        assert_eq!(size(3840, 2160, "", 5000., true), (3840, 2160));
        assert_eq!(
            ["full", "half", "quarter"].map(|q| size(1920, 1080, q, 100., false)),
            [(1920, 1080), (960, 540), (480, 270)]
        );
    }
}
