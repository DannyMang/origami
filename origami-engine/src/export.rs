use crate::preview::{Preview, lead, learn, warm};
use crate::{grade, lut::Lut, shape, text};
use anyhow::{Result, bail};
use origami_lang::{Clip, Kind, Timeline, Val, ffmpeg, start, status};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    iter,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{self, Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Codec {
    H264,
    Hevc,
    ProRes,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opts {
    pub height: u32,
    pub codec: Codec,
    pub quality: u8,
    pub audio: bool,
    pub range: Option<(f64, f64)>,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            height: 0,
            codec: Codec::H264,
            quality: 70,
            audio: true,
            range: None,
        }
    }
}

pub fn sized(tl: &Timeline, o: &Opts) -> (f64, (usize, usize)) {
    let k = match o.height {
        0 => 1.,
        h => (h as f64 / tl.size.1 as f64).min(16384. / tl.size.0.max(tl.size.1) as f64),
    };
    let even = |v: u32| ((v as f64 * k / 2.).round() as usize).max(1) * 2;
    (k, (even(tl.size.0), even(tl.size.1)))
}

fn span(tl: &Timeline, o: &Opts) -> (f64, f64) {
    let (a, b) = o.range.unwrap_or((0., tl.duration));
    (a.clamp(0., tl.duration), b.clamp(0., tl.duration))
}

static ROOMS: AtomicUsize = AtomicUsize::new(0);
static LIVE: Mutex<Vec<u32>> = Mutex::new(vec![]);
const STALL: Duration = Duration::from_secs(if cfg!(test) { 5 } else { 30 });

fn kill(pid: u32) {
    let _ = status(Command::new("/bin/kill").args(["-9", &pid.to_string()]));
}

pub fn halt() {
    let live = LIVE.lock().map(|l| l.clone()).unwrap_or_default();
    live.into_iter().for_each(kill);
}

fn tether(pid: u32, part: &str, dir: &Path) -> Option<Child> {
    start(
        Command::new("/bin/sh")
            .args([
                "-c",
                "read x; kill -9 \"$0\"; /bin/rm -f -- \"$1\"; /bin/rm -rf -- \"$2\"",
                &pid.to_string(),
                part,
            ])
            .arg(dir)
            .stdin(Stdio::piped())
            .stderr(Stdio::null()),
    )
    .ok()
}

