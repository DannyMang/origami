use std::{
    env, fs,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

// An `origami render` that is force quit, or ended together with every program it started, as ctrl-C
// in a terminal does, should leave neither a hidden half-written file beside its output nor its
// temporary folder.
#[test]
fn killed_renders_leave_nothing() {
    let dir = env::temp_dir().join("origami-engine-killed");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("edit.js"), "stack(color('#203040').for(900), text('hi').for(900))").unwrap();
    for target in ["", "-"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_origami"))
            .arg("render")
            .args([dir.join("edit.js"), dir.join("out.mp4")])
            .process_group(0)
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let room = env::temp_dir().join(format!("origami-{}-0", child.id()));
        let left = || (fs::read_dir(&dir).unwrap().count() > 1, room.exists());
        let wait = |want| {
            let since = Instant::now();
            while left() != want && since.elapsed() < Duration::from_secs(30) {
                thread::sleep(Duration::from_millis(50));
            }
            left()
        };
        let before = wait((true, true));
        let sign = if target.is_empty() { "-KILL" } else { "-TERM" };
        let sent = Command::new("/bin/kill")
            .args([sign, "--", &format!("{target}{}", child.id())])
            .status();
        assert!(sent.unwrap().success() && !child.wait().unwrap().success());
        assert_eq!((before, wait((false, false))), ((true, true), (false, false)), "{sign}");
    }
}
