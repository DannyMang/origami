use std::{env, fs, process::Command};

// The origami command evaluates a script in its helper process, reading media it has not seen before,
// and prints exactly the timeline or the error that evaluating it in-process gives, and a script that
// fills memory, hands back a timeline too big to read or nests brackets so deep behind a quote mark in
// a pattern that reading it crashes the helper is stopped with a plain error instead of taking the
// Mac's memory, and reading a large timeline with a bad clip stays small in the app.
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
    let deep = there(&format!("const r = /'/, x = {}{}\ntext('a')", "[".repeat(20000), "]".repeat(20000)));
    assert!(deep.ends_with("Error: line 1: brackets are nested more than 100 deep\n"), "{deep}");
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

// A still written onto a picture or video the script reads, or through a link to one, is refused naming
// the line that reads it and leaves the file whole, while a still onto a new name is written.
#[test]
fn frames_keep_sources_safe() {
    let dir = env::temp_dir().join("origami-engine-frame");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (input, file) in [("color=red:s=64x36:d=1", "pic.png"), ("color=blue:s=64x36:d=1", "v.mp4")] {
        let made = origami_lang::status(
            origami_lang::ffmpeg()
                .args(["-v", "error", "-f", "lavfi", "-i", input, "-frames:v", "1"])
                .arg(dir.join(file)),
        );
        assert!(made.unwrap().success());
    }
    std::os::unix::fs::symlink(dir.join("v.mp4"), dir.join("link.mp4")).unwrap();
    fs::write(
        dir.join("edit.js"),
        "settings({ size: [64, 36] })\nstack(image('pic.png').for(1),\n  video('v.mp4'))",
    )
    .unwrap();
    let before = [fs::read(dir.join("pic.png")).unwrap(), fs::read(dir.join("v.mp4")).unwrap()];
    let frame = |out: &str| {
        let made = Command::new(env!("CARGO_BIN_EXE_origami"))
            .arg("frame")
            .arg(dir.join("edit.js"))
            .arg("0")
            .arg(dir.join(out))
            .output()
            .unwrap();
        (made.status.success(), String::from_utf8_lossy(&made.stderr).into_owned())
    };
    for (out, line) in [("pic.png", 2), ("v.mp4", 3), ("link.mp4", 3)] {
        let (done, said) = frame(out);
        assert!(
            !done && said.contains("would overwrite") && said.contains(&format!("which line {line} reads")),
            "{said}"
        );
    }
    assert_eq!(
        [fs::read(dir.join("pic.png")).unwrap(), fs::read(dir.join("v.mp4")).unwrap()],
        before
    );
    assert!(frame("still.png").0 && dir.join("still.png").is_file());
}

// A path that is not valid text, as a shell can pass, gives a plain error instead of a crash.
#[test]
fn odd_paths_are_errors() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let odd = OsStr::from_bytes(b"/nowhere/x\xff.js");
    let out = Command::new(env!("CARGO_BIN_EXE_origami")).arg("ir").arg(odd).output().unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && said.starts_with("Error: missing script: /nowhere/x"),
        "{said}"
    );
}
