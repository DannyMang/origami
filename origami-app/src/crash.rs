use crate::Origami;
use gpui::Context;
use std::{
    backtrace::Backtrace,
    env,
    fs::{self, OpenOptions},
    future::{Ready, ready},
    io::Write,
    os::unix::ffi::OsStrExt,
    panic,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    thread,
    time::Duration,
};

pub static UNSAVED: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);
pub const MARK: char = '\u{1e}';

pub fn home() -> PathBuf {
    match cfg!(test) {
        true => env::temp_dir().join("origami-home-test"),
        false => env::home_dir().filter(|h| h.is_absolute()).unwrap_or_else(|| "/var/empty".into()),
    }
}

pub fn journal() -> PathBuf {
    home().join("Library/Logs/Origami/origami.log")
}

pub fn reply(cmd: &mut Command) -> String {
    origami_lang::output(cmd)
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn kept(script: &Path, what: &str) -> PathBuf {
    let key = script
        .as_os_str()
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |h: u64, b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
    home()
        .join("Library/Application Support/Origami")
        .join(what)
        .join(format!("{key:016x}"))
}

pub fn spare(script: &Path) -> PathBuf {
    kept(script, "recovered")
}

fn stash(script: &Path, text: &str) {
    let spare = spare(script);
    let _ = spare.parent().map(fs::create_dir_all);
    let _ = origami_lang::save(&spare, format!("{}\0{text}", script.to_string_lossy()));
}

impl Origami {
    pub fn keep(&mut self, _: &mut Context<Self>) -> Ready<()> {
        if self.dirty {
            stash(&self.script, &self.text());
        }
        if let Some(s) = &self.shelf {
            s.drain(true, Duration::from_millis(240));
        }
        ready(())
    }
}

pub fn record(what: &str, at: &str, log: &Path) {
    let when = reply(&mut Command::new("/bin/date"));
    let me = thread::current();
    let entry = format!(
        "{} thread '{}' {what}\n{}\n",
        when,
        me.name().unwrap_or("<unnamed>"),
        Backtrace::force_capture()
    );
    let entry = format!("{}{MARK}{}\n", entry.replace(MARK, ""), at.replace([MARK, '\n'], " "));
    let _ = log.parent().map(fs::create_dir_all);
    if fs::symlink_metadata(log).is_ok_and(|m| m.len() > 4 << 20) {
        let _ = fs::rename(log, log.with_extension("log.old"));
    }
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .and_then(|mut f| f.write_all(entry.as_bytes()));
}

fn rescue(what: &str, at: &str, log: &Path) {
    if let Some((script, text)) = UNSAVED.try_lock().ok().and_then(|u| u.clone()) {
        stash(&script, &text);
    }
    record(what, at, log);
}

pub fn guard() {
    let log = journal();
    let default = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        origami_engine::halt();
        let at = info
            .location()
            .map_or(String::new(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let first = info.payload_as_str().unwrap_or_default().lines().next().unwrap_or_default();
        rescue(&info.to_string(), &format!("{at}\t{first}"), &log);
        default(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use std::time::Instant;

    // A crash with unsaved changes should keep the code on screen in Origami's own folder under a name
    // made from the script's path, replacing a link there instead of writing through it, and log the
    // panic; opening the script again should bring that code back with the keep mine / use disk bar,
    // while a link there, code kept for another script under the same name or a .recovered file
    // shipped beside the script is never read.
    #[gpui::test]
    fn crash_keeps_unsaved(cx: &mut TestAppContext) {
        let dir = env::temp_dir().join("origami-crash-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (script, log) = (dir.join("edit.js"), dir.join("Logs/origami.log"));
        fs::write(&script, "text(\"a\")\n").unwrap();
        fs::write(dir.join("other"), "precious").unwrap();
        let _ = fs::remove_file(spare(&script));
        fs::create_dir_all(spare(&script).parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(dir.join("other"), spare(&script)).unwrap();
        *UNSAVED.lock().unwrap() = Some((script.clone(), "text(\"b\")\n".into()));
        rescue("panicked at here:\nboom", "here\tboom", &log);
        *UNSAVED.lock().unwrap() = None;
        assert_eq!(
            fs::read_to_string(spare(&script)).unwrap(),
            format!("{}\0text(\"b\")\n", script.display())
        );
        assert_eq!(fs::read_to_string(dir.join("other")).unwrap(), "precious");
        let logged = fs::read_to_string(&log).unwrap();
        assert!(logged.contains("panicked at here:\nboom\n") && logged.ends_with("\u{1e}here\tboom\n"));
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.open(script.clone(), cx);
            app
        });
        cx.run_until_parked();
        view.update(cx, |o, cx| o.watch(cx));
        cx.run_until_parked();
        let shown = view.update(cx, |o, _| (o.lines.clone(), o.clash, o.dirty));
        assert_eq!(shown, (vec!["text(\"b\")".to_string()], true, true));
        assert!(!spare(&script).exists());
        std::os::unix::fs::symlink(dir.join("other"), spare(&script)).unwrap();
        view.update(cx, |o, cx| o.open(script.clone(), cx));
        assert!(!view.update(cx, |o, _| o.dirty) && spare(&script).is_symlink());
        fs::remove_file(spare(&script)).unwrap();
        fs::write(dir.join("edit.js.recovered"), "text(\"c\")\n").unwrap();
        view.update(cx, |o, cx| o.open(script.clone(), cx));
        assert!(!view.update(cx, |o, _| o.dirty));
        fs::write(spare(&script), format!("{}\0text(\"d\")\n", dir.join("other.js").display())).unwrap();
        view.update(cx, |o, cx| o.open(script.clone(), cx));
        assert!(!view.update(cx, |o, _| o.dirty) && spare(&script).exists());
    }

    // Quitting while another program holds the script's history lock gives up within a quarter of a
    // second, and the version it could not write is written by the next drain.
    #[gpui::test]
    fn quits_past_a_held_history(cx: &mut TestAppContext) {
        let dir = env::temp_dir().join("origami-quit-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("edit.js");
        fs::write(&script, "a\n").unwrap();
        let history = kept(&script, "history");
        let _ = fs::remove_dir_all(&history);
        fs::create_dir_all(&history).unwrap();
        let held = fs::File::create(history.join("lock")).unwrap();
        held.lock().unwrap();
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.open(script.clone(), cx);
            app
        });
        let start = Instant::now();
        let shelf = view.update(cx, |o, cx| {
            o.shelve("b\n".into());
            drop(o.keep(cx));
            o.shelf.clone().unwrap()
        });
        assert!(start.elapsed() < Duration::from_millis(400) && shelf.list().is_empty());
        drop(held);
        assert!(shelf.drain(true, Duration::from_secs(5)) && shelf.list().len() == 1);
    }
}
