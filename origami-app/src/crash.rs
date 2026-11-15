use std::{
    backtrace::Backtrace,
    env,
    fs::{self, OpenOptions},
    io::Write,
    panic,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    thread,
};

pub static UNSAVED: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

pub fn spare(script: &Path) -> PathBuf {
    let mut name = script.as_os_str().to_owned();
    name.push(".recovered");
    name.into()
}

fn rescue(what: &str, log: &Path) {
    if let Some((script, text)) = UNSAVED.try_lock().ok().and_then(|u| u.clone()) {
        let _ = fs::write(spare(&script), text);
    }
    let when = Command::new("date")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let me = thread::current();
    let entry = format!(
        "{} thread '{}' {what}\n{}\n",
        when.unwrap_or_default(),
        me.name().unwrap_or("<unnamed>"),
        Backtrace::force_capture()
    );
    let _ = log.parent().map(fs::create_dir_all);
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .and_then(|mut f| f.write_all(entry.as_bytes()));
}

pub fn guard() {
    let log = PathBuf::from(env::var("HOME").unwrap_or_default()).join("Library/Logs/Origami/origami.log");
    let default = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        origami_engine::halt();
        rescue(&info.to_string(), &log);
        default(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    // A crash with unsaved changes should keep the code on screen next to the script and log the
    // panic, and opening the script again should bring that code back with the keep mine / use disk bar.
    #[gpui::test]
    fn crash_keeps_unsaved(cx: &mut TestAppContext) {
        let dir = env::temp_dir().join("origami-crash-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (script, log) = (dir.join("edit.js"), dir.join("Logs/origami.log"));
        fs::write(&script, "text(\"a\")\n").unwrap();
        *UNSAVED.lock().unwrap() = Some((script.clone(), "text(\"b\")\n".into()));
        rescue("panicked at here:\nboom", &log);
        *UNSAVED.lock().unwrap() = None;
        assert_eq!(fs::read_to_string(spare(&script)).unwrap(), "text(\"b\")\n");
        assert!(fs::read_to_string(&log).unwrap().contains("panicked at here:\nboom\n"));
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.open(script.clone());
            app
        });
        cx.run_until_parked();
        view.update(cx, |o, cx| o.watch(cx));
        cx.run_until_parked();
        let shown = view.update(cx, |o, _| (o.lines.clone(), o.clash, o.dirty));
        assert_eq!(shown, (vec!["text(\"b\")".to_string()], true, true));
        assert!(!spare(&script).exists());
    }
}
