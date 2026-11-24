use crate::{
    export::{alpha, grown, live, place, plain, seat},
    fx, grade,
    lut::spread,
    shape,
    text::{self, Glyph},
};
use lru::LruCache;
use origami_lang::{Clip, Kind, Part, Timeline, Val, ffmpeg, ffprobe, literal, output, start, watch};
use std::{
    collections::HashMap,
    fs,
    io::{BufReader, Read},
    num::NonZeroUsize,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, LazyLock, Mutex,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
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

const AGAIN: u32 = 300;
static KNOWN: LazyLock<Mutex<HashMap<String, (Info, u32)>>> = LazyLock::new(Mutex::default);
static LANDS: LazyLock<Mutex<HashMap<(String, u64), (f64, u32)>>> = LazyLock::new(Mutex::default);
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

fn lean(path: &str) -> &'static [&'static str] {
    match path.ends_with(".LRF") {
        true => &["-probesize", "32768", "-analyzeduration", "0"],
        false => &[],
    }
}

fn ask(path: &str, show: &str) -> String {
    let args = ["-v", "error", "-select_streams", "v:0", "-show_entries", show, "-of", "csv=p=0"];
    let out = output(ffprobe().args(lean(path)).args(args).args(literal(path)).arg(path)).map(|o| o.stdout);
    String::from_utf8_lossy(&out.unwrap_or_default()).into()
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
        let info = (lead, down, n, d, num(dur), tb, late, start, stamps.into(), ts);
        (info, if text.is_empty() { AGAIN } else { u32::MAX })
    };
    let known = KNOWN.lock().unwrap().get_mut(path).and_then(|(info, left)| {
        *left = left.checked_sub(1)?;
        Some(info.clone())
    });
    known.unwrap_or_else(|| {
        let made = make();
        KNOWN.lock().unwrap().insert(path.into(), made.clone());
        made.0
    })
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
        (at.max(start) + pts.unwrap_or(0.), if pts.is_none() { AGAIN } else { u32::MAX })
    };
    let key = (src.to_string(), at.to_bits());
    let known = LANDS.lock().unwrap().get_mut(&key).and_then(|(found, left)| {
        *left = left.checked_sub(1)?;
        Some(*found)
    });
    known.unwrap_or_else(|| {
        let made = make();
        LANDS.lock().unwrap().insert(key, made);
        made.0
    })
}

pub fn learn(tl: &Timeline, from: f64) {
    let used = tl
        .clips
        .iter()
        .filter(|c| c.end > from && matches!(c.kind, Kind::Video | Kind::Sound));
    study(used.map(|c| c.src.clone()).collect());
}

pub fn warm(tl: &Timeline, from: f64) {
    learn(tl, from);
    let used = tl
        .clips
        .iter()
        .filter(|c| c.end > from && c.kind == Kind::Video && Path::new(&c.src).is_file());
    let mut seeks: Vec<(String, f64, f64)> = used
        .filter_map(|c| match probe(&c.src) {
            (.., late, start, _, false) if late > 0 => Some((c.src.clone(), c.src_in, start)),
            _ => None,
        })
        .filter(|(src, at, _)| !LANDS.lock().unwrap().contains_key(&(src.clone(), at.to_bits())))
        .collect();
    seeks.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    seeks.dedup();
    spread(&mut seeks, 1, |_, part| {
        for (src, at, start) in part.iter() {
            lands(src, *at, *start);
        }
    });
}

fn study(mut new: Vec<String>) {
    new.retain(|f| !KNOWN.lock().unwrap().contains_key(f) && Path::new(f).is_file());
    new.sort();
    new.dedup();
    if new.len() > 1 {
        spread(&mut new, 1, |_, part| part.iter().for_each(|f| drop(probe(f))));
    }
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

fn still(path: &str, w: usize, h: usize) -> Option<Vec<u8>> {
    raw(
        &[literal(path), &["-i", path, "-vf", &format!("scale={w}:{h}")]].concat(),
        w * h * 4,
    )
}

pub fn thumb(path: &str, t: f64, w: usize, h: usize) -> Option<Vec<u8>> {
    let from = |path: &str| {
        let ((lead, down, ..), seek) = (probe(path), ["-skip_frame", "nokey", "-ss", &t.to_string()]);
        let fast = if t > 0. { &seek[..] } else { &[] };
        raw(
            &[lead, fast, literal(path), &["-i", path, "-vf", &format!("{down}scale={w}:{h}")]].concat(),
            w * h * 4,
        )
    };
    let lrf = Path::new(path).with_extension("LRF");
    let side = Some(lrf.to_string_lossy()).filter(|l| l != path && lrf.is_file());
    side.and_then(|l| from(&l)).or_else(|| from(path))
}

const DAY: &str = if cfg!(test) { "60" } else { "86400" };

pub fn peaks(path: &str) -> Vec<f32> {
    let args = ["-v", "error", "-i", path, "-t", DAY, "-ac", "1", "-ar", "4000", "-f", "u8", "-"];
    let made = start(
        ffmpeg()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()),
    );
    let Ok(mut ff) = made else { return vec![] };
    let (seen, mut pcm, mut chunk, mut peaks) = (watch(ff.id()), BufReader::new(ff.stdout.take().unwrap()), vec![], vec![]);
    while let Ok(1..) = pcm.by_ref().take(200).read_to_end(&mut chunk) {
        seen.beat();
        peaks.push(chunk.drain(..).map(|s| (s as f32 - 128.).abs()).fold(0., f32::max));
    }
    drop((seen, pcm));
    let _ = ff.wait();
    let top = peaks.iter().fold(1., |m: f32, &p| m.max(p));
    peaks.into_iter().map(|p| p / top).collect()
}

fn spin((x, y): (f64, f64), k: f64, deg: f64) -> (f64, f64) {
    let (s, c) = deg.to_radians().sin_cos();
    (k * (x * c - y * s), k * (x * s + y * c))
}

