use crate::{grade, lut::Lut, text};
use anyhow::{Context, Result, bail};
use origami_lang::{Clip, Kind, Timeline};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read},
    process::{Command, Stdio},
};

fn spot(c: &Clip, (w, h): (f64, f64), (tw, th): (f64, f64)) -> (f64, f64, f64, f64) {
    let (x, y) = (c.pos.0.map_or((w - tw) / 2., |x| x * w), c.pos.1.map_or((h - th) / 2., |y| y * h));
    (tw.round(), th.round(), x.round(), y.round())
}

pub fn place(c: &Clip, (w, h): (f64, f64), k: f64) -> (f64, f64, f64, f64) {
    let (bw, bh) = (w * c.scale, h * c.scale);
    let r = (bw / c.w as f64).min(bh / c.h as f64);
    match c.kind {
        Kind::Text => {
            let (_, tw, th) = text::raster(c, k);
            spot(c, (w, h), (tw as f64, th as f64))
        }
        _ if c.w > 0 => spot(c, (w, h), (c.w as f64 * r, c.h as f64 * r)),
        _ => (w, h, 0., 0.),
    }
}

pub fn alpha(c: &Clip, t: f64) -> f64 {
    let side = |d: f64, f: f64| if f > 0. { d / f } else { 1. };
    side(t - c.start, c.fade.0).min(side(c.end - t, c.fade.1)).clamp(0., 1.)
}

fn graph(tl: &Timeline, video: bool, from: f64) -> (Vec<String>, String) {
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
        let (len, on) = (c.end - c.start - skip, format!("enable='gte(t,{})*lt(t,{})'", c.start, c.end));
        let n = ins.iter().filter(|a| *a == "-i").count();
        let (tw, th, x, y) = place(c, size, 1.);
        let own = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        match c.kind {
            Kind::Image if video => ins.extend(own(&["-loop", "1", "-t", &len.to_string(), "-i", &c.src])),
            Kind::Text if video => {
                let file = env::temp_dir().join(format!("origami-text-{i}.bgra")).to_string_lossy().to_string();
                let _ = fs::write(&file, text::raster(c, 1.).0);
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
                let file = env::temp_dir().join(format!("origami-{i}-{name}")).to_string_lossy().to_string();
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
            f.push(match c.kind {
                Kind::Color => {
                    let [r, g, b] = text::rgb(&c.src);
                    let px = lut.map_or(vec![b, g, r], |l| l.apply(&[b, g, r, 255]));
                    let shade = mask.map_or(String::new(), |m| format!("[d{i}];[d{i}][{m}:v]overlay={on}"));
                    let fill = format!("color=0x{:02x}{:02x}{:02x}", px[2], px[1], px[0]);
                    format!("[b{b0}]drawbox=w=iw:h=ih:{fill}:t=fill:{on}{shade}[b{}]", b0 + 1)
                }
                _ => {
                    let mut s = format!("[{n}:v]setpts=(PTS-STARTPTS)/{}+{}/TB,scale={tw}:{th}", c.speed, c.start);
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
                "[{n}:a]asetpts=PTS-STARTPTS,atempo={},volume={},aresample=48000,aformat=channel_layouts=stereo",
                c.speed, c.gain
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
    let (mut a, f) = graph(tl, true, 0.);
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
    let (mut a, f) = graph(tl, false, t);
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
    for c in &tl.clips {
        let mut files = c.grade.iter().flat_map(|g| g.input_lut.iter().chain(g.look.iter().map(|l| &l.0)));
        if let Some(file) = files.find(|f| Lut::read(f).is_none()) {
            bail!("line {}: {file} is not a .cube file with a 0 to 1 domain", c.line);
        }
    }
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-nostats", "-progress", "pipe:1"])
        .args(command(tl, out))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("ffmpeg not found")?;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.parse::<f64>().ok()) {
            progress((us / 1e6 / tl.duration).clamp(0., 1.));
        }
    }
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err)?;
    if !child.wait()?.success() {
        bail!("ffmpeg: {}", err.trim());
    }
    Ok(())
}
