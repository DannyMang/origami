use anyhow::{Result, bail};
use origami_engine::{Preview, footprint};
use std::{
    env,
    io::Write,
    process::{Command, Stdio},
};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let pos: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.starts_with("--")).collect();
    match pos[..] {
        ["ir", script] => println!(
            "{}",
            serde_json::to_string_pretty(&origami_lang::eval(script.as_ref(), true, footprint)?)?
        ),
        ["render", script, out] => {
            let tl = origami_lang::eval(script.as_ref(), flag("--relock"), footprint)?;
            if flag("--dry-run") {
                let quote = |a: String| format!("'{}'", a.replace('\'', "'\\''"));
                println!(
                    "ffmpeg {}",
                    origami_engine::command(&tl, out)
                        .into_iter()
                        .map(quote)
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            } else {
                origami_engine::render(&tl, out, |p| eprint!("\r{:3.0}%", p * 100.))?;
                eprintln!();
            }
        }
        ["frame", script, t, out] => {
            let tl = origami_lang::eval(script.as_ref(), flag("--relock"), footprint)?;
            let size = format!("{}x{}", tl.size.0, tl.size.1);
            let px = Preview::default().frame(&tl, t.parse()?, (tl.size.0 as usize, tl.size.1 as usize));
            let mut child = Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "bgra",
                    "-video_size",
                    &size,
                    "-i",
                    "-",
                    "-y",
                    out,
                ])
                .stdin(Stdio::piped())
                .spawn()?;
            child.stdin.take().unwrap().write_all(&px)?;
            if !child.wait()?.success() {
                bail!("could not write {out}");
            }
        }
        _ => bail!(
            "usage: origami render <script> <out.mp4> [--dry-run] [--relock] | origami frame <script> <seconds> <out.png> | origami ir <script>"
        ),
    }
    Ok(())
}
