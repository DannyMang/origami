use std::{
    env,
    ffi::OsStr,
    fs,
    io::{
        self,
        ErrorKind::{Interrupted, NotFound},
        Read,
    },
    os::unix::{fs::PermissionsExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::{LazyLock, Mutex, Once, PoisonError},
    thread,
    time::{Duration, Instant},
};

static SPAWN: Mutex<()> = Mutex::new(());
static QUIET: Mutex<Vec<(u32, Instant)>> = Mutex::new(vec![]);
const STILL: Duration = Duration::from_secs(if cfg!(test) { 3 } else { 20 });

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

pub fn stop(pid: u32) {
    unsafe { kill(pid as i32, 9) };
}

pub struct Watch(u32);

pub fn watch(pid: u32) -> Watch {
    static WATCHER: Once = Once::new();
    WATCHER.call_once(|| {
        thread::spawn(|| {
            loop {
                thread::sleep(STILL / 8);
                let quiet = QUIET.lock().unwrap_or_else(PoisonError::into_inner);
                quiet.iter().filter(|q| q.1.elapsed() > STILL).for_each(|q| stop(q.0));
            }
        });
    });
    QUIET.lock().unwrap_or_else(PoisonError::into_inner).push((pid, Instant::now()));
    Watch(pid)
}

impl Watch {
    pub fn beat(&self) {
        let mut quiet = QUIET.lock().unwrap_or_else(PoisonError::into_inner);
        quiet.iter_mut().filter(|q| q.0 == self.0).for_each(|q| q.1 = Instant::now());
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        QUIET.lock().unwrap_or_else(PoisonError::into_inner).retain(|q| q.0 != self.0);
    }
}

static TOOLS: LazyLock<[PathBuf; 2]> = LazyLock::new(|| {
    let exe = env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    ["ffmpeg", "ffprobe"].map(|name| find(name, &exe, &env::var_os("PATH").unwrap_or_default()))
});

fn find(name: &str, exe: &Path, path: &OsStr) -> PathBuf {
    let known = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"].map(PathBuf::from);
    let dirs = [exe.to_path_buf()].into_iter().chain(env::split_paths(path)).chain(known.clone());
    dirs.map(|d| d.join(name))
        .find(|p| p.is_absolute() && fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .unwrap_or_else(|| known[0].join(name))
}

pub fn ffmpeg() -> Command {
    let mut cmd = Command::new(&TOOLS[0]);
    cmd.args(["-protocol_whitelist", "file,pipe,fd"]);
    cmd
}

pub fn ffprobe() -> Command {
    let mut cmd = Command::new(&TOOLS[1]);
    cmd.args(["-protocol_whitelist", "file", "-max_alloc", "268435456"]);
    cmd
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
    TOOLS.iter().filter(|t| !t.is_file()).find_map(|t| absent(t.as_os_str()))
}

pub fn playable(path: &Path) -> io::Result<()> {
    let named = path
        .extension()
        .is_some_and(|e| ["m3u8", "m3u"].iter().any(|x| e.eq_ignore_ascii_case(x)));
    let head = fs::metadata(path).ok().filter(|m| m.is_file()).map(|_| {
        let mut head = vec![];
        let _ = fs::File::open(path).and_then(|f| f.take(8).read_to_end(&mut head));
        head
    });
    match head {
        Some(h) if named || h.starts_with(b"#EXTM3U") || h.starts_with(b"ffconcat") => {
            Err(io::Error::other("playlist files are not supported: use a video or sound file"))
        }
        _ => Ok(()),
    }
}

pub fn literal(path: &str) -> &'static [&'static str] {
    let stills = [
        "png", "jpg", "jpeg", "bmp", "tif", "tiff", "tga", "exr", "dpx", "jp2", "qoi", "ppm", "pgm", "pam", "hdr", "sgi",
    ];
    let ext = Path::new(path)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    let (mut chars, mut marks) = (path.chars(), 0);
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.find(|c| !c.is_ascii_digit()) {
                Some('d') => marks += 1,
                Some('%') => {}
                _ => return &[],
            }
        }
    }
    match marks == 1 && stills.contains(&ext.as_str()) {
        true => &["-pattern_type", "none"],
        false => &[],
    }
}

pub fn start(cmd: &mut Command) -> io::Result<Child> {
    if absent(cmd.get_program()).is_some() {
        cmd.get_args().try_for_each(|a| playable(Path::new(a)))?;
    }
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

fn drain(pipe: Option<impl Read>, seen: &Watch) -> Vec<u8> {
    let (mut v, mut buf) = (vec![], vec![0; 1 << 16]);
    if let Some(mut p) = pipe {
        loop {
            match p.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => v.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == Interrupted => {}
                Err(_) => break,
            }
            seen.beat();
        }
    }
    v
}

