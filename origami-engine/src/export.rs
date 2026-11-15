use crate::preview::Preview;
use crate::{grade, lut::Lut, shape, text};
use anyhow::{Context, Result, bail};
use origami_lang::{Clip, Kind, Timeline, Val, ffmpeg};
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

static ROOMS: AtomicUsize = AtomicUsize::new(0);
static LIVE: Mutex<Vec<u32>> = Mutex::new(vec![]);
const STALL: Duration = Duration::from_secs(if cfg!(test) { 5 } else { 30 });

fn kill(pid: u32) {
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
}

pub fn halt() {
    LIVE.lock().map(|l| l.iter().copied().for_each(kill)).unwrap_or_default();
}

fn tether(pid: u32) -> Option<Child> {
    Command::new("sh")
        .args(["-c", &format!("read x; kill -9 {pid}")])
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
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
        _ => (0.5, 0.5),
    }
}

pub fn seat(c: &Clip, (w, h): (f64, f64), (tw, th): (f64, f64), i: usize) -> (f64, f64) {
    let (q, m) = (c.scale.at(i) / c.scale.max(), mid(c));
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
        Kind::Rect | Kind::Line => {
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
    c.parts.is_empty() && c.rotate == Val::Fixed(0.) && c.shown == Val::Fixed(1.) && c.vals().all(fixed)
}

fn piped(tl: &Timeline) -> bool {
    !tl.clips.iter().all(|c| plain(c) && c.opacity == Val::Fixed(1.))
}

pub fn live(c: &Clip, t: f64) -> bool {
    c.start <= t + 1e-6 && t + 1e-6 < c.end
}

pub fn alpha(c: &Clip, t: f64) -> f64 {
    let side = |d: f64, f: f64| if f > 0. { d / f } else { 1. };
    side(t - c.start, c.fade.0).min(side(c.end - t, c.fade.1)).clamp(0., 1.)
}

fn graph(tl: &Timeline, video: bool, from: f64, dir: &Path) -> (Vec<String>, String) {
    let size = (tl.size.0 as f64, tl.size.1 as f64);
    let (mut ins, mut f, mut mix, mut b0) = (vec![], vec![], String::new(), 0);
    if video {
        f.push(format!(
            "color=black:s={}x{}:r={}:d={}[b0]",
            tl.size.0, tl.size.1, tl.fps, tl.duration
        ));
    }
    for (i, c) in tl.clips.iter().enumerate().filter(|(_, c)| c.end > from) {
        let skip = (from - c.start).max(0.);
        let (len, on) = (
            c.end - c.start - skip,
            format!("enable='gte(t+1e-6,{})*lt(t+1e-6,{})'", c.start, c.end),
        );
        let n = ins.iter().filter(|a| *a == "-i").count();
        let (tw, th, x, y) = place(c, size, 1.);
        let own = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let solid = c.kind == Kind::Color && text::rgba(&c.src)[3] == 255;
        let video = video && tw >= 1. && th >= 1.;
        match c.kind {
            Kind::Image if video => ins.extend(own(&["-loop", "1", "-t", &len.to_string(), "-i", &c.src])),
            Kind::Text | Kind::Rect | Kind::Line | Kind::Color if video && !solid => {
                let file = dir.join(format!("text-{i}.bgra")).to_string_lossy().to_string();
                let _ = fs::write(&file, shape::image(c, size, 1.).0);
                let raw = [
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "bgra",
                    "-video_size",
                    &format!("{tw}x{th}"),
                    "-framerate",
                    &tl.fps.to_string(),
                ];
                ins.extend(
                    own(&["-stream_loop", "-1", "-t", &len.to_string()])
                        .into_iter()
                        .chain(own(&raw))
                        .chain(own(&["-i", &file])),
                );
            }
            Kind::Video | Kind::Sound => ins.extend(own(&[
                "-ss",
                &(c.src_in + skip * c.speed).to_string(),
                "-t",
                &(len * c.speed).to_string(),
                "-i",
                &c.src,
            ])),
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
                let m = ins.iter().filter(|a| *a == "-i").count();
                let file = temp("vignette.bgra", &grade::mask(v, tw as usize, th as usize));
                ins.extend(own(&[
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "bgra",
                    "-video_size",
                    &format!("{tw}x{th}"),
                    "-i",
                    &file,
                ]));
                m
            });
            f.push(match solid {
                true => {
                    let [r, g, b, _] = text::rgba(&c.src);
                    let px = lut.map_or(vec![b, g, r], |l| l.apply(&[b, g, r, 255]));
                    let shade = mask.map_or(String::new(), |m| format!("[d{i}];[d{i}][{m}:v]overlay={on}"));
                    let fill = format!("color=0x{:02x}{:02x}{:02x}", px[2], px[1], px[0]);
                    format!("[b{b0}]drawbox=w=iw:h=ih:{fill}:t=fill:{on}{shade}[b{}]", b0 + 1)
                }
                false => {
                    let pad = if c.kind == Kind::Video { "tpad=stop=2:stop_mode=clone," } else { "" };
                    let mut s = format!("[{n}:v]{pad}setpts=(PTS-STARTPTS)/{}+{}/TB,scale={tw}:{th}", c.speed, c.start);
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
                "[{n}:a]asetpts=PTS-STARTPTS,{},volume={},aresample=48000,aformat=channel_layouts=stereo",
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

pub fn command(tl: &Timeline, out: &str) -> Vec<String> {
    args(tl, out, &room())
}

fn args(tl: &Timeline, out: &str, dir: &Path) -> Vec<String> {
    let (mut a, mut f) = graph(tl, !piped(tl), 0., dir);
    if piped(tl) {
        let n = a.iter().filter(|a| *a == "-i").count();
        f += &format!(";[{n}:v]scale=out_color_matrix=bt709:out_range=tv,format=yuv420p[v]");
        let raw = [
            "-f",
            "rawvideo",
            "-pixel_format",
            "bgra",
            "-video_size",
            &format!("{}x{}", tl.size.0, tl.size.1),
            "-framerate",
            &tl.fps.to_string(),
            "-i",
            "-",
            "-colorspace",
            "bt709",
        ];
        a.extend(raw.map(String::from));
    }
    let tail = [
        "-filter_complex",
        &f,
        "-map",
        "[v]",
        "-map",
        "[a]",
        "-c:v",
        "libx264",
        "-crf",
        "18",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
    ];
    a.extend(
        tail.into_iter()
            .chain(["-t", &tl.duration.to_string(), "-y", out])
            .map(String::from),
    );
    a
}

pub fn audio_args(tl: &Timeline, t: f64, rate: u32, speed: f64) -> Vec<String> {
    let (mut a, f) = graph(tl, false, t, &env::temp_dir());
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

pub fn render(tl: &Timeline, out: &str, mut progress: impl FnMut(f64)) -> Result<()> {
    if tl.duration <= 0. {
        bail!("nothing to export: the timeline is empty");
    }
    for c in &tl.clips {
        let mut files = c.grade.iter().flat_map(|g| g.input_lut.iter().chain(g.look.iter().map(|l| &l.0)));
        if let Some(file) = files.find(|f| Lut::read(f).is_none()) {
            bail!("line {}: {file} is not a .cube file with a 0 to 1 domain", c.line);
        }
    }
    let id = |p: &Path| fs::metadata(p).map(|m| (m.dev(), m.ino())).ok();
    if let Some((line, file)) = id(out.as_ref()).and_then(|o| tl.files.iter().find(|(_, f)| id(f) == Some(o))) {
        bail!(
            "export to {out} would overwrite {}, which line {line} reads: pick another name",
            file.display()
        );
    }
    let dir = room();
    let mut child = ffmpeg()
        .args(["-v", "error", "-nostats", "-progress", "pipe:1"])
        .args(args(tl, out, &dir))
        .stdin(if piped(tl) { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("ffmpeg not found")?;
    let (pipe, size, pid, stuck) = (
        child.stdin.take(),
        (tl.size.0 as usize, tl.size.1 as usize),
        child.id(),
        &AtomicBool::new(false),
    );
    LIVE.lock().unwrap().push(pid);
    thread::scope(|s| {
        let (beat, pulse) = mpsc::channel();
        s.spawn(move || {
            let tie = tether(pid);
            if iter::repeat_with(|| pulse.recv_timeout(STALL)).find_map(Result::err) == Some(RecvTimeoutError::Timeout) {
                stuck.store(true, Relaxed);
                kill(pid);
            }
            if let Some(mut t) = tie {
                let _ = (t.kill(), t.wait());
            }
        });
        s.spawn(move || {
            let (mut pv, mut pipe) = (Preview::default(), pipe?);
            pv.room = 1 << 26;
            (0..(tl.duration * tl.fps as f64).round() as u32)
                .try_for_each(|k| pipe.write_all(&pv.frame(tl, k as f64 / tl.fps as f64, size)).ok())
        });
        for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
            let _ = beat.send(());
            if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
                progress((us / 1e6 / tl.duration).clamp(0., 1.));
            }
        }
    });
    LIVE.lock().unwrap().retain(|p| *p != pid);
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err)?;
    let done = child.wait()?.success();
    let _ = fs::remove_dir_all(dir);
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::footprint;

    static ONE: Mutex<()> = Mutex::new(());

    // Slow and very fast motion with sound, a fade longer than its clip, and layers too small to see
    // should export and play. Two exports at once should not disturb each other or leave files behind, and a
    // failing export should say what failed and, when a clip caused it, on which line.
    #[test]
    fn exports_odd_timelines() {
        let _one = ONE.lock();
        let dir = env::temp_dir().join("origami-export-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let args = [
            "-f",
            "lavfi",
            "-i",
            "color=black:s=64x36:r=30:d=10",
            "-f",
            "lavfi",
            "-i",
            "sine=d=10",
            "-shortest",
        ];
        for file in ["a.mp4", "b.mp4"] {
            let made = ffmpeg().args(["-v", "error", "-y"]).args(args).arg(dir.join(file)).status();
            assert!(made.unwrap().success());
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
            render(&tl(src), &out(&format!("{i}.mp4")), |_| {}).unwrap();
            let heard = ffmpeg().args(["-v", "error"]).args(audio_args(&tl(src), 0., 48000, 1.)).output();
            assert!(heard.unwrap().status.success(), "{src}");
        }
        let (one, two) = (tl("text('one').for(2)"), tl("text('two').for(2)"));
        thread::scope(|s| {
            let a = s.spawn(|| render(&one, &out("one.mp4"), |_| {}));
            render(&two, &out("two.mp4"), |_| {}).unwrap();
            a.join().unwrap().unwrap();
        });
        assert_ne!(fs::read(dir.join("one.mp4")).unwrap(), fs::read(dir.join("two.mp4")).unwrap());
        let mine = format!("origami-{}-", process::id());
        let left = fs::read_dir(env::temp_dir())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&mine));
        assert_eq!(left.count(), 0);
        let gone = tl("text('a').for(1)\nvideo('b.mp4').trim(0, 1)");
        fs::remove_file(dir.join("b.mp4")).unwrap();
        let e = render(&gone, &out("gone.mp4"), |_| {}).unwrap_err().to_string();
        assert!(
            e.starts_with(&format!("export of {} failed at line 2 (b): ", out("gone.mp4"))),
            "{e}"
        );
        let e = render(&tl("text('a').for(1)"), &out("no/where.mp4"), |_| {})
            .unwrap_err()
            .to_string();
        assert!(e.starts_with(&format!("export of {} failed: ", out("no/where.mp4"))), "{e}");
        let e = render(&tl("stack()"), &out("empty.mp4"), |_| {}).unwrap_err().to_string();
        assert_eq!(e, "nothing to export: the timeline is empty");
    }

    // A clip that starts on a timeline frame after lengths such as 0.1 + 0.2, which add up to a hair
    // more than 0.3 in floating point, should show from that frame in the preview and the export.
    #[test]
    fn clips_meet_on_the_frame() {
        let dir = env::temp_dir().join("origami-meet-test");
        fs::create_dir_all(&dir).unwrap();
        let made = ffmpeg()
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=white:s=64x36:r=30:d=2"])
            .arg(dir.join("w.mp4"))
            .status();
        assert!(made.unwrap().success());
        let src =
            "settings({ fps: 30, size: [64, 36] })\ncat(color('#ff0000').for(0.1), color('#00ff00').for(0.2), video('w.mp4').trim(0, 1))";
        fs::write(dir.join("edit.js"), src).unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
        assert!(tl.clips[2].start > 0.3);
        let out = dir.join("meet.mp4").to_string_lossy().to_string();
        render(&tl, &out, |_| {}).unwrap();
        let args = ["-v", "error", "-i", &out, "-f", "rawvideo", "-pix_fmt", "gray", "-"];
        let px = ffmpeg().args(args).output().unwrap().stdout;
        let shown = Preview::default().frame(&tl, 9. / 30., (64, 36))[(18 * 64 + 32) * 4];
        assert!(px[9 * 64 * 36 + 18 * 64 + 32] > 200 && shown > 200);
    }

    // A clip trimmed so its media runs out a little before the clip ends, a slowed-down clip and a
    // clip at a lower frame rate than the timeline should hold their last picture to the end of the
    // clip, in the preview and in the export, instead of flashing black.
    #[test]
    fn ends_hold_the_last_frame() {
        let dir = env::temp_dir().join("origami-ends-test");
        fs::create_dir_all(&dir).unwrap();
        for (file, rate) in [("w30.mp4", 30), ("w15.mp4", 15)] {
            let src = format!("color=white:s=64x36:r={rate}:d=6");
            let made = ffmpeg()
                .args(["-v", "error", "-y", "-f", "lavfi", "-i", &src])
                .arg(dir.join(file))
                .status();
            assert!(made.unwrap().success());
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
        render(&tl, &out, |_| {}).unwrap();
        let args = ["-v", "error", "-i", &out, "-f", "rawvideo", "-pix_fmt", "gray", "-"];
        let px = ffmpeg().args(args).output().unwrap().stdout;
        let made: Vec<u8> = px.chunks(64 * 36).map(|f| f[18 * 64 + 32]).collect();
        assert_eq!(made.len(), n);
        assert!(shown.iter().chain(&made).all(|v| *v > 200), "{shown:?} {made:?}");
    }

    // An export onto a file the script reads, spelled another way (different case, a detour through
    // .., a symlink), should be refused and leave the file whole.
    #[test]
    fn keeps_sources_safe() {
        let dir = env::temp_dir().join("origami-self-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        let made = ffmpeg()
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=white:s=64x36:r=30:d=1"])
            .arg(dir.join("a.mp4"))
            .status();
        assert!(made.unwrap().success());
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
            let e = render(&tl, &out, |_| {}).unwrap_err().to_string();
            assert!(
                e.starts_with(&format!("export to {out} would overwrite ")) && e.contains(&format!("which line {line} reads")),
                "{e}"
            );
        }
        assert_eq!(fs::read(dir.join("a.mp4")).unwrap(), before);
    }

    // Shapes, lines and text too large to draw, directly or through a scale or keyed scale, should
    // stop the script with an error on their line instead of taking all memory.
    #[test]
    fn refuses_huge_layers() {
        let dir = env::temp_dir().join("origami-huge-test");
        fs::create_dir_all(&dir).unwrap();
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
        let _one = ONE.lock();
        let dir = env::temp_dir().join("origami-stall-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let made = ffmpeg()
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=white:s=64x36:r=30:d=1"])
            .arg(dir.join("a.mp4"))
            .status();
        assert!(made.unwrap().success());
        fs::write(dir.join("edit.js"), "video('a.mp4').trim(0, 1)").unwrap();
        let mut tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
        assert!(Command::new("mkfifo").arg(dir.join("never.mp4")).status().unwrap().success());
        tl.clips[0].src = dir.join("never.mp4").to_string_lossy().to_string();
        let out = dir.join("out.mp4").to_string_lossy().to_string();
        let e = render(&tl, &out, |_| {}).unwrap_err().to_string();
        assert_eq!(e, format!("export of {out} stopped: ffmpeg made no progress for 5 seconds"));
    }

    // An export's ffmpeg should be killed soon after its tie to the program is cut, which the system does
    // when the program dies, even by force quit.
    #[test]
    fn dies_with_the_program() {
        let args = ["-v", "error", "-re", "-f", "lavfi", "-i", "color=d=60", "-f", "null", "-"];
        let mut ff = ffmpeg().args(args).spawn().unwrap();
        drop(tether(ff.id()));
        let gone = (0..500).any(|_| {
            thread::sleep(Duration::from_millis(10));
            ff.try_wait().unwrap().is_some()
        });
        let _ = ff.kill();
        assert!(gone);
    }
}
