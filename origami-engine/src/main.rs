use anyhow::{Result, bail};
use std::env;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let pos: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.starts_with("--")).collect();
    match pos[..] {
        ["ir", script] => println!("{}", serde_json::to_string_pretty(&origami_lang::eval(script.as_ref(), true)?)?),
        ["render", script, out] => {
            let tl = origami_lang::eval(script.as_ref(), flag("--relock"))?;
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
        _ => bail!("usage: origami render <script> <out.mp4> [--dry-run] [--relock] | origami ir <script>"),
    }
    Ok(())
}
