use std::{
    env,
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};

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

#[cfg(test)]
mod tests {
    use super::*;

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
