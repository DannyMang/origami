use crate::{
    Origami, pulse,
    view::{BLUE, INK, TEXT},
};
use gpui::{Bounds, Context, Keystroke, PathBuilder, Pixels, Point, Window, point, px, rgb, rgba};
use origami_engine::{footprint, place};
use origami_lang::{Kind, Timeline};
use std::{ops::Range, sync::Arc};

pub struct Words {
    z: u32,
    row: usize,
    span: Range<usize>,
    quote: char,
    old: String,
    now: Vec<char>,
    at: usize,
    all: bool,
    new: bool,
    busy: bool,
    was: Arc<Timeline>,
}

fn quoted(line: &str, from: usize, said: &str) -> Option<(Range<usize>, char)> {
    let named = |i: usize| line[..i].ends_with(|c: char| c.is_alphanumeric() || "_$.".contains(c));
    let calls = line.match_indices("text(").chain(line.match_indices("caption("));
    calls.filter(|(i, _)| *i >= from && !named(*i)).find_map(|(i, call)| {
        let rest = &line[i + call.len()..];
        let open = line.len() - rest.trim_start().len();
        let quote = line[open..].chars().next().filter(|c| "\"'`".contains(*c))?;
        let (mut out, mut chars) = (String::new(), line[open + 1..].char_indices());
        let end = loop {
            match chars.next()? {
                (i, c) if c == quote => break open + 1 + i,
                (_, '$') if quote == '`' => return None,
                (_, '\\') => out.push(match chars.next()?.1 {
                    'n' => '\n',
                    c if c.is_alphanumeric() => return None,
                    c => c,
                }),
                (_, c) => out.push(c),
            }
        };
        (out == said && line[end + 1..].trim_start().starts_with([',', ')'])).then_some((open + 1..end, quote))
    })
}

fn escaped(said: &str, quote: char) -> String {
    let safe = |c: char| match c {
        '\n' => "\\n".into(),
        '\\' => "\\\\".into(),
        '$' if quote == '`' => "\\$".into(),
        c if c == quote => format!("\\{c}"),
        c => c.to_string(),
    };
    said.chars().map(safe).collect()
}

pub fn mark(bar: [(f64, f64); 4], all: bool, sheet: Bounds<Pixels>, window: &mut Window) {
    let at = bar.map(|(x, y)| sheet.origin + point(sheet.size.width * x as f32, sheet.size.height * y as f32));
    let (mut fill, mut rim) = (PathBuilder::fill(), PathBuilder::stroke(px(1.)));
    fill.add_polygon(&at, true);
    rim.add_polygon(&at, true);
    if let (Ok(fill), Ok(rim)) = (fill.build(), rim.build()) {
        window.paint_path(fill, if all { rgba(BLUE << 8 | 0x66) } else { rgb(TEXT) });
        window.paint_path(rim, rgb(if all { BLUE } else { INK }));
    }
}

impl Origami {
    pub fn retype(&mut self, at: Point<Pixels>) -> bool {
        let Some(c) = self.hit(at).filter(|c| c.kind == Kind::Text) else {
            return false;
        };
        let twins = self.tl.clips.iter().filter(|o| o.home == c.home && o.kind == Kind::Text).count();
        if twins > 1 {
            self.note = format!("line {} makes {twins} texts, edit the script to change their words", c.home);
        } else {
            self.spell(c.z, ((c.home as usize).saturating_sub(1), 0), &c.src, false);
        }
        true
    }

    pub fn spell(&mut self, z: u32, (row, from): (usize, usize), said: &str, new: bool) {
        let Some((span, quote)) = self.lines.get(row).and_then(|l| quoted(l, from, said)) else {
            return self.note = format!(
                "the words on line {} are not written out in quotes there, edit the script to change them",
                row + 1
            );
        };
        (self.player, self.sel) = (None, Some(z));
        self.words = Some(Words {
            z,
            row,
            quote,
            old: self.lines[row][span.clone()].into(),
            span,
            now: said.chars().collect(),
            at: said.chars().count(),
            all: new,
            new,
            busy: false,
            was: self.tl.clone(),
        });
        self.note = "type the words \u{b7} enter saves \u{b7} shift+enter starts a line \u{b7} esc cancels".into();
    }

