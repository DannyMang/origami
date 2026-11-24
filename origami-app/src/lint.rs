use crate::{
    Origami,
    complete::{DEFS, kind},
    knob::calls,
    view::{BORDER, DIM, PANEL, RED},
};
use gpui::{Div, Window, div, prelude::*, px, rgb};
use origami_lang::Kind;
use std::{sync::Arc, time::Duration};

const MOST: usize = 500;

#[derive(Clone)]
pub struct Issue {
    pub row: usize,
    pub span: (usize, usize),
    pub text: String,
}

impl Origami {
    fn flags(&self, row: usize, line: &str) -> Vec<Issue> {
        let (mut out, mut at) = (vec![], (0, 0));
        let mut cols = |b: usize| {
            at = match b >= at.0 {
                true => (b, at.1 + line[at.0..b].chars().count()),
                false => (b, at.1 - line[b..at.0].chars().count()),
            };
            at.1
        };
        let mut flag = |a: usize, b: usize, text: String| {
            if out.len() < MOST {
                let span = (cols(a), cols(b));
                out.push(Issue { row, span, text })
            }
        };
        let media = |c: &&origami_lang::Clip| c.line as usize == row + 1 && matches!(c.kind, Kind::Video | Kind::Sound);
        let total = self.tl.clips.iter().find(media).map_or(0., |c| c.total);
        for (call, name, end, found) in calls(line) {
            for k in found.iter().filter(|k| k.label != "#") {
                let what = if k.label.parse::<usize>().is_ok() { &call } else { &k.label };
                match k.hi <= k.lo {
                    true if k.value < 0. && call != "shift" => flag(k.span.0, k.span.1, format!("{call} cannot be negative")),
                    false if k.value < k.lo - 1e-9 || k.value > k.hi + 1e-9 => {
                        flag(k.span.0, k.span.1, format!("{what} takes {} to {}", k.lo, k.hi))
                    }
                    _ => {}
                }
            }
            match (call.as_str(), &found[..]) {
                ("trim", [a, b]) if b.value <= a.value => flag(name, end + 1, "trim start must be before its end".into()),
                ("trim", [.., b]) if total > 0. && b.value > total + 0.05 => {
                    flag(b.span.0, b.span.1, format!("the file is only {total:.2} s long"))
                }
                ("trim", [a, ..]) if total > 0. && a.value > total => {
                    flag(a.span.0, a.span.1, format!("the file is only {total:.2} s long"))
                }
                _ => {}
            }
        }
        out
    }

    pub fn issues(&self) -> Vec<Issue> {
        let (mut out, key) = (vec![], (self.rev, Arc::as_ptr(&self.tl) as usize));
        let mut kept = self.linted.borrow_mut();
        kept.resize_with(self.lines.len(), Default::default);
        for ((row, line), (seen, at, found)) in self.lines.iter().enumerate().zip(kept.iter_mut()) {
            if seen != line || *at != key {
                (*seen, *at, *found) = (line.clone(), key, self.flags(row, line));
            }
            out.extend(found.iter().take(MOST - out.len()).cloned());
        }
        let (mut open, mut seat): (Vec<char>, Vec<(usize, usize, usize)>) = (vec![], vec![]);
        for (row, line) in self.lines.iter().enumerate() {
            let (mut quote, mut last, mut name) = (None, ' ', 0);
            for (col, c) in line.chars().enumerate() {
                match (quote, c) {
                    (Some(q), _) if c == q && last != '\\' => quote = None,
                    (Some(_), _) => {}
                    (_, '"' | '\'' | '`') => quote = Some(c),
                    (_, '/') if last == '/' => break,
                    (_, '(' | '[' | '{') => {
                        if open.len() < MOST {
                            seat.push((row, col - name, col));
                        }
                        open.push(c);
                    }
                    (_, ')' | ']' | '}') => match open.last() {
                        Some(o) if "([{".find(*o) == ")]}".find(c) => {
                            open.pop();
                            seat.truncate(open.len());
                        }
                        _ if out.len() < MOST => out.push(Issue {
                            row,
                            span: (col, col + 1),
                            text: format!("nothing opens this {c}"),
                        }),
                        _ => {}
                    },
                    _ => {}
                }
                (last, name) = (c, if c.is_alphanumeric() || c == '_' { name + 1 } else { 0 });
            }
        }
        for (&(row, from, col), c) in seat.iter().zip(&open).take(MOST - out.len()) {
            out.push(Issue {
                row,
                span: (from, col + 1),
                text: format!("this {c} is never closed"),
            });
        }
        if let Some(line) = (self.bad as usize).checked_sub(1).and_then(|r| self.lines.get(r)) {
            let from = line.chars().take_while(|c| c.is_whitespace()).count();
            out.push(Issue {
                row: self.bad as usize - 1,
                span: (from, line.chars().count()),
                text: self.err.clone(),
            });
        }
        out
    }