fn blit(
    out: &mut [u8],
    (pw, ph): (usize, usize),
    src: &[u8],
    (sw, sh): (usize, usize),
    k: f64,
    deg: f64,
    (cx, cy): (f64, f64),
    a: f64,
    mul: bool,
) {
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
    let uv = |dx: f64, dy: f64| ((dx * cos + dy * sin) / k + hw - 0.5, (dy * cos - dx * sin) / k + hh - 0.5);
    let cols: Vec<f64> = match sin == 0. {
        true => (x0..x1).map(|x| uv(x as f64 + 0.5 - cx, 0.).0).collect(),
        false => vec![],
    };
    let paint = |from: usize, rows: &mut [u8]| {
        for (y, row) in (y0 + from / (pw * 4)..).zip(rows.chunks_exact_mut(pw * 4)) {
            let dy = y as f64 + 0.5 - cy;
            let flat = uv(0., dy).1;
            for (x, d) in (x0..x1).zip(row[x0 * 4..].chunks_exact_mut(4)) {
                let (u, v) = match cols.get(x - x0) {
                    Some(&u) => (u, flat),
                    None => uv(x as f64 + 0.5 - cx, dy),
                };
                let ((ux, vy), mut sum) = ((u.floor(), v.floor()), [0.; 4]);
                let (fx, fy) = (u - ux, v - vy);
                let mut tap = |ox: i64, oy: i64, w: f64| {
                    let (sx, sy) = (ux as i64 + ox, vy as i64 + oy);
                    if sx < 0 || sy < 0 || sx >= sw as i64 || sy >= sh as i64 {
                        return;
                    }
                    let s = &src[(sy as usize * sw + sx as usize) * 4..][..4];
                    let w = w * s[3] as f64 / 255.;
                    for i in 0..3 {
                        sum[i] += s[i] as f64 * w;
                    }
                    sum[3] += w;
                };
                match fx == 0. && fy == 0. {
                    true => tap(0, 0, 1.),
                    false => {
                        for (ox, oy, w) in [
                            (0, 0, (1. - fx) * (1. - fy)),
                            (1, 0, fx * (1. - fy)),
                            (0, 1, (1. - fx) * fy),
                            (1, 1, fx * fy),
                        ] {
                            tap(ox, oy, w);
                        }
                    }
                }
                if sum[3] == 0. {
                    continue;
                }
                for i in 0..3 {
                    let ink = if mul {
                        sum[i] * (d[i] as f64 + 255. - d[3] as f64) / 255.
                    } else {
                        sum[i]
                    };
                    d[i] = (ink * a + d[i] as f64 * (1. - sum[3] * a)) as u8;
                }
                d[3] = (d[3] as f64 + (255. - d[3] as f64) * sum[3] * a) as u8;
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

fn straight(mut px: Vec<u8>) -> Vec<u8> {
    spread(&mut px, 4, |_, part| {
        for p in part.chunks_exact_mut(4).filter(|p| p[3] > 0 && p[3] < 255) {
            for i in 0..3 {
                p[i] = (p[i] as u32 * 255 / p[3] as u32).min(255) as u8;
            }
        }
    });
    px
}

fn nudge(px: &[u8], w: usize, (fx, fy): (f64, f64)) -> Vec<u8> {
    let (mut out, n, cut) = (px.to_vec(), w * 4, |f: f64| (f as usize, (f.fract() * 256.) as u32));
    let ((kx, a), (ky, b), h) = (cut(fx), cut(fy), px.len() / n.max(1));
    if kx + ky == 0 && a + b == 0 {
        return out;
    }
    let mix = |t: u8, u: u8, l: u8, v: u8| {
        let (top, low) = (t as u32 * (256 - a) + u as u32 * a, l as u32 * (256 - a) + v as u32 * a);
        ((top * (256 - b) + low * b) >> 16) as u8
    };
    let band = h.div_ceil(thread::available_parallelism().map_or(1, usize::from)).max(1) * n;
    thread::scope(|s| {
        for (j, rows) in out.chunks_mut(band).enumerate() {
            s.spawn(move || {
                for (y, row) in rows.chunks_exact_mut(n).enumerate() {
                    let [top, low] = [0, 1].map(|d| &px[(j * band / n + y + ky + d).min(h - 1) * n..][..n]);
                    let m = (w - 1).saturating_sub(kx) * 4;
                    let ((head, tail), (i, k)) = (row.split_at_mut(m), (n - 4 - m, n - m));
                    for ((((p, t), u), l), v) in head.iter_mut().zip(&top[i..]).zip(&top[k..]).zip(&low[i..]).zip(&low[k..]) {
                        *p = mix(*t, *u, *l, *v);
                    }
                    for (x, p) in (m / 4..).zip(tail.chunks_exact_mut(4)) {
                        let (i, k) = ((x + kx).min(w - 1) * 4, (x + kx + 1).min(w - 1) * 4);
                        for c in 0..4 {
                            p[c] = mix(top[i + c], top[k + c], low[i + c], low[k + c]);
                        }
                    }
                }
            });
        }
    });
    out
}

fn reach(src: &str) -> i64 {
    if src.ends_with(".LRF") { 20 } else { 30 }
}

fn reuse(spare: &mut Vec<Vec<u8>>, len: usize) -> Vec<u8> {
    match spare.iter().position(|v| v.len() == len) {
        Some(i) => spare.swap_remove(i),
        None => vec![0; len],
    }
}

fn recycle(spare: &mut Vec<Vec<u8>>, give: &Sender<Vec<u8>>, len: usize) {
    if let Some(i) = spare.iter().position(|v| v.len() == len) {
        let _ = give.send(spare.swap_remove(i));
    }
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
    child: Option<Child>,
    next: i64,
    feed: Option<(Receiver<Vec<u8>>, Sender<Vec<u8>>, bool)>,
}

impl Decoder {
    fn feed(&mut self, len: usize, n: i64, back: bool) {
        let Some((pid, mut out)) = self.child.as_mut().and_then(|c| Some((c.id(), c.stdout.take()?))) else {
            return;
        };
        let ((send, feed), (give, take)) = (mpsc::sync_channel(if back { 2 } else { 0 }), mpsc::channel());
        thread::spawn(move || {
            (0..n).try_for_each(|_| {
                let mut px = take.try_recv().unwrap_or_else(|_| vec![0; len]);
                let seen = watch(pid);
                out.read_exact(&mut px).ok()?;
                drop(seen);
                send.send(px).ok()
            })
        });
        self.feed = Some((feed, give, back));
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            thread::spawn(move || child.wait());
        }
    }
}

pub struct Preview {
    decoders: Vec<(Key, Decoder)>,
    frames: LruCache<(Key, i64), Px>,
    held: usize,
    stills: HashMap<String, Option<Px>>,
    texts: HashMap<String, Laid>,
    was: (HashMap<String, Option<Px>>, HashMap<String, Laid>),
    spare: Vec<Vec<u8>>,
    stamps: HashMap<String, (Option<(u64, SystemTime)>, Option<bool>)>,
    at: f64,
    back: bool,
    primed: (Vec<u32>, (usize, usize)),
    pub(crate) room: usize,
    most: usize,
    shown: usize,
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
            spare: vec![],
            stamps: HashMap::new(),
            at: -1.,
            back: false,
            primed: Default::default(),
            room,
            most,
            shown: 0,
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
        }
        .min((8. * 8192. / w.max(h)).floor());
        (((w * n / 8.) as usize).max(1), ((h * n / 8.) as usize).max(1))
    }

    fn cell(c: &Clip, t: f64, fps: u32, (tw, th): (usize, usize), fast: bool) -> (Key, i64, (f64, f64)) {
        let (lrf, fps) = (Path::new(&c.src).with_extension("LRF"), fps as f64);
        let src = if fast && lrf.is_file() {
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
        let read = KNOWN.lock().unwrap().get(src).map(|(_, left)| *left > AGAIN);
        let Some((was, seen)) = self.stamps.insert(src.into(), (now, read)) else {
            return;
        };
        if was != now {
            KNOWN.lock().unwrap().remove(src);
            LANDS.lock().unwrap().retain(|k, _| k.0 != src);
            for stills in [&mut self.stills, &mut self.was.0] {
                stills.retain(|k, _| !k.contains(src));
            }
        } else if (seen, read) != (Some(false), Some(true)) {
            return;
        }
        self.decoders.retain(|(k, _)| k.0 != src);
        let old: Vec<_> = self.frames.iter().map(|(k, _)| k.clone()).filter(|k| k.0.0 == src).collect();
        for k in old {
            self.held -= self.frames.pop(&k).map_or(0, |px| px.len());
        }
    }

    fn near(&self, key: &Key, k: i64) -> Option<usize> {
        self.decoders
            .iter()
            .position(|(o, d)| o == key && d.next <= k && k <= d.next + reach(&key.0))
    }

    pub fn ready(&mut self, tl: &Timeline, t: f64, (pw, ph): (usize, usize)) -> bool {
        let (k, shown) = (pw as f64 / tl.size.0 as f64, |c: &&Clip| {
            c.kind == Kind::Video && c.video && live(c, t)
        });
        let lrf = |c: &Clip| Path::new(&c.src).with_extension("LRF").to_string_lossy().into_owned();
        study(tl.clips.iter().filter(shown).flat_map(|c| [c.src.clone(), lrf(c)]).collect());
        self.shown = tl.clips.iter().filter(shown).count() + 1;
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
        let old: Vec<u32> = self.decoders.iter().flat_map(|(_, d)| d.child.as_ref().map(Child::id)).collect();
        self.at = f64::NAN;
        self.ready(tl, t, size);
        let ids = self.decoders.iter().flat_map(|(_, d)| d.child.as_ref().map(Child::id));
        self.primed = (ids.filter(|n| !old.contains(n)).collect(), size);
    }

    fn block(&self, key: &Key) -> i64 {
        (self.room / 4 / (key.1 * key.2 * 4).max(1)).min(45) as i64
    }

    fn lap(&self, key: &Key, k: i64) -> i64 {
        let back = self.decoders.iter().any(|(o, d)| o == key && k < d.next && d.next <= k + 60);
        if back { self.block(key) } else { 0 }
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
                let child = origami_lang::start(
                    ffmpeg()
                        .args(["-v", "error"])
                        .args(lead)
                        .args(lean(&key.0))
                        .args(args)
                        .args(out)
                        .stdin(Stdio::null())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null()),
                )
                .ok()?;
                self.decoders
                    .drain(..self.decoders.len().saturating_sub(self.most.max(self.shown) - 1));
                (
                    key.clone(),
                    Decoder {
                        child: Some(child),
                        next: from,
                        feed: None,
                    },
                )
            }
        };
        self.decoders.push(found);
        Some(())
    }

    fn evict(&mut self, len: usize) {
        let feeds = self.decoders.iter().filter(|(_, d)| d.feed.as_ref().is_some_and(|f| f.2));
        let flight: usize = feeds.map(|(k, _)| 3 * k.1 * k.2 * 4).sum();
        while self.held + flight + len > self.room
            && let Some(old) = self.frames.pop_lru()
        {
            self.held -= old.1.len();
            self.spare.extend(Arc::try_unwrap(old.1));
        }
    }

    fn keep(&mut self, at: (Key, i64), px: Vec<u8>) {
        self.evict(px.len());
        self.held += px.len();
        self.held -= self.frames.push(at, Arc::new(px)).map_or(0, |f| f.1.len());
    }

    fn drain(&mut self) {
        for i in (0..self.decoders.len()).rev() {
            while let Some((feed, _, true)) = &self.decoders[i].1.feed {
                match feed.try_recv() {
                    Ok(px) => {
                        let len = px.len();
                        self.keep((self.decoders[i].0.clone(), self.decoders[i].1.next), px);
                        if let Some((_, give, _)) = &self.decoders[i].1.feed {
                            recycle(&mut self.spare, give, len);
                        }
                        self.decoders[i].1.next += 1;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.decoders.remove(i);
                        break;
                    }
                }
            }
        }
    }

    fn ahead(&mut self, key: &Key, k: i64, rate: (f64, f64)) {
        let (block, len, mut low) = (self.block(key), key.1 * key.2 * 4, k);
        while low > 0 && k - low < block && self.frames.contains(&(key.clone(), low - 1)) {
            low -= 1;
        }
        if low == 0 || self.frames.contains(&(key.clone(), low - 1)) || self.near(key, low - 1).is_some() {
            return;
        }
        let from = (low - reach(&key.0).min(block)).max(0);
        if self.decoder(key, low - 1, from, rate).is_some()
            && self.back
            && let Some((_, d)) = self.decoders.last_mut()
        {
            d.feed(len, low - from, true);
        }
    }

    fn video(&mut self, key: Key, k: i64, rate: (f64, f64)) -> Option<Px> {
        self.drain();
        let (hit, lap) = (self.frames.get(&(key.clone(), k)).cloned(), self.lap(&key, k));
        let missed = hit.is_none();
        let px = hit.or_else(|| self.read(&key, k, lap, rate));
        if self.back || missed && lap > 0 {
            self.ahead(&key, k, rate);
        }
        px
    }

    fn read(&mut self, key: &Key, k: i64, lap: i64, rate: (f64, f64)) -> Option<Px> {
        let len = key.1 * key.2 * 4;
        self.decoder(key, k, (k - lap).max(0), rate)?;
        let (_, mut d) = self.decoders.pop()?;
        let mut again = true;
        while d.next <= k {
            if d.feed.is_none() {
                d.feed(len, i64::MAX, false);
            }
            self.evict(len);
            if let Some((_, give, _)) = &d.feed {
                recycle(&mut self.spare, give, len);
            }
            let Some(px) = d.feed.as_ref().and_then(|f| f.0.recv().ok()) else {
                let died = d.feed.as_ref().is_some_and(|f| f.2) || d.child.take().is_some_and(|mut c| c.wait().is_ok_and(|s| !s.success()));
                if died && again && self.decoder(key, k, d.next, rate).is_some() {
                    (again, d) = (false, self.decoders.pop()?.1);
                    continue;
                }
                break;
            };
            self.keep((key.clone(), d.next), px);
            d.next += 1;
        }
        self.decoders.push((key.clone(), d));
        self.frames.get(&(key.clone(), k)).cloned()
    }

    fn layer(&mut self, c: &Clip, t: f64, tl: &Timeline, size: (usize, usize), k: f64) -> Option<(Px, (usize, usize), (i64, i64))> {
        let (tw, th, x, y) = place(c, (size.0 as f64, size.1 as f64), k);
        let (tw, th, fps) = (tw as usize, th as usize, tl.fps);
        let i = ((t - c.start) * fps as f64).round() as usize;
        if c.kind == Kind::Group {
            let mut px = vec![0; size.0 * size.1 * 4];
            self.draw(&mut px, tl, t, size, k, c.id);
            let px = Arc::new(straight(px));
            let px = c.grade.as_ref().map_or(px.clone(), |g| grade::paint(g, px, size, vec![]));
            return Some((fx::paint(&fx::now(&c.fx, i), px, size, k), size, (x as i64, y as i64)));
        }
        let now = fx::now(&c.fx, i);
        let (fixed, done) = (!now.is_empty() && now == c.fx && c.kind != Kind::Video, |base: &str| {
            format!("{base}|{:?}|{k}|{now:?}", c.grade)
        });
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
            _ => {
                let base = match c.kind {
                    Kind::Image => {
                        self.renew(&c.src);
                        format!("{}|{tw}x{th}", c.src)
                    }
                    _ => format!(
                        "{}|{}|{}|{}|{}|{}|{:?}|{:?}|{tw}x{th}",
                        c.src,
                        c.color,
                        c.font,
                        c.background,
                        c.align,
                        c.radius,
                        c.ends,
                        shape::outline(c, i)
                    ),
                };
                if fixed && [&self.stills, &self.was.0].iter().any(|m| m.contains_key(&done(&base))) {
                    let px = kept(&mut self.stills, &mut self.was.0, done(&base), || None)?;
                    return (px.len() >= tw * th * 4).then_some((px, (tw, th), (x as i64, y as i64)));
                }
                let make = || match c.kind {
                    Kind::Image => still(&c.src, tw, th).map(Arc::new),
                    _ => Some(Arc::new(shape::image(c, (size.0 as f64, size.1 as f64), k, i).0)),
                };
                (kept(&mut self.stills, &mut self.was.0, base.clone(), make)?, base)
            }
        };
        let graded = c.grade.as_ref().map(|g| (g, format!("{base}|{g:?}{:?}", grade::files(g))));
        let px = match &graded {
            Some((g, key)) => kept(&mut self.stills, &mut self.was.0, key.clone(), || {
                Some(grade::paint(g, px, (tw, th), reuse(&mut self.spare, tw * th * 4)))
            })?,
            None => px,
        };
        let px = match (now.is_empty() || px.len() < tw * th * 4, c.kind != Kind::Video || c.step > 0.) {
            (true, _) => px,
            (false, true) => {
                let px = kept(&mut self.stills, &mut self.was.0, done(&base), || {
                    Some(fx::paint(&now, px, (tw, th), k))
                })?;
                if fixed {
                    self.stills.remove(&base);
                    if let Some((_, key)) = &graded {
                        self.stills.remove(key);
                    }
                }
                px
            }
            (false, false) => fx::paint(&now, px, (tw, th), k),
        };
        (px.len() >= tw * th * 4).then_some((px, (tw, th), (x as i64, y as i64)))
    }

    fn moved(&mut self, out: &mut [u8], c: &Clip, t: f64, tl: &Timeline, (pw, ph): (usize, usize), k: f64, i: usize, a: f64) {
        let ((w, h), q, turn) = ((pw as f64, ph as f64), grown(c, i), c.rotate.at(i));
        let mul = c.blend == "multiply";
        let mid = |tw, th| seat(c, (w, h), (tw as f64, th as f64), i);
        if c.kind != Kind::Text || c.parts.is_empty() && c.shown == Val::Fixed(1.) {
            if let Some((px, (sw, sh), _)) = self.layer(c, t, tl, (pw, ph), k) {
                blit(out, (pw, ph), &px, (sw, sh), q, turn, mid(sw, sh), a, mul);
            }
            return;
        }
        if !c.fx.is_empty() {
            let mut own = vec![0; out.len()];
            let bare = Clip {
                fx: vec![],
                blend: String::new(),
                ..c.clone()
            };
            self.moved(&mut own, &bare, t, tl, (pw, ph), k, i, 1.);
            let px = fx::paint(&fx::now(&c.fx, i), Arc::new(straight(own)), (pw, ph), k);
            return blit(out, (pw, ph), &px, (pw, ph), 1., 0., (w / 2., h / 2.), a, mul);
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
            g.px = c.grade.as_ref().map_or(g.px.clone(), |x| grade::paint(x, g.px, (g.w, g.h), vec![]));
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
            let plate = c
                .grade
                .as_ref()
                .map_or(plate.clone(), |g| grade::paint(g, plate, (*tw, *th), vec![]));
            blit(out, (pw, ph), &plate, (*tw, *th), q, turn, mid, a, mul);
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
                mul,
            );
        }
    }

    fn draw(&mut self, out: &mut [u8], tl: &Timeline, t: f64, (pw, ph): (usize, usize), k: f64, into: u32) {
        let drawn = |c: &&Clip| c.into == into && c.scale.max() > 0. && (c.kind != Kind::Text || c.size > 0.);
        for c in tl.clips.iter().filter(|c| c.video && live(c, t)).filter(drawn) {
            let Some((id, how)) = &c.matte else {
                self.one(out, c, tl, t, (pw, ph), k);
                continue;
            };
            let (mut own, mut cut) = (vec![0; out.len()], vec![0; out.len()]);
            self.one(&mut own, c, tl, t, (pw, ph), k);
            self.draw(&mut cut, tl, t, (pw, ph), k, *id);
            let (mul, luma, flip) = (c.blend == "multiply", how.starts_with("luma"), how.ends_with("Inverted"));
            spread(out, 4, |from, part| {
                let pairs = own[from..].chunks_exact(4).zip(cut[from..].chunks_exact(4));
                for (d, (s, m)) in part.chunks_exact_mut(4).zip(pairs).filter(|x| x.1.0[3] > 0) {
                    let key = match luma {
                        true => 0.0722 * m[0] as f64 + 0.7152 * m[1] as f64 + 0.2126 * m[2] as f64,
                        false => m[3] as f64,
                    };
                    let key = if flip { 255. - key } else { key } / 255.;
                    let (sa, back) = (s[3] as f64 * key / 255., 255. - d[3] as f64);
                    for i in 0..3 {
                        let ink = if mul {
                            s[i] as f64 * (d[i] as f64 + back) / 255.
                        } else {
                            s[i] as f64
                        };
                        d[i] = (ink * key + d[i] as f64 * (1. - sa)) as u8;
                    }
                    d[3] = (d[3] as f64 + back * sa) as u8;
                }
            });
        }
    }

    fn one(&mut self, out: &mut [u8], c: &Clip, tl: &Timeline, t: f64, (pw, ph): (usize, usize), k: f64) {
        let t = if c.step > 0. {
            c.start + ((t - c.start) * c.step + 1e-6).floor() / c.step
        } else {
            t
        };
        let i = ((t - c.start) * tl.fps as f64).round() as usize;
        let (a, solid) = ((alpha(c, t) * c.opacity.at(i)).clamp(0., 1.), c.kind == Kind::Video);
        if !plain(c) {
            self.moved(out, c, t, tl, (pw, ph), k, i, a);
            return;
        }
        let Some((px, (sw, sh), (x, y))) = self.layer(c, t, tl, (pw, ph), k) else {
            return;
        };
        let (x0, x1) = ((-x).max(0) as usize, (pw as i64 - x).clamp(0, sw as i64) as usize);
        let (y0, y1) = (y.clamp(0, ph as i64) as usize, (y + sh as i64).clamp(0, ph as i64) as usize);
        let paint = |from: usize, rows: &mut [u8]| {
            for (sy, row) in ((y0 + from / (pw * 4)) as i64 - y..).zip(rows.chunks_exact_mut(pw * 4)) {
                let dst = &mut row[(x + x0 as i64) as usize * 4..][..(x1 - x0) * 4];
                let src = &px[(sy as usize * sw + x0) * 4..][..dst.len()];
                if solid && a >= 1. {
                    dst.copy_from_slice(src);
                    continue;
                }
                for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                    let sa = a * if solid { 1. } else { s[3] as f64 / 255. };
                    for i in 0..3 {
                        d[i] = (s[i] as f64 * sa + d[i] as f64 * (1. - sa)) as u8;
                    }
                    d[3] = (d[3] as f64 + (255. - d[3] as f64) * sa) as u8;
                }
            }
        };
        match x1.saturating_sub(x0) * (y1 - y0) {
            0 => {}
            n if n < 1 << 16 || solid && a >= 1. => paint(0, &mut out[y0 * pw * 4..y1 * pw * 4]),
            _ => spread(&mut out[y0 * pw * 4..y1 * pw * 4], pw * 4, paint),
        }
    }

    pub fn frame(&mut self, tl: &Timeline, t: f64, (pw, ph): (usize, usize)) -> Vec<u8> {
        let k = pw as f64 / tl.size.0 as f64;
        let (primed, size) = std::mem::take(&mut self.primed);
        if self.fast || size != (pw, ph) {
            self.decoders
                .retain(|(_, d)| d.child.as_ref().is_none_or(|c| !primed.contains(&c.id())));
        }
        let mut out = [0, 0, 0, 255].repeat(pw * ph);
        (self.rough, self.back) = (false, !self.fast && t < self.at && self.at - t < 5. / tl.fps as f64);
        self.shown = tl.clips.iter().filter(|c| c.kind == Kind::Video && c.video && live(c, t)).count() + 1;
        self.at = t;
        let was = std::mem::replace(&mut self.was, (std::mem::take(&mut self.stills), std::mem::take(&mut self.texts)));
        self.spare
            .extend(was.0.into_values().flatten().filter_map(|px| Arc::try_unwrap(px).ok()));
        self.draw(&mut out, tl, t, (pw, ph), k, 0);
        let soon = |c: &&Clip| c.kind == Kind::Video && t < c.start && c.start < t + 1.;
        let next = tl.clips.iter().filter(soon).map(|c| c.start).fold(f64::MAX, f64::min);
        for c in tl.clips.iter().filter(soon).filter(|c| c.start == next) {
            let (tw, th, ..) = place(c, (pw as f64, ph as f64), k);
            let (key, first, rate) = Self::cell(c, c.start, tl.fps, (tw as usize, th as usize), self.fast);
            if !self.frames.contains(&(key.clone(), first))
                && self.decoder(&key, first, first, rate).is_some()
                && let Some((_, d)) = self.decoders.last_mut()
                && d.feed.is_none()
            {
                d.feed(key.1 * key.2 * 4, i64::MAX, false);
            }
        }
        let (fps, back) = (tl.fps as f64, self.back);
        let gone = |c: &&Clip| back && c.kind == Kind::Video && c.video && c.scale.max() > 0. && c.end <= t + 1e-6 && t - 1. < c.end;
        let last = tl.clips.iter().filter(gone).map(|c| c.end).fold(f64::MIN, f64::max);
        for c in tl.clips.iter().filter(gone).filter(|c| c.end == last) {
            let (tw, th, ..) = place(c, (pw as f64, ph as f64), k);
            let at = (((c.end - 1e-6) * fps).ceil() - 1.) / fps;
            let (key, end, rate) = Self::cell(c, at, tl.fps, (tw as usize, th as usize), false);
            self.ahead(&key, end + 1, rate);
        }
        self.spare.clear();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        export::hold,
        tests::{TIDY, clip, film, scratch},
    };

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

    // While scrubbing, a sidecar that cannot be read, is a playlist or a named pipe, or ends before the
    // full file does should give way to the full file at once instead of leaving the clip out.
    #[test]
    fn broken_sidecars_fall_back() {
        let dir = scratch("sidecar");
        for name in ["text", "short", "list", "pipe"] {
            film(&dir.join(format!("{name}.mp4")), "color=red:s=64x36:r=30:d=4", &[]);
        }
        film(&dir.join("a.ts"), "color=blue:s=64x36:r=30:d=1", &[]);
        fs::write(dir.join("live.m3u8"), "#EXTM3U\n#EXT-X-TARGETDURATION:100000\n#EXTINF:1,\na.ts\n").unwrap();
        fs::write(dir.join("text.LRF"), "not a video").unwrap();
        fs::write(dir.join("list.LRF"), "ffconcat version 1.0\nfile live.m3u8\n").unwrap();
        film(&dir.join("short.LRF"), "color=blue:s=64x36:r=30:d=1", &["-f", "mp4"]);
        let made = origami_lang::status(Command::new("mkfifo").arg(dir.join("pipe.LRF")));
        assert!(made.unwrap().success());
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
        let from = std::time::Instant::now();
        assert!([seen("list", 2.), seen("pipe", 2.)] == [(true, false); 2] && from.elapsed().as_secs() < 2);
        assert_eq!(
            [seen("text", 2.), seen("short", 0.5), seen("short", 2.)],
            [(true, false), (false, true), (true, false)]
        );
    }

    // A clip's timeline thumbnail comes from the .LRF sidecar beside it when that file has a picture at
    // that time, and from the clip itself when the sidecar is unreadable or ends sooner.
    #[test]
    fn thumbnails_read_sidecars() {
        let dir = scratch("thumbs");
        for name in ["good", "text", "short"] {
            film(&dir.join(format!("{name}.mp4")), "color=red:s=64x36:r=30:d=4", &["-g", "15"]);
        }
        film(&dir.join("good.LRF"), "color=blue:s=64x36:r=30:d=4", &["-g", "15", "-f", "mp4"]);
        film(&dir.join("short.LRF"), "color=blue:s=64x36:r=30:d=1", &["-g", "15", "-f", "mp4"]);
        fs::write(dir.join("text.LRF"), "not a video").unwrap();
        let blue = |name: &str, t: f64| {
            let px = thumb(&dir.join(name).to_string_lossy(), t, 16, 9).unwrap();
            (px[(4 * 16 + 8) * 4] > 128, px[(4 * 16 + 8) * 4 + 2] > 128)
        };
        assert_eq!(
            [
                blue("good.mp4", 2.),
                blue("text.mp4", 2.),
                blue("short.mp4", 0.5),
                blue("short.mp4", 2.)
            ],
            [(true, false), (false, true), (true, false), (false, true)]
        );
    }

    // A JPEG still, which ffmpeg gives no picture for when told to seek before reading it, draws in
    // the preview and as a timeline thumbnail.
    #[test]
    fn jpegs_draw() {
        let file = scratch("jpeg").join("bars.jpg");
        film(&file, "testsrc=s=640x360", &["-frames:v", "1"]);
        let path = file.to_string_lossy();
        assert!(still(&path, 16, 9).is_some() && thumb(&path, 0., 16, 9).is_some());
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

    // With more videos on screen than the usual number of decoders, each keeps its own decoder from frame
    // to frame instead of starting a new ffmpeg for every layer of every frame.
    #[test]
    fn every_video_on_screen_keeps_its_decoder() {
        let file = scratch("wall").join("a.mp4");
        film(&file, "testsrc=s=64x36:r=30:d=1", &[]);
        let n = Preview::default().most + 2;
        let layer = |i: usize| Clip {
            src: file.to_string_lossy().into(),
            w: 64,
            h: 36,
            scale: Val::Fixed(1. - i as f64 * 0.1),
            ..clip("video", "")
        };
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 1.,
            clips: (0..n).map(layer).collect(),
            ..Default::default()
        };
        let mut pv = Preview::default();
        let ids = |pv: &Preview| {
            let mut ids: Vec<u32> = pv.decoders.iter().flat_map(|(_, d)| d.child.as_ref().map(Child::id)).collect();
            ids.sort();
            ids
        };
        pv.frame(&tl, 0., (64, 36));
        let first = ids(&pv);
        for k in 1..20 {
            pv.frame(&tl, k as f64 / 30., (64, 36));
        }
        assert_eq!((first.len(), ids(&pv)), (n, first));
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

    // Stepping backwards should read the frames before the kept ones in the background while those are
    // shown, so they are kept before a step reaches them, and show the same pictures as playing forwards.
    #[test]
    fn stepping_back_reads_ahead() {
        let file = scratch("back").join("a.mp4");
        film(&file, "testsrc=s=64x36:r=30:d=4", &[]);
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 4.,
            clips: vec![Clip {
                src: file.to_string_lossy().into(),
                w: 64,
                h: 36,
                end: 4.,
                ..clip("video", "")
            }],
            ..Default::default()
        };
        let (mut back, mut played) = (Preview::default(), Preview::default());
        let shown = |pv: &mut Preview, k: i64| pv.frame(&tl, k as f64 / 30., (64, 36));
        let mut stepped: Vec<_> = (60..=100).rev().map(|k| shown(&mut back, k)).collect();
        let key = (file.to_string_lossy().into_owned(), 64, 36);
        let kept = (0..500).any(|_| {
            thread::sleep(std::time::Duration::from_millis(10));
            shown(&mut back, 60);
            (30..60).all(|k| back.frames.contains(&(key.clone(), k)))
        });
        stepped.extend((0..60).rev().map(|k| shown(&mut back, k)));
        stepped.reverse();
        assert!(kept);
        assert!(stepped == (0..=100).map(|k| shown(&mut played, k)).collect::<Vec<_>>());
    }

    // Stepping backwards toward a cut should read the last frames of the clip before it in the background,
    // so crossing the cut finds them kept, and show the same pictures as playing forwards.
    #[test]
    fn stepping_back_reads_across_cuts() {
        let dir = scratch("cut");
        let files = [dir.join("a.mp4"), dir.join("b.mp4")];
        for f in &files {
            film(f, "testsrc=s=64x36:r=30:d=2", &[]);
        }
        let part = |i: usize| Clip {
            src: files[i].to_string_lossy().into(),
            w: 64,
            h: 36,
            start: 2. * i as f64,
            end: 2. * i as f64 + 2.,
            ..clip("video", "")
        };
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 4.,
            clips: vec![part(0), part(1)],
            ..Default::default()
        };
        let (mut back, mut played) = (Preview::default(), Preview::default());
        let shown = |pv: &mut Preview, k: i64| pv.frame(&tl, k as f64 / 30., (64, 36));
        let mut stepped: Vec<_> = (70..=80).rev().map(|k| shown(&mut back, k)).collect();
        let key = (files[0].to_string_lossy().into_owned(), 64, 36);
        let kept = (0..500).any(|_| {
            thread::sleep(std::time::Duration::from_millis(10));
            shown(&mut back, 70);
            (40..60).all(|k| back.frames.contains(&(key.clone(), k)))
        });
        stepped.extend((0..70).rev().map(|k| shown(&mut back, k)));
        stepped.reverse();
        assert!(kept);
        assert!(stepped == (0..=80).map(|k| shown(&mut played, k)).collect::<Vec<_>>());
    }

    // A decoder whose ffmpeg dies while playing should be replaced at once, so the frames after it show
    // the same pictures as an undisturbed run.
    #[test]
    fn dead_decoders_are_replaced() {
        let file = scratch("dead").join("a.mp4");
        film(&file, "testsrc=s=64x36:r=30:d=4", &[]);
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 4.,
            clips: vec![Clip {
                src: file.to_string_lossy().into(),
                w: 64,
                h: 36,
                end: 4.,
                ..clip("video", "")
            }],
            ..Default::default()
        };
        let (mut cut, mut played) = (Preview::default(), Preview::default());
        let shown = |pv: &mut Preview, k: i64| pv.frame(&tl, k as f64 / 30., (64, 36));
        let mut seen: Vec<_> = (0..40).map(|k| shown(&mut cut, k)).collect();
        for c in cut.decoders.iter_mut().flat_map(|(_, d)| d.child.as_mut()) {
            c.kill().unwrap();
        }
        seen.extend((40..90).map(|k| shown(&mut cut, k)));
        assert!(seen == (0..90).map(|k| shown(&mut played, k)).collect::<Vec<_>>());
    }

    // Playing forwards should read each video's next frame from ffmpeg while the current one is drawn, so
    // it is in hand even if ffmpeg stops right after, and show the same pictures as a fresh preview.
    #[test]
    fn playing_reads_ahead() {
        let file = scratch("ahead").join("a.mp4");
        film(&file, "testsrc=s=256x144:r=30:d=1", &[]);
        let tl = Timeline {
            fps: 30,
            size: (256, 144),
            duration: 1.,
            clips: vec![Clip {
                src: file.to_string_lossy().into(),
                w: 256,
                h: 144,
                end: 1.,
                ..clip("video", "")
            }],
            ..Default::default()
        };
        let mut pv = Preview::default();
        let ids = |pv: &Preview| {
            pv.decoders
                .iter()
                .flat_map(|(_, d)| d.child.as_ref().map(Child::id))
                .collect::<Vec<_>>()
        };
        pv.frame(&tl, 0., (256, 144));
        let first = ids(&pv);
        thread::sleep(std::time::Duration::from_millis(500));
        for c in pv.decoders.iter_mut().flat_map(|(_, d)| d.child.as_mut()) {
            c.kill().unwrap();
        }
        let next = pv.frame(&tl, 1. / 30., (256, 144));
        assert_eq!((first.len(), ids(&pv)), (1, first));
        assert!(next == Preview::default().frame(&tl, 1. / 30., (256, 144)));
    }

    // Frames past the end of a file should be left to the decoder that reached it, without starting a
    // new ffmpeg for each one (a new one would read the longer file now under that name).
    #[test]
    fn file_ends_start_no_decoders() {
        let file = scratch("end").join("a.mp4");
        film(&file, "testsrc=s=64x36:r=30:d=1", &[]);
        let (mut pv, key) = (Preview::default(), (file.to_string_lossy().into_owned(), 64, 36));
        let mut shown = |ks: std::ops::Range<i64>| ks.filter(|k| pv.video(key.clone(), *k, (30., 1.)).is_some()).count();
        assert_eq!(shown(20..31), 10);
        film(&file, "testsrc=s=64x36:r=30:d=3", &[]);
        assert_eq!(shown(31..61), 0);
    }

    // A sound's waveform covers at most a day of it, however few samples a second it has.
    #[test]
    fn waves_stop_after_a_day() {
        let file = scratch("slow").join("a.wav");
        film(&file, "aevalsrc=0.5*sin(t):s=1:d=120", &["-c:a", "pcm_u8"]);
        assert_eq!(peaks(&file.to_string_lossy()).len(), DAY.parse::<usize>().unwrap() * 20);
    }

    // Warming a timeline works out, all at once and before any frame is drawn, the first frame of every clip
    // of footage with B-frames, so drawing never stops to start an ffmpeg to find one.
    #[test]
    fn warming_finds_every_first_frame() {
        let file = scratch("warm").join("a.mp4");
        film(&file, "testsrc=s=64x36:r=30:d=2", &["-bf", "2"]);
        let path = file.to_string_lossy().into_owned();
        let clips: Vec<Clip> = (0..6)
            .map(|i| Clip {
                src: path.clone(),
                w: 64,
                h: 36,
                start: i as f64 * 0.2,
                end: (i + 1) as f64 * 0.2,
                src_in: 0.1 + i as f64 * 0.13,
                ..clip("video", "")
            })
            .collect();
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 1.2,
            clips,
            ..Default::default()
        };
        let found = |tl: &Timeline| {
            let lands = LANDS.lock().unwrap();
            tl.clips
                .iter()
                .filter(|c| lands.contains_key(&(path.clone(), c.src_in.to_bits())))
                .count()
        };
        assert!(probe(&path).6 > 0 && found(&tl) == 0);
        warm(&tl, 0.);
        assert_eq!(found(&tl), 6);
    }

    // A file ffprobe could not read should be asked about again after a few hundred lookups rather
    // than on every one, so it is described properly once it can be read.
    #[test]
    fn failed_probes_are_asked_again() {
        let file = scratch("unread").join("a.mp4");
        fs::write(&file, "not a film").unwrap();
        let path = file.to_string_lossy();
        assert_eq!(probe(&path).2, 0.);
        film(&file, "testsrc=s=64x36:r=30:d=1", &[]);
        let rates: Vec<f64> = (0..301).map(|_| probe(&path).2).collect();
        assert!(rates[..300].iter().all(|n| *n == 0.) && rates[300] == 30.);
    }

    // A first frame ffmpeg could not find should be looked for again after a few hundred lookups rather
    // than on every one, so it is found once the file can be read.
    #[test]
    fn failed_lands_are_asked_again() {
        let file = scratch("unlanded").join("a.mp4");
        fs::write(&file, "not a film").unwrap();
        let path = file.to_string_lossy();
        assert_eq!(lands(&path, 0.51, 0.), 0.51);
        film(&file, "testsrc=s=64x36:r=30:d=1", &[]);
        let found: Vec<f64> = (0..301).map(|_| lands(&path, 0.51, 0.)).collect();
        assert!(found[..300].iter().all(|t| *t == 0.51) && found[300] > 0.52);
    }

    // Frames shown while a file's probe was failing should be dropped once it reads the file, so the
    // preview shows what a fresh one shows.
    #[test]
    fn recovered_probes_drop_old_frames() {
        let file = scratch("healed").join("a.mp4");
        fs::write(&file, "not a film").unwrap();
        let path = file.to_string_lossy().into_owned();
        assert_eq!(probe(&path).2, 0.);
        film(
            &file,
            "color=black:s=64x36:r=25:d=2,format=yuv444p,geq=lum='16+4*N':cb=128:cr=128",
            &[],
        );
        let tl = Timeline {
            fps: 30,
            size: (64, 36),
            duration: 2.,
            clips: vec![Clip {
                src: path.clone(),
                w: 64,
                h: 36,
                end: 2.,
                ..clip("video", "")
            }],
            ..Default::default()
        };
        let shown = |pv: &mut Preview| (0..30).map(|k| pv.frame(&tl, k as f64 / 30., (64, 36))).collect::<Vec<_>>();
        let mut pv = Preview::default();
        shown(&mut pv);
        (0..301).for_each(|_| drop(probe(&path)));
        assert!(shown(&mut pv) == shown(&mut Preview::default()));
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
    // playing, a quality written in the script wins up to 8192 pixels on the long side, a tiny timeline
    // still gets a pixel and an empty picture draws nothing.
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
        assert_eq!(
            [
                (7680, 4320, "full"),
                (8192, 4320, "full"),
                (16384, 16384, "full"),
                (16384, 2, "half"),
                (16384, 9000, "")
            ]
            .map(|(w, h, q)| size(w, h, q, 20000., true)),
            [(7680, 4320), (8192, 4320), (8192, 8192), (8192, 1), (8192, 4500)]
        );
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

    // An effect on a still is worked out once while its settings stay the same and again when its seed
    // changes, and every frame shows the same pixels as a fresh preview.
    #[test]
    fn effects_on_stills_are_kept() {
        let fx = r#"[{"roughen":{"border":4,"scale":20,"sharp":1,"seed":[1,[0,0,1,1]]}},{"blur":2}]"#;
        let tl = Timeline {
            fps: 4,
            size: (200, 100),
            duration: 1.,
            clips: vec![Clip {
                fx: serde_json::from_str(fx).unwrap(),
                ..clip("rect", "")
            }],
            ..Default::default()
        };
        let mut pv = Preview::default();
        let mut made = vec![];
        for k in 0..4 {
            let px = pv.frame(&tl, k as f64 / 4., (200, 100));
            assert!(px == Preview::default().frame(&tl, k as f64 / 4., (200, 100)));
            made.extend(pv.stills.iter().filter(|(k, _)| k.contains("Roughen")).flat_map(|(_, v)| v.clone()));
        }
        let same: Vec<bool> = made.windows(2).map(|w| Arc::ptr_eq(&w[0], &w[1])).collect();
        assert_eq!((made.len(), same), (4, vec![true, false, true]));
    }

    // A picture effect on text drawn letter by letter, as each and shown draw it, changes its letters as it
    // does on text drawn whole instead of being left out.
    #[test]
    fn letters_take_effects() {
        let ink = |fx: &str, shown: f64| {
            let tl = Timeline {
                fps: 4,
                size: (200, 100),
                duration: 1.,
                clips: vec![Clip {
                    src: "HH".into(),
                    size: 60.,
                    color: "#ffffff".into(),
                    shown: Val::Fixed(shown),
                    fx: serde_json::from_str(fx).unwrap(),
                    ..clip("text", "")
                }],
                ..Default::default()
            };
            let px = Preview::default().frame(&tl, 0., (200, 100));
            px.chunks(4).filter(|p| p[0] > 250 && p[1] > 250 && p[2] > 250).count()
        };
        let grey = r#"[{"fractal":{"scale":20,"seed":0}}]"#;
        let (whole, letters) = (ink("[]", 1.), ink("[]", 0.99));
        assert!(whole > 400 && letters > 400, "{whole} {letters}");
        assert!(ink(grey, 1.) < whole / 8 && ink(grey, 0.99) < letters / 8);
    }

    // A still whose effect never changes keeps only its finished picture from frame to frame, not the
    // picture and grade it was made from, and shows the same pixels as a fresh preview.
    #[test]
    fn fixed_effects_keep_only_the_result() {
        let tl = Timeline {
            fps: 4,
            size: (200, 100),
            duration: 1.,
            clips: vec![Clip {
                fx: serde_json::from_str(r#"[{"roughen":{"border":4,"scale":20,"sharp":1,"seed":2}}]"#).unwrap(),
                grade: serde_json::from_str(r#"{"exposure":1}"#).unwrap(),
                ..clip("rect", "")
            }],
            ..Default::default()
        };
        let mut pv = Preview::default();
        let mut made = vec![];
        for k in 0..3 {
            assert!(pv.frame(&tl, k as f64 / 4., (200, 100)) == Preview::default().frame(&tl, k as f64 / 4., (200, 100)));
            made.extend(pv.stills.values().flatten().cloned());
        }
        assert!(made.len() == 3 && made.windows(2).all(|w| Arc::ptr_eq(&w[0], &w[1])));
    }

    // An effect on a video is kept from frame to frame only while a clip stepped to a lower rate holds its
    // picture, so a playing video holds no extra frame, and every frame matches a fresh preview.
    #[test]
    fn effects_on_video_are_kept_when_stepped() {
        let file = scratch("fxvideo").join("a.mp4");
        film(&file, "testsrc=s=64x36:r=30:d=1", &[]);
        let kept = |step: f64| {
            let c = Clip {
                src: file.to_string_lossy().into(),
                w: 64,
                h: 36,
                step,
                fx: serde_json::from_str(r#"[{"blur":2}]"#).unwrap(),
                ..clip("video", "")
            };
            let tl = Timeline {
                fps: 30,
                size: (64, 36),
                duration: 1.,
                clips: vec![c],
                ..Default::default()
            };
            let mut pv = Preview::default();
            let same = (0..6).all(|k| pv.frame(&tl, k as f64 / 30., (64, 36)) == Preview::default().frame(&tl, k as f64 / 30., (64, 36)));
            (same, pv.stills.keys().any(|k| k.contains("Blur")))
        };
        assert_eq!([kept(0.), kept(10.)], [(true, false), (true, true)]);
    }

    // A layer drawn on whole pixels shows its own pixels, blends its half-clear ones, leaves the picture
    // under its clear ones as it was, and multiplies into the picture when blended so.
    #[test]
    fn whole_pixel_layers_keep_their_pixels() {
        let src = [[10, 20, 30, 255], [0, 0, 0, 0], [200, 100, 50, 128], [40, 80, 120, 255]].concat();
        let drawn = |mul: bool| {
            let mut out = [90, 90, 90, 255].repeat(16);
            blit(&mut out, (4, 4), &src, (2, 2), 1., 0., (2., 2.), 1., mul);
            [(0, 0), (1, 1), (2, 1), (1, 2), (2, 2)].map(|(x, y)| out[(y * 4 + x) * 4..][..4].to_vec())
        };
        assert_eq!(
            drawn(false),
            [
                [90, 90, 90, 255],
                [10, 20, 30, 255],
                [90, 90, 90, 255],
                [145, 95, 69, 255],
                [40, 80, 120, 255]
            ]
        );
        assert_eq!(
            drawn(true),
            [
                [90, 90, 90, 255],
                [3, 7, 10, 255],
                [90, 90, 90, 255],
                [80, 62, 53, 255],
                [14, 28, 42, 255]
            ]
        );
    }

    // A matte shows its clip only where the matte is drawn, or only where it is not when inverted, keyed by
    // the matte's alpha or its brightness, in every row of the picture.
    #[test]
    fn mattes_cut_their_clips() {
        let shown = |how: &str| {
            let cut = Clip {
                w: 32,
                h: 36,
                color: "#ffffff".into(),
                into: 5,
                pos: (Some(Val::Fixed(0.)), None),
                ..clip("rect", "")
            };
            let red = Clip {
                w: 64,
                h: 36,
                color: "#ff0000".into(),
                matte: Some((5, how.into())),
                ..clip("rect", "")
            };
            let tl = Timeline {
                fps: 30,
                size: (64, 36),
                duration: 1.,
                clips: vec![
                    Clip {
                        src: "#00ff00".into(),
                        ..clip("color", "")
                    },
                    red,
                    cut,
                ],
                ..Default::default()
            };
            let px = Preview::default().frame(&tl, 0.5, (64, 36));
            [(10, 0), (50, 0), (10, 35), (50, 35)].map(|(x, y)| px[(y * 64 + x) * 4 + 2] > 128)
        };
        let left = [true, false, true, false];
        assert_eq!(["alpha", "luma"].map(shown), [left; 2]);
        assert_eq!(["alphaInverted", "lumaInverted"].map(shown), [left.map(|v| !v); 2]);
    }

    // An export should keep two frames for one video layer, and for three overlapping slowed layers
    // enough that every repeated frame comes from the cache, so no decoder is started again.
    #[test]
    fn export_cache_fits_the_layers() {
        let dir = scratch("hold");
        let files = ["a", "b", "c"].map(|f| dir.join(format!("{f}.mp4")));
        for f in &files {
            film(f, "testsrc=s=64x36:r=30:d=1", &[]);
        }
        let layer = |i: usize| Clip {
            src: files[i].to_string_lossy().into(),
            w: 64,
            h: 36,
            end: 2.,
            speed: 0.5,
            opacity: Val::Fixed([1., 0.5, 1.][i]),
            scale: Val::Fixed([1., 1., 0.5][i]),
            ..clip("video", "")
        };
        let tl = |n: usize| Timeline {
            fps: 30,
            size: (64, 36),
            duration: 2.,
            clips: (0..n).map(layer).collect(),
            ..Default::default()
        };
        assert_eq!(hold(&tl(1), (64, 36), 1.), 2 * 64 * 36 * 4);
        let mut pv = Preview {
            room: hold(&tl(3), (64, 36), 1.),
            ..Default::default()
        };
        let ids = |pv: &Preview| {
            let mut ids: Vec<u32> = pv.decoders.iter().flat_map(|(_, d)| d.child.as_ref().map(Child::id)).collect();
            ids.sort();
            ids
        };
        pv.frame(&tl(3), 0., (64, 36));
        let first = ids(&pv);
        for k in 1..60 {
            pv.frame(&tl(3), k as f64 / 30., (64, 36));
        }
        assert_eq!((first.len(), ids(&pv)), (3, first));
    }
}
