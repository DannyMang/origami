use crate::{
    Origami, pulse,
    release::{Update, VERSION, latest},
    view::DOCS,
};
use gpui::{
    Action, App, Context, Div, KeyBinding, KeyDownEvent, Keystroke, Menu, MenuItem, OsAction, PathPromptOptions, PromptButton, PromptLevel,
    Window, actions, prelude::*,
};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

actions!(
    origami,
    [
        About, Updates, Hide, HideOthers, Quit, New, Open, Export, History, Close, Minimize, Zoom, FullScreen, Docs, WhatsNew, Share
    ]
);

#[derive(Clone, PartialEq, Action)]
#[action(namespace = origami, no_json)]
pub struct Chord(pub &'static str);

#[derive(Clone, PartialEq, Action)]
#[action(namespace = origami, no_json)]
pub struct Reopen(pub PathBuf);

const CHORDS: [&str; 14] = [
    "cmd-s",
    "cmd-z",
    "shift-cmd-z",
    "cmd-x",
    "cmd-c",
    "cmd-v",
    "cmd-d",
    "cmd-a",
    "cmd-f",
    "cmd-=",
    "cmd--",
    "cmd-0",
    "alt-cmd-[",
    "alt-cmd-]",
];

pub const STARTER: &str = "stack(\n  cat(\n    color(\"#282c33\").for(5),\n  ),\n  text(\"hello origami\"),\n)\n";

pub static ARRIVED: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn local(url: &str) -> Option<PathBuf> {
    let mut bytes = url.strip_prefix("file://")?.bytes();
    let hex = |b: Option<u8>| (b? as char).to_digit(16);
    let mut out = vec![];
    while let Some(b) = bytes.next() {
        out.push(match b {
            b'%' => (hex(bytes.next())? * 16 + hex(bytes.next())?) as u8,
            b => b,
        });
    }
    String::from_utf8(out).ok().map(PathBuf::from)
}

pub fn place(p: &Path) -> (String, String) {
    let name = |s: Option<&OsStr>| s.unwrap_or_default().to_string_lossy().to_string();
    (name(p.file_name()), name(p.parent().and_then(Path::file_name)))
}

fn news() -> String {
    format!("{}/changelog.html", DOCS.trim_end_matches('/'))
}

pub fn menus(recent: &[PathBuf]) -> Vec<Menu> {
    let menu = |name: &'static str, items| Menu { name: name.into(), items };
    let key = |name: &'static str, keys| MenuItem::action(name, Chord(keys));
    let os = |name: &'static str, keys, kind| MenuItem::os_action(name, Chord(keys), kind);
    let opened = recent.iter().filter(|p| p.exists()).map(|p| {
        let (name, dir) = place(p);
        MenuItem::action(format!("{name} \u{2014} {dir}"), Reopen(p.clone()))
    });
    let share = pulse::shared().map(|on| {
        let name = if on {
            "\u{2713} Share Usage and Crash Reports"
        } else {
            "Share Usage and Crash Reports"
        };
        MenuItem::action(name, Share)
    });
    let rest = [
        MenuItem::separator(),
        MenuItem::action("Hide Origami", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::separator(),
        MenuItem::action("Quit Origami", Quit),
    ];
    vec![
        menu(
            "Origami",
            [
                MenuItem::action("About Origami", About),
                MenuItem::action("Check for Updates\u{2026}", Updates),
            ]
            .into_iter()
            .chain(share)
            .chain(rest)
            .collect(),
        ),
        menu(
            "File",
            vec![
                MenuItem::action("New", New),
                MenuItem::action("Open\u{2026}", Open),
                MenuItem::submenu(menu("Open Recent", opened.collect())),
                MenuItem::separator(),
                key("Save", "cmd-s"),
                MenuItem::action("History\u{2026}", History),
                MenuItem::action("Export\u{2026}", Export),
                MenuItem::separator(),
                MenuItem::action("Close Window", Close),
            ],
        ),
        menu(
            "Edit",
            vec![
                os("Undo", "cmd-z", OsAction::Undo),
                os("Redo", "shift-cmd-z", OsAction::Redo),
                MenuItem::separator(),
                os("Cut", "cmd-x", OsAction::Cut),
                os("Copy", "cmd-c", OsAction::Copy),
                os("Paste", "cmd-v", OsAction::Paste),
                key("Duplicate", "cmd-d"),
                os("Select All", "cmd-a", OsAction::SelectAll),
                MenuItem::separator(),
                key("Find", "cmd-f"),
            ],
        ),
        menu(
            "View",
            vec![
                MenuItem::action("Full-screen Preview", FullScreen),
                MenuItem::separator(),
                key("Bigger Text", "cmd-="),
                key("Smaller Text", "cmd--"),
                key("Actual Size", "cmd-0"),
                MenuItem::separator(),
                key("Fold", "alt-cmd-["),
                key("Unfold", "alt-cmd-]"),
            ],
        ),
        menu(
            "Window",
            vec![MenuItem::action("Minimize", Minimize), MenuItem::action("Zoom", Zoom)],
        ),
        menu(
            "Help",
            vec![MenuItem::action("Origami Docs", Docs), MenuItem::action("What's New", WhatsNew)],
        ),
    ]
}

pub fn install(cx: &mut App, recent: &[PathBuf]) {
    cx.bind_keys([
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-n", New, None),
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("shift-cmd-e", Export, None),
        KeyBinding::new("cmd-y", History, None),
        KeyBinding::new("cmd-w", Close, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("f", FullScreen, Some("menu")),
    ]);
    cx.bind_keys(CHORDS.map(|k| KeyBinding::new(k, Chord(k), None)));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &Docs, cx| cx.open_url(DOCS));
    cx.on_action(|_: &WhatsNew, cx| cx.open_url(&news()));
    route(cx, |_, _: &About, window, cx| {
        let version = format!("Version {VERSION}");
        drop(window.prompt(PromptLevel::Info, "Origami", Some(&version), &[PromptButton::ok("OK")], cx));
    });
    route(cx, |this, _: &Updates, window, cx| this.check(window, cx));
    route(cx, |this, _: &Share, _, cx| this.share(pulse::shared() != Some(true), cx));
    route(cx, |this, _: &Quit, window, cx| this.leave(window, cx));
    route(cx, |this, _: &New, _, cx| {
        this.create();
        cx.notify();
    });
    route(cx, |this, _: &Open, window, cx| this.browse(window, cx));
    route(cx, |this, Reopen(path): &Reopen, window, cx| this.switch(path.clone(), window, cx));
    cx.set_menus(menus(recent));
}

fn route<A: Action + Clone>(cx: &mut App, f: fn(&mut Origami, &A, &mut Window, &mut Context<Origami>)) {
    cx.on_action(move |a: &A, cx| {
        let a = a.clone();
        cx.defer(move |cx| {
            for handle in cx.windows().into_iter().filter_map(|w| w.downcast::<Origami>()) {
                let _ = handle.update(cx, |this, window, cx| {
                    window.activate_window();
                    f(this, &a, window, cx);
                });
            }
        });
    });
}

impl Origami {
    pub fn listen(&self, root: Div, cx: &mut Context<Self>) -> Div {
        let chord = |this: &mut Self, Chord(keys): &Chord, window: &mut Window, cx: &mut Context<Self>| {
            let keystroke = Keystroke::parse(keys).unwrap();
            this.key(&KeyDownEvent { keystroke, is_held: false }, window, cx);
            cx.stop_propagation();
        };
        root.track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Close, window, cx| this.leave(window, cx)))
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &Zoom, window, _| window.zoom_window())
            .when(!self.script.as_os_str().is_empty(), |root| {
                root.on_action(cx.listener(chord))
                    .on_action(cx.listener(|this, _: &FullScreen, _, cx| {
                        this.fullscreen();
                        cx.notify();
                    }))
                    .on_action(cx.listener(|this, _: &Export, _, cx| {
                        this.ask();
                        cx.notify();
                    }))
                    .on_action(cx.listener(|this, _: &History, _, cx| {
                        this.history(Some(0));
                        cx.notify();
                    }))
            })
    }

    fn check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let found = cx.background_executor().spawn(async { latest() });
        cx.spawn_in(window, async move |this, cx| {
            let found = found.await;
            this.update_in(cx, |this, window, cx| this.offer(found, true, window, cx))
        })
        .detach();
    }

    pub fn offer(&mut self, found: Option<Option<Update>>, asked: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(found) = &found {
            self.latest = found.clone();
            cx.notify();
        }
        let (said, detail) = match &found {
            Some(Some((tag, _, true))) => (
                format!("Origami v{tag} is available"),
                Some(format!(
                    "v{VERSION} is no longer supported. Download the new version to keep getting fixes; your scripts stay as they are."
                )),
            ),
            _ if !asked => return,
            Some(Some((tag, ..))) => (format!("Origami v{tag} is available"), None),
            Some(None) => (format!("Origami v{VERSION} is the newest version"), None),
            None => ("Could not check for updates".into(), None),
        };
        let picks = match found {
            Some(Some(_)) => [PromptButton::new("Download"), PromptButton::cancel("Later")].to_vec(),
            _ => [PromptButton::ok("OK")].to_vec(),
        };
        let answer = window.prompt(PromptLevel::Info, &said, detail.as_deref(), &picks, cx);
        cx.spawn(async move |_, cx| {
            if let (Ok(0), Some(Some((_, url, _)))) = (answer.await, found) {
                cx.update(|cx| cx.open_url(&url))?;
            }
            anyhow::Ok(())
        })
        .detach();
    }

    pub fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        };
        let picked = cx.prompt_for_paths(options);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = picked.await
                && let Some(path) = paths.pop()
            {
                let _ = this.update_in(cx, |this, window, cx| this.switch(path, window, cx));
            }
        })
        .detach();
    }

    pub fn switch(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = place(&path).0;
        let Some(path) = fs::canonicalize(&path).ok().filter(|p| p.extension().is_some_and(|e| e == "js")) else {
            let detail = format!("{name} was not opened: it is missing or not a .js script.");
            let ok = [PromptButton::ok("OK")];
            return drop(window.prompt(PromptLevel::Warning, "Origami opens .js scripts", Some(&detail), &ok, cx));
        };
        if self.take.is_some() {
            return self.note = "finish the voiceover before opening another script".into();
        }
        if path != self.script {
            self.unsaved(&format!("Save them before opening {name}?"), window, cx, move |this, _, cx| {
                this.open(path, cx);
                cx.notify();
            });
        }
    }

    pub fn unsaved(
        &mut self,
        detail: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if !self.dirty {
            return then(self, window, cx);
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "You have unsaved changes",
            Some(detail),
            &["Save", "Discard", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let pick = answer.await.unwrap_or(2);
            this.update_in(cx, |this, window, cx| {
                if pick == 0 {
                    this.clash = false;
                    this.store();
                }
                match pick {
                    0 if this.dirty => cx.notify(),
                    0 | 1 => then(this, window, cx),
                    _ => {}
                }
            })
        })
        .detach();
    }

    pub fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.unsaved("Save them before closing?", window, cx, Self::quit);
    }

    pub fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.export.is_none() {
            self.dirty = false;
            return cx.quit();
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            "An export is running. Quit and stop it?",
            None,
            &["Quit", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(0) {
                let _ = this.update(cx, |this, cx| {
                    this.dirty = false;
                    cx.quit();
                });
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ClipboardItem, Entity, TestAppContext, VisualTestContext};
    use std::{env, sync::Arc};

    fn items(menu: &Menu) -> Vec<(String, Box<dyn Action>)> {
        let item = |i: &MenuItem| match i {
            MenuItem::Action { name, action, .. } => vec![(name.to_string(), action.boxed_clone())],
            MenuItem::Submenu(m) => items(m),
            _ => vec![],
        };
        menu.items.iter().flat_map(item).collect()
    }

    fn press(cx: &mut VisualTestContext, keys: &str) -> bool {
        let typed = Keystroke::parse(keys).unwrap();
        let used = cx.update(|window, cx| window.dispatch_keystroke(typed.clone(), cx));
        let app = &mut cx.cx;
        let item = menus(&[]).iter().flat_map(items).map(|i| i.1).find(|a| {
            let keymap = app.update(|cx| cx.key_bindings());
            let first = keymap
                .borrow()
                .bindings_for_action(a.as_ref())
                .next()
                .map(|b| b.keystrokes()[0].inner().clone());
            first == Some(typed.clone())
        });
        if let Some(action) = item.filter(|a| !used && app.update(|cx| cx.is_action_available(a.as_ref()))) {
            app.update(|cx| cx.dispatch_action(action.as_ref()));
        }
        cx.run_until_parked();
        used
    }

    fn project<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Origami>, &'a mut VisualTestContext, PathBuf) {
        let dir = env::temp_dir().join(format!("origami-{name}-test"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = fs::canonicalize(&dir).unwrap().join("edit.js");
        let src = "stack(\n  color(\"#203040\").for(4),\n  text(\"a\").for(4),\n)";
        fs::write(&path, src).unwrap();
        let tl = origami_lang::eval(&path, false, origami_engine::footprint).unwrap();
        cx.update(|cx| install(cx, &[]));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (path.clone(), src.lines().map(String::from).collect(), Arc::new(tl));
            window.focus(&app.focus);
            window.activate_window();
            app
        });
        (view, cx, path)
    }

    // A file handed over by Finder arrives as a file URL whose spaces and accents should decode to the real
    // path; Open Recent lists the recent scripts that still exist, and every editing item carries a shortcut.
    #[test]
    fn urls_and_menus() {
        let film = local("file:///Users/me/My%20Films/caf%C3%A9.js");
        assert_eq!(film, Some(PathBuf::from("/Users/me/My Films/café.js")));
        assert_eq!((local("https://example.com/a.js"), local("file:///a%2")), (None, None));
        let dir = env::temp_dir().join("origami-menus-test/trip");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edit.js"), "").unwrap();
        let all: Vec<_> = menus(&[dir.join("edit.js"), dir.join("gone.js")]).iter().flat_map(items).collect();
        let reopen = |(name, a): &(String, Box<dyn Action>)| a.as_any().downcast_ref::<Reopen>().map(|r| (name.clone(), r.0.clone()));
        let recent: Vec<_> = all.iter().filter_map(reopen).collect();
        assert_eq!(recent, [("edit.js \u{2014} trip".to_string(), dir.join("edit.js"))]);
        let mut chords: Vec<_> = all
            .iter()
            .filter_map(|i| i.1.as_any().downcast_ref::<Chord>().map(|c| c.0))
            .collect();
        let mut keys = CHORDS.to_vec();
        chords.sort();
        keys.sort();
        assert_eq!(chords, keys);
    }

    // A shortcut the window already handles must not also fire its menu item: cmd+z undoes one step and cmd+v
    // pastes once whether pressed or picked from the menu, and every editing shortcut and f are used up by the
    // window, so macOS never hands them to the menu bar as well.
    #[gpui::test]
    fn menu_keys_fire_once(cx: &mut TestAppContext) {
        let (view, cx, _) = project(cx, "keys-once");
        for keys in CHORDS.iter().chain(&["f"]) {
            assert!(press(cx, keys), "{keys}");
        }
        let state = |cx: &mut VisualTestContext| view.update(cx, |o, _| (o.lines.join("\n"), o.past.len()));
        view.update(cx, |o, _| {
            (o.full, o.finding, o.cursor) = (false, None, None);
            (o.lines, o.past) = (vec!["c".into()], Default::default());
            o.past.push(vec!["a".into()]);
            o.past.push(vec!["b".into()]);
        });
        assert!(press(cx, "cmd-z"));
        assert_eq!(state(cx), ("b".into(), 1));
        cx.dispatch_action(Chord("cmd-z"));
        assert_eq!(state(cx), ("a".into(), 0));
        view.update(cx, |o, _| (o.lines, o.cursor) = (vec![String::new()], Some((0, 0))));
        cx.write_to_clipboard(ClipboardItem::new_string("xy".into()));
        assert!(press(cx, "cmd-v"));
        assert_eq!(state(cx).0, "xy");
        cx.dispatch_action(Chord("cmd-v"));
        assert_eq!(state(cx).0, "xyxy");
    }

    // On the launch screen the editing, save and export items are disabled and their keys do nothing, while
    // cmd+n still opens the new project dialog.
    #[gpui::test]
    fn launch_menus(cx: &mut TestAppContext) {
        cx.update(|cx| install(cx, &[]));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let app = crate::fresh(cx.focus_handle(), vec![]);
            window.focus(&app.focus);
            window.activate_window();
            app
        });
        let on = |a: &dyn Action| cx.cx.update(|cx| cx.is_action_available(a));
        assert_eq!([on(&Chord("cmd-z")), on(&FullScreen), on(&Export)], [false; 3]);
        assert_eq!([on(&New), on(&Open), on(&Quit), on(&Updates)], [true; 4]);
        assert!(!press(cx, "cmd-z") && press(cx, "cmd-n"));
        assert!(view.update(cx, |o, _| o.begin.is_some()));
    }

    // Quitting with cmd+q or :q while an export runs should ask first and keep the app and the export on
    // Cancel; with unsaved changes cmd+w asks about those first, and Save writes them before the export question.
    #[gpui::test]
    fn quit_asks_while_exporting(cx: &mut TestAppContext) {
        let (view, cx, path) = project(cx, "quit-export");
        let job = Arc::new(std::sync::Mutex::new(("exporting 10%".to_string(), false)));
        view.update(cx, |o, _| o.export = Some(job));
        let asked = |cx: &mut VisualTestContext| cx.pending_prompt().map(|p| p.0);
        let running = Some("An export is running. Quit and stop it?".to_string());
        cx.simulate_keystrokes("cmd-q");
        assert_eq!(asked(cx), running);
        cx.simulate_prompt_answer("Cancel");
        view.update_in(cx, |o, window, cx| o.run("q", window, cx));
        assert_eq!(asked(cx), running);
        cx.simulate_prompt_answer("Cancel");
        view.update(cx, |o, _| {
            o.lines.push("// mine".into());
            o.dirty = true;
        });
        cx.simulate_keystrokes("cmd-w");
        assert_eq!(asked(cx), Some("You have unsaved changes".into()));
        cx.simulate_prompt_answer("Save");
        cx.run_until_parked();
        assert_eq!(asked(cx), running);
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert!(fs::read_to_string(&path).unwrap().ends_with("// mine\n"));
        assert!(view.update(cx, |o, _| o.export.is_some() && !o.dirty));
    }

    // Being told to quit without a prompt, as from the Dock or at logout, should keep unsaved code in
    // Origami's own folder for the next open, while a quit that chose to discard the changes should keep nothing.
    #[gpui::test]
    fn forced_quit_keeps_unsaved(cx: &mut TestAppContext) {
        let (view, cx, path) = project(cx, "forced-quit");
        let spare = crate::crash::spare(&path);
        let _ = fs::remove_file(&spare);
        let edit = |o: &mut Origami, _: &mut Context<Origami>| {
            o.lines.push("// mine".into());
            o.dirty = true;
        };
        view.update(cx, |_, cx| cx.on_app_quit(Origami::keep).detach());
        view.update(cx, edit);
        view.update_in(cx, |o, window, cx| o.run("q!", window, cx));
        drop(view.update(cx, |o, cx| o.keep(cx)));
        assert!(!spare.exists());
        view.update(cx, edit);
        cx.cx.update(|cx| cx.shutdown());
        assert!(fs::read_to_string(spare).unwrap().ends_with("// mine\n"));
        assert!(!fs::read_to_string(path).unwrap().contains("// mine"));
    }

    // Opening another script from Open Recent, the Open dialog or Finder with unsaved changes should ask first:
    // Cancel keeps the edits, Discard opens the other script and leaves the file on disk as it was, and a
    // file that is not a .js script is refused.
    #[gpui::test]
    fn opening_keeps_unsaved(cx: &mut TestAppContext) {
        let (view, cx, path) = project(cx, "open-unsaved");
        let other = path.with_file_name("other.js");
        fs::write(&other, "stack(text(\"b\").for(1))\n").unwrap();
        view.update(cx, |o, _| {
            o.lines.push("// mine".into());
            o.dirty = true;
        });
        let shown = |cx: &mut VisualTestContext| view.update(cx, |o, _| (o.script.clone(), o.dirty, o.lines.last().cloned()));
        cx.dispatch_action(Reopen(other.clone()));
        let ask = (
            "You have unsaved changes".to_string(),
            "Save them before opening other.js?".to_string(),
        );
        assert_eq!(cx.pending_prompt(), Some(ask));
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert_eq!(shown(cx), (path.clone(), true, Some("// mine".into())));
        let arrived = local(&format!("file://{}", other.display())).unwrap();
        view.update_in(cx, |o, window, cx| o.switch(arrived, window, cx));
        cx.simulate_prompt_answer("Discard");
        cx.run_until_parked();
        assert_eq!(view.update(cx, |o, _| (o.script.clone(), o.dirty)), (other.clone(), false));
        assert!(!fs::read_to_string(&path).unwrap().contains("// mine"));
        view.update_in(cx, |o, window, cx| o.switch(path.with_file_name("clip.mp4"), window, cx));
        assert_eq!(cx.pending_prompt().map(|p| p.0), Some("Origami opens .js scripts".into()));
        cx.simulate_prompt_answer("OK");
    }

    // Check for Updates says when it could not check instead of claiming the newest version; when Origami
    // opens, an ordinary update or a failed check asks nothing and a failed check keeps the update already
    // known, while a version below the feed's minimum is told so, Download opening the release page.
    #[gpui::test]
    fn offers_updates(cx: &mut TestAppContext) {
        let (view, cx, _) = project(cx, "offer");
        let get = "https://github.com/DannyMang/origami/releases/latest".to_string();
        let up: Update = ("99.0.0".into(), get.clone(), false);
        let offer = |cx: &mut VisualTestContext, found: Option<Option<Update>>, asked: bool| {
            view.update_in(cx, |o, window, cx| o.offer(found, asked, window, cx))
        };
        offer(cx, None, true);
        assert_eq!(cx.pending_prompt().map(|p| p.0), Some("Could not check for updates".into()));
        cx.simulate_prompt_answer("OK");
        offer(cx, Some(None), true);
        assert_eq!(
            cx.pending_prompt().map(|p| p.0),
            Some(format!("Origami v{VERSION} is the newest version"))
        );
        cx.simulate_prompt_answer("OK");
        for found in [Some(Some(up.clone())), None] {
            offer(cx, found, false);
            assert!(!cx.has_pending_prompt());
        }
        assert_eq!(view.update(cx, |o, _| o.latest.clone()), Some(up));
        offer(cx, Some(Some(("99.0.0".into(), get.clone(), true))), false);
        let (said, detail) = cx.pending_prompt().unwrap();
        assert!(said == "Origami v99.0.0 is available" && detail.starts_with(&format!("v{VERSION} is no longer supported")));
        cx.simulate_prompt_answer("Download");
        cx.run_until_parked();
        assert_eq!(cx.opened_url(), Some(get));
    }
}
