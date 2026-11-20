use crate::{
    export::{alpha, live, place, plain, seat},
    grade,
    lut::spread,
    shape,
    text::{self, Glyph},
};
use lru::LruCache;
use origami_lang::{Clip, Kind, Part, Timeline, Val, ffmpeg, ffprobe, output};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    num::NonZeroUsize,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, LazyLock, Mutex},
    thread,
    time::SystemTime,
};

type Key = (String, usize, usize);
type Px = Arc<Vec<u8>>;
type Laid = Arc<(Vec<Glyph>, usize, usize)>;

type Info = (
    &'static [&'static str],
    &'static str,
    f64,
    f64,
    f64,
    f64,
    i64,
    f64,
    Arc<[(f64, bool)]>,
    bool,
);

static KNOWN: LazyLock<Mutex<HashMap<String, Info>>> = LazyLock::new(Mutex::default);
static LANDS: LazyLock<Mutex<HashMap<(String, u64), f64>>> = LazyLock::new(Mutex::default);
static RAM: LazyLock<u64> = LazyLock::new(|| {
    let told = output(Command::new("/usr/sbin/sysctl").args(["-n", "hw.memsize"])).map(|o| o.stdout);
    String::from_utf8_lossy(&told.unwrap_or_default())
        .trim()
        .parse()
        .unwrap_or(u64::MAX)
});

fn fit(ram: u64) -> (usize, usize) {
    ((ram / 8).clamp(1 << 28, 1 << 30) as usize, if ram > 8 << 30 { 6 } else { 4 })
}

fn ask(path: &str, show: &str) -> String {
    let args = ["-v", "error", "-select_streams", "v:0", "-show_entries", show, "-of", "csv=p=0"];
    let out = output(ffprobe().args(args).arg(path)).map(|o| o.stdout).unwrap_or_default();
    String::from_utf8_lossy(&out).into()
}

fn probe(path: &str) -> Info {
    let make = || {
        let show = "stream=codec_name,has_b_frames,pix_fmt,r_frame_rate,avg_frame_rate,time_base,start_time,duration:format=format_name,start_time";
        let text = ask(path, show);
        let mut cols = text.lines().next().unwrap_or_default().split(',');
        let mut col = || cols.next().unwrap_or_default();
        let (codec, late, pix, real, rate) = (col(), col(), col(), col(), col());
        let (tb, first, dur) = (col().strip_prefix("1/"), col(), col());
        let (format, zero) = text.lines().last().unwrap_or_default().rsplit_once(',').unwrap_or_default();
        let (n, d) = rate.split_once('/').unwrap_or_default();
        let num = |s: &str| s.parse::<f64>().unwrap_or(0.);
        let (n, d, tb, zero) = (num(n), num(d), tb.map_or(0., num), num(zero));
        let (ts, vfr) = (matches!(format, "mpegts" | "mpeg"), real != rate && tb > 0.);
        let packets: Vec<Option<(f64, bool)>> = match ts || vfr {
            true => ask(path, "packet=pts,flags")
                .lines()
                .map(|l| {
                    let (pts, flags) = l.split_once(',')?;
                    Some((pts.parse::<f64>().ok()? - (zero * tb).round(), flags.starts_with('K')))
                })
                .collect(),
            false => vec![],
        };
        let mut stamps: Vec<(f64, bool)> = packets.iter().copied().collect::<Option<_>>().unwrap_or_default();
        stamps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut keys: Vec<f64> = packets.iter().flatten().filter(|s| s.1).map(|s| s.0).collect();
        keys.extend(packets.iter().flatten().map(|s| s.0).reduce(f64::max));
        let gap = keys.windows(2).map(|w| w[1] - w[0]).fold(0., f64::max) / tb * n / d;
        let late = match (ts, late != "0") {
            (true, _) => gap.ceil() as i64 + 16,
            (false, true) => 16,
            _ => 0,
        };
        let (lead, down): (&[&str], &str) = match (codec, pix) {
            ("h264", _) => (&[], ""),
            ("hevc", "yuv420p10le") if cfg!(target_arch = "aarch64") => (
                &[
                    "-hwaccel",
                    "videotoolbox",
                    "-hwaccel_output_format",
                    "videotoolbox_vld",
                    "-threads",
                    "1",
                ],
                "hwdownload,format=p010le,",
            ),
            ("hevc", _) => (&["-hwaccel", "videotoolbox", "-threads", "1"], ""),
            _ => (&["-hwaccel", "videotoolbox"], ""),
        };
        let start = first.parse().map_or(0., |f: f64| f - zero);
        let stamps = if ts { vec![] } else { stamps };
        (lead, down, n, d, num(dur), tb, late, start, stamps.into(), ts)
    };
    let known = KNOWN.lock().unwrap().get(path).cloned();
    let info = known.unwrap_or_else(make);
    KNOWN.lock().unwrap().entry(path.into()).or_insert(info).clone()
}

fn lands(src: &str, at: f64, start: f64) -> f64 {
    let make = || {
        let args = ["-hide_banner", "-ss", &at.to_string(), "-i", src, "-map", "0:v:0", "-frames:v", "1"];
        let out = output(ffmpeg().args(args).args(["-vf", "showinfo", "-f", "null", "-"]));
        let text = String::from_utf8_lossy(&out.map(|o| o.stderr).unwrap_or_default()).to_string();
        let pts = text
            .split("pts_time:")
            .nth(1)
            .and_then(|s| s.split_whitespace().next()?.parse().ok());
        at.max(start) + pts.unwrap_or(0.)
    };
    let key = (src.to_string(), at.to_bits());
    let known = LANDS.lock().unwrap().get(&key).copied();
    let found = known.unwrap_or_else(make);
    *LANDS.lock().unwrap().entry(key).or_insert(found)
}

