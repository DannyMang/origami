use crate::{
    Origami, crash,
    menu::menus,
    release::VERSION,
    view::{BAR, DIM, TEXT},
};
use gpui::{Context, Div, MouseButton, div, prelude::*, px, rgb};
use origami_engine::{Opts, sized};
use origami_lang::Timeline;
use regex::Regex;
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Read,
    mem,
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex, MutexGuard, PoisonError},
    thread,
};

const HOST: &str = "https://us.i.posthog.com";
const KEY: &str = match option_env!("POSTHOG_KEY") {
    Some(key) => key,
    None => "",
};

type Said = (String, String, usize);

static SAID: LazyLock<Mutex<Said>> = LazyLock::new(|| Mutex::new(read(&file())));

static MAC: LazyLock<String> = LazyLock::new(|| crash::reply(Command::new("/usr/bin/sw_vers").arg("-productVersion")));

fn file() -> PathBuf {
    crash::home().join("Library/Application Support/Origami/pulse")
}

fn said() -> MutexGuard<'static, Said> {
    SAID.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read(file: &Path) -> Said {
    let kept = fs::read_to_string(file).unwrap_or_default();
    let mut lines = kept.lines();
    let mut next = || lines.next().unwrap_or_default().to_string();
    (next(), next(), next().parse().unwrap_or(0))
}

fn write(file: &Path, (answer, id, mark): &Said) {
    let _ = file.parent().map(fs::create_dir_all);
    let _ = fs::write(file, format!("{answer}\n{id}\n{mark}\n"));
}

fn fresh() -> String {
    let mut raw = [0; 16];
    let read = fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut raw));
    read.map(|_| raw.iter().map(|b| format!("{b:02x}")).collect()).unwrap_or_default()
}

fn settle(said: &mut Said, yes: bool) {
    said.0 = if yes { "yes" } else { "no" }.into();
    if yes && said.1.is_empty() {
        said.1 = fresh();
    }
}

fn stance(key: &str, off: bool, answer: &str) -> Option<Option<bool>> {
    (!key.is_empty() && !off).then(|| (!answer.is_empty()).then(|| answer == "yes"))
}

fn now() -> Option<Option<bool>> {
    let off = cfg!(test) || env::var_os("ORIGAMI_NO_TELEMETRY").is_some();
    stance(KEY, off, &said().0)
}

pub fn shared() -> Option<bool> {
    now().map(|a| a == Some(true))
}

fn payload(key: &str, id: &str, event: &str, mut properties: Value) -> Value {
    properties["$process_person_profile"] = false.into();
    properties["$geoip_disable"] = true.into();
    properties["app"] = VERSION.into();
    properties["macos"] = MAC.as_str().into();
    properties["chip"] = env::consts::ARCH.into();
    json!({ "api_key": key, "event": event, "distinct_id": id, "properties": properties })
}

fn post(event: &'static str, properties: Value) {
    let id = said().1.clone();
    if now() != Some(Some(true)) || id.is_empty() {
        return;
    }
    let _ = thread::Builder::new().spawn(move || {
        let (body, to) = (payload(KEY, &id, event, properties).to_string(), format!("{HOST}/i/v0/e/"));
        let json = "Content-Type: application/json";
        origami_lang::output(Command::new("/usr/bin/curl").args(["-fs", "--max-time", "5", "-H", json, "-d", &body, &to]))
    });
}

pub fn beat(event: &'static str) {
    post(event, json!({}));
}

pub fn exported(tl: &Timeline, o: &Opts, succeeded: bool) {
    let (codec, (width, height)) = (format!("{:?}", o.codec).to_lowercase(), sized(tl, o).1);
    let seconds = o.range.map_or(tl.duration, |(a, b)| b - a).round() as u64;
    post(
        "exported",
        json!({ "codec": codec, "width": width, "height": height, "seconds": seconds, "succeeded": succeeded }),
    );
}

