use crate::Origami;
use gpui::{Context, Pixels, Point, Window};
use std::{mem, time::Instant};

fn reach(wide: f32, room: f32) -> f32 {
    (wide - room + 40.).max(0.)
}

pub fn links(chain: &str) -> Vec<String> {
    let (mut out, mut depth, mut quote) = (vec![String::new()], 0, None);
    for c in chain.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (_, '"' | '\'' | '`') => quote = Some(c),
            (_, '(' | '[' | '{') => depth += 1,
            (_, ')' | ']' | '}') => depth -= 1,
            (_, '.') if depth == 0 && !out[0].is_empty() => out.push(String::new()),
            _ => {}
        }
        out.last_mut().unwrap().push(c);
    }
    out
}

pub fn same<T: PartialEq>(old: &[T], new: &[T]) -> (usize, usize) {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (head, tail)
}

#[derive(Default)]
pub struct History(Vec<(usize, usize, Vec<String>)>);

impl History {
    pub fn push(&mut self, lines: Vec<String>) {
        if let Some((head, tail, old)) = self.0.last_mut() {
            (*head, *tail) = same(old, &lines);
            *old = old.drain(*head..old.len() - *tail).collect();
        }
        self.0.push((0, 0, lines));
    }

    pub fn pop(&mut self) -> Option<Vec<String>> {
        let (.., top) = self.0.pop()?;
        if let Some((head, tail, mid)) = self.0.last_mut() {
            *mid = [&top[..*head], mid, &top[top.len() - *tail..]].concat();
            (*head, *tail) = (0, 0);
        }
        Some(top)
    }