    fn inked(&self) -> Option<(usize, String)> {
        let w = self.words.as_ref()?;
        let line = self.lines.get(w.row).filter(|l| l.get(w.span.clone()) == Some(&w.old))?;
        let said = escaped(&w.now.iter().collect::<String>(), w.quote);
        Some((w.row, format!("{}{said}{}", &line[..w.span.start], &line[w.span.end..])))
    }

    pub fn sketch(&mut self, cx: &mut Context<Self>) {
        let Some((row, line)) = self.inked().filter(|_| self.words.as_ref().is_some_and(|w| !w.busy)) else {
            return;
        };
        let (script, mut lines) = (self.script.clone(), self.lines.clone());
        lines[row] = line;
        let now = self.words.as_mut().map(|w| {
            w.busy = true;
            w.now.clone()
        });
        cx.spawn(async move |this, cx| {
            let draft = async move { origami_lang::peek(&script, &lines.join("\n"), footprint) };
            let tl = cx.background_executor().spawn(draft).await;
            this.update(cx, |this, cx| {
                let Some(w) = this.words.as_mut() else { return };
                let late = Some(&w.now) != now.as_ref();
                w.busy = false;
                if let Ok(tl) = tl {
                    (this.tl, this.rev) = (Arc::new(tl), this.rev + 1);
                    this.refresh(cx);
                }
                if late {
                    this.sketch(cx);
                }
                cx.notify();
            })
        })
        .detach();
    }

    pub fn redraft(&mut self, cx: &mut Context<Self>) {
        if let Some(w) = self.words.as_mut() {
            w.was = self.tl.clone();
            self.sketch(cx);
        }
    }

    pub fn strike(&mut self, key: &Keystroke, cx: &mut Context<Self>) {
        let (cmd, name) = (key.modifiers.platform || key.modifiers.control, key.key.as_str());
        let typed = key.key_char.clone().unwrap_or(key.key.clone());
        let put: String = match name {
            "escape" => return self.scrap(),
            "enter" if !key.modifiers.shift => return self.seal(true),
            "s" if cmd => return self.seal(true),
            "v" if cmd => {
                let text = cx.read_from_clipboard().and_then(|i| i.text()).unwrap_or_default();
                text.chars().filter(|c| *c == '\n' || !c.is_control()).collect()
            }
            "enter" => "\n".into(),
            "space" => " ".into(),
            _ if !cmd && typed.chars().count() == 1 && !typed.contains(char::is_control) => typed,
            _ => String::new(),
        };
        let Some(w) = self.words.as_mut() else { return };
        let (n, wipe) = (w.now.len(), !put.is_empty() || matches!(name, "backspace" | "delete"));
        match name {
            _ if w.all && wipe => (w.at, w.now) = (0, vec![]),
            "backspace" if w.at > 0 => {
                w.at -= 1;
                w.now.remove(w.at);
            }
            "delete" if w.at < n => drop(w.now.remove(w.at)),
            "left" | "up" | "home" => {
                w.at = if cmd || w.all || name != "left" {
                    0
                } else {
                    w.at.saturating_sub(1)
                }
            }
            "right" | "down" | "end" => w.at = if cmd || w.all || name != "right" { n } else { (w.at + 1).min(n) },
            _ => {}
        }
        w.all = cmd && name == "a" && n > 0;
        w.now.splice(w.at..w.at, put.chars());
        w.at += put.chars().count();
        let (z, said): (u32, String) = (w.z, w.now.iter().collect());
        if let Some(c) = Arc::make_mut(&mut self.tl)
            .clips
            .iter_mut()
            .find(|c| c.z == z && c.kind == Kind::Text)
        {
            c.src = said;
        }
        self.rev += 1;
        self.note.clear();
        self.refresh(cx);
        self.sketch(cx);
    }