const SHAPES: &str = concat!(
    r"^(index out of bounds: the len is \d+ but the index is \d+$",
    r"|called `Option::unwrap\(\)` on a `None` value$",
    r"|called `Result::unwrap\(\)` on an `Err` value",
    r"|attempt to (add|subtract|multiply|negate|shift left|shift right) with overflow$",
    r"|attempt to (divide|calculate the remainder) (by|with a divisor of) zero$",
    r"|byte index \d+ is (not a char boundary|out of bounds)",
    r"|range (start|end) index \d+ out of range for slice of length \d+$",
    r"|slice index starts at \d+ but ends at \d+$",
    r"|capacity overflow$)"
);

fn crash(log: &str) -> Option<(String, String)> {
    let (at, what) = log.rsplit_once(" panicked at ")?.1.split_once(":\n")?;
    let from = at.rfind("/src/").and_then(|i| at[..i].rfind('/'));
    let from = from.or(at.rfind('/').filter(|_| at.starts_with('/'))).map_or(0, |i| i + 1);
    let fixed = |shape: &str, s: &str, or: &str| Some(Regex::new(shape).ok()?.find(s).map_or(or, |m| m.as_str()).to_string());
    let what = fixed(SHAPES, what.lines().next().unwrap_or_default(), "panic")?;
    Some((what, fixed(r"^[a-z0-9_./-]+\.rs:\d+:\d+$", &at[from..], "unknown")?))
}

pub fn launch() {
    let log = fs::read(crash::journal()).unwrap_or_default();
    let log = String::from_utf8_lossy(&log);
    let mark = mem::replace(&mut said().2, log.len());
    if mark != log.len() {
        write(&file(), &said());
    }
    if let Some((message, at)) = log.get(mark..).and_then(crash) {
        post("crash", json!({ "message": message, "at": at }));
    }
    beat("app opened");
}

impl Origami {
    pub fn share(&mut self, yes: bool, cx: &mut Context<Self>) {
        let mut said = said();
        settle(&mut said, yes);
        write(&file(), &said);
        drop(said);
        cx.set_menus(menus(&self.recent));
        cx.notify();
    }