    fn about(&self, row: usize, col: usize) -> Option<(usize, String)> {
        let line: Vec<char> = self.lines.get(row)?.chars().collect();
        let word = |c: &char| c.is_alphanumeric() || *c == '_';
        let from = col.min(line.len()) - line[..col.min(line.len())].iter().rev().take_while(|c| word(c)).count();
        let name: String = line[from..].iter().take_while(|c| word(c)).collect();
        let (dot, call) = (
            from > 0 && line[from - 1] == '.',
            line.get(from + name.chars().count()) == Some(&'('),
        );
        if let Some(d) = DEFS.iter().find(|d| d.name == name && call && d.method == dot) {
            return Some((from, format!("{}({})  {}", d.name, d.args, d.about)));
        }
        if dot && !call && ["pos", "scale", "rotate", "opacity"].contains(&name.as_str()) {
            let tip = format!(
                "{name} read as an argument: the property it is given to follows this clip's {name} at the same moment on the timeline"
            );
            return Some((from, tip));
        }
        let made = kind(&self.lines, &name).filter(|_| !name.is_empty() && !dot && !call)?;
        let clip = self.tl.clips.iter().find(|c| c.label == name);
        let more = match (made, clip) {
            ("Video", Some(c)) => format!(
                "  {:.2} s, {}x{}{}",
                c.total,
                c.w,
                c.h,
                if c.audio { ", has sound" } else { ", silent" }
            ),
            ("Audio", Some(c)) => format!("  {:.2} s", c.total),
            _ => String::new(),
        };
        let can = match made {
            "Audio" => "timing and sound methods; no picture, so no colour",
            "Shape" => "timing, placement and colour methods; no source to trim",
            "Text" => "timing, placement, colour and per-letter methods; no source to trim",
            "Group" => "methods apply to every clip inside",
            _ => "timing, sound, placement and colour methods",
        };
        Some((from, format!("{name}: {made}{more}  ({can})")))
    }

    pub fn dwell(&mut self) -> bool {
        let due = self.hover.is_some() && !self.tipped && self.since.elapsed() > Duration::from_millis(450);
        self.tipped |= due;
        due
    }

    pub fn tip(&self, window: &Window) -> Option<(Div, String)> {
        let (row, col) = self
            .hover
            .filter(|_| self.knobbed.is_none() && self.offers.1.is_empty() && self.turning.is_none())?;
        let scans = self.scans();
        let issue = scans.2.iter().find(|i| i.row == row && i.span.0 <= col && col <= i.span.1);
        let (from, text, bad) = match issue {
            Some(i) => (i.span.0, i.text.clone(), true),
            None => self.about(row, col).filter(|_| self.tipped).map(|a| (a.0, a.1, false))?,
        };
        let (hidden, top) = (&scans.1, self.top as usize);
        if row < top || hidden.get(row) != Some(&false) {
            return None;
        }
        let w = f32::from(window.viewport_size().width);
        let y = self.head() + (top..row).filter(|i| !hidden[*i]).count() as f32 * self.row();
        Some((
            div()
                .absolute()
                .left(px((w - self.side + self.inset() + from as f32 * self.glyph()).min(w - 420.)))
                .top(px(y + self.row()))
                .max_w(px(410.))
                .px_3()
                .py_1()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(if bad { RED } else { BORDER }))
                .rounded(px(6.))
                .text_size(px(12.))
                .text_color(rgb(if bad { RED } else { DIM }))
                .child(text.clone()),
            text,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::MOST;

    // Warnings on lines of tens of thousands of brackets should take moments and stop at the first
    // few hundred, still naming each unclosed call, counting columns past wide letters and keeping
    // the script's error.
    #[gpui::test]
    fn huge_lines_lint_quickly(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            let lines = [
                format!("  ab({}", "a(".repeat(50_000)),
                "(\\\"".repeat(50_000),
                "(/*".repeat(50_000),
            ];
            (app.lines, app.bad, app.err) = (lines.into(), 1, "wrong".into());
            app
        });
        view.update(cx, |o, _| {
            let start = std::time::Instant::now();
            let found = o.issues();
            let spans: Vec<_> = found[..2].iter().map(|i| (i.row, i.span, i.text.as_str())).collect();
            assert_eq!(
                spans,
                [(0, (2, 5), "this ( is never closed"), (0, (5, 7), "this ( is never closed")]
            );
            assert_eq!((found.len(), found[MOST].text.as_str()), (MOST + 1, "wrong"));
            o.lines = vec!["\u{e9}.opacity(3)".repeat(20_000)];
            let found = o.issues();
            assert_eq!(
                (found.len(), found[1].span, found[1].text.as_str()),
                (MOST + 1, (22, 23), "opacity takes 0 to 1")
            );
            assert!(start.elapsed().as_secs() < 2);
        });
    }
}