    pub fn last(&self) -> Option<&Vec<String>> {
        self.0.last().map(|e| &e.2)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl Origami {
    pub fn glyph(&self) -> f32 {
        self.font * 0.606
    }

    pub fn gutter(&self) -> f32 {
        12. + 5. * self.glyph()
    }

    pub fn inset(&self) -> f32 {
        self.gutter() - self.left
    }

    pub fn sway(&mut self, by: f32) {
        let wide = self.lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * self.glyph();
        self.left = (self.left - by).clamp(0., reach(wide, self.side - self.gutter()));
    }

    pub fn row(&self) -> f32 {
        (self.font * 1.7).round()
    }

    pub fn scale(&mut self, by: f32) {
        self.font = (self.font + by).clamp(9., 28.);
    }

    pub fn text(&self) -> String {
        self.lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect()
    }

    pub fn store(&mut self) {
        self.dirty = self.clash || self.sealed;
        if !self.dirty {
            let text = self.text();
            let _ = origami_lang::save(&self.script, &text);
            self.shelve(text);
        }
    }

    fn mutate(&mut self, f: impl FnOnce(&mut Self)) -> bool {
        let old = self.lines.clone();
        f(self);
        let changed = self.lines != old;
        if changed {
            self.past.push(old);
            self.future.clear();
        }
        changed
    }

    pub fn change(&mut self, f: impl FnOnce(&mut Self)) {
        if self.mutate(f) {
            self.store();
        }
    }

    pub fn draft(&mut self, f: impl FnOnce(&mut Self)) {
        self.dirty |= self.mutate(f);
    }

    fn apply(&mut self) {
        if !self.dirty {
            self.store();
        }
    }

    pub fn undo(&mut self) {
        if let Some(lines) = self.past.pop() {
            self.future.push(mem::replace(&mut self.lines, lines));
            self.apply();
        }
    }

    pub fn redo(&mut self) {
        if let Some(lines) = self.future.pop() {
            self.past.push(mem::replace(&mut self.lines, lines));
            self.apply();
        }
    }

    pub fn reveal(&mut self, row: usize) {
        let row = row.min(self.lines.len().saturating_sub(1));
        self.unfold(row);
        let (hidden, mut top) = (self.hidden(), (self.top as usize).min(row));
        while (top..row).filter(|r| !hidden[*r]).count() > self.fit.saturating_sub(1).min(23) {
            top += 1;
        }
        self.top = top as f32;
    }

    pub fn add(&mut self, code: &str, cat: bool) -> Option<usize> {
        let close = |l: &String| l.trim_start().starts_with(')');
        let at = match self.lines.iter().position(|l| cat && l.contains("cat(")) {
            Some(a) => self.lines[a..].iter().position(close).map(|i| a + i),
            None => self.lines.iter().rposition(close),
        };
        let Some(at) = at else {
            self.note = "no stack( ) block to add to, edit the script".into();
            return None;
        };
        let pad = self.lines[at].len() - self.lines[at].trim_start().len() + 2;
        self.change(|this| {
            if let Some(prev) = this.lines[..at]
                .last_mut()
                .filter(|l| !l.trim_end().ends_with([',', '(']) && !l.trim().is_empty())
            {
                prev.push(',');
            }
            this.lines.insert(at, format!("{}{code},", " ".repeat(pad)));
        });
        Some(at)
    }

    pub fn flash(&mut self, row: usize, rows: usize) {
        self.glow = Some((Instant::now(), row, rows));
    }

    pub fn lit(&self) -> Option<(f32, usize, usize)> {
        let (since, row, rows) = self.glow?;
        let left = 1. - since.elapsed().as_secs_f32() / 2.;
        (left > 0.).then_some((left, row, rows))
    }

    pub fn write(&mut self, word: &str) {
        let code = format!("{word}(\"{word}\").at({:.2}).for(3)", self.t);
        if let Some(row) = self.add(&code, false) {
            let col = self.lines[row].find('"').map_or(0, |i| i + 1);
            self.cursor = Some((row, col));
            self.reveal(row);
            self.flash(row, 1);
        }
    }

    pub fn head(&self) -> f32 {
        76. + if self.finding.is_some() { 30. } else { 0. } + if self.clash { 30. } else { 0. } + self.ledge()
    }

    pub fn spot(&self, at: Point<Pixels>, window: &Window) -> (usize, usize) {
        let x = f32::from(at.x) - f32::from(window.viewport_size().width) + self.side - self.inset();
        let (hidden, n) = (self.hidden(), ((f32::from(at.y) - self.head()) / self.row()).max(0.) as usize);
        let mut seen = (self.top as usize..self.lines.len()).filter(|r| !hidden[*r]);
        let row = seen.nth(n).unwrap_or(self.lines.len().saturating_sub(1));
        let len = self.lines.get(row).map_or(0, |l| l.chars().count());
        (row, ((x / self.glyph()).round().max(0.) as usize).min(len))
    }

    pub fn place(&mut self, at: Point<Pixels>, window: &Window) {
        self.cursor = Some(self.spot(at, window));
    }

    pub fn widen(&mut self, clicks: usize) {
        let Some((r, c)) = self.cursor.filter(|_| clicks > 1) else {
            return;
        };
        let line: Vec<char> = self.lines[r].chars().collect();
        let word = |c: &&char| c.is_alphanumeric() || **c == '_';
        let (a, b) = match clicks {
            2 => (
                c - line[..c].iter().rev().take_while(word).count(),
                c + line[c..].iter().take_while(word).count(),
            ),
            _ => (0, line.len()),
        };
        (self.anchor, self.cursor) = (Some((r, a)), Some((r, b)));
    }

    fn byte(&self, (r, c): (usize, usize)) -> usize {
        self.lines[r].char_indices().nth(c).map_or(self.lines[r].len(), |x| x.0)
    }

    pub fn chosen(&self) -> Option<((usize, usize), (usize, usize))> {
        let last = self.lines.len().checked_sub(1)?;
        let fix = |(r, c): (usize, usize)| (r.min(last), c.min(self.lines[r.min(last)].chars().count()));
        let (a, b) = (fix(self.anchor?), fix(self.cursor?));
        (a != b).then(|| (a.min(b), a.max(b)))
    }

    pub fn picked(&self) -> String {
        let Some((a, b)) = self.chosen() else { return String::new() };
        let (i, j) = (self.byte(a), self.byte(b));
        if a.0 == b.0 {
            return self.lines[a.0][i..j].into();
        }
        let mut out = vec![self.lines[a.0][i..].to_string()];
        out.extend(self.lines[a.0 + 1..b.0].iter().cloned());
        out.push(self.lines[b.0][..j].into());
        out.join("\n")
    }

    pub fn cut(&mut self) {
        let Some((a, b)) = self.chosen() else { return };
        let (i, j) = (self.byte(a), self.byte(b));
        let tail = self.lines[b.0][j..].to_string();
        self.lines[a.0].truncate(i);
        self.lines[a.0].push_str(&tail);
        self.lines.drain(a.0 + 1..=b.0);
        (self.cursor, self.anchor) = (Some(a), None);
    }

    pub fn all(&mut self) {
        let last = self.lines.len().saturating_sub(1);
        self.anchor = Some((0, 0));
        self.cursor = Some((last, self.lines.get(last).map_or(0, |l| l.chars().count())));
    }

    pub fn edit(&mut self, k: &str, shift: bool) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if matches!(k, "left" | "right" | "up" | "down" | "home" | "end" | "top" | "bottom") {
            self.anchor = self.anchor.or(self.cursor).filter(|_| shift);
        } else if k != "escape" && self.chosen().is_some() {
            self.cut();
            if matches!(k, "backspace" | "delete") {
                return;
            }
        } else {
            self.anchor = None;
        }
        let Some((r, c)) = self.cursor else { return };
        let len = |this: &Self, r: usize| this.lines[r].chars().count();
        let r = r.min(self.lines.len() - 1);
        let c = c.min(len(self, r));
        let i = self.byte((r, c));
        let (r, c) = match k {
            "escape" => return (self.cursor, self.anchor) = (None, None),
            "left" => (r, c.saturating_sub(1)),
            "right" => (r, c + 1),
            "up" => (r.saturating_sub(1), c),
            "down" => (r + 1, c),
            "home" => (r, 0),
            "end" => (r, usize::MAX),
            "top" => (0, 0),
            "bottom" => (self.lines.len(), usize::MAX),
            "enter" | "\n" => {
                let mut rest = self.lines[r].split_off(i);
                if k == "enter" && self.lines[r].ends_with('.') && rest.starts_with(char::is_alphabetic) {
                    self.lines[r].pop();
                    rest.insert(0, '.');
                }
                let head = &self.lines[r];
                let deep = head.trim_end().ends_with(['(', '[', '{']) as usize * 2;
                let chain = k == "enter" && rest.starts_with('.') && !head.trim().is_empty();
                let deep = if chain {
                    !head.trim_start().starts_with('.') as usize * 2
                } else {
                    deep
                };
                let pad = (head.len() - head.trim_start().len() + deep) * (k == "enter") as usize;
                let links = if chain { links(&rest) } else { vec![rest] };
                for (n, link) in links.into_iter().enumerate() {
                    self.lines.insert(r + 1 + n, " ".repeat(pad) + &link);
                }
                (r + 1, pad)
            }
            "backspace" if c > 0 => {
                let p = self.lines[r][..i].char_indices().last().map_or(0, |x| x.0);
                self.lines[r].replace_range(p..i, "");
                (r, c - 1)
            }
            "backspace" if r > 0 => {
                let (rest, n) = (self.lines.remove(r), len(self, r - 1));
                self.lines[r - 1].push_str(&rest);
                (r - 1, n)
            }
            k => {
                let s = match k {
                    "space" => " ",
                    "tab" => "  ",
                    k if k.chars().count() == 1 => k,
                    _ => "",
                };
                self.lines[r].insert_str(i, s);
                (r, c + s.chars().count())
            }
        };
        let r = r.min(self.lines.len() - 1);
        self.cursor = Some((r, c.min(len(self, r))));
        self.reveal(r);
        let x = c.min(len(self, r)) as f32 * self.glyph();
        self.left = self.left.clamp((x - self.side + self.gutter() + 40.).max(0.), x);
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let text = cx.read_from_clipboard().and_then(|i| i.text()).unwrap_or_default();
        self.draft(|this| {
            for c in text.chars().filter(|c| *c != '\r') {
                this.edit(&c.to_string(), false);
            }
        });
    }

    pub fn delete(&mut self) {
        if self.cull() {
            return;
        }
        if let Some((_, rows, _)) = self.usage("remove").filter(|(c, ..)| !self.alone(c, "remove")) {
            self.change(|this| drop(this.lines.drain(rows)));
        }
    }
}

#[cfg(test)]
mod tests {
    // A long run of random edits, undos and redos should bring back every earlier script exactly.
    #[test]
    fn history_keeps_every_step() {
        let (mut seed, mut lines) = (7u64, vec![String::new(); 3]);
        let mut roll = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n.max(1)
        };
        let (mut past, mut future) = (super::History::default(), super::History::default());
        let (mut was, mut ahead): (Vec<Vec<String>>, Vec<Vec<String>>) = (vec![], vec![]);
        for _ in 0..20000 {
            match roll(10) {
                0..2 => {
                    if let Some(old) = past.pop() {
                        assert_eq!(Some(&old), was.pop().as_ref());
                        ahead.push(std::mem::replace(&mut lines, old));
                        future.push(ahead.last().unwrap().clone());
                    }
                }
                2 => {
                    if let Some(new) = future.pop() {
                        assert_eq!(Some(&new), ahead.pop().as_ref());
                        was.push(std::mem::replace(&mut lines, new));
                        past.push(was.last().unwrap().clone());
                    }
                }
                k => {
                    let old = lines.clone();
                    let at = roll(lines.len() + 1);
                    match k {
                        3 => lines.insert(at, format!("{}", roll(3))),
                        4 if at < lines.len() => drop(lines.remove(at)),
                        5 => lines.splice(at.., (0..roll(4)).map(|i| i.to_string())).for_each(drop),
                        6 => lines = lines.iter().rev().cloned().collect(),
                        _ if at < lines.len() => lines[at].push(['a', 'b'][roll(2)]),
                        _ => {}
                    }
                    match roll(5) {
                        0 => assert_eq!(past.pop().as_ref(), was.pop().as_ref()),
                        1 => continue,
                        _ => {}
                    }
                    was.push(old.clone());
                    past.push(old);
                    future.clear();
                    ahead.clear();
                }
            }
            assert_eq!((past.len(), future.len(), past.last()), (was.len(), ahead.len(), was.last()));
        }
        while let Some(old) = past.pop() {
            assert_eq!(Some(old), was.pop());
        }
        while let Some(new) = future.pop() {
            assert_eq!(Some(new), ahead.pop());
        }
    }

