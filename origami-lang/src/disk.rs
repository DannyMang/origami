use std::{
    ffi::{CString, c_char, c_void},
    fs,
    io::{self, Write},
    os::unix::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::Path,
    process, ptr,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};

const ACL_XATTR_NOFOLLOW: u32 = 1 | 4 | 1 << 18;
const NONBLOCK_NOFOLLOW: i32 = 0x4 | 0x100;

unsafe extern "C" {
    fn copyfile(from: *const c_char, to: *const c_char, state: *mut c_void, flags: u32) -> i32;
}

pub fn save(path: &Path, data: impl AsRef<[u8]>) -> io::Result<()> {
    static PARTS: AtomicU64 = AtomicU64::new(0);
    let old = fs::symlink_metadata(path).ok().filter(|m| m.is_file());
    if old.is_some() {
        fs::OpenOptions::new().write(true).custom_flags(NONBLOCK_NOFOLLOW).open(path)?;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let part = path.with_file_name(format!(".{name}.{}-{}", process::id(), PARTS.fetch_add(1, Relaxed)));
    let _ = fs::remove_file(&part);
    let wrote = fs::File::create_new(&part).and_then(|mut f| {
        if let Some(m) = &old {
            f.set_permissions(m.permissions())?;
            let c = |p: &Path| CString::new(p.as_os_str().as_bytes()).unwrap_or_default();
            unsafe { copyfile(c(path).as_ptr(), c(&part).as_ptr(), ptr::null_mut(), ACL_XATTR_NOFOLLOW) };
        }
        f.write_all(data.as_ref())?;
        fs::rename(&part, path)
    });
    if wrote.is_err() {
        let _ = fs::remove_file(&part);
    }
    wrote
}

pub fn load(path: &Path) -> io::Result<String> {
    match fs::metadata(path)? {
        m if !m.is_file() => Err(io::Error::other("not a plain file")),
        m if m.len() > 1 << 24 => Err(io::Error::other("the file is over 16 MB")),
        _ => fs::read_to_string(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{output, status, tests::scratch};
    use std::{io::Read, os::unix::fs::MetadataExt, process::Command};

    // A save writes a hidden file beside the old one and moves it into place, keeping the old file's
    // permissions and extended attributes; a read-only file is left as it was, a link or pipe in its place
    // is replaced instead of written through, and a save that fails leaves no hidden file behind.
    #[test]
    fn saves_whole_files() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("save");
        let (file, other) = (dir.join("edit.js"), dir.join("other"));
        let xattr = |args: &[&str]| output(Command::new("/usr/bin/xattr").args(args).arg(&file)).unwrap();
        fs::write(&file, "old").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        xattr(&["-w", "origami.test", "kept"]);
        let inode = fs::metadata(&file).unwrap().ino();
        save(&file, "new").unwrap();
        let after = fs::metadata(&file).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        assert_eq!((after.ino() != inode, after.permissions().mode() & 0o777), (true, 0o640));
        assert_eq!(xattr(&["-p", "origami.test"]).stdout, b"kept\n");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o440)).unwrap();
        assert!(save(&file, "newer").is_err() && fs::read_to_string(&file).unwrap() == "new");
        fs::remove_file(&file).unwrap();
        fs::write(&other, "keep").unwrap();
        std::os::unix::fs::symlink(&other, &file).unwrap();
        save(&file, "a").unwrap();
        assert!(!file.is_symlink() && fs::read_to_string(&other).unwrap() == "keep");
        fs::remove_file(&file).unwrap();
        assert!(status(Command::new("mkfifo").arg(&file)).unwrap().success());
        save(&file, "b").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "b");
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(save(&file, "c").is_err());
        let names: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 2);
    }

    // A save writes a whole new file and swaps it in, so the old file is never cut short and nothing is
    // left beside it, keeps the file's permissions, replaces a link instead of writing through it, and
    // refuses a read-only file, a folder or a missing folder, leaving each as it was.
    #[test]
    fn saves_by_swapping_in_a_whole_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("swap");
        let path = dir.join("edit.js");
        fs::write(&path, "old\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let mut held = fs::File::open(&path).unwrap();
        save(&path, "new\n").unwrap();
        let mut was = String::new();
        held.read_to_string(&mut was).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            (was.as_str(), fs::read_to_string(&path).unwrap().as_str(), mode),
            ("old\n", "new\n", 0o640)
        );
        fs::write(dir.join("keep.js"), "k\n").unwrap();
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(dir.join("keep.js"), &path).unwrap();
        save(&path, "mine\n").unwrap();
        assert_eq!(fs::read_to_string(dir.join("keep.js")).unwrap(), "k\n");
        assert!(!path.is_symlink() && fs::read_to_string(&path).unwrap() == "mine\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        fs::create_dir(dir.join("folder.js")).unwrap();
        for p in [path.clone(), dir.join("folder.js"), dir.join("gone/edit.js")] {
            assert!(save(&p, "lost\n").is_err(), "{}", p.display());
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "mine\n");
        let mut left: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        left.sort();
        assert_eq!(left, ["edit.js", "folder.js", "keep.js"]);
    }
}