    pub fn consent(&self, cx: &mut Context<Self>) -> Option<Div> {
        let pick = |label: &'static str, yes: bool| {
            div()
                .px_2()
                .rounded(px(3.))
                .bg(rgb(BAR))
                .text_color(rgb(TEXT))
                .cursor_pointer()
                .child(label)
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.share(yes, cx)))
        };
        (now() == Some(None)).then(|| {
            div()
                .absolute()
                .bottom(px(52.))
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(12.))
                .text_color(rgb(DIM))
                .child("Help improve Origami? Send anonymous usage and crash reports.")
                .child(pick("Yes", true))
                .child(pick("No thanks", false))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Nothing is sent, asked or shown in the menu while the key is empty or ORIGAMI_NO_TELEMETRY is
    // set; with a key the question shows until it is answered, and events go only after a yes.
    #[test]
    fn sends_only_after_a_yes() {
        for answer in ["", "yes", "no"] {
            assert_eq!(stance("", false, answer), None);
            assert_eq!(stance("key", true, answer), None);
        }
        assert_eq!(stance("key", false, ""), Some(None));
        assert_eq!(stance("key", false, "yes"), Some(Some(true)));
        assert_eq!(stance("key", false, "no"), Some(Some(false)));
        assert_eq!(stance("key", false, "junk"), Some(Some(false)));
        assert_eq!((now(), shared()), (None, None));
    }

    // An event carries the key, its name, the install id and exactly its own properties plus the
    // anonymous flags, app version, macOS version and chip.
    #[test]
    fn builds_the_payload() {
        let sent = payload("key", "abc", "exported", json!({ "codec": "h264", "seconds": 12 }));
        let properties = json!({
            "codec": "h264",
            "seconds": 12,
            "$process_person_profile": false,
            "$geoip_disable": true,
            "app": VERSION,
            "macos": MAC.as_str(),
            "chip": env::consts::ARCH,
        });
        let whole = json!({ "api_key": "key", "event": "exported", "distinct_id": "abc", "properties": properties });
        assert_eq!(sent, whole);
        assert_eq!(
            payload("key", "abc", "font picked", json!({}))["properties"]
                .as_object()
                .unwrap()
                .len(),
            5
        );
    }

    // A crash report is the newest panic's first line only when it is one of the standard library's
    // fixed messages (with any text after the fixed part dropped), otherwise just "panic", and its source
    // file without the folders above the crate, or "unknown" when that does not look like a source file.
    #[test]
    fn scrubs_a_crash() {
        let log = |at: &str, what: &str| {
            format!("Mon thread 'main' panicked at old.rs:1:1:\nold\n\nTue thread 'main' panicked at {at}:\n{what}\n   0: frame\n")
        };
        let report = |at: &str, what: &str| crash(&log(at, what)).unwrap();
        let app = "/Users/me/code/origami-app/src/view.rs:10:5";
        assert_eq!(
            report(app, "called `Result::unwrap()` on an `Err` value: missing /Users/me/trip/clip.mp4"),
            (
                "called `Result::unwrap()` on an `Err` value".into(),
                "origami-app/src/view.rs:10:5".into()
            )
        );
        for kept in [
            "called `Option::unwrap()` on a `None` value",
            "index out of bounds: the len is 3 but the index is 7",
            "attempt to subtract with overflow",
            "attempt to divide by zero",
            "range end index 9 out of range for slice of length 4",
            "capacity overflow",
        ] {
            assert_eq!(report("src/main.rs:3:9", kept).0, kept);
        }
        assert_eq!(
            report(
                "src/main.rs:3:9",
                "byte index 2 is not a char boundary; it is inside 'é' of `text(\"héllo\")`"
            )
            .0,
            "byte index 2 is not a char boundary"
        );
        for dropped in [
            "clip Acme merger teaser for Bob Smith has no frames",
            "no file at /Users/me/My Films/a.mov today",
            "\u{201c}smart quoted secret\u{201d}",
            "Token ghp_abcdefghijklmnop leaked",
            "index out of bounds: the len is 3 but the index is 7 for Bob",
            &"x".repeat(900),
        ] {
            assert_eq!(report("src/main.rs:3:9", dropped).0, "panic");
        }
        assert_eq!(
            report("/Users/me/.cargo/registry/src/index/gpui-0.2.2/src/app.rs:7:1", "x").1,
            "gpui-0.2.2/src/app.rs:7:1"
        );
        assert_eq!(report("/Users/me/odd.rs:7:1", "x").1, "odd.rs:7:1");
        assert_eq!(report("src/main.rs:3:9", "x").1, "src/main.rs:3:9");
        assert_eq!(
            report("src/main.rs:3:9", "x panicked at Bob Smith.rs:1:1:\nleak"),
            ("panic".into(), "unknown".into())
        );
        assert_eq!(crash("Mon saved unsaved code\n"), None);
    }

    // The answer, the install id and the crash mark survive a relaunch, the id is random, made only
    // on a yes and kept when the answer changes, and a missing or damaged file reads as unanswered.
    #[test]
    fn keeps_the_answer_and_id() {
        let file = env::temp_dir().join("origami-pulse-test/Origami/pulse");
        let _ = fs::remove_dir_all(file.parent().unwrap());
        assert_eq!(read(&file), (String::new(), String::new(), 0));
        let mut said = read(&file);
        settle(&mut said, false);
        assert_eq!(said, ("no".into(), String::new(), 0));
        settle(&mut said, true);
        let id = said.1.clone();
        assert!(id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) && id != fresh());
        said.2 = 77;
        write(&file, &said);
        assert_eq!(read(&file), ("yes".into(), id.clone(), 77));
        let mut again = read(&file);
        settle(&mut again, false);
        settle(&mut again, true);
        write(&file, &again);
        assert_eq!(read(&file), ("yes".into(), id, 77));
        fs::write(&file, "yes").unwrap();
        assert_eq!(read(&file), ("yes".into(), String::new(), 0));
    }
}