    pub fn seal(&mut self, stay: bool) {
        let line = self.inked();
        let Some(w) = self.words.take() else { return };
        if w.now.is_empty() {
            self.words = Some(w);
            return match stay {
                true => self.note = "a text needs words: type some, or esc cancels".into(),
                false => self.scrap(),
            };
        }
        let Some((row, line)) = line else {
            (self.tl, self.rev) = (w.was, self.rev + 1);
            return self.note = format!("line {} changed while you typed, the words were not saved", w.row + 1);
        };
        let steps = self.past.len();
        self.change(|this| this.lines[row] = line);
        if w.new && self.past.len() > steps {
            self.past.pop();
        }
        if !w.new {
            pulse::beat("text edited on picture");
        }
        self.note.clear();
    }

    fn scrap(&mut self) {
        let Some(w) = self.words.take() else { return };
        (self.tl, self.rev) = (w.was, self.rev + 1);
        self.note.clear();
        if w.new {
            self.sel = None;
            self.undo();
        }
    }

    pub fn caret(&self) -> Option<([(f64, f64); 4], bool)> {
        let w = self.words.as_ref()?;
        let c = self.tl.clips.iter().find(|c| c.z == w.z && c.kind == Kind::Text)?;
        let (size, mut m) = ((self.tl.size.0 as f64, self.tl.size.1 as f64), c.clone());
        let mut fit = |s: &str| {
            m.src = s.into();
            let (w, h, ..) = place(&m, size, 1.);
            (w, h)
        };
        let (said, head): (String, String) = (w.now.iter().collect(), w.now[..w.at].iter().collect());
        let (row, pre) = (head.matches('\n').count(), head.rsplit('\n').next().unwrap_or_default());
        let ((bar, one), (_, two), (pad, _)) = (fit("|"), fit("|\n|"), fit(""));
        let (wide, pen) = (
            fit(said.split('\n').nth(row).unwrap_or_default()).0,
            fit(&format!("{pre}|")).0 - bar,
        );
        let ((tw, th), lean) = (
            fit(&said),
            ["left", "", "right"].iter().position(|a| *a == c.align).unwrap_or(1) as f64,
        );
        let (turn, q) = (self.turned(&m), c.scale.at(self.frame(c) as usize) / c.scale.max());
        let thin = 2. * size.0 / f32::from(self.sheet().size.width).max(1.) as f64;
        let (x, y) = (
            pad / 2. + (tw - wide) * lean / 2. + pen,
            (one - (two - one)) / 2. + row as f64 * (two - one),
        );
        let (a, b) = match w.all {
            true => ((0., 0.), (tw, th)),
            false => ((x, y), (x + thin / q.max(1e-6), y + two - one)),
        };
        let spot = |(x, y): (f64, f64)| turn(((x - tw / 2.) * q, (y - th / 2.) * q));
        Some(([a, (b.0, a.1), b, (a.0, b.1)].map(spot), w.all))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ClipboardItem, Entity, Modifiers, MouseButton, MouseDownEvent, TestAppContext, VisualTestContext};
    use std::{env, fs};

