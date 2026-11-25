use lru::LruCache;
use origami_lang::{load, save};
use std::{
    env, fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
    thread,
    time::Duration,
};

const CAP: usize = 4 << 20;

type Memo = LruCache<String, String>;

static MEMO: LazyLock<Mutex<(Memo, bool)>> = LazyLock::new(|| Mutex::new((read(&load(&file()).unwrap_or_default()), false)));

fn file() -> PathBuf {
    let home = match cfg!(test) {
        true => env::temp_dir().join("origami-home-test"),
        false => env::home_dir().filter(|h| h.is_absolute()).unwrap_or_else(|| "/var/empty".into()),
    };
    home.join("Library/Caches/Origami/media.json")
}

fn read(text: &str) -> Memo {
    let mut memo = Memo::unbounded();
    for (key, told) in serde_json::from_str::<Vec<(String, String)>>(text).unwrap_or_default() {
        memo.put(key, told);
    }
    memo
}

fn write(memo: &mut Memo) -> String {
    let mut size = 0;
    let fits = |(key, told): &(&String, &String)| {
        size += key.len() + told.len();
        size <= CAP
    };
    let keep = memo.iter().take_while(fits).count();
    while memo.len() > keep {
        memo.pop_lru();
    }
    serde_json::to_string(&memo.iter().rev().collect::<Vec<_>>()).unwrap_or_default()
}

fn stamp(path: &Path) -> Option<String> {
    let m = fs::metadata(path).ok()?;
    let times = [m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec()];
    Some(format!("{} {} {} {times:?}", m.dev(), m.ino(), m.size()))
}

pub fn key(cmd: &Command, src: &str) -> Option<String> {
    let (tool, src) = (Path::new(cmd.get_program()), Path::new(src));
    let args: Vec<_> = cmd.get_args().collect();
    let stamps = format!("{args:?}\n{}\n{}\n{}", stamp(src)?, tool.display(), stamp(tool)?);
    Some(stamps).filter(|_| src.is_absolute())
}

pub fn memo(key: Option<String>, make: impl FnOnce() -> (String, bool)) -> String {
    let known = key.as_ref().and_then(|k| MEMO.lock().unwrap().0.get(k).cloned());
    known.unwrap_or_else(|| {
        let (told, keep) = make();
        if let Some(key) = key.filter(|_| keep) {
            let mut memo = MEMO.lock().unwrap();
            memo.0.put(key, told.clone());
            if !std::mem::replace(&mut memo.1, true) {
                thread::spawn(store);
            }
        }
        told
    })
}

fn store() {
    thread::sleep(Duration::from_secs(2));
    let text = {
        let mut memo = MEMO.lock().unwrap();
        memo.1 = false;
        write(&mut memo.0)
    };
    let file = file();
    let _ = file.parent().map(|d| fs::DirBuilder::new().recursive(true).mode(0o700).create(d));
    let _ = save(&file, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::scratch;

    fn asked(dir: &Path) -> Option<String> {
        key(Command::new(dir.join("tool")).arg("-i"), &dir.join("a.mp4").to_string_lossy())
    }

    // An answer worth keeping is asked for once and then remembered; one that is not is asked for again.
    #[test]
    fn kept_answers_are_not_asked_twice() {
        let dir = scratch("memo-hit");
        ["tool", "a.mp4"].iter().for_each(|f| fs::write(dir.join(f), "one").unwrap());
        let mut asks = 0;
        let mut ask = |keep| {
            memo(asked(&dir), || {
                asks += 1;
                (asks.to_string(), keep)
            })
        };
        assert_eq!([ask(false), ask(true), ask(true)], ["1", "2", "2"]);
    }

    // A media file that was replaced, a different ffmpeg, a missing file and a path that is not
    // absolute should all be asked about afresh instead of answered from memory.
    #[test]
    fn changed_files_and_tools_ask_again() {
        let dir = scratch("memo-miss");
        ["tool", "a.mp4"].iter().for_each(|f| fs::write(dir.join(f), "one").unwrap());
        let first = asked(&dir).unwrap();
        assert_eq!(asked(&dir).unwrap(), first);
        fs::write(dir.join("a.mp4"), "two").unwrap();
        let second = asked(&dir).unwrap();
        fs::write(dir.join("tool"), "two").unwrap();
        let third = asked(&dir).unwrap();
        assert!(first != second && second != third);
        fs::remove_file(dir.join("a.mp4")).unwrap();
        assert!(asked(&dir).is_none() && key(&Command::new(dir.join("tool")), "tool").is_none());
    }

    // A cache file that is garbage, cut short or the wrong shape should count as empty.
    #[test]
    fn broken_cache_files_are_ignored() {
        let mut memo = read(r#"[["a","1"],["b","2"]]"#);
        assert_eq!(memo.len(), 2);
        let whole = write(&mut memo);
        for text in [
            "",
            "\0\u{1}",
            &whole[..whole.len() - 3],
            r#"{"a":"1"}"#,
            r#"[["a",1]]"#,
            r#"[["a"]]"#,
        ] {
            assert_eq!(read(text).len(), 0);
        }
    }

    // The cache file should stay under its size limit by dropping the answers used longest ago, and
    // should come back in the same order.
    #[test]
    fn old_answers_are_dropped_at_the_limit() {
        let mut memo = Memo::unbounded();
        (0..6).for_each(|i| drop(memo.put(i.to_string(), "x".repeat(CAP / 4))));
        memo.get("2");
        let text = write(&mut memo);
        let kept = |memo: &Memo| memo.iter().map(|e| e.0.clone()).collect::<Vec<_>>();
        assert!(text.len() < CAP + 64 && kept(&memo) == ["2", "5", "4"]);
        assert_eq!(kept(&read(&text)), kept(&memo));
    }
}
