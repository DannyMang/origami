use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, ErrorKind::NotFound},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::{LazyLock, Mutex, PoisonError},
};

static SPAWN: Mutex<()> = Mutex::new(());

static TOOLS: LazyLock<[PathBuf; 2]> = LazyLock::new(|| {
    let exe = env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    ["ffmpeg", "ffprobe"].map(|name| find(name, &exe, &env::var_os("PATH").unwrap_or_default()))
});

fn find(name: &str, exe: &Path, path: &OsStr) -> PathBuf {
    let known = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"].map(PathBuf::from);
    let dirs = [exe.to_path_buf()].into_iter().chain(env::split_paths(path)).chain(known);
    dirs.map(|d| d.join(name))
        .find(|p| p.is_absolute() && fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .unwrap_or_else(|| name.into())
}

pub fn ffmpeg() -> Command {
    Command::new(&TOOLS[0])
}

pub fn ffprobe() -> Command {
    Command::new(&TOOLS[1])
}

fn absent(tool: &OsStr) -> Option<String> {
    let name = Path::new(tool)
        .file_name()?
        .to_str()
        .filter(|n| ["ffmpeg", "ffprobe"].contains(n))?;
    Some(format!(
        "{name} was not found: install it with \"brew install ffmpeg\", then reopen Origami"
    ))
}

pub fn missing() -> Option<String> {
    TOOLS.iter().filter(|t| !t.is_absolute()).find_map(|t| absent(t.as_os_str()))
}

pub fn start(cmd: &mut Command) -> io::Result<Child> {
    let _one = SPAWN.lock().unwrap_or_else(PoisonError::into_inner);
    let made = cmd.spawn();
    made.map_err(|e| {
        absent(cmd.get_program())
            .filter(|_| e.kind() == NotFound)
            .map_or(e, |m| io::Error::new(NotFound, m))
    })
}

pub fn status(cmd: &mut Command) -> io::Result<ExitStatus> {
    start(cmd)?.wait()
}

pub fn output(cmd: &mut Command) -> io::Result<Output> {
    start(cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()))?.wait_with_output()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering::Relaxed},
            mpsc,
        },
        thread,
        time::Duration,
    };

    // Reading a short program's output should end when that program ends, however many long-lived
    // programs other threads start at the same moment.
    #[test]
    fn output_ends_beside_long_lived_programs() {
        let round = |_| {
            let (busy, (done, ended)) = (AtomicBool::new(true), mpsc::channel());
            thread::scope(|s| {
                for _ in 0..8 {
                    let (busy, done) = (&busy, done.clone());
                    s.spawn(move || {
                        while busy.load(Relaxed) {
                            output(&mut Command::new("true")).unwrap();
                        }
                        done.send(()).unwrap();
                    });
                }
                let hold = || (0..50).map(|_| start(Command::new("sleep").arg("60")).unwrap()).collect::<Vec<_>>();
                let held: Vec<_> = (0..4).map(|_| s.spawn(hold)).collect();
                let mut held: Vec<Child> = held.into_iter().flat_map(|h| h.join().unwrap()).collect();
                busy.store(false, Relaxed);
                let prompt = (0..8).all(|_| ended.recv_timeout(Duration::from_secs(10)).is_ok());
                held.iter_mut().for_each(|c| c.kill().and_then(|_| c.wait().map(drop)).unwrap());
                prompt
            })
        };
        assert!((0..12).all(round));
    }

    // A missing ffmpeg or ffprobe should say in plain words how to install it, and any other missing
    // program should keep the system's own error.
    #[test]
    fn missing_tool_says_how_to_install() {
        let said = |p: &str| start(&mut Command::new(p)).unwrap_err().to_string();
        let fix = "was not found: install it with \"brew install ffmpeg\", then reopen Origami";
        assert_eq!(said("/nowhere/ffmpeg"), format!("ffmpeg {fix}"));
        assert_eq!(said("/nowhere/ffprobe"), format!("ffprobe {fix}"));
        assert!(!said("/nowhere/sleep").contains("was not found"));
        assert_eq!(absent(OsStr::new("ffprobe")), Some(format!("ffprobe {fix}")));
    }

    // A copy next to the program should win over PATH, PATH should be searched in order skipping
    // relative entries and files that cannot be run, the usual install folders should be tried when
    // PATH is bare (as when opened from Finder), and a tool found nowhere should be left to the system by name.
    #[test]
    fn finds_tools_in_order() {
        let dir = env::temp_dir().join("origami-tools");
        let _ = fs::remove_dir_all(&dir);
        let [app, one, two] = ["app", "one", "two"].map(|d| dir.join(d));
        for d in [&app, &one, &two] {
            fs::create_dir_all(d).unwrap();
        }
        let made = |at: PathBuf, mode| {
            fs::write(&at, "").unwrap();
            fs::set_permissions(at, fs::Permissions::from_mode(mode)).unwrap();
        };
        for at in [app.join("ffmpeg"), two.join("ffmpeg"), two.join("ffprobe")] {
            made(at, 0o755);
        }
        made(one.join("ffprobe"), 0o644);
        let path = env::join_paths([Path::new("src"), &one, &two]).unwrap();
        assert_eq!(find("ffmpeg", &app, &path), app.join("ffmpeg"));
        assert_eq!(find("ffprobe", &app, &path), two.join("ffprobe"));
        made(one.join("ffprobe"), 0o755);
        assert_eq!(find("ffprobe", &app, &path), one.join("ffprobe"));
        assert_eq!(find("lib.rs", &app, &path), PathBuf::from("lib.rs"));
        assert!(find("ffmpeg", Path::new(""), OsStr::new("/usr/bin:/bin")).is_absolute());
    }
}