    // The words of a text or caption can be rewritten only where they are one quoted string in the
    // call on that line: a name, a template with ${}, joined strings, a method called text or the
    // file of captions( ) are left alone. New words are written with the quotes the script already
    // uses, with quotes, backslashes and line breaks escaped so they read back the same.
    #[test]
    fn finds_plain_words() {
        let at = |line: &'static str, said: &str| quoted(line, 0, said).map(|(r, q)| (&line[r], q));
        assert_eq!(at("let a = text(\"hi\", { size: 24 }).for(2)", "hi"), Some(("hi", '"')));
        assert_eq!(at("  caption( 'it\\'s' ),", "it's"), Some(("it\\'s", '\'')));
        assert_eq!(at("text(`a\\nb`)", "a\nb"), Some(("a\\nb", '`')));
        assert_eq!(at("stack(text(\"a\"), text(\"b\").at(1))", "b"), Some(("b", '"')));
        assert_eq!(at("text(\"see text('x')\")", "see text('x')"), Some(("see text('x')", '"')));
        for (line, said) in [
            ("text(name)", "name"),
            ("text(`a${n}`)", "a1"),
            ("text(`a$`)", "a$"),
            ("text(\"a\" + \"\")", "a"),
            ("text(\"\\u0041\")", "A"),
            ("context(\"a\")", "a"),
            ("title.text(\"a\")", "a"),
            ("captions(\"a.srt\")", "a.srt"),
            ("text(\"a\",", "b"),
            ("text(\"a", "a"),
        ] {
            assert_eq!(at(line, said), None, "{line}");
        }
        assert_eq!(
            quoted("stack(text(\"text\"), text(\"text\").at(1))", 7, "text").map(|p| p.0),
            Some(26..30)
        );
        let said = "say \"hi\" it's `$5`\\ now\nthen";
        assert_eq!(escaped(said, '"'), "say \\\"hi\\\" it's `$5`\\\\ now\\nthen");
        for quote in ['"', '\'', '`'] {
            let line = format!("text({quote}{}{quote})", escaped(said, quote));
            assert_eq!(quoted(&line, 0, said), Some((6..line.len() - 2, quote)), "{line}");
        }
    }