pub fn output(cmd: &mut Command) -> io::Result<Output> {
    let mut child = start(cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()))?;
    let (seen, out, err) = (watch(child.id()), child.stdout.take(), child.stderr.take());
    let (stdout, stderr) = thread::scope(|s| {
        let err = s.spawn(|| drain(err, &seen));
        (drain(out, &seen), err.join().unwrap_or_default())
    });
    drop(seen);
    let status = child.wait()?;
    let name = Path::new(cmd.get_program()).file_name().unwrap_or_default().to_string_lossy();
    match status.signal() {
        Some(9) => Err(io::Error::other(format!(
            "{name} was stopped after {} seconds without output",
            STILL.as_secs()
        ))),
        _ => Ok(Output { status, stdout, stderr }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc,
    };

    // A program that sends nothing for too long is stopped and the call says so, while one that keeps
    // talking may run longer than that.
    #[test]
    fn silent_programs_are_stopped() {
        let from = Instant::now();
        let said = output(Command::new("sleep").arg("60")).unwrap_err().to_string();
        assert_eq!(said, "sleep was stopped after 3 seconds without output");
        assert!(from.elapsed() < STILL * 2);
        let chat = "for i in 1 2 3 4 5 6 7 8; do echo $i; sleep 0.5; done";
        assert_eq!(output(Command::new("/bin/sh").args(["-c", chat])).unwrap().stdout.len(), 16);
    }

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

    // A playlist, known by its name or its first bytes, is refused before ffmpeg or ffprobe starts, while
    // films, files not made yet and named pipes are passed on unread.
    #[test]
    fn playlists_never_reach_ffmpeg() {
        let dir = env::temp_dir().join("origami-lang-lists");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let film = dir.join("a.mp4");
        assert!(
            status(ffmpeg().args(["-v", "error", "-f", "lavfi", "-i", "color=s=16x16:d=1"]).arg(&film))
                .unwrap()
                .success()
        );
        let lists = [
            ("live.M3U8", "a.mp4"),
            ("a.m3u", ""),
            ("hls.mp4", "#EXTM3U\n"),
            ("cat.mov", "ffconcat version 1.0\nfile a.mp4\n"),
        ];
        for (name, text) in lists {
            fs::write(dir.join(name), text).unwrap();
            for cmd in [
                ffprobe().arg(dir.join(name)),
                ffmpeg().arg("-i").arg(dir.join(name)).args(["-f", "null", "-"]),
            ] {
                let said = output(cmd).unwrap_err().to_string();
                assert_eq!(said, "playlist files are not supported: use a video or sound file");
            }
        }
        assert!(output(ffprobe().arg(&film)).unwrap().status.success());
        assert!(status(Command::new("mkfifo").arg(dir.join("pipe.m3u8"))).unwrap().success());
        assert!(playable(&dir.join("pipe.m3u8")).is_ok() && playable(&dir.join("new.m3u8")).is_ok());
    }

    // A still image whose path holds one frame number pattern such as %d or %03d is read as that very
    // file, while other names, films and pictures that ffmpeg never reads as numbered files are left alone.
    #[test]
    fn numbered_names_are_read_as_named() {
        let none: &[&str] = &["-pattern_type", "none"];
        for path in ["a%d.png", "/x/a%05d.JPG", "/x%3d/a.tif", "a%%%d.jpeg"] {
            assert_eq!(literal(path), none, "{path}");
        }
        for path in [
            "a1.png",
            "b50%.png",
            "a%%d.png",
            "a%d%d.png",
            "a%x.png",
            "a%d.gif",
            "a%d.mp4",
            "a%d.avif",
            "a%d",
        ] {
            assert!(literal(path).is_empty(), "{path}");
        }
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

    // A copy next to the program wins over PATH, PATH is searched in order skipping relative entries and
    // files that cannot be run, the usual install folders are tried when PATH is bare, and a tool found
    // nowhere points at the Homebrew folder, never at a bare name.
    #[test]
    fn finds_tools_in_order() {
        let dir = env::temp_dir().join("origami-lang-tools");
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
        assert_eq!(find("lib.rs", &app, &path), PathBuf::from("/opt/homebrew/bin/lib.rs"));
        assert!(find("ffmpeg", Path::new(""), OsStr::new("/usr/bin:/bin")).is_absolute());
    }
}