pub fn lead(src: &str) -> f64 {
    match Path::new(src).is_file().then(|| probe(src)) {
        Some((_, _, n, d, .., late, _, _, true)) if n > 0. => late as f64 * d / n,
        _ => 0.,
    }
}

fn raw(args: &[&str], len: usize) -> Option<Vec<u8>> {
    let tail = ["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgra", "-"];
    let out = output(ffmpeg().args(["-v", "error"]).args(args).args(tail)).ok()?;
    (out.stdout.len() == len).then_some(out.stdout)
}

fn still(path: &str, t: f64, w: usize, h: usize) -> Option<Vec<u8>> {
    raw(&["-ss", &t.to_string(), "-i", path, "-vf", &format!("scale={w}:{h}")], w * h * 4)
}

pub fn thumb(path: &str, t: f64, w: usize, h: usize) -> Option<Vec<u8>> {
    let ((lead, down, ..), fast) = (probe(path), ["-skip_frame", "nokey", "-ss", &t.to_string(), "-i", path]);
    raw(&[lead, &fast, &["-vf", &format!("{down}scale={w}:{h}")]].concat(), w * h * 4)
}

pub fn peaks(path: &str) -> Vec<f32> {
    let args = ["-v", "error", "-i", path, "-ac", "1", "-ar", "4000", "-f", "u8", "-"];
    let pcm = output(ffmpeg().args(args)).map(|o| o.stdout).unwrap_or_default();
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
    if !(k > 0. && a > 0.) || out.is_empty() {
        return;
    }
    let ((sin, cos), (hw, hh)) = (deg.to_radians().sin_cos(), (sw as f64 / 2., sh as f64 / 2.));
    let (rx, ry) = (
        k * (hw * cos.abs() + hh * sin.abs()) + 1.,
        k * (hw * sin.abs() + hh * cos.abs()) + 1.,
    );
    let span = |c: f64, r: f64, n: usize| ((c - r).clamp(0., n as f64) as usize, (c + r).ceil().clamp(0., n as f64) as usize);
    let ((x0, x1), (y0, y1)) = (span(cx, rx, pw), span(cy, ry, ph));
    let paint = |from: usize, rows: &mut [u8]| {
        for (y, row) in (y0 + from / (pw * 4)..).zip(rows.chunks_exact_mut(pw * 4)) {
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
    let rows = &mut out[y0 * pw * 4..y1 * pw * 4];
    if (x1 - x0) * (y1 - y0) < 1 << 16 {
        paint(0, rows)
    } else {
        spread(rows, pw * 4, paint)
    }
}

fn nudge(px: &[u8], w: usize, (fx, fy): (f64, f64)) -> Vec<u8> {
    let (mut out, n, cut) = (px.to_vec(), w * 4, |f: f64| (f as usize, (f.fract() * 256.) as u32));
    let ((kx, a), (ky, b), h) = (cut(fx), cut(fy), px.len() / n.max(1));
    if kx + ky == 0 && a + b == 0 {
        return out;
    }
    let band = h.div_ceil(thread::available_parallelism().map_or(1, usize::from)).max(1) * n;
    thread::scope(|s| {
        for (j, rows) in out.chunks_mut(band).enumerate() {
            s.spawn(move || {
                for (y, row) in rows.chunks_exact_mut(n).enumerate() {
                    let [top, low] = [0, 1].map(|d| &px[(j * band / n + y + ky + d).min(h - 1) * n..][..n]);
                    for (x, p) in row.chunks_exact_mut(4).enumerate() {
                        let (i, k) = ((x + kx).min(w - 1) * 4, (x + kx + 1).min(w - 1) * 4);
                        for c in 0..4 {
                            let l = top[i + c] as u32 * (256 - a) + top[k + c] as u32 * a;
                            let r = low[i + c] as u32 * (256 - a) + low[k + c] as u32 * a;
                            p[c] = ((l * (256 - b) + r * b) >> 16) as u8;
                        }
                    }
                }
            });
        }
    });
    out
}

fn kept<V: Clone>(now: &mut HashMap<String, V>, was: &mut HashMap<String, V>, key: String, make: impl FnOnce() -> V) -> V {
    now.entry(key).or_insert_with_key(|k| was.remove(k).unwrap_or_else(make)).clone()
}

fn index(c: &Clip, t: f64, fps: f64, (n, d, tb): (f64, f64, f64), dur: f64) -> i64 {
    let (rate, per) = (n / d, tb * d / n);
    let (step, at, first) = (fps / rate / c.speed, (t - c.start) * fps, (c.src_in * rate - 1e-6).ceil());
    let tail = if dur > 0. { (dur * rate + 1e-3).round() - first } else { f64::MAX };
    let last = ((c.end - c.start) * fps / step - 1e-6).ceil().min(tail) - 1.;
    let i = match tb > 0. && per.fract() == 0. {
        true => {
            let tick = |i: f64| ((i * per / c.speed + c.start / (1. / tb) + 1e-6) as i64 * fps as i64 + tb as i64 / 2) / tb as i64;
            let (k, mut i) = ((t * fps).round() as i64, (at / step - 3.).floor().max(0.));
            while tick(i + 1.) <= k {
                i += 1.;
            }
            while i > 0. && tick(i - 1.) == tick(i) {
                i -= 1.;
            }
            i
        }
        false => {
            let i = ((at - 0.5) / step - 1e-6).ceil().max(0.);
            if i * step < at + 0.5 - 1e-6 { i } else { i - 1. }
        }
    };
    (first + i.min(last).max(0.)) as i64
}

fn seam(c: &Clip, t: f64, fps: f64, (n, d, tb): (f64, f64, f64), dur: f64, seq: bool) -> Option<(f64, f64, f64, f64)> {
    let at = |c: &Clip, t: f64| index(c, t, fps, (n, d, tb), dur);
    let shown = |p: &[&Clip], k: i64| p.iter().rev().find(|c| live(c, k as f64 / fps)).map(|c| at(c, k as f64 / fps));
    let (a, v, i, first) = (c.src_in, c.speed, at(c, t), |s: f64| (s * n / d - 1e-6).ceil() as i64);
    let (b, cut) = (a + (c.end - c.start) * v, |f: i64| a + (f - first(a)) as f64 * d / n);
    let span = ((c.start + (cut(i) - 4. * d / n - a) / v) * fps).floor() as i64..(c.end * fps).ceil() as i64 + 2;
    let was: Vec<_> = span.clone().map(|k| shown(&[c], k)).collect();
    let read = |v: f64| serde_json::from_str(&v.to_string()).unwrap_or(v);
    let edge = if tb > 0. { 0.6 / tb } else { 1e-4 };
    let reads = |from: f64, to: f64, j: i64| (j - first(from)) as f64 * d / n + edge <= to - from;
    let tail = was.iter().rev().find_map(|w| *w).filter(|j| reads(a, b, *j));
    let tiers: [&[i64]; 7] = [&[i, i + 1], &[i + 2], &[i - 1], &[i + 3], &[i - 2], &[i + 4], &[i - 3]];
    let order = tiers
        .into_iter()
        .flat_map(|near| (3..7).flat_map(move |p| [true, false].map(|whole| (near, p, whole))));
    for (near, p, whole) in order {
        let k = 10f64.powi(p);
        let reach = if p == 3 { (d / n * k) as i64 + 3 } else { (edge * k) as i64 + 10 };
        let grid = |v: f64| (v * k).round() / k;
        let flat = |v: f64| (grid(v) - v).abs() < 1e-9;
        for &f in near {
            let mid = (cut(f) * k).round() as i64;
            let fit = |mut s: i64| {
                while first(s as f64 / k) > f {
                    s -= 1;
                }
                while first(s as f64 / k) < f {
                    s += 1;
                }
                s
            };
            let mut tries: Vec<_> = (mid - reach..=mid + reach).map(|m| (m, fit(m))).collect();
            tries.sort_by_key(|(m, s)| ((m - s).abs(), (m - mid).abs()));
            for (m, s) in tries.into_iter().map(|(m, s)| (m as f64 / k, s as f64 / k)) {
                let by = if seq { (m - a) / v } else { grid((m - a) / v) };
                let end = s + (b - a) - by * v;
                let end = if flat(end) {
                    grid(end)
                } else if (end - b).abs() < 1e-9 {
                    b
                } else {
                    continue;
                };
                let x = if seq || !flat(c.start + by) {
                    c.start + by
                } else {
                    grid(c.start + by)
                };
                let stop = read(x + (end - s) / v);
                let cuts = first(s - 5e-6) == f && tail.is_none_or(|j| reads(s, end, j));
                if m - a < 0.04 || end - s < 0.04 || (stop == c.end) != whole || !cuts {
                    continue;
                }
                let one = Clip {
                    end: read(c.start + (m - a) / v),
                    ..c.clone()
                };
                let two = Clip {
                    start: read(x),
                    end: stop,
                    src_in: s,
                    ..c.clone()
                };
                let k = ((one.end.min(two.start) - 1e-6) * fps).ceil() as i64 - 1;
                let last = was
                    .get((k - span.start) as usize)
                    .copied()
                    .flatten()
                    .filter(|_| live(&one, k as f64 / fps));
                if last.is_none_or(|j| reads(a, m, j)) && span.clone().zip(&was).all(|(k, w)| shown(&[&one, &two], k) == *w) {
                    return Some((m, s, by, end));
                }
            }
        }
    }
    None
}

fn stamp(c: &Clip, t: f64, fps: f64, tb: f64, stamps: &[(f64, bool)]) -> i64 {
    let at = |s: f64| stamps.partition_point(|p| p.0 < s.round());
    let first = at(c.src_in * tb).min(stamps.len() - 1);
    let end = at(stamps[first].0 + (c.end - c.start) * c.speed * tb).max(first + 1);
    let tick = |p: &(f64, bool)| {
        (((p.0 - stamps[first].0) / c.speed + c.start / (1. / tb) + 1e-6) as i64 * fps as i64 + tb as i64 / 2) / tb as i64
    };
    let (k, kept) = ((t * fps).round() as i64, &stamps[first..end]);
    let (before, by) = (kept.partition_point(|p| tick(p) < k), kept.partition_point(|p| tick(p) <= k));
    (first + before.min(by.max(1) - 1)) as i64
}

fn rate(src: &str, fps: f64) -> (f64, f64, f64) {
    match probe(src) {
        (_, _, n, d, _, tb, ..) if n > 0. && d > 0. => (n, d, tb),
        _ => (fps, 1., 0.),
    }
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
    texts: HashMap<String, Laid>,
    was: (HashMap<String, Option<Px>>, HashMap<String, Laid>),
    stamps: HashMap<String, Option<(u64, SystemTime)>>,
    at: f64,
    primed: (Vec<u32>, (usize, usize)),
    pub(crate) room: usize,
    most: usize,
    pub fast: bool,
    pub rough: bool,
}

impl Default for Preview {
    fn default() -> Self {
        let (room, most) = fit(*RAM);
        Self {
            decoders: vec![],
            frames: LruCache::new(NonZeroUsize::new(600).unwrap()),
            held: 0,
            stills: HashMap::new(),
            texts: HashMap::new(),
            was: Default::default(),
            stamps: HashMap::new(),
            at: -1.,
            primed: Default::default(),
            room,
            most,
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
        (((w * n / 8.) as usize).max(1), ((h * n / 8.) as usize).max(1))
    }

    fn cell(c: &Clip, t: f64, fps: u32, (tw, th): (usize, usize), fast: bool) -> (Key, i64, (f64, f64)) {
        let (lrf, fps) = (Path::new(&c.src).with_extension("LRF"), fps as f64);
        let src = if fast && lrf.exists() {
            lrf.to_string_lossy().into()
        } else {
            c.src.clone()
        };
        let ((n, d, tb), (.., late, start, stamps, ts)) = (rate(&src, fps), probe(&src));
        let src_in = match (late > 0 && !ts, stamps.is_empty()) {
            (true, true) => ((lands(&src, c.src_in, start) - start) * n / d).round() * d / n,
            (true, false) => lands(&src, c.src_in, start) - start,
            _ => (c.src_in - start).max(0.),
        };
        let i = match stamps.is_empty() {
            true if src_in == c.src_in => index(c, t, fps, (n, d, tb), probe(&c.src).4),
            true => index(&Clip { src_in, ..c.clone() }, t, fps, (n, d, tb), probe(&c.src).4),
            false => stamp(&Clip { src_in, ..c.clone() }, t, fps, tb, &stamps),
        };
        ((src, tw, th), i, (n, d))
    }

    pub fn seam(c: &Clip, t: f64, fps: u32, seq: bool) -> Option<(f64, f64, f64, f64)> {
        seam(c, t, fps as f64, rate(&c.src, fps as f64), probe(&c.src).4, seq)
    }

    fn renew(&mut self, src: &str) {
        let now = fs::metadata(src).and_then(|m| Ok((m.len(), m.modified()?))).ok();
        if self.stamps.insert(src.into(), now).is_none_or(|was| was == now) {
            return;
        }
        KNOWN.lock().unwrap().remove(src);
        LANDS.lock().unwrap().retain(|k, _| k.0 != src);
        self.decoders.retain(|(k, _)| k.0 != src);
        let old: Vec<_> = self.frames.iter().map(|(k, _)| k.clone()).filter(|k| k.0.0 == src).collect();
        for k in old {
            self.held -= self.frames.pop(&k).map_or(0, |px| px.len());
        }
        for stills in [&mut self.stills, &mut self.was.0] {
            stills.retain(|k, _| !k.contains(src));
        }
    }

    fn near(&self, key: &Key, k: i64) -> Option<usize> {
        self.decoders.iter().position(|(o, d)| o == key && d.next <= k && k <= d.next + 30)
    }

    pub fn ready(&mut self, tl: &Timeline, t: f64, (pw, ph): (usize, usize)) -> bool {
        let (k, shown) = (pw as f64 / tl.size.0 as f64, |c: &&Clip| {
            c.kind == Kind::Video && c.video && live(c, t)
        });
        let mut all = true;
        for c in tl.clips.iter().filter(shown) {
            let (tw, th, ..) = place(c, (pw as f64, ph as f64), k);
            let (key, i, rate) = Self::cell(c, t, tl.fps, (tw as usize, th as usize), false);
            if !self.frames.contains(&(key.clone(), i)) && self.near(&key, i).is_none() {
                all = false;
                if (t != self.at || self.decoders.iter().all(|(o, _)| o.0 != key.0)) && self.lap(&key, i) == 0 {
                    self.decoder(&key, i, i, rate);
                }
            }
        }
        all
    }

    pub fn prime(&mut self, tl: &Timeline, t: f64, size: (usize, usize)) {
        let old: Vec<u32> = self.decoders.iter().map(|(_, d)| d.child.id()).collect();
        self.at = f64::NAN;
        self.ready(tl, t, size);
        let ids = self.decoders.iter().map(|(_, d)| d.child.id());
        self.primed = (ids.filter(|n| !old.contains(n)).collect(), size);
    }

    fn lap(&self, key: &Key, k: i64) -> i64 {
        let back = self.decoders.iter().any(|(o, d)| o == key && k < d.next && d.next <= k + 60);
        if back {
            (self.room / 4 / (key.1 * key.2 * 4).max(1)).min(45) as i64
        } else {
            0
        }
    }

    fn decoder(&mut self, key: &Key, k: i64, from: i64, (n, d): (f64, f64)) -> Option<()> {
        let found = match self.near(key, k) {
            Some(i) => self.decoders.remove(i),
            None => {
                let (lead, down, .., tb, late, start, stamps, _) = probe(&key.0);
                let (at, pick) = match stamps.is_empty() {
                    false => {
                        let k = stamps.iter().take(from as usize + 1).rposition(|s| s.1).unwrap_or(0);
                        ((stamps[k].0 / tb - 5e-4).max(0.), format!("trim=start_frame={}", from - k as i64))
                    }
                    true => {
                        let seek = (from - late).max(0);
                        let fill = format!("fps={n}/{d}:start_time={}", (from - seek) as f64 * d / n);
                        (seek as f64 * d / n + start, fill)
                    }
                };
                let args = ["-ss", &at.to_string(), "-i", &key.0];
                let out = [
                    "-vf",
                    &format!("{pick},{down}scale={}:{}", key.1, key.2),
                    "-fps_mode",
                    "passthrough",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "bgra",
                    "-",
                ];
                let lrf: &[&str] = match key.0.ends_with(".LRF") {
                    true => &["-probesize", "32768", "-analyzeduration", "0"],
                    false => &[],
                };
                let child = origami_lang::start(
                    ffmpeg()
                        .args(["-v", "error"])
                        .args(lead)
                        .args(lrf)
                        .args(args)
                        .args(out)
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null()),
                )
                .ok()?;
                self.decoders.drain(..self.decoders.len().saturating_sub(self.most - 1));
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
        self.decoder(&key, k, (k - self.lap(&key, k)).max(0), rate)?;
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
        let (px, base) = match c.kind {
            Kind::Video => {
                let tries = [self.fast, false];
                let ((key, i), px) = tries[..1 + self.fast as usize].iter().find_map(|fast| {
                    let (key, i, rate) = Self::cell(c, t, fps, (tw, th), *fast);
                    self.renew(&key.0);
                    Some(((key.clone(), i), self.video(key, i, rate)?))
                })?;
                let base = format!("{key:?}|{i}");
                let lean = if key.0 == c.src {
                    (0., 0.)
                } else {
                    (tw as f64 / c.w as f64, th as f64 / c.h as f64)
                };
                self.rough |= lean.0 > 0.;
                (if lean.0 > 0. { Arc::new(nudge(&px, tw, lean)) } else { px }, base)
            }
            Kind::Image => {
                self.renew(&c.src);
                let base = format!("{}|{tw}x{th}", c.src);
                (
                    kept(&mut self.stills, &mut self.was.0, base.clone(), || {
                        still(&c.src, 0., tw, th).map(Arc::new)
                    })?,
                    base,
                )
            }
            _ => {
                let base = format!(
                    "{}|{}|{}|{}|{}|{}|{:?}|{tw}x{th}",
                    c.src, c.color, c.font, c.background, c.align, c.radius, c.ends
                );
                let make = || Some(Arc::new(shape::image(c, (size.0 as f64, size.1 as f64), k).0));
                (kept(&mut self.stills, &mut self.was.0, base.clone(), make)?, base)
            }
        };
        let px = match &c.grade {
            Some(g) => kept(&mut self.stills, &mut self.was.0, format!("{base}|{g:?}"), || {
                Some(grade::paint(g, px, (tw, th)))
            })?,
            None => px,
        };
        (px.len() >= tw * th * 4).then_some((px, (tw, th), (x as i64, y as i64)))
    }

    fn moved(&mut self, out: &mut [u8], c: &Clip, t: f64, fps: u32, (pw, ph): (usize, usize), k: f64, i: usize, a: f64) {
        let ((w, h), q, turn) = ((pw as f64, ph as f64), c.scale.at(i) / c.scale.max(), c.rotate.at(i));
        let mid = |tw, th| seat(c, (w, h), (tw as f64, th as f64), i);
        if c.kind != Kind::Text || c.parts.is_empty() && c.shown == Val::Fixed(1.) {
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
        let text = kept(&mut self.texts, &mut self.was.1, key, laid);
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
        let (primed, size) = std::mem::take(&mut self.primed);
        if self.fast || size != (pw, ph) {
            self.decoders.retain(|(_, d)| !primed.contains(&d.child.id()));
        }
        let mut out = [0, 0, 0, 255].repeat(pw * ph);
        (self.rough, self.at) = (false, t);
        self.was = (std::mem::take(&mut self.stills), std::mem::take(&mut self.texts));
        for c in tl.clips.iter().filter(|c| c.video && live(c, t)).filter(drawn) {
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
        let soon = |c: &&Clip| c.kind == Kind::Video && t < c.start && c.start < t + 1.;
        let next = tl.clips.iter().filter(soon).map(|c| c.start).fold(f64::MAX, f64::min);
        for c in tl.clips.iter().filter(soon).filter(|c| c.start == next) {
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
    use crate::tests::{TIDY, clip, film, scratch};

    // Each timeline frame shows the source frame a plain export shows, for 29.97 fps footage on a 30 fps
    // timeline, faster, slower and trimmed clips, last frames held to the clip's end, and frames landing
    // half-way between two timeline frames whatever the file's time base.
    #[test]
    fn frames_match_export() {
        let held = |(start, end, src_in, speed): (f64, f64, f64, f64), fps: f64, rate: (f64, f64, f64), dur: f64, frames: &[i64]| {
            let c = Clip {
                start,
                end,
                src_in,
                speed,
                ..clip("video", "")
            };
            frames
                .iter()
                .map(|n| index(&c, *n as f64 / fps, fps, rate, dur))
                .collect::<Vec<_>>()
        };
        let see = |span, fps, rate, frames: &[i64]| held(span, fps, rate, 0., frames);
        let (film, thirty, day) = ((30000., 1001., 30000.), (30., 1., 15360.), (24., 1., 12288.));
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
        assert_eq!(see((0., 2., 0.5, 1.), 30., thirty, &[0, 1, 59]), [15, 16, 74]);
        assert_eq!(held((0., 1.99, 4.01, 1.), 30., thirty, 6., &[0, 58, 59]), [121, 179, 179]);
        assert_eq!(see((0., 0.99, 1.01, 1.), 30., thirty, &[0, 28, 29]), [31, 59, 60]);
        assert_eq!(
            held((0.4, 0.606, 5.8, 1.), 30., (24000., 1001., 24000.), 6.006, &[12, 15, 17]),
            [140, 142, 143]
        );
        assert_eq!(held((0., 1., 5.98, 1.), 24., day, 6., &[0, 5]), [144, 144]);
        assert_eq!(see((0.05, 3.05, 0., 1.), 60., day, &[3, 5, 60, 65]), [0, 1, 23, 25]);
        assert_eq!(see((0., 2., 0., 2.), 30., thirty, &[0, 1, 2, 30]), [0, 1, 3, 59]);
        assert_eq!(see((0.05, 3.05, 0., 1.), 60., (24., 1., 0.), &[5, 60]), [0, 22]);
    }

    // Splitting a clip in a cat or a stack leaves every timeline frame showing the source frame it showed
    // before, at common rates, time bases, speeds and trims, with times of at most six decimals, a first
    // piece that still reads its last frame, and only a few cuts near the clip's ends refused.
    #[test]
    fn seams_keep_every_frame() {
        let rates = [
            (30000., 1001., 30000.),
            (30., 1., 15360.),
            (25., 1., 12800.),
            (24., 1., 12288.),
            (60., 1., 15360.),
        ];
        let read = |v: f64| serde_json::from_str::<f64>(&v.to_string()).unwrap();
        let (mut changed, mut longer, mut refused, mut early) = (vec![], vec![], vec![], vec![]);
        for (rate, fps) in rates.into_iter().flat_map(|rate| [24., 25., 30., 60.].map(|fps| (rate, fps))) {
            let shown = |clips: &[Clip], t: f64| clips.iter().rev().find(|c| live(c, t)).map(|c| (c.z, index(c, t, fps, rate, 0.)));
            let (mut bad, mut more, mut no, mut held) = (0, 0, 0, 0);
            for (seq, (speed, src_in, start, len)) in [true, false].into_iter().flat_map(|seq| {
                [
                    (0.5, 0., 0., 3.),
                    (0.5, 0.37, 0.2, 2.),
                    (1., 0., 0., 3.),
                    (1., 0.51, 0.7, 2.9),
                    (1.5, 0., 1.234, 2.),
                    (1.5, 0.2, 0., 2.5),
                    (2., 0.1, 0., 3.),
                    (2., 0., 0.5, 2.),
                    (0.5, 0.1, 0.35, 2.5),
                    (0.5, 0.021, 0.2, 2.5),
                    (1., 0.021, 0.05, 2.5),
                    (1., 0.333, 0.35, 2.5),
                    (2., 0.063, 0.15, 2.5),
                ]
                .map(|case| (seq, case))
            }) {
                let b = src_in + len * speed;
                let c = Clip {
                    start,
                    end: read(start + (b - src_in) / speed),
                    src_in,
                    speed,
                    ..clip("video", "")
                };
                let next = Clip {
                    start: c.end,
                    end: c.end + 1.,
                    z: 1,
                    ..clip("video", "")
                };
                for k in (start * fps).ceil() as usize + 1..(c.end * fps) as usize {
                    let Some((m, s, by, end)) = seam(&c, k as f64 / fps, fps, rate, 0., seq) else {
                        no += 1;
                        continue;
                    };
                    let x = if seq {
                        start + by
                    } else {
                        format!("{:.9}", start + by).parse().unwrap()
                    };
                    let one = Clip {
                        end: read(start + (m - src_in) / speed),
                        ..c.clone()
                    };
                    let two = Clip {
                        start: read(x),
                        end: read(x + (end - s) / speed),
                        src_in: s,
                        ..c.clone()
                    };
                    let after = Clip {
                        start: if seq { two.end } else { c.end },
                        end: next.end,
                        ..next.clone()
                    };
                    let (was, now) = ([next.clone(), c.clone()], [after, one, two]);
                    let same = |j: usize| shown(&now, j as f64 / fps) == shown(&was, j as f64 / fps);
                    assert!(same(k), "{rate:?} {fps} {speed} {k}");
                    bad += (0..(next.end * fps) as usize + 2).filter(|j| !same(*j)).count();
                    let alone = |j: &usize| live(&now[1], *j as f64 / fps) && !live(&now[2], *j as f64 / fps);
                    let lead = (src_in * rate.0 / rate.1 - 1e-6).ceil() as i64;
                    let cut = |j: usize| {
                        (index(&now[1], j as f64 / fps, fps, rate, 0.) - lead) as f64 * rate.1 / rate.0 + 0.6 / rate.2 > m - src_in
                    };
                    held += (0..(next.end * fps) as usize).filter(alone).filter(|j| cut(*j)).count();
                    let written = [m, s, end, if seq { 0. } else { by }];
                    more += written.iter().any(|v| ((v * 1e3).round() / 1e3 - v).abs() > 1e-9) as usize;
                    assert!(written.iter().all(|v| ((v * 1e6).round() / 1e6 - v).abs() < 1e-9), "{written:?}");
                }
            }
            changed.push(bad);
            longer.push(more);
            refused.push(no);
            early.push(held);
        }
        assert_eq!((changed, early), (vec![0; 20], vec![0; 20]));
        assert_eq!(longer, [33, 70, 0, 23, 0, 0, 0, 0, 226, 0, 0, 247, 0, 100, 227, 40, 0, 0, 0, 0]);
        assert_eq!(refused.iter().sum::<usize>(), 76);
    }

    // While scrubbing, a sidecar that cannot be read, or that ends before the full file does, should
    // give way to the full file instead of leaving the clip out of the picture.
    #[test]
    fn broken_sidecars_fall_back() {
        let dir = scratch("sidecar");
        for name in ["text", "short"] {
            film(&dir.join(format!("{name}.mp4")), "color=red:s=64x36:r=30:d=4", &[]);
        }
        fs::write(dir.join("text.LRF"), "not a video").unwrap();
        film(&dir.join("short.LRF"), "color=blue:s=64x36:r=30:d=1", &["-f", "mp4"]);
        let seen = |name: &str, t: f64| {
            let c = Clip {
                src: dir.join(format!("{name}.mp4")).to_string_lossy().into(),
                w: 64,
                h: 36,
                end: 4.,
                ..clip("video", "")
            };
            let tl = Timeline {
                fps: 30,
                size: (64, 36),
                duration: 4.,
                clips: vec![c],
                ..Default::default()
            };
            let mut pv = Preview {
                fast: true,
                ..Default::default()
            };
            let px = pv.frame(&tl, t, (64, 36));
            (px[(18 * 64 + 32) * 4 + 2] > 128, px[(18 * 64 + 32) * 4] > 128)
        };
        assert_eq!(
            [seen("text", 2.), seen("short", 0.5), seen("short", 2.)],
            [(true, false), (false, true), (true, false)]
        );
    }

    // In a montage of many short clips the preview should warm up a decoder for the next clip only,
    // so the clip on screen keeps its decoder instead of losing it to clips further ahead.
    #[test]
    fn montages_keep_the_decoder_on_screen() {
        let file = scratch("montage").join("a.mp4");
        film(&file, "color=red:s=64x36:r=30:d=1", &[]);
        let clips = (0..8).map(|i| Clip {
            src: file.to_string_lossy().into(),
            w: 64,
            h: 36,
            start: i as f64 * 0.1,
            end: (i + 1) as f64 * 0.1,
            scale: Val::Fixed(1. - i as f64 * 0.05),
            ..clip("video", "")
        });
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 0.8,
            clips: clips.collect(),
            ..Default::default()
        };
        let mut pv = Preview::default();
        pv.frame(&tl, 0., (64, 36));
        assert!(pv.decoders.iter().any(|(k, _)| k.1 == 64));
    }

    // A media file replaced on disk under the same name should show its new pictures in a preview
    // that already showed the old ones.
    #[test]
    fn replaced_files_show_again() {
        let dir = scratch("swap");
        let (file, take) = (dir.join("swap.mp4"), dir.join("take.mp4"));
        for (ink, path) in [("red", &file), ("blue", &take)] {
            film(path, &format!("color={ink}:s=64x36:r=30:d=2"), &[]);
        }
        let c = Clip {
            src: file.to_string_lossy().into(),
            w: 64,
            h: 36,
            end: 2.,
            ..clip("video", "")
        };
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 2.,
            clips: vec![c],
            ..Default::default()
        };
        let mut pv = Preview::default();
        let blue = |pv: &mut Preview| pv.frame(&tl, 1., (64, 36))[(18 * 64 + 32) * 4] > 128;
        assert!(!blue(&mut pv));
        thread::sleep(std::time::Duration::from_millis(20));
        fs::copy(&take, &file).unwrap();
        assert!(blue(&mut pv));
    }

    // In footage with B-frames, a picture shown just before a keyframe can be decoded only after the
    // pictures before it: a jump straight to it should still show it, as playing up to it does.
    #[test]
    fn jumps_before_keyframes_show_the_frame() {
        let file = scratch("bframes").join("a.mp4");
        let src = "color=black:s=64x36:r=30:d=2,format=yuv444p,geq=lum='16+8*(N-25*floor(N/25))':cb=128:cr=128";
        let x264 = [
            "-c:v",
            "libx264",
            "-g",
            "24",
            "-x264-params",
            "bframes=3:open-gop=1:scenecut=0",
            "-pix_fmt",
            "yuv420p",
        ];
        film(&file, src, &x264);
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 2.,
            clips: vec![Clip {
                src: file.to_string_lossy().into(),
                w: 64,
                h: 36,
                end: 2.,
                ..clip("video", "")
            }],
            ..Default::default()
        };
        let mut played = Preview::default();
        let wrong: Vec<usize> = (0..60)
            .filter(|k| played.frame(&tl, *k as f64 / 30., (64, 36)) != Preview::default().frame(&tl, *k as f64 / 30., (64, 36)))
            .collect();
        assert!(wrong.is_empty(), "{wrong:?}");
    }

    // Variable-frame-rate footage should show the frame a plain export shows: the clip starts on the
    // first frame at or after the trim point, each later frame shows from the timeline frame its time
    // rounds to, and of two frames rounding to the same timeline frame the first one is shown there.
    #[test]
    fn stamps_match_export() {
        let shown = |stamps: &[(f64, bool)], (start, end, src_in, speed): (f64, f64, f64, f64), fps: f64, frames: &[i64]| {
            let c = Clip {
                start,
                end,
                src_in,
                speed,
                ..clip("video", "")
            };
            frames
                .iter()
                .map(|k| stamp(&c, *k as f64 / fps, fps, 15360., stamps))
                .collect::<Vec<_>>()
        };
        let uneven: Vec<_> = (0..180).map(|n| ((n + n / 2) as f64 * 512., n % 24 == 0)).collect();
        let sixty: Vec<_> = (0..480).map(|n| ((n + n / 3 * 2) as f64 * 256., n % 30 == 0)).collect();
        assert_eq!(
            shown(&uneven, (0., 2.7, 0.5, 1.), 30., &[0, 1, 2, 3, 4, 5, 6, 7]),
            [10, 11, 11, 12, 13, 13, 14, 15]
        );
        assert_eq!(shown(&uneven, (0., 1.334, 1., 1.5), 24., &[0, 1, 2, 3, 31]), [20, 21, 22, 24, 59]);
        assert_eq!(shown(&uneven, (0., 0.466667, 8.5, 1.), 30., &[0, 1, 2, 13]), [170, 171, 171, 179]);
        assert_eq!(
            shown(&sixty, (0., 3., 0., 1.), 30., &[0, 1, 2, 3, 4, 5, 6, 7]),
            [0, 1, 2, 3, 5, 6, 7, 8]
        );
        assert_eq!(shown(&sixty, (0., 1.98, 1.01, 0.5), 30., &[0, 1, 2, 3, 4]), [37, 38, 38, 38, 39]);
    }

    // MPEG-TS and MPEG-PS footage, whose times start late and whose sound starts before its pictures,
    // should show the frame the export shows, played or jumped to: a trimmed clip starts on the first
    // frame at or after the trim point, not on a later keyframe, and runs to the last frame of the file.
    #[test]
    fn transport_streams_match_export() {
        let _tidy = TIDY.read();
        let dir = scratch("streams");
        let lum = "if(between(Y,60,119),16+8*(N-25*floor(N/25)),255*random(1))";
        let src = format!("color=black:s=320x180:r=30:d=4,format=yuv444p,geq=lum='{lum}':cb=128:cr=128");
        for (file, codec) in [
            ("a.ts", &["-c:v", "libx264", "-g", "24", "-c:a", "aac"][..]),
            ("a.mpg", &["-c:v", "mpeg1video", "-q:v", "3", "-g", "12", "-bf", "2", "-c:a", "ac3"]),
        ] {
            film(
                &dir.join(file),
                &src,
                &[&["-f", "lavfi", "-i", "sine=d=4", "-shortest", "-pix_fmt", "yuv420p"], codec].concat(),
            );
        }
        for (file, fps, trim, frames) in [
            ("a.ts", 30, "trim(0.72, 2.2)", 44),
            ("a.mpg", 30, "trim(2.5)", 45),
            ("a.mpg", 24, "trim(0.72, 2.2)", 36),
        ] {
            let script = format!("settings({{ fps: {fps}, size: [64, 36] }})\nvideo('{file}').{trim}");
            fs::write(dir.join("edit.js"), script).unwrap();
            let tl = origami_lang::eval(&dir.join("edit.js"), true, crate::footprint).unwrap();
            let out = dir.join("out.mp4").to_string_lossy().to_string();
            crate::render(&tl, &out, &Default::default(), |_| {}).unwrap();
            let args = ["-v", "error", "-i", &out, "-f", "rawvideo", "-pix_fmt", "gray", "-"];
            let px = output(ffmpeg().args(args)).unwrap().stdout;
            let mut played = Preview::default();
            let wrong: Vec<usize> = (0..px.len() / (64 * 36))
                .filter(|k| {
                    let t = *k as f64 / fps as f64;
                    let made = px[k * 64 * 36 + 18 * 64 + 32];
                    let [a, b] = [played.frame(&tl, t, (64, 36)), Preview::default().frame(&tl, t, (64, 36))];
                    [a, b].iter().any(|p| p[(18 * 64 + 32) * 4].abs_diff(made) > 4)
                })
                .collect();
            assert!(px.len() == frames * 64 * 36 && wrong.is_empty(), "{file} {trim} {wrong:?}");
        }
    }

    // A sidecar picture is shifted by the drawn size of one full-size pixel, which is more than a
    // pixel when the clip is drawn larger than its file: whole pixels should be carried and the rest
    // blended, holding the edges, and nothing should move when there is nothing to shift.
    #[test]
    fn nudges_by_any_amount() {
        let px: Vec<u8> = (0..12).flat_map(|i| [i as u8 * 20; 4]).collect();
        let at = |out: &[u8], x: usize, y: usize| out[(y * 4 + x) * 4];
        assert_eq!(nudge(&px, 4, (0., 0.)), px);
        let two = nudge(&px, 4, (2., 0.));
        assert_eq!([0, 1, 2, 3].map(|x| at(&two, x, 0)), [40, 60, 60, 60]);
        let far = nudge(&px, 4, (1.5, 1.));
        assert_eq!([at(&far, 0, 0), at(&far, 2, 1), at(&far, 3, 2)], [110, 220, 220]);
        let tiny = nudge(&px, 4, (0.5, 0.25));
        assert_eq!([at(&tiny, 0, 0), at(&tiny, 3, 2)], [30, 220]);
    }

    // A clip that is not text is drawn as itself even when its internals carry text-only motion and a
    // huge text size, instead of being laid out as text.
    #[test]
    fn only_text_is_laid_out() {
        let c = Clip {
            src: "#ff0000".into(),
            size: 4e9,
            shown: Val::Fixed(0.5),
            ..clip("color", "")
        };
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 1.,
            clips: vec![c],
            ..Default::default()
        };
        let mid = (18 * 64 + 32) * 4;
        assert_eq!(Preview::default().frame(&tl, 0.5, (64, 36))[mid..mid + 4], [0, 0, 255, 255]);
    }

    // Auto quality renders at the on-screen size in eighths of the timeline and at most 1920 across while
    // playing, a quality written in the script wins, a tiny timeline still gets a pixel and an empty
    // picture draws nothing.
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
        assert_eq!(size(2, 2, "quarter", 100., false), (1, 1));
        let turned = Clip {
            rotate: Val::Fixed(30.),
            ..clip("rect", "")
        };
        let tl = Timeline {
            fps: 30,
            size: (2, 2),
            duration: 1.,
            clips: vec![turned],
            ..Default::default()
        };
        assert!(Preview::default().frame(&tl, 0., (0, 0)).is_empty());
    }

    // A Mac with 8 GB of memory or less should keep half a gigabyte of frames and four decoders, down
    // to a quarter of a gigabyte on smaller ones; a larger Mac keeps up to a gigabyte and six.
    #[test]
    fn limits_follow_the_machine() {
        let sized = [2, 8, 12, 16, 96].map(|gb| fit(gb << 30));
        assert_eq!(sized, [(1 << 28, 4), (1 << 30, 4), (1 << 30, 6), (1 << 30, 6), (1 << 30, 6)]);
        let (room, most) = fit(8 << 30);
        let mut pv = Preview {
            room,
            most,
            ..Default::default()
        };
        for i in 0..9 {
            pv.decoder(&(format!("/none/{i}.mp4"), 64, 36), 0, 0, (30., 1.));
        }
        assert_eq!(pv.decoders.len(), 4);
    }

    // The preview should keep only the text and shape pictures the last frame drew, so a long run of
    // different titles does not keep growing memory, while a picture that stays on screen is drawn
    // once.
    #[test]
    fn pictures_last_one_frame() {
        let flash = (0..40).map(|i| Clip {
            start: i as f64 / 30.,
            end: (i + 1) as f64 / 30.,
            color: format!("#{i:02x}0000"),
            ..clip("rect", "")
        });
        let stay = Clip {
            end: 2.,
            w: 80,
            ..clip("rect", "")
        };
        let tl = Timeline {
            fps: 30,
            size: (200, 100),
            duration: 2.,
            clips: flash.chain([stay]).collect(),
            ..Default::default()
        };
        let mut p = Preview::default();
        let wide = |p: &Preview| p.stills.values().flatten().find(|px| px.len() == 80 * 20 * 4).cloned().unwrap();
        p.frame(&tl, 0., (200, 100));
        let first = wide(&p);
        for i in 1..40 {
            p.frame(&tl, i as f64 / 30., (200, 100));
            assert_eq!((p.stills.len(), p.was.0.len()), (2, 1));
            assert!(Arc::ptr_eq(&first, &wide(&p)));
        }
    }
}
