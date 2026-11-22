use crate::{
    Origami, crash,
    menu::STARTER,
    mouse::recent,
    ship::{button, frame, hint, home, row},
    view::{BAR, BG, BLUE, BORDER, DIM, MUTED, RED},
};
use gpui::{Context, Div, Keystroke, MouseButton, MouseDownEvent, SharedString, Window, div, prelude::*, px, rgb};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Begin {
    name: String,
    dir: PathBuf,
    folders: Vec<String>,
    note: String,
}

fn last() -> PathBuf {
    recent().with_file_name("projects")
}

fn folders(dir: &Path) -> Vec<String> {
    let mut all: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.'))
        .collect();
    all.sort_by_key(|n| n.to_lowercase());
    all
}

impl Origami {
    pub fn create(&mut self) {
        let last = origami_lang::load(&last()).map(PathBuf::from).ok();
        let dir = last.filter(|d| d.is_dir()).unwrap_or(crash::home().join("Movies/Origami"));
        self.begin = Some(Begin {
            name: String::new(),
            folders: folders(&dir),
            dir,
            note: String::new(),
        });
    }

    fn enter(&mut self, dir: PathBuf) {
        if let Some(b) = &mut self.begin {
            (b.folders, b.dir) = (folders(&dir), dir);
            b.note.clear();
        }
    }

    pub fn name(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = &mut self.begin else { return };
        if key.modifiers.platform || key.modifiers.control {
            return cx.propagate();
        }
        b.note.clear();
        match key.key.as_str() {
            "escape" => self.begin = None,
            "enter" => self.found(window, cx),
            "backspace" => drop(b.name.pop()),
            _ => {
                let typed = key.key_char.as_deref().unwrap_or_default().chars();
                b.name.extend(typed.filter(|c| !(c.is_control() || matches!(c, '/' | ':'))));
                b.name = b.name.trim_start_matches(['.', ' ']).into();
            }
        }
        cx.notify();
    }

    fn found(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = &mut self.begin else { return };
        let name = b.name.trim().to_string();
        let path = b.dir.join(&name).join("edit.js");
        if name.is_empty() {
            return b.note = "Give the project a name.".into();
        }
        if path.exists() {
            return b.note = format!("{name} already has a script here: pick another name or folder.");
        }
        self.unsaved(&format!("Save them before creating {name}?"), window, cx, move |this, _, cx| {
            let made = fs::create_dir_all(path.parent().unwrap()).and_then(|_| fs::write(&path, STARTER));
            match (made, &mut this.begin) {
                (Err(e), Some(b)) => b.note = format!("Could not create it: {e}"),
                _ => {
                    let dir = path.parent().and_then(Path::parent).unwrap_or(Path::new("/"));
                    let _ = fs::create_dir_all(last().parent().unwrap());
                    let _ = origami_lang::save(&last(), dir.to_string_lossy().to_string());
                    this.begin = None;
                    this.open(fs::canonicalize(&path).unwrap_or(path), cx);
                }
            }
            cx.notify();
        });
    }

    pub fn starter(&self, cx: &mut Context<Self>) -> Option<Div> {
        let b = self.begin.as_ref()?;
        let shut = |this: &mut Self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.begin = None;
            cx.notify();
        };
        let naming = div()
            .w_full()
            .h(px(30.))
            .px(px(10.))
            .flex()
            .items_center()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BLUE))
            .rounded(px(6.))
            .child(b.name.clone())
            .child(div().w(px(1.)).h(px(16.)).bg(rgb(BLUE)))
            .children(b.name.is_empty().then(|| div().pl(px(6.)).text_color(rgb(DIM)).child("my film")));
        let folder = |label: String, to: PathBuf| {
            div()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(BAR)))
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.enter(to.clone());
                        cx.notify();
                    }),
                )
        };
        let up = b.dir.parent().map(|p| folder("..".into(), p.to_path_buf()).text_color(rgb(MUTED)));
        let inside = b.folders.iter().map(|n| folder(n.clone(), b.dir.join(n)));
        let list = div()
            .id(SharedString::from(b.dir.to_string_lossy().to_string()))
            .w_full()
            .h(px(190.))
            .p(px(2.))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .bg(rgb(BG))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(6.))
            .children(up)
            .children(inside);
        let made = home(&b.dir.join(b.name.trim()).join("edit.js"));
        let said = match b.note.is_empty() {
            true => hint(&format!("Creates {made}")),
            false => hint(&b.note).text_color(rgb(RED)),
        };
        let body = div()
            .child(row("Name", div().child(naming)))
            .child(row("Location", div().child(hint(&home(&b.dir))).child(list)));
        let foot = div()
            .child(said.flex_1().min_w_0())
            .child(button("Cancel", false).on_mouse_down(MouseButton::Left, cx.listener(shut)))
            .child(
                button("Create", true)
                    .child(div().text_size(px(11.)).child("\u{21b5}"))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.found(window, cx))),
            );
        let head = "A project is a folder with an edit.js script in it".to_string();
        Some(frame("New project", head, body, foot, cx.listener(shut)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // cmd+n on the launch screen opens the new project dialog instead of a system panel, in Movies/Origami
    // the first time: typing names the project, picking a folder in the list moves into it,
    // Enter makes the folder with the starter script and opens it, the next opening starts in the folder
    // last used, a name that already holds a script is refused, and Esc closes the dialog.
    #[gpui::test]
    fn names_and_places_a_project(cx: &mut gpui::TestAppContext) {
        let dir = fs::canonicalize(env::temp_dir()).unwrap().join("origami-begin-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("films/.hidden")).unwrap();
        cx.update(crate::menu::install);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let app = crate::fresh(cx.focus_handle(), vec![]);
            window.focus(&app.focus);
            window.activate_window();
            app
        });
        let _ = fs::remove_file(last());
        let seen = |cx: &mut gpui::VisualTestContext| view.update(cx, |o, _| o.begin.as_ref().map(|b| (b.dir.clone(), b.folders.clone())));
        cx.simulate_keystrokes("cmd-n");
        cx.run_until_parked();
        assert_eq!(seen(cx).map(|s| s.0), Some(crash::home().join("Movies/Origami")));
        view.update(cx, |o, _| o.enter(dir.clone()));
        assert_eq!(seen(cx), Some((dir.clone(), vec!["films".to_string()])));
        view.update(cx, |o, _| o.enter(dir.join("films")));
        cx.simulate_input("my/ trip");
        cx.simulate_keystrokes("backspace enter");
        cx.run_until_parked();
        let script = dir.join("films/my tri/edit.js");
        assert_eq!(
            view.update(cx, |o, _| (o.script.clone(), o.begin.is_none())),
            (script.clone(), true)
        );
        assert_eq!(fs::read_to_string(&script).unwrap(), STARTER);
        cx.simulate_keystrokes("cmd-n");
        cx.run_until_parked();
        assert_eq!(seen(cx), Some((dir.join("films"), vec!["my tri".to_string()])));
        cx.simulate_input("my tri");
        cx.simulate_keystrokes("enter");
        assert!(view.update(cx, |o, _| o.begin.as_ref().is_some_and(|b| !b.note.is_empty())
            && o.script == script));
        cx.simulate_keystrokes("escape");
        assert!(view.update(cx, |o, _| o.begin.is_none()));
    }
}