fn room() -> PathBuf {
    let dir = env::temp_dir().join(format!("origami-{}-{}", process::id(), ROOMS.fetch_add(1, Relaxed)));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn tempo(mut s: f64) -> String {
    let mut out = vec![];
    while s < 0.5 {
        out.push("atempo=0.5".to_string());
        s *= 2.;
    }
    while s > 100. {
        out.push("atempo=100".into());
        s /= 100.;
    }
    out.push(format!("atempo={s}"));
    out.join(",")
}

fn spot(c: &Clip, (w, h): (f64, f64), (tw, th): (f64, f64)) -> (f64, f64, f64, f64) {
    let (at, m) = (
        |p: &Option<Val>, w, tw, m| p.as_ref().map_or(m * w - tw / 2., |x| x.at(0) * w),
        mid(c),
    );
    let (x, y) = (at(&c.pos.0, w, tw, m.0), at(&c.pos.1, h, th, m.1));
    (tw.round(), th.round(), x.round(), y.round())
}

fn mid(c: &Clip) -> (f64, f64) {
    match (c.kind, c.ends) {
        (Kind::Line, [x1, y1, x2, y2]) => ((x1 + x2) / 2., (y1 + y2) / 2.),
        (Kind::Path, _) => {
            let [x1, y1, x2, y2] = shape::bounds(c);
            ((x1 + x2) / 2., (y1 + y2) / 2.)
        }
        _ => (0.5, 0.5),
    }
}

pub fn grown(c: &Clip, i: usize) -> f64 {
    c.scale.at(i) / if c.kind == Kind::Group { 1. } else { c.scale.max() }
}

pub fn seat(c: &Clip, (w, h): (f64, f64), (tw, th): (f64, f64), i: usize) -> (f64, f64) {
    let (q, m) = (grown(c, i), mid(c));
    let at = |p: &Option<Val>, w: f64, tw: f64, m: f64| p.as_ref().map_or(m * w, |x| x.at(i) * w + tw * q / 2.);
    (at(&c.pos.0, w, tw, m.0), at(&c.pos.1, h, th, m.1))
}

pub fn place(c: &Clip, (w, h): (f64, f64), k: f64) -> (f64, f64, f64, f64) {
    let (bw, bh) = (w * c.scale.max(), h * c.scale.max());
    let r = (bw / c.w as f64).min(bh / c.h as f64);
    match c.kind {
        Kind::Text => {
            let (_, tw, th) = text::glyphs(c, k, false);
            spot(c, (w, h), (tw as f64, th as f64))
        }
        Kind::Rect | Kind::Line | Kind::Path => {
            let (tw, th) = shape::extent(c, (w, h), k);
            spot(c, (w, h), (tw as f64, th as f64))
        }
        _ if c.w > 0 => spot(c, (w, h), (c.w as f64 * r, c.h as f64 * r)),
        _ => (w, h, 0., 0.),
    }
}

pub fn footprint(c: &Clip, size: (f64, f64)) -> (f64, f64) {
    let (w, h, ..) = place(c, size, 1.);
    (w, h)
}

pub fn plain(c: &Clip) -> bool {
    let fixed = |(n, v): (&str, &Val)| n == "opacity" || matches!(v, Val::Fixed(_));
    c.parts.is_empty()
        && c.blend.is_empty()
        && c.step == 0.
        && c.into == 0
        && c.kind != Kind::Group
        && c.matte.is_none()
        && c.fx.is_empty()
        && c.points.iter().all(|p| matches!(p, (Val::Fixed(_), Val::Fixed(_))))
        && c.rotate == Val::Fixed(0.)
        && c.shown == Val::Fixed(1.)
        && c.vals().all(fixed)
}

fn piped(tl: &Timeline, o: &Opts) -> bool {
    span(tl, o) != (0., tl.duration) || !tl.clips.iter().all(|c| plain(c) && c.opacity == Val::Fixed(1.))
}

pub(crate) fn hold(tl: &Timeline, (w, h): (usize, usize), k: f64) -> usize {
    let mut ends = vec![];
    for c in tl.clips.iter().filter(|c| c.kind == Kind::Video && c.video) {
        let (tw, th, ..) = place(c, (w as f64, h as f64), k);
        let len = (tw as usize * th as usize * 4) as i64;
        ends.extend([(c.start, len), (c.end, -len)]);
    }
    ends.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let peak = ends.iter().scan(0, |s, e| {
        *s += e.1;
        Some(*s)
    });
    let peak = peak.fold(0, i64::max);
    let most = ends.iter().map(|e| e.1).fold(0, i64::max);
    ((peak + most) as usize).min((peak as usize).max(1 << 26))
}

pub fn live(c: &Clip, t: f64) -> bool {
    c.start <= t + 1e-6 && t + 1e-6 < c.end
}

pub fn alpha(c: &Clip, t: f64) -> f64 {
    let side = |d: f64, f: f64| if f > 0. { d / f } else { 1. };
    side(t - c.start, c.fade.0).min(side(c.end - t, c.fade.1)).clamp(0., 1.)
}

fn own(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn raw(size: &str, more: &[&str]) -> Vec<String> {
    own(&[&["-f", "rawvideo", "-pixel_format", "bgra", "-video_size", size], more].concat())
}

fn inputs(a: &[String]) -> usize {
    a.iter().filter(|a| *a == "-i").count()
}

fn graph(tl: &Timeline, video: bool, from: f64, dir: &Path, (k, (w, h)): (f64, (usize, usize))) -> (Vec<String>, String) {
    let size = (w as f64, h as f64);
    let (mut ins, mut f, mut mix, mut b0) = (vec![], vec![], String::new(), 0);
    if video {
        f.push(format!("color=black:s={w}x{h}:r={}:d={}[b0]", tl.fps, tl.duration));
    }
    learn(tl, from);
    for (i, c) in tl.clips.iter().enumerate().filter(|(_, c)| c.end > from) {
        let skip = (from - c.start).max(0.);
        let (len, on) = (
            c.end - c.start - skip,
            format!("enable='gte(t+1e-6,{})*lt(t+1e-6,{})'", c.start, c.end),
        );
        let n = inputs(&ins);
        let (tw, th, x, y) = place(c, size, k);
        let solid = c.kind == Kind::Color && text::rgba(&c.src)[3] == 255;
        let (video, mut cut) = (video && tw >= 1. && th >= 1., String::new());
        match c.kind {
            Kind::Image if video => ins.extend(own(&[
                "-loop",
                "1",
                "-t",
                &len.to_string(),
                "-protocol_whitelist",
                "file",
                "-i",
                &c.src,
            ])),
            Kind::Text | Kind::Rect | Kind::Line | Kind::Path | Kind::Color if video && !solid => {
                let file = dir.join(format!("text-{i}.bgra")).to_string_lossy().to_string();
                let _ = fs::write(&file, shape::image(c, size, k, 0).0);
                ins.extend(own(&["-stream_loop", "-1", "-t", &len.to_string()]));
                ins.extend(raw(&format!("{tw}x{th}"), &["-framerate", &tl.fps.to_string(), "-i", &file]));
            }
            Kind::Video | Kind::Sound => {
                let (at, slack) = (c.src_in + skip * c.speed, lead(&c.src));
                let lead = slack.min(at);
                if slack > 0. {
                    cut = format!("trim=start={}:duration={},", lead - 2e-5, len * c.speed);
                }
                ins.extend(own(&[
                    "-ss",
                    &(at - lead).to_string(),
                    "-t",
                    &(len * c.speed + lead + slack).to_string(),
                    "-protocol_whitelist",
                    "file",
                    "-i",
                    &c.src,
                ]))
            }
            _ => {}
        }
        if video && c.video {
            let temp = |name: &str, data: &[u8]| {
                let file = dir.join(format!("{i}-{name}")).to_string_lossy().to_string();
                let _ = fs::write(&file, data);
                file
            };
            let lut = c.grade.as_ref().and_then(grade::lut);
            let mask = c.grade.as_ref().and_then(|g| g.vignette.as_ref()).map(|v| {
                let m = inputs(&ins);
                let file = temp("vignette.bgra", &grade::mask(v, tw as usize, th as usize));
                ins.extend(raw(&format!("{tw}x{th}"), &["-i", &file]));
                m
            });
            f.push(match solid {
                true => {
                    let [r, g, b, _] = text::rgba(&c.src);
                    let px = lut.map_or(vec![b, g, r], |l| l.apply(&[b, g, r, 255], vec![]));
                    let shade = mask.map_or(String::new(), |m| format!("[d{i}];[d{i}][{m}:v]overlay={on}"));
                    let fill = format!("color=0x{:02x}{:02x}{:02x}", px[2], px[1], px[0]);
                    format!("[b{b0}]drawbox=w=iw:h=ih:{fill}:t=fill:{on}{shade}[b{}]", b0 + 1)
                }
                false => {
                    let pad = if c.kind == Kind::Video { "tpad=stop=2:stop_mode=clone," } else { "" };
                    let mut s = format!("[{n}:v]{cut}{pad}setpts=(PTS-STARTPTS)/{}+{}/TB+1e-6,scale={tw}:{th}", c.speed, c.start);
                    if lut.is_some() || mask.is_some() {
                        s += ",format=bgra";
                    }
                    if let Some(l) = lut {
                        s += &format!(",lut3d=file={}:interp=trilinear", temp("lut.cube", l.text().as_bytes()));
                    }
                    if let Some(m) = mask {
                        s += &format!(
                            ",split[p{i}][q{i}];[p{i}]format=bgr24[o{i}];[q{i}]alphaextract[a{i}];[o{i}][{m}:v]overlay=format=rgb,format=bgra[v{i}];[v{i}][a{i}]alphamerge"
                        );
                    }
                    s += ",format=yuva420p";
                    if c.fade.0 > 0. {
                        s += &format!(",fade=in:st={}:d={}:alpha=1", c.start, c.fade.0);
                    }
                    if c.fade.1 > 0. {
                        s += &format!(",fade=out:st={}:d={}:alpha=1", c.end - c.fade.1, c.fade.1);
                    }
                    format!("{s}[c{i}];[b{b0}][c{i}]overlay={x}:{y}:eof_action=pass:{on}[b{}]", b0 + 1)
                }
            });
            b0 += 1;
        }
        if c.audio && c.gain > 0. {
            let mut s = format!(
                "[{n}:a]{}asetpts=PTS-STARTPTS,{},volume={},aresample=48000,aformat=channel_layouts=stereo",
                cut.replace("trim", "atrim"),
                tempo(c.speed),
                c.gain
            );
            if c.fade.0 > skip {
                s += &format!(",afade=t=in:d={}", c.fade.0 - skip);
            }
            if c.fade.1 > 0. {
                s += &format!(",afade=t=out:st={}:d={}", (len - c.fade.1).max(0.), c.fade.1);
            }
            f.push(format!("{s},adelay={}:all=1[a{i}]", ((c.start - from).max(0.) * 1000.).round()));
            mix += &format!("[a{i}]");
        }
    }
    let n = mix.matches('[').count();
    f.push(match n {
        0 => format!("anullsrc=r=48000:cl=stereo,atrim=0:{}[a]", tl.duration - from),
        _ => format!(
            "{mix}amix=inputs={n}:normalize=0:duration=longest,apad,atrim=0:{}[a]",
            tl.duration - from
        ),
    });
    if video {
        f.push(format!("[b{b0}]null[v]"));
    }
    (ins, f.join(";"))
}

pub fn command(tl: &Timeline, out: &str, o: &Opts) -> Vec<String> {
    args(tl, out, o, &room())
}

fn args(tl: &Timeline, out: &str, o: &Opts, dir: &Path) -> Vec<String> {
    let ((k, (w, h)), (from, to), pipe) = (sized(tl, o), span(tl, o), piped(tl, o));
    let (codec, tag, fmt) = match o.codec {
        Codec::ProRes => ("prores_videotoolbox", "apcn", "p210le"),
        Codec::H264 if w <= 4096 && h <= 4096 => ("h264_videotoolbox", "avc1", "yuv420p"),
        _ => ("hevc_videotoolbox", "hvc1", "yuv420p"),
    };
    let (mut a, mut f) = graph(tl, !pipe, from, dir, (k, (w, h)));
    if pipe {
        f += &format!(";[{}:v]scale=out_color_matrix=bt709:out_range=tv,format={fmt}[v]", inputs(&a));
        a.extend(raw(
            &format!("{w}x{h}"),
            &["-framerate", &tl.fps.to_string(), "-i", "-", "-colorspace", "bt709"],
        ));
    }
    if !o.audio {
        f += ";[a]anullsink";
    }
    let (q, sound) = (o.quality.to_string(), ["-map", "[a]", "-c:a", "aac", "-b:a", "256k"]);
    let tail = [
        "-filter_complex",
        &f,
        "-map",
        "[v]",
        "-c:v",
        codec,
        "-tag:v",
        tag,
        "-allow_sw",
        "1",
        "-pix_fmt",
        fmt,
    ];
    let grade: &[&str] = match o.codec {
        Codec::ProRes => &["-profile:v", "standard"],
        _ => &["-q:v", &q],
    };
    let sound: &[&str] = if o.audio { &sound } else { &["-an"] };
    let end = (to - from).to_string();
    a.extend(
        tail.iter()
            .chain(grade)
            .chain(sound)
            .chain(&["-t", &end, "-y", out])
            .map(|s| s.to_string()),
    );
    a
}

pub fn audio_args(tl: &Timeline, t: f64, rate: u32, speed: f64) -> Vec<String> {
    let (mut a, f) = graph(tl, false, t, &env::temp_dir(), (1., (tl.size.0 as usize, tl.size.1 as usize)));
    let f = format!("{f};[a]atempo={speed}[o]");
    let tail = [
        "-filter_complex",
        &f,
        "-map",
        "[o]",
        "-f",
        "f32le",
        "-ar",
        &rate.to_string(),
        "-ac",
        "2",
        "-",
    ];
    a.extend(tail.map(String::from));
    a
}

pub fn render(tl: &Timeline, out: &str, o: &Opts, mut progress: impl FnMut(f64)) -> Result<()> {
    if tl.duration <= 0. {
        bail!("nothing to export: the timeline is empty");
    }
    let ((from, to), pipe) = (span(tl, o), piped(tl, o));
    if to - from < 1. / tl.fps as f64 {
        bail!("nothing to export: the range is empty");
    }
    for c in &tl.clips {
        let mut files = c.grade.iter().flat_map(|g| g.input_lut.iter().chain(g.look.iter().map(|l| &l.0)));
        if let Some(file) = files.find(|f| Lut::read(f).is_none()) {
            bail!("line {}: {file} is not a .cube file with a 0 to 1 domain", c.line);
        }
    }
    let (k, size) = sized(tl, o);
    if let Some(c) = tl.clips.iter().find(|c| {
        let (w, h, ..) = place(c, (size.0 as f64, size.1 as f64), k);
        w.max(h) > 16384.
    }) {
        bail!(
            "line {}: {} would be drawn over 16384 pixels on a side at this export size: export at a smaller size or lower its size or scale",
            c.line,
            c.label
        );
    }
    let id = |p: &Path| fs::metadata(p).map(|m| (m.dev(), m.ino())).ok();
    if let Some((line, file)) = id(out.as_ref()).and_then(|o| tl.files.iter().find(|(_, f)| id(f) == Some(o))) {
        bail!(
            "export to {out} would overwrite {}, which line {line} reads: pick another name",
            file.display()
        );
    }
    let (dir, mut salt) = (&room(), [0; 8]);
    fs::File::open("/dev/urandom")?.read_exact(&mut salt)?;
    let part = Path::new(out).with_file_name(format!(
        ".{:016x}.{}",
        u64::from_le_bytes(salt),
        Path::new(out).file_name().unwrap_or_default().to_string_lossy()
    ));
    let part = &*part.to_string_lossy();
    let mut child = start(
        ffmpeg()
            .args(["-v", "error", "-nostats", "-progress", "pipe:1"])
            .args(args(tl, part, o, dir))
            .stdin(if pipe { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )?;
    let (pipe, pid, stuck) = (child.stdin.take(), child.id(), &AtomicBool::new(false));
    LIVE.lock().unwrap().push(pid);
    thread::scope(|s| {
        let (beat, pulse) = mpsc::channel();
        s.spawn(move || {
            let tie = tether(pid, part, dir);
            if iter::repeat_with(|| pulse.recv_timeout(STALL)).find_map(Result::err) == Some(RecvTimeoutError::Timeout) {
                stuck.store(true, Relaxed);
                kill(pid);
            }
            if let Some(mut t) = tie {
                let _ = (t.kill(), t.wait());
            }
        });
        if let Some(mut pipe) = pipe {
            let (send, take) = mpsc::sync_channel::<Vec<u8>>(1);
            s.spawn(move || take.iter().try_for_each(|px| pipe.write_all(&px).ok()));
            s.spawn(move || {
                let (mut pv, fps) = (Preview::default(), tl.fps as f64);
                pv.room = hold(tl, size, k);
                warm(tl, from);
                ((from * fps).round() as u32..(to * fps).round() as u32)
                    .try_for_each(|k| send.send(pv.frame(tl, k as f64 / fps, size)).ok())
            });
        }
        for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
            let _ = beat.send(());
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
                progress((us / 1e6 / (to - from)).clamp(0., 1.));
            }
        }
    });
    LIVE.lock().unwrap().retain(|p| *p != pid);
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err)?;
    let done = child.wait()?.success();
    let _ = fs::remove_dir_all(dir);
    if !done || stuck.load(Relaxed) {
        let _ = fs::remove_file(part);
    }
    if stuck.load(Relaxed) {
        bail!("export of {out} stopped: ffmpeg made no progress for {} seconds", STALL.as_secs());
    }
    if !done {
        let why = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("ffmpeg failed").trim();
        let why = why.strip_prefix('[').and_then(|w| w.split_once("] ")).map_or(why, |w| w.1);
        let clip = tl
            .clips
            .iter()
            .find(|c| matches!(c.kind, Kind::Video | Kind::Sound | Kind::Image) && err.contains(&c.src));
        let at = clip.map_or(String::new(), |c| format!(" at line {} ({})", c.line, c.label));
        bail!("export of {out} failed{at}: {}", why.chars().take(200).collect::<String>());
    }
    fs::rename(part, out).or_else(|e| {
        let _ = fs::remove_file(part);
        bail!("export of {out} failed: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        footprint,
        tests::{TIDY, film, scratch},
    };
    use std::time::Instant;

    // Slow and very fast motion with sound, a fade longer than its clip and layers too small to see export
    // and play, two exports at once neither disturb each other nor leave files behind, and a failing export
    // says what failed and, when a clip caused it, on which line, keeping the file that was there.
    #[test]
    fn exports_odd_timelines() {
        let _tidy = TIDY.write();
        let dir = scratch("odd");
        for file in ["a.mp4", "b.mp4"] {
            film(
                &dir.join(file),
                "color=black:s=64x36:r=30:d=10",
                &["-f", "lavfi", "-i", "sine=d=10", "-shortest"],
            );
        }
        let tl = |src: &str| {
            fs::write(dir.join("edit.js"), src).unwrap();
            origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap()
        };
        let out = |name: &str| dir.join(name).to_string_lossy().to_string();
        for (i, src) in [
            "video('a.mp4').trim(0, 2).speed(0.25)",
            "video('a.mp4').speed(150)",
            "video('a.mp4').trim(0, 1).fade(5)",
            "stack(color('#333').for(1), text('').for(1), text('a').for(1).scale(0), video('a.mp4').trim(0, 1).scale(0))",
        ]
        .into_iter()
        .enumerate()
        {
            render(&tl(src), &out(&format!("{i}.mp4")), &Opts::default(), |_| {}).unwrap();
            let heard = origami_lang::output(ffmpeg().args(["-v", "error"]).args(audio_args(&tl(src), 0., 48000, 1.)));
            assert!(heard.unwrap().status.success(), "{src}");
        }
        let (one, two) = (tl("text('one').for(2)"), tl("text('two').for(2)"));
        thread::scope(|s| {
            let a = s.spawn(|| render(&one, &out("one.mp4"), &Opts::default(), |_| {}));
            render(&two, &out("two.mp4"), &Opts::default(), |_| {}).unwrap();
            a.join().unwrap().unwrap();
        });
        assert_ne!(fs::read(dir.join("one.mp4")).unwrap(), fs::read(dir.join("two.mp4")).unwrap());
        let mine = format!("origami-{}-", process::id());
        let left = || {
            let all = fs::read_dir(env::temp_dir()).unwrap().flatten();
            all.filter(|e| e.file_name().to_string_lossy().starts_with(&mine)).count()
        };
        let since = Instant::now();
        while left() > 0 && since.elapsed() < Duration::from_secs(30) {
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(left(), 0);
        let gone = tl("text('a').for(1)\nvideo('b.mp4').trim(0, 1)");
        fs::remove_file(dir.join("b.mp4")).unwrap();
        fs::write(out("gone.mp4"), "old").unwrap();
        let e = render(&gone, &out("gone.mp4"), &Opts::default(), |_| {}).unwrap_err().to_string();
        assert_eq!(fs::read_to_string(out("gone.mp4")).unwrap(), "old");
        assert!(
            fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with('.'))
        );
        assert!(
            e.starts_with(&format!("export of {} failed at line 2 (b): ", out("gone.mp4"))),
            "{e}"
        );
        let e = render(&tl("text('a').for(1)"), &out("no/where.mp4"), &Opts::default(), |_| {})
            .unwrap_err()
            .to_string();
        assert!(e.starts_with(&format!("export of {} failed: ", out("no/where.mp4"))), "{e}");
        let e = render(&tl("stack()"), &out("empty.mp4"), &Opts::default(), |_| {})
            .unwrap_err()
            .to_string();
        assert_eq!(e, "nothing to export: the timeline is empty");
    }

    // Export options should change only what they say: a range starts on its first frame and keeps its
    // length, a size scales the whole frame but never past 16384 on a side and refuses, naming its line, a
    // clip it would draw past that, audio can be left out, and each format writes its codec.
    #[test]
    fn exports_with_options() {
        let _tidy = TIDY.read();
        let dir = scratch("opts");
        film(&dir.join("s.wav"), "sine=d=3", &[]);
        let src = "settings({ fps: 30, size: [64, 36] })\nstack(cat(color('#ff0000').for(1), color('#00ff00').for(1)), sound('s.wav').trim(0, 2))";
        fs::write(dir.join("edit.js"), src).unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
        let at = |size, height| sized(&Timeline { size, ..tl.clone() }, &Opts { height, ..Opts::default() }).1;
        assert_eq!([0, 720, 1080, 2160].map(|h| at((16384, 2), h)), [(16384, 2); 4]);
        assert_eq!(
            [0, 720, 2160].map(|h| at((1920, 1080), h)),
            [(1920, 1080), (1280, 720), (3840, 2160)]
        );
        let probe = |out: &str, what: &[&str]| {
            let args = [&["-v", "error", "-i", out][..], what].concat();
            origami_lang::output(ffmpeg().args(args)).unwrap()
        };
        let small = Opts {
            height: 72,
            codec: Codec::Hevc,
            quality: 50,
            audio: false,
            range: Some((1., 2.)),
        };
        let out = dir.join("small.mp4").to_string_lossy().to_string();
        render(&tl, &out, &small, |_| {}).unwrap();
        let px = probe(&out, &["-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).stdout;
        assert_eq!(px.len(), 128 * 72 * 3 * 30);
        assert!(px[(36 * 128 + 64) * 3 + 1] > 200 && px[(36 * 128 + 64) * 3] < 60);
        assert!(!probe(&out, &["-map", "0:a", "-f", "null", "-"]).status.success());
        let big = dir.join("big.mp4").to_string_lossy().to_string();
        render(&tl, &big, &Opts { range: None, ..small }, |_| {}).unwrap();
        let px = probe(&big, &["-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).stdout;
        assert_eq!(px.len(), 128 * 72 * 3 * 60);
        assert!(px[(36 * 128 + 64) * 3] > 200 && px[128 * 72 * 3 * 45 + (36 * 128 + 64) * 3 + 1] > 200);
        let pro = dir.join("pro.mov").to_string_lossy().to_string();
        let opts = Opts {
            codec: Codec::ProRes,
            ..Opts::default()
        };
        render(&tl, &pro, &opts, |_| {}).unwrap();
        assert!(probe(&pro, &["-map", "0:a", "-f", "null", "-"]).status.success());
        let frames = probe(&pro, &["-f", "rawvideo", "-pix_fmt", "gray", "-"]).stdout.len() / (64 * 36);
        assert_eq!(frames, 60);
        let codec = origami_lang::output(origami_lang::ffprobe().args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
            &pro,
        ]));
        assert!(String::from_utf8_lossy(&codec.unwrap().stdout).starts_with("prores"));
        fs::write(dir.join("huge.js"), "settings({ size: [64, 36] })\ntext('A', { size: 9000 })").unwrap();
        let huge = origami_lang::eval(&dir.join("huge.js"), false, footprint).unwrap();
        let e = render(&huge, &big, &Opts { height: 2160, ..opts }, |_| {}).unwrap_err().to_string();
        assert!(e.starts_with("line 2: A would be drawn over 16384 pixels on a side"), "{e}");
    }

    // A clip that starts on a timeline frame after lengths such as 0.1 + 0.2, which add up to a hair
    // more than 0.3 in floating point, should show from that frame in the preview and the export.
    #[test]
    fn clips_meet_on_the_frame() {
        let _tidy = TIDY.read();
        let dir = scratch("meet");
        film(&dir.join("w.mp4"), "color=white:s=64x36:r=30:d=2", &[]);
        let src =
            "settings({ fps: 30, size: [64, 36] })\ncat(color('#ff0000').for(0.1), color('#00ff00').for(0.2), video('w.mp4').trim(0, 1))";
        fs::write(dir.join("edit.js"), src).unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
        assert!(tl.clips[2].start > 0.3);
        let out = dir.join("meet.mp4").to_string_lossy().to_string();
        render(&tl, &out, &Opts::default(), |_| {}).unwrap();
        let args = ["-v", "error", "-i", &out, "-f", "rawvideo", "-pix_fmt", "gray", "-"];
        let px = origami_lang::output(ffmpeg().args(args)).unwrap().stdout;
        let shown = Preview::default().frame(&tl, 9. / 30., (64, 36))[(18 * 64 + 32) * 4];
        assert!(px[9 * 64 * 36 + 18 * 64 + 32] > 200 && shown > 200);
    }

    // A clip trimmed so its media runs out a little before the clip ends, a slowed-down clip and a
    // clip at a lower frame rate than the timeline should hold their last picture to the end of the
    // clip, in the preview and in the export, instead of flashing black.
    #[test]
    fn ends_hold_the_last_frame() {
        let _tidy = TIDY.read();
        let dir = scratch("ends");
        for (file, rate) in [("w30.mp4", 30), ("w15.mp4", 15)] {
            film(&dir.join(file), &format!("color=white:s=64x36:r={rate}:d=6"), &[]);
        }
        let src = "settings({ fps: 30 })\ncat(video('w30.mp4').trim(4.01), video('w30.mp4').trim(0, 1).speed(0.5), video('w15.mp4').trim(0, 1), video('w30.mp4').trim(5.51))";
        fs::write(dir.join("edit.js"), src).unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
        let n = (tl.duration * 30.).round() as usize;
        let mut pv = Preview::default();
        let shown: Vec<u8> = (0..n)
            .map(|k| pv.frame(&tl, k as f64 / 30., (64, 36))[(18 * 64 + 32) * 4])
            .collect();
        let out = dir.join("ends.mp4").to_string_lossy().to_string();
        render(&tl, &out, &Opts::default(), |_| {}).unwrap();
        let args = ["-v", "error", "-i", &out, "-f", "rawvideo", "-pix_fmt", "gray", "-"];
        let px = origami_lang::output(ffmpeg().args(args)).unwrap().stdout;
        let made: Vec<u8> = px.chunks(64 * 36).map(|f| f[18 * 64 + 32]).collect();
        assert_eq!(made.len(), n);
        assert!(shown.iter().chain(&made).all(|v| *v > 200), "{shown:?} {made:?}");
    }

    // An export onto a file the script reads, spelled another way (different case, a detour through ..,
    // a symlink), is refused and leaves the file whole, and a link or a named pipe planted at the
    // destination is replaced, not written through.
    #[test]
    fn keeps_sources_safe() {
        let _tidy = TIDY.read();
        let dir = scratch("self");
        fs::create_dir_all(dir.join("sub")).unwrap();
        film(&dir.join("a.mp4"), "color=white:s=64x36:r=30:d=1", &[]);
        fs::write(dir.join("subs.srt"), "1\n00:00:00,000 --> 00:00:01,000\nhi\n").unwrap();
        std::os::unix::fs::symlink(dir.join("a.mp4"), dir.join("link.mp4")).unwrap();
        fs::write(
            dir.join("edit.js"),
            "text('a').for(1)\nvideo('a.mp4').trim(0, 1)\ncaptions('subs.srt')",
        )
        .unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
        let before = fs::read(dir.join("a.mp4")).unwrap();
        for (out, line) in [("A.MP4", 2), ("sub/../a.mp4", 2), ("link.mp4", 2), ("subs.srt", 3)] {
            let out = dir.join(out).to_string_lossy().to_string();
            let e = render(&tl, &out, &Opts::default(), |_| {}).unwrap_err().to_string();
            assert!(
                e.starts_with(&format!("export to {out} would overwrite ")) && e.contains(&format!("which line {line} reads")),
                "{e}"
            );
        }
        assert_eq!(fs::read(dir.join("a.mp4")).unwrap(), before);
        fs::write(dir.join("precious"), "keep").unwrap();
        std::os::unix::fs::symlink(dir.join("precious"), dir.join("planted.mp4")).unwrap();
        render(&tl, &dir.join("planted.mp4").to_string_lossy(), &Opts::default(), |_| {}).unwrap();
        assert!(fs::read_to_string(dir.join("precious")).unwrap() == "keep" && !dir.join("planted.mp4").is_symlink());
        assert!(status(Command::new("mkfifo").arg(dir.join("pipe.mp4"))).unwrap().success());
        render(&tl, &dir.join("pipe.mp4").to_string_lossy(), &Opts::default(), |_| {}).unwrap();
        assert!(fs::metadata(dir.join("pipe.mp4")).unwrap().is_file());
    }

    // Shapes, lines and text too large to draw, directly or through a scale or keyed scale, should
    // stop the script with an error on their line instead of taking all memory.
    #[test]
    fn refuses_huge_layers() {
        let dir = scratch("huge");
        for src in [
            "rect(1e9, 1e9).for(1)",
            "rect(100, 100).scale(1e8).for(1)",
            "rect(100, 100).for(1).scale(keys([0, 1], [1, 1e4]))",
            "text('hi').scale(1e6).for(1)",
            "text('hi', { size: 1e9 }).for(1)",
            "line(0, 0, 1, 1, { width: 1e9 }).for(1)",
        ] {
            fs::write(dir.join("edit.js"), format!("text('a')\nstack({src})")).unwrap();
            let e = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap_err().to_string();
            assert!(e.starts_with("line 2: ") && e.contains("over 16384 on a side"), "{src}: {e}");
        }
    }

    // An export whose ffmpeg makes no progress should be stopped and reported instead of running forever.
    #[test]
    fn stops_a_stalled_export() {
        let _tidy = TIDY.read();
        let dir = scratch("stall");
        film(&dir.join("a.mp4"), "color=white:s=64x36:r=30:d=1", &[]);
        fs::write(dir.join("edit.js"), "video('a.mp4').trim(0, 1)").unwrap();
        let mut tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
        assert!(status(Command::new("mkfifo").arg(dir.join("never.mp4"))).unwrap().success());
        tl.clips[0].src = dir.join("never.mp4").to_string_lossy().to_string();
        let out = dir.join("out.mp4").to_string_lossy().to_string();
        let e = render(&tl, &out, &Opts::default(), |_| {}).unwrap_err().to_string();
        assert_eq!(e, format!("export of {out} stopped: ffmpeg made no progress for 5 seconds"));
    }

    // An export's ffmpeg should be killed, and the hidden file it was writing and its folder of layer
    // pictures removed, soon after its tie to the program is cut, which the system does when the program
    // dies, even by force quit.
    #[test]
    fn dies_with_the_program() {
        let dir = scratch("tether");
        let (part, room) = (dir.join(".00ff.it's \"$HOME\" $(x).mp4"), dir.join("origami-1-0 $(x)"));
        fs::create_dir_all(&room).unwrap();
        fs::write(room.join("text-0.bgra"), "layer").unwrap();
        fs::write(&part, "half").unwrap();
        let args = ["-v", "error", "-re", "-f", "lavfi", "-i", "color=d=60", "-f", "null", "-"];
        let mut ff = start(ffmpeg().args(args)).unwrap();
        drop(tether(ff.id(), &part.to_string_lossy(), &room));
        let gone = (0..500).any(|_| {
            thread::sleep(Duration::from_millis(10));
            ff.try_wait().unwrap().is_some() && !part.exists() && !room.exists()
        });
        let _ = ff.kill();
        assert!(gone);
    }
}