    fn window<'a>(cx: &'a mut TestAppContext, name: &str, src: &str) -> (Entity<Origami>, &'a mut VisualTestContext) {
        let dir = env::temp_dir().join("origami-words-test");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("two.srt"),
            "1\n00:00:00,000 --> 00:00:02,000\none\n\n2\n00:00:02,000 --> 00:00:04,000\ntwo\n",
        )
        .unwrap();
        let path = dir.join(format!("{name}.js"));
        fs::write(&path, src).unwrap();
        let tl = origami_lang::eval(&path, false, footprint).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl, app.t) = (path, src.lines().map(String::from).collect(), Arc::new(tl), 1.);
            window.focus(&app.focus);
            app
        });
        cx.run_until_parked();
        (view, cx)
    }

    fn click(view: &Entity<Origami>, cx: &mut VisualTestContext, (x, y): (f32, f32), count: usize) {
        let sheet = view.update(cx, |o, _| o.sheet());
        let position = match count {
            0 => point(px(200.), px(10.)),
            _ => sheet.origin + point(sheet.size.width * x, sheet.size.height * y),
        };
        for click_count in 1..=count.max(1) {
            cx.simulate_event(MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Modifiers::default(),
                click_count,
                first_mouse: false,
            });
            cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
        }
        cx.run_until_parked();
    }

    // Double-clicking a text in the preview should let its words be typed on the picture, in mouse
    // mode and in keyboard mode: the picture follows every key, a caret shows where the next letter
    // goes, and no other shortcut fires. Enter writes the words into the script as one undo step,
    // escape leaves the script alone, a click elsewhere saves like enter, and shift+enter starts a
    // new line. A text emptied of its words is not saved.
    #[gpui::test]
    fn retypes(cx: &mut TestAppContext) {
        let src = "let name = text('it', { size: 60 }).for(4)\nstack(\n  color(\"#203040\").for(4),\n  name,\n)";
        let (view, cx) = window(cx, "retypes", src);
        let line = |cx: &mut VisualTestContext| view.update(cx, |o, _| (o.lines[0].clone(), o.past.len(), o.words.is_some()));
        let shown = |cx: &mut VisualTestContext| view.update(cx, |o, _| o.tl.clips[1].src.clone());
        let said = |cx: &mut VisualTestContext| view.update(cx, |o, _| o.words.as_ref().map(|w| (w.now.iter().collect::<String>(), w.at)));
        let caret = |cx: &mut VisualTestContext| view.update(cx, |o, _| o.caret().unwrap().0);
        let first = src.lines().next().unwrap().to_string();
        let wrote = |words: &str| first.replace("'it'", &format!("'{words}'"));
        for mouse in [true, false] {
            view.update(cx, |o, _| (o.mouse, o.sel) = (mouse, None));
            click(&view, cx, (0.5, 0.5), 2);
            assert_eq!(said(cx), Some(("it".into(), 2)));
            assert_eq!(
                view.update(cx, |o, _| (o.sel, o.player.is_some(), o.note.starts_with("type the words"))),
                (Some(1), false, true)
            );
            let end = caret(cx);
            cx.simulate_keystrokes("space s t x : f");
            cx.run_until_parked();
            assert_eq!((said(cx), shown(cx)), (Some(("it stx:f".into(), 8)), "it stx:f".into()));
            assert_eq!(
                view.update(cx, |o, _| (o.pen, o.player.is_some(), o.full, o.mode == crate::Mode::Normal, o.t)),
                (false, false, false, true, 1.)
            );
            assert_eq!(line(cx), (first.clone(), 0, true));
            assert!(caret(cx)[0].0 > end[0].0 && caret(cx)[0].1 == end[0].1);
            cx.simulate_keystrokes("backspace backspace backspace left left backspace ' cmd-z");
            assert_eq!(said(cx), Some(("it'st".into(), 3)));
            cx.simulate_keystrokes("right right right delete home delete end left");
            assert_eq!(said(cx), Some(("t'st".into(), 3)));
            cx.simulate_keystrokes("enter");
            assert_eq!(line(cx), (wrote("t\\'st"), 1, false));
            assert_eq!(
                view.update(cx, |o, _| fs::read_to_string(&o.script).unwrap()),
                format!("{}\n", src.replace("'it'", "'t\\'st'"))
            );
            cx.run_until_parked();
            assert_eq!(shown(cx), "t'st");
            cx.simulate_keystrokes("cmd-z");
            assert_eq!(line(cx), (first.clone(), 0, false));
            view.update(cx, |o, _| o.tl = Arc::new(origami_lang::eval(&o.script, false, footprint).unwrap()));
        }
        view.update(cx, |o, _| o.mouse = true);

        click(&view, cx, (0.5, 0.5), 2);
        cx.simulate_keystrokes("!");
        cx.run_until_parked();
        assert_eq!(shown(cx), "it!");
        cx.simulate_keystrokes("escape");
        assert_eq!((line(cx), shown(cx)), ((first.clone(), 0, false), "it".into()));
        assert_eq!(view.update(cx, |o, _| (o.mouse, o.caret().is_none())), (true, true));

        click(&view, cx, (0.5, 0.5), 2);
        cx.simulate_keystrokes("!");
        click(&view, cx, (0., 0.), 0);
        assert_eq!(line(cx), (wrote("it!"), 1, false));
        cx.simulate_keystrokes("cmd-z");
        view.update(cx, |o, _| o.tl = Arc::new(origami_lang::eval(&o.script, false, footprint).unwrap()));

        click(&view, cx, (0.5, 0.5), 2);
        let one = caret(cx);
        cx.simulate_keystrokes("shift-enter");
        cx.run_until_parked();
        let two = caret(cx);
        assert!(two[0].1 > one[0].1 && two[0].0 < one[0].0, "{one:?} {two:?}");
        cx.simulate_keystrokes("b enter");
        assert_eq!(line(cx), (wrote("it\\nb"), 1, false));
        let saved = view.update(cx, |o, _| origami_lang::eval(&o.script, false, footprint).unwrap());
        assert_eq!(saved.clips[1].src, "it\nb");
        cx.simulate_keystrokes("cmd-z");
        view.update(cx, |o, _| o.tl = Arc::new(origami_lang::eval(&o.script, false, footprint).unwrap()));

        click(&view, cx, (0.5, 0.5), 2);
        cx.simulate_keystrokes("cmd-a");
        let all = view.update(cx, |o, _| o.caret().unwrap());
        assert!(all.1 && all.0[2].0 - all.0[0].0 > 0.01);
        cx.simulate_keystrokes("o");
        assert_eq!(said(cx), Some(("o".into(), 1)));
        cx.simulate_keystrokes("cmd-a backspace");
        cx.run_until_parked();
        assert_eq!((said(cx), shown(cx)), (Some((String::new(), 0)), String::new()));
        assert!(view.update(cx, |o, _| o.image.is_some() && o.caret().is_some()));
        cx.simulate_keystrokes("enter");
        assert_eq!(line(cx), (first.clone(), 0, true));
        assert!(view.update(cx, |o, _| o.note.starts_with("a text needs words")));
        cx.write_to_clipboard(ClipboardItem::new_string("a'b\r\n\u{7}c".into()));
        cx.simulate_keystrokes("cmd-v left cmd-s");
        assert_eq!(line(cx), (wrote("a\\'b\\nc"), 1, false));
        cx.simulate_keystrokes("cmd-z");
        view.update(cx, |o, _| o.tl = Arc::new(origami_lang::eval(&o.script, false, footprint).unwrap()));

        click(&view, cx, (0.5, 0.5), 2);
        cx.simulate_keystrokes("cmd-a delete");
        click(&view, cx, (0., 0.), 0);
        assert_eq!((line(cx), shown(cx)), ((first.clone(), 0, false), "it".into()));
        view.update(cx, |o, _| o.lines[0] = first.replace("'it'", "'other'"));
        click(&view, cx, (0.5, 0.5), 2);
        assert_eq!(line(cx).2, false);
        view.update(cx, |o, _| o.lines[0] = first.clone());
        click(&view, cx, (0.5, 0.5), 2);
        cx.simulate_keystrokes("!");
        view.update(cx, |o, _| o.lines[0] = first.replace("'it'", "'other'"));
        cx.simulate_keystrokes("enter");
        assert_eq!(line(cx), (first.replace("'it'", "'other'"), 0, false));
        assert_eq!(
            view.update(cx, |o, _| o.note.clone()),
            "line 1 changed while you typed, the words were not saved"
        );
    }

    // Words that are not one quoted string in the script, or one line that makes several texts,
    // should be left alone with a note saying why, and a double click beside any text should not
    // start typing.
    #[gpui::test]
    fn refuses(cx: &mut TestAppContext) {
        let quotes = "are not written out in quotes there, edit the script to change them";
        for (name, src, says) in [
            (
                "named",
                "const n = \"v\"\nstack(color(\"#203040\").for(4),\n  text(n).for(4))".to_string(),
                format!("the words on line 3 {quotes}"),
            ),
            (
                "joined",
                "stack(color(\"#203040\").for(4), text(`a${1}`).for(4))".into(),
                format!("the words on line 1 {quotes}"),
            ),
            (
                "mapped",
                "stack(color(\"#203040\").for(4),\n  [0, 1].map(i => text(\"a\").for(4)))".into(),
                "line 2 makes 2 texts, edit the script to change their words".into(),
            ),
            (
                "twice",
                "let a = text(\"a\").for(2)\nstack(color(\"#203040\").for(4), a, a.at(1))".into(),
                "line 1 makes 2 texts, edit the script to change their words".into(),
            ),
            (
                "subs",
                "stack(color(\"#203040\").for(4),\n  captions(\"two.srt\").pos(0.4, 0.4))".into(),
                "line 2 makes 2 texts, edit the script to change their words".into(),
            ),
        ] {
            let (view, cx) = window(cx, name, &src);
            view.update(cx, |o, _| o.mouse = false);
            click(&view, cx, (0.02, 0.02), 2);
            assert_eq!(view.update(cx, |o, _| (o.words.is_some(), o.note.clone())), (false, String::new()));
            let at = if name == "subs" { 0.41 } else { 0.5 };
            click(&view, cx, (at, at), 2);
            assert_eq!(
                view.update(cx, |o, _| (o.words.is_some(), o.note.clone(), o.lines.join("\n"), o.past.len())),
                (false, says, src, 0),
                "{name}"
            );
        }
    }
}
