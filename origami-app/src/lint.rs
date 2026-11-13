use crate::{
    Origami,
    complete::{DEFS, kind},
    knob::calls,
    view::{BORDER, DIM, PANEL},
};
use gpui::{Div, Window, div, prelude::*, px, rgb};
use origami_lang::Kind;
use std::time::Duration;

const RED: u32 = 0xd07277;

pub struct Issue {
    pub row: usize,
    pub span: (usize, usize),
    pub text: String,
}

impl Origami {
    pub fn issues(&self) -> Vec<Issue> {
        let mut out = vec![];
        for (row, line) in self.lines.iter().enumerate() {
            let cols = |b: usize| line[..b].chars().count();
            let mut flag = |a: usize, b: usize, text: String| {
                out.push(Issue {
                    row,
                    span: (cols(a), cols(b)),
                    text,
                })
            };
            for (call, name, end, found) in calls(line) {
                let timed = ["trim", "for", "at", "shift"].contains(&call.as_str());
                for k in found.iter().filter(|k| k.label != "#") {
                    let what = if k.label.parse::<usize>().is_ok() { &call } else { &k.label };
                    match timed {
                        true if k.value < 0. && call != "shift" => flag(k.span.0, k.span.1, format!("{call} cannot be negative")),
                        false if k.value < k.lo - 1e-9 || k.value > k.hi + 1e-9 => {
                            flag(k.span.0, k.span.1, format!("{what} takes {} to {}", k.lo, k.hi))
                        }
                        _ => {}
                    }
                }
                let media = |c: &&origami_lang::Clip| c.line as usize == row + 1 && matches!(c.kind, Kind::Video | Kind::Sound);
                let total = self.tl.clips.iter().find(media).map_or(0., |c| c.total);
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
        }
        let mut open: Vec<(usize, usize, char)> = vec![];
        for (row, line) in self.lines.iter().enumerate() {
            let (mut quote, mut last) = (None, ' ');
            for (col, c) in line.chars().enumerate() {
                match (quote, c) {
                    (Some(q), _) if c == q && last != '\\' => quote = None,
                    (Some(_), _) => {}
                    (_, '"' | '\'' | '`') => quote = Some(c),
                    (_, '/') if last == '/' => break,
                    (_, '(' | '[' | '{') => open.push((row, col, c)),
                    (_, ')' | ']' | '}') => match open.last() {
                        Some(o) if "([{".find(o.2) == ")]}".find(c) => drop(open.pop()),
                        _ => out.push(Issue {
                            row,
                            span: (col, col + 1),
                            text: format!("nothing opens this {c}"),
                        }),
                    },
                    _ => {}
                }
                last = c;
            }
        }
        for (row, col, c) in open {
            let word = |c: &char| c.is_alphanumeric() || *c == '_';
            let name = self.lines[row]
                .chars()
                .take(col)
                .collect::<Vec<_>>()
                .iter()
                .rev()
                .take_while(|c| word(c))
                .count();
            out.push(Issue {
                row,
                span: (col - name, col + 1),
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
        let issue = self
            .issues()
            .into_iter()
            .find(|i| i.row == row && i.span.0 <= col && col <= i.span.1);
        let (from, text, bad) = match issue {
            Some(i) => (i.span.0, i.text, true),
            None => self.about(row, col).filter(|_| self.tipped).map(|a| (a.0, a.1, false))?,
        };
        let (hidden, top) = (self.hidden(), self.top as usize);
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
