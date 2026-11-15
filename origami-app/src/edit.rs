use crate::Origami;
use gpui::{Context, Pixels, Point, Window};
use std::{fs, mem};

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

impl Origami {
    pub fn glyph(&self) -> f32 {
        self.font * 0.6023
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

    pub fn store(&mut self) {
        self.dirty = self.clash;
        if !self.clash {
            let _ = fs::write(&self.script, self.lines.iter().map(|l| l.clone() + "\n").collect::<String>());
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

    pub fn write(&mut self, word: &str) {
        let code = format!("{word}(\"{word}\").at({:.2}).for(3)", self.t);
        if let Some(row) = self.add(&code, false) {
            let col = self.lines[row].find('"').map_or(0, |i| i + 1);
            self.cursor = Some((row, col));
            self.reveal(row);
        }
    }

    pub fn head(&self) -> f32 {
        76. + if self.finding.is_some() { 30. } else { 0. } + if self.clash { 30. } else { 0. }
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
    use std::fs;

    fn settle(view: &Entity<crate::Origami>, cx: &mut VisualTestContext) {
        cx.run_until_parked();
        view.update(cx, |o, cx| o.watch(cx));
        cx.run_until_parked();
    }

    // Opening scripts with mistakes should mark the line that is wrong. Undoing back to an empty
    // script should stay empty and redo should bring the text back, deleting the script file outside the app should keep the code
    // on screen and offer to save it again, and :e with a name that does not exist opens nothing.
    #[gpui::test]
    fn loads_and_marks(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join("origami-loads-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("edit.js");
        fs::write(&path, "stack()\n").unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.open(path.clone());
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
        view.update(cx, |o, _| (o.cursor, o.past) = (Some((0, 0)), vec![]));
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
        view.update(cx, |o, cx| {
            o.run("e other.js", cx);
            assert_eq!(
                (o.script.clone(), o.note.as_str()),
                (path.clone(), "no script named other.js here, nothing opened")
            );
        });
    }
}