    // A chain such as .trim(...).exposure(...) should be split into its separate calls.
    #[test]
    fn links() {
        let got = super::links(".trim(0, 39.673).inputLut(\"a.cube\").exposure(0.3),");
        assert_eq!(got, [".trim(0, 39.673)", ".inputLut(\"a.cube\")", ".exposure(0.3),"]);
    }

    // A row wider than its panel can scroll sideways by the overflow plus a little spare room; a row
    // that fits cannot scroll.
    #[test]
    fn reach() {
        assert_eq!(super::reach(900., 400.), 540.);
        assert_eq!(super::reach(300., 400.), 0.);
    }

    use gpui::{Entity, TestAppContext, VisualTestContext};
    use std::{fs, os::unix::fs::PermissionsExt};

    fn settle(view: &Entity<crate::Origami>, cx: &mut VisualTestContext) {
        cx.run_until_parked();
        view.update(cx, |o, cx| o.watch(cx));
        cx.run_until_parked();
    }

    // Replacing a media file under the same name should reload the timeline and drop the thumbnails
    // and waveforms made from the old file; with unsaved edits the script on screen stays as it is.
    #[gpui::test]
    fn replaced_media_reloads(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join("origami-replaced-test");
        fs::create_dir_all(&dir).unwrap();
        let (path, clip) = (dir.join("edit.js"), dir.join("v.mp4"));
        let make = |secs: u32| {
            let src = format!("color=red:s=64x36:r=30:d={secs}");
            let made = origami_lang::status(
                origami_lang::ffmpeg()
                    .args(["-v", "error", "-y", "-f", "lavfi", "-i", &src])
                    .arg(&clip),
            );
            assert!(made.unwrap().success());
        };
        make(2);
        fs::write(&path, "stack(video(\"v.mp4\"))\n").unwrap();
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.open(path.clone(), cx);
            app
        });
        settle(&view, cx);
        let thumb = format!("{}@0.0", view.update(cx, |o, _| o.tl.clips[0].src.clone()));
        let state = |cx: &mut VisualTestContext| {
            view.update(cx, |o, _| {
                (
                    o.tl.duration,
                    matches!(o.assets.get(&thumb), Some(Some(crate::Asset::Wave(_)))),
                    o.rev,
                    o.clash,
                )
            })
        };
        view.update(cx, |o, _| o.assets.insert(thumb.clone(), Some(crate::Asset::Wave(vec![]))));
        let (_, _, rev, _) = state(cx);
        make(3);
        settle(&view, cx);
        assert_eq!(state(cx), (3., false, rev + 2, false));
        view.update(cx, |o, _| (o.lines, o.dirty) = (vec!["stack(".into()], true));
        make(4);
        settle(&view, cx);
        assert_eq!(state(cx), (3., false, rev + 3, false));
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), ["stack("]);
    }

    // A script that never ends reports that it took too long and one with a mistake marks the wrong line,
    // undoing back to an empty script saves it empty, deleting the script file outside the app keeps the
    // code on screen and offers to save it again, :e with a name that does not exist opens nothing, and a
    // save replaces a link put where the script was instead of writing through it and keeps the file's
    // permissions.
    #[gpui::test]
    fn loads_and_marks(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join("origami-loads-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("edit.js");
        fs::write(&path, "stack()\n").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.open(path.clone(), cx);
            window.focus(&app.focus);
            app
        });
        settle(&view, cx);
        fs::write(&path, "while (true) {}").unwrap();
        settle(&view, cx);
        assert!(view.update(cx, |o, _| o.err.starts_with("script took too long")));
        let a = "const a = text(\"a\").for(1)\n";
        for (src, want) in [
            (
                "const b = text(\"b\").for(1)\nstack(a, b,\n  text(\"c\").for(1).opacity(t => NaN))",
                4,
            ),
            ("stack(a,\n  text(\"c\").for(NaN))", 3),
            ("stack(a,\n  text(\"c\", { size: \"big\" }).for(1))", 3),
            ("stack(a,\n  5)", 2),
            ("\n42", 3),
            ("stack(a,\n  text(\"c\".for(1))", 3),
            ("const b = text(\"b).for(1)\nstack(a, b)", 2),
            ("stack(a, color().for(1))", 2),
            ("stack(a,\n  text(\"c\").for(1).scale(t => { throw new Error(\"x\") }))", 3),
            ("stack(a.fade(\"a\"))", 2),
            ("\nstack(a, bb)", 3),
        ] {
            fs::write(&path, format!("{a}{src}")).unwrap();
            settle(&view, cx);
            assert_eq!(view.update(cx, |o, _| (o.bad, o.err.is_empty())), (want, false), "{src}");
        }
        fs::write(&path, "").unwrap();
        settle(&view, cx);
        view.update(cx, |o, _| (o.cursor, o.past) = (Some((0, 0)), Default::default()));
        cx.simulate_keystrokes("t cmd-s");
        settle(&view, cx);
        assert_eq!(fs::read_to_string(&path).unwrap(), "t\n");
        cx.simulate_keystrokes("cmd-z");
        settle(&view, cx);
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        assert_eq!(view.update(cx, |o, _| (o.lines.len(), o.past.len())), (0, 0));
        cx.simulate_keystrokes("cmd-shift-z");
        settle(&view, cx);
        let one = vec!["t".to_string()];
        fs::remove_file(&path).unwrap();
        settle(&view, cx);
        assert_eq!(
            view.update(cx, |o, _| (o.lines.clone(), o.clash, o.dirty)),
            (one.clone(), true, true)
        );
        view.update_in(cx, |o, window, cx| {
            o.run("e other.js", window, cx);
            assert_eq!(
                (o.script.clone(), o.note.as_str()),
                (path.clone(), "no script named other.js here, nothing opened")
            );
        });
        fs::write(dir.join("keep.js"), "k\n").unwrap();
        std::os::unix::fs::symlink(dir.join("keep.js"), &path).unwrap();
        let store = |cx: &mut VisualTestContext| {
            view.update(cx, |o, _| {
                o.clash = false;
                o.store();
            })
        };
        store(cx);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        store(cx);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        let kept = (fs::read_to_string(dir.join("keep.js")).unwrap(), fs::read_to_string(&path).unwrap());
        assert_eq!((kept, path.is_symlink(), mode), (("k\n".into(), "t\n".into()), false, 0o640));
    }
}
