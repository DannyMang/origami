use std::{env, fs, process::Command};

// The origami command evaluates a script in its helper process, reading media it has not seen before,
// and prints exactly the timeline or the error that evaluating it in-process gives, and a script that
// fills memory or hands back a timeline too big to read is stopped with a plain error instead of taking
// the Mac's memory, and reading a large timeline with a bad clip stays small in the app.
#[test]
fn scripts_run_in_a_helper() {
    let dir = env::temp_dir().join("origami-engine-helper");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("subs.srt"), "1\n00:00:01,000 --> 00:00:02,500\nhello there\n").unwrap();
    for (input, file) in [("color=black:s=64x36:d=2", "v.mp4"), ("sine=d=2", "a.wav")] {
        let made = origami_lang::status(
            origami_lang::ffmpeg()
                .args(["-v", "error", "-f", "lavfi", "-i", input])
                .arg(dir.join(file)),
        );
        assert!(made.unwrap().success());
    }
    let script = dir.join("edit.js");
    let there = |src: &str| {
        fs::write(&script, src).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_origami")).arg("ir").arg(&script).output().unwrap();
        String::from_utf8_lossy(if out.status.success() { &out.stdout } else { &out.stderr }).into_owned()
    };
    for src in [
        "settings({ fps: 24 })
const t = text('hi', { size: 80, align: 'left' }).for(2.5).pos(keys([0, 0.1], [2, 0.9, 'bounceOut']), 0.3).rotate(t => t * 7.3)
const tag = text('tag').for(3).pos(t.pos.x, add(t.pos.y, 0.05)).opacity(mul(t.opacity, 0.5)).scale(keys([0, 1], ['50%', 1.7]))
stack(cat(t, rect(300, 80, { radius: 9 }).for(1).loop(2)), tag, line(0, 0, 1, 1).speed(2).saturation(40),
  text('a b c').each('word', (i, n) => ({ opacity: delay(t => t, i / n) })), captions('subs.srt'))",
        "cat(video('v.mp4').trim(0.5), sound('a.wav').volume(0.5), video('./v.mp4'))",
        "text('a')\n  video('none.mp4')",
        "text('a')\nconst x = (",
        "const t = text('a')\nt.$.font = '/etc/hosts'\nt",
    ] {
        let said = there(src);
        let here = origami_lang::eval(&script, true, origami_engine::footprint);
        let here = here.map_or_else(|e| format!("Error: {e}"), |t| serde_json::to_string_pretty(&t).unwrap());
        assert_eq!(
            said.lines().take(here.lines().count()).collect::<Vec<_>>().join("\n"),
            here,
            "{src}"
        );
    }
    let fat = there("text('a')\nconst a = []\nfor (let i = 0; i < 400; i++) a.push(new Float64Array(2 ** 23))\ntext('b')");
    assert!(fat.starts_with("Error: script used too much memory: stopped at 2048 MB"), "{fat}");
    let big = there("const t = text('a')\nt.$.label = 'a'.repeat(1 << 12).repeat(1 << 12)\nt");
    assert!(
        big.starts_with("Error: the timeline is over 16 MB as text"),
        "{}",
        big.lines().next().unwrap_or_default()
    );
    fs::write(
        &script,
        "const a = text('a').for(1)\na.$.opacity = [1, new Array(3e6).fill(0)]\nconst b = text('b').for(1)\nb.$.kind = 'bogus'\ncat(a, b)",
    )
    .unwrap();
    let out = Command::new("/usr/bin/time")
        .arg("-l")
        .arg(env!("CARGO_BIN_EXE_origami"))
        .arg("ir")
        .arg(&script)
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    let peak: u64 = said
        .lines()
        .find_map(|l| l.strip_suffix("peak memory footprint")?.trim().parse().ok())
        .unwrap();
    assert!(
        said.starts_with("Error: line 3: unknown variant `bogus`") && peak < 128 << 20,
        "{said}"
    );
}
