use crate::{
    Origami,
    edit::links,
    keys::{link, rewrite, secs, stamp},
    knob::close,
    view::{BORDER, DIM, RED},
    voice::placed,
};
use gpui::{ClipboardItem, Context, Div, Window, div, prelude::*, px, rgb};
use origami_engine::Preview;
use origami_lang::{Clip, Kind};
use std::ops::Range;

fn nest(src: &str) -> Option<bool> {
    let (mut open, mut quote, mut note) = (vec![], None, false);
    for (i, c) in src.char_indices() {
        match (quote, c) {
            (_, '\n') => (quote, note) = (None, false),
            _ if note => {}
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (_, '"' | '\'' | '`') => quote = Some(c),
            (_, '/') if src[i..].starts_with("//") => note = true,
            (_, '(' | '[' | '{') => open.push(i),
            (_, ')' | ']' | '}') => drop(open.pop()),
            _ => {}
        }
    }
    let head = &src[..open.pop()?];
    match head.rsplit(|c: char| !c.is_alphanumeric() && c != '_').next()? {
        "cat" => Some(true),
        "stack" => Some(false),
        _ => None,
    }
}

fn whole(item: &str) -> bool {
    let (mut depth, mut quote) = (0, None);
    for c in item.chars() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (_, '"' | '\'' | '`') => quote = Some(c),
            (_, '(' | '[' | '{') => depth += 1,
            (_, ')' | ']' | '}') => depth -= 1,
            _ if depth == 0 && !(c.is_alphanumeric() || "_$.".contains(c)) => return false,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0 && item.starts_with(char::is_alphabetic) && !["cat(", "stack("].iter().any(|k| item.starts_with(k))
}

fn at(s: &str) -> Option<f64> {
    let a = s.find(".at(")? + 4;
    secs(s[a..close(s.as_bytes(), a - 1)?].trim().trim_matches(['"', '\'']))
}

fn places(v: f64) -> usize {
    (3..9)
        .find(|d| ((v * 10f64.powi(*d)).round() / 10f64.powi(*d) - v).abs() < 1e-9)
        .unwrap_or(9) as usize
}

fn exact(v: f64) -> String {
    format!("{:.*}", places(v), v)
}

fn plain(line: &str) -> &str {
    let line = line.trim();
    line.strip_suffix(',').unwrap_or(line)
}

fn named(line: &str) -> Option<&str> {
    let rest = ["let ", "const ", "var "].iter().find_map(|k| line.trim_start().strip_prefix(k))?;
    Some(rest.split_once('=')?.0.trim())
}

pub fn set(lines: &mut [String], line: u32, op: &str, arg: impl Fn(&str) -> String) {
    let row = &mut lines[line as usize - 1];
    if row.contains(')') {
        return rewrite(lines, line, op, arg);
    }
    let end = plain(row).len() + row.len() - row.trim_start().len();
    row.insert_str(end, &format!(".{op}({})", arg("")));
}

impl Origami {
    pub fn usage(&mut self, verb: &str) -> Option<(Clip, Range<usize>, String)> {
        self.note.clear();
        let c = self.focused()?.clone();
        self.used(c, verb)
    }

    pub fn used(&mut self, mut c: Clip, verb: &str) -> Option<(Clip, Range<usize>, String)> {
        let last = (c.line as usize).checked_sub(1).filter(|i| *i < self.lines.len())?;
        let first = (0..=last).rev().find(|i| !self.lines[*i].trim_start().starts_with('.'))?;
        let twins: Vec<u32> = self.tl.clips.iter().filter(|o| o.line == c.line).map(|o| o.z).collect();
        let bare = named(&self.lines[first]).and_then(|name| {
            let uses: Vec<usize> = (0..self.lines.len()).filter(|i| plain(&self.lines[*i]) == name).collect();
            let nth = twins.iter().position(|z| *z == c.z)?;
            (uses.len() == twins.len()).then(|| uses[nth])
        });
        let (first, last) = bare.map_or((first, last), |i| (i, i));
        let item: String = self.lines[first..=last].iter().map(|l| l.trim()).collect();
        let item = item.strip_suffix(',').unwrap_or(&item);
        let body = match named(item) {
            Some(_) if ["move", "trim"].contains(&verb) => item.split_once('=').map_or(item, |p| p.1.trim()),
            _ => item,
        };
        if bare.is_none() && twins.len() > 1 || !whole(body) {
            self.note = format!("line {} holds more than {}, edit the script to {verb} it", c.line, c.label);
            return None;
        }
        c.line = last as u32 + 1;
        Some((c, first..last + 1, item.into()))
    }

    pub fn alone(&mut self, c: &Clip, verb: &str) -> bool {
        let alone = self.tl.clips.iter().all(|o| o.z == c.z);
        if alone {
            self.note = format!("{} is the last clip, edit the script to {verb} it", c.label);
        }
        alone
    }

    fn cuts(&self, c: &Clip, seq: bool) -> Option<(f64, f64, f64, f64)> {
        let (a, b) = (c.src_in, c.src_in + (c.end - c.start) * c.speed);
        let m = ((a + (self.t - c.start) * c.speed) * 1e3).round() / 1e3;
        let inside = c.start < self.t && self.t < c.end;
        match c.kind {
            Kind::Video if inside => Preview::seam(c, self.t, self.tl.fps, seq),
            Kind::Sound if inside => Some((m, m, ((m - a) / c.speed * 1e3).round() / 1e3, b)),
            _ => None,
        }
        .filter(|(m, s, _, end)| m - a >= 0.04 && end - s >= 0.04)
    }

    fn echo(&mut self, c: &Clip, rows: Range<usize>, by: f64, cut: Option<(f64, f64, f64, f64)>, verb: &str) {
        let mut next = self.lines[rows.clone()].to_vec();
        let (n, def) = (next.len() as u32, (c.home as usize).saturating_sub(1).min(rows.start));
        let chain = |(i, l): &(usize, &String)| *i == 0 || l.trim_start().starts_with('.');
        let fixed = self.lines[def..rows.start]
            .iter()
            .enumerate()
            .take_while(chain)
            .any(|(_, l)| l.contains(".at("));
        let timed = link(&next, n, "at");
        let old = timed.map_or(Some(0.).filter(|_| !fixed), |i| at(&next[i]));
        let (Some(cat), Some(old)) = (nest(&self.lines[..rows.start].join("\n")), old) else {
            return self.note = format!(
                "{} is not a plain item of cat( ) or stack( ), edit the script to {verb} it",
                c.label
            );
        };
        self.change(|this| {
            if let Some((m, s, _, end)) = cut {
                let open = places(end) > 3 && (end - c.total).abs() < 1e-9;
                let tail = if open { exact(s) } else { format!("{}, {}", exact(s), exact(end)) };
                set(&mut next, n, "trim", |_| tail.clone());
                set(&mut this.lines, c.line, "trim", |_| format!("{}, {}", exact(c.src_in), exact(m)));
            }
            if !(cat && c.video && timed.is_none()) {
                let at = if cut.is_some() {
                    old + by
                } else {
                    ((old + by) * 1e3).round() / 1e3
                };
                set(&mut next, n, "at", |_| exact(at).trim_end_matches('0').trim_end_matches('.').into());
            }
            let last = &mut this.lines[rows.end - 1];
            if !last.trim_end().ends_with(',') {
                last.push(',');
            }
            this.lines.splice(rows.end..rows.end, next);
        });
    }

    pub fn split(&mut self) {
        let Some((c, rows, _)) = self.usage("split") else { return };
        let (item, n) = (&self.lines[rows.clone()], rows.len() as u32);
        let seq = c.video && link(item, n, "at").is_none() && nest(&self.lines[..rows.start].join("\n")) == Some(true);
        let held = link(item, n, "for").is_some();
        let Some(cut) = self.cuts(&c, seq).filter(|_| !held) else {
            return self.note = "put the playhead inside a video or sound clip to split it".into();
        };
        self.echo(&c, rows, cut.2, Some(cut), "split");
        let at = c.start + cut.2;
        if self.note.is_empty() && ((at - self.t) * self.tl.fps as f64).abs() >= 1. {
            self.note = format!("split at {}, the nearest cut that keeps every frame", stamp(at));
        }
    }

    pub fn twin(&mut self) {
        if let Some((c, rows, _)) = self.usage("duplicate") {
            self.echo(&c, rows, c.end - c.start, None, "duplicate");
        }
    }

    pub fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        let verb = if cut { "cut" } else { "copy" };
        let Some((_, rows, item)) = self.usage(verb).filter(|(c, ..)| !(cut && self.alone(c, verb))) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(item.clone()));
        (self.note, self.copied) = (format!("copied {item}"), item);
        if cut {
            self.change(|this| drop(this.lines.drain(rows)));
        }
    }

    pub fn lay(&mut self) {
        if self.copied.is_empty() {
            return self.note = "nothing copied yet: select a clip and press cmd+c".into();
        }
        let name: String = self.copied.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !self.copied[name.len()..].starts_with('(') && !self.lines.iter().any(|l| named(l) == Some(&name)) {
            return self.note = format!("{name} is no longer defined in the script, nothing pasted");
        }
        let keep = |l: &String| !l.starts_with(".at(") && !l.starts_with(".shift(");
        let chain: String = links(&self.copied).into_iter().filter(keep).collect();
        let (item, mut row) = (format!("{chain}.at({})", (self.t * 100.).round() / 100.), 0);
        self.change(|this| row = placed(&mut this.lines, 0, &item, &["stack("]));
        self.sel = Some(self.tl.clips.len() as u32);
        self.reveal(row);
        self.note = format!("pasted {item}");
    }

    pub fn actions(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let c = self.focused().filter(|_| self.menu == "clip")?;
        let row = |label: &'static str, key: &'static str, cx: &mut Context<Self>, f: fn(&mut Self, &mut Context<Self>)| {
            let hint = div().text_color(rgb(DIM)).child(key);
            let line = div().flex_1().flex().justify_between().gap_4().child(label).child(hint);
            self.tool(line, cx, move |t, _, cx| f(t, cx))
        };
        let items = vec![
            row("Split at playhead", "S", cx, |t, _| t.split()).when(self.cuts(c, true).is_none(), |d| d.text_color(rgb(DIM))),
            row("Cut", "\u{2318}X", cx, |t, cx| t.copy(true, cx)),
            row("Copy", "\u{2318}C", cx, |t, cx| t.copy(false, cx)),
            row("Paste at playhead", "\u{2318}V", cx, |t, _| t.lay()),
            row("Duplicate", "\u{2318}D", cx, |t, _| t.twin()),
            div().h(px(1.)).my_1().bg(rgb(BORDER)),
            row("Delete", "\u{232b}", cx, |t, _| t.delete()).text_color(rgb(RED)),
        ];
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let (x, y) = (f32::from(self.pin.x).min(w - 228.), f32::from(self.pin.y));
        Some(Self::list(items).w(px(220.)).left(px(x)).bottom(px(h - y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::RULER;
    use gpui::{Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point};
    use std::{fs, sync::Arc};

    const DEF: &str = "const v = video(\"v.mp4\")\nconst a = sound(\"a.wav\")\n";
    const SRC: &str = "stack(
  cat(
    v.trim(0, 4),
    v
      .trim(4, 8)
      .exposure(0.3),
  ),
  a.trim(0, 6).at(1),
  text(\"hi\").at(2).for(3),
)";
    const NEST: &str = "cat(
  video(\"v.mp4\"),
  stack(
    v.speed(2).shift(1),
    a.trim(0, 6).at(\"0:01\").shift(0.5)
  )
)";

    fn open<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Origami>, &'a mut VisualTestContext) {
        let dir = std::env::temp_dir().join(format!("origami-clip-{name}"));
        fs::create_dir_all(&dir).unwrap();
        for (file, src) in [("a.wav", "sine=d=10"), ("v.mp4", "color=black:s=64x64:r=30:d=10")] {
            let made = origami_lang::status(
                origami_lang::ffmpeg()
                    .args(["-v", "error", "-y", "-f", "lavfi", "-i", src])
                    .arg(dir.join(file)),
            );
            assert!(made.unwrap().success());
        }
        cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.script = dir.join("edit.js");
            window.focus(&app.focus);
            app
        })
    }

    fn sync(view: &Entity<Origami>, cx: &mut VisualTestContext) -> Vec<(f64, f64)> {
        let near = |t: f64| (t * 1000.).round() / 1000.;
        view.update(cx, |o, _| {
            fs::write(&o.script, o.lines.join("\n")).unwrap();
            o.tl = Arc::new(origami_lang::eval(&o.script, false, origami_engine::footprint).unwrap());
            o.tl.clips.iter().map(|c| (near(c.start), near(c.end))).collect()
        })
    }

    fn act(o: &mut Origami, verb: &str, cx: &mut Context<Origami>) {
        match verb {
            "split" => o.split(),
            "twin" => o.twin(),
            "copy" => o.copy(false, cx),
            "cut" => o.copy(true, cx),
            "lay" => o.lay(),
            _ => o.delete(),
        }
    }

    // Timeline commands (split, duplicate, copy, cut, paste, delete) should rewrite the script the way
    // you would by hand and leave the clips at the right times; a split cuts where the frame on screen
    // starts, with times to the millisecond unless a frame needs more decimals, so no frame is lost,
    // and says where it cut when that is not the playhead.
    // When a clip cannot be changed safely, the script is left alone and a note says why.
    #[gpui::test]
    fn shapes(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, "shapes");
        let run = |cx: &mut VisualTestContext, src: &str, steps: &[(&str, u32, f64)]| {
            let src = if src.starts_with("const") {
                src.to_string()
            } else {
                format!("{DEF}{src}")
            };
            view.update(cx, |o, _| {
                (o.lines, o.past) = (src.lines().map(String::from).collect(), Default::default())
            });
            for (verb, z, t) in steps {
                sync(&view, cx);
                view.update(cx, |o, cx| {
                    (o.sel, o.t) = (Some(*z), *t);
                    act(o, verb, cx);
                });
            }
            let spans = sync(&view, cx);
            view.update(cx, |o, _| (o.lines[2..].join("\n"), spans, o.past.len(), o.note.clone()))
        };
        let same = |got: (String, Vec<(f64, f64)>, usize, String), src: &str, says: &str| {
            assert_eq!((got.0.as_str(), got.2), (src, 0));
            assert!(got.3.contains(says), "{}", got.3);
        };
        let got = run(cx, SRC, &[("split", 1, 6.)]);
        let want = SRC.replace(
            "      .trim(4, 8)\n      .exposure(0.3),",
            "      .trim(4.000, 6.000)\n      .exposure(0.3),\n    v\n      .trim(6.000, 8.000)\n      .exposure(0.3),",
        );
        assert_eq!((got.0, got.2), (want, 1));
        assert_eq!(got.1, [(0., 4.), (4., 6.), (6., 8.), (1., 7.), (2., 5.)]);
        assert!(view.update(cx, |o, _| o.tl.clips[1..3].iter().all(|c| c.grade.is_some())));
        let got = run(cx, SRC, &[("split", 2, 3.)]);
        assert!(
            got.0.contains("  a.trim(0.000, 2.000).at(1),\n  a.trim(2.000, 6.000).at(3),\n"),
            "{}",
            got.0
        );
        assert_eq!((&got.1[2..4], got.2), (&[(1., 3.), (3., 7.)][..], 1));
        let got = run(cx, SRC, &[("twin", 0, 0.)]);
        assert!(got.0.contains("    v.trim(0, 4),\n    v.trim(0, 4),\n    v\n"), "{}", got.0);
        assert_eq!((&got.1[..3], got.2), (&[(0., 4.), (4., 8.), (8., 12.)][..], 1));
        let got = run(cx, SRC, &[("twin", 2, 0.)]);
        assert!(got.0.contains("  a.trim(0, 6).at(1),\n  a.trim(0, 6).at(7),\n"), "{}", got.0);
        assert_eq!((&got.1[2..4], got.2), (&[(1., 7.), (7., 13.)][..], 1));
        let got = run(cx, SRC, &[("twin", 3, 0.)]);
        assert!(
            got.0.contains("  text(\"hi\").at(2).for(3),\n  text(\"hi\").at(5).for(3),\n)"),
            "{}",
            got.0
        );
        assert_eq!((&got.1[3..], got.2), (&[(2., 5.), (5., 8.)][..], 1));
        let gone = SRC.replace("    v\n      .trim(4, 8)\n      .exposure(0.3),\n", "");
        let got = run(cx, SRC, &[("delete", 1, 0.)]);
        assert_eq!((got.0, got.1, got.2), (gone.clone(), vec![(0., 4.), (1., 7.), (2., 5.)], 1));
        let got = run(cx, SRC, &[("cut", 1, 0.), ("lay", 0, 2.5)]);
        let want = gone.replace(".for(3),\n", ".for(3),\n  v.trim(4, 8).exposure(0.3).at(2.5),\n");
        assert_eq!((got.0, got.2), (want, 2));
        assert_eq!(got.1, [(0., 4.), (1., 7.), (2., 5.), (2.5, 6.5)]);
        assert_eq!(
            view.update(cx, |o, _| (o.copied.clone(), o.sel)),
            ("v.trim(4, 8).exposure(0.3)".into(), Some(3))
        );
        assert_eq!(
            cx.read_from_clipboard().and_then(|i| i.text()).unwrap(),
            "v.trim(4, 8).exposure(0.3)"
        );

        let got = run(cx, NEST, &[("split", 0, 3.)]);
        assert!(
            got.0
                .contains("  video(\"v.mp4\").trim(0.000, 3.000),\n  video(\"v.mp4\").trim(3.000, 10.000),\n"),
            "{}",
            got.0
        );
        assert_eq!(got.1, [(0., 3.), (3., 10.), (11., 16.), (11.5, 17.5)]);
        let got = run(cx, NEST, &[("split", 1, 13.)]);
        let want = "    v.speed(2).shift(1).trim(0.000, 3.967),\n    v.speed(2).shift(1).trim(3.966, 9.998).at(1.984),\n";
        assert!(got.0.contains(want), "{}", got.0);
        assert_eq!(got.1, [(0., 10.), (11., 12.984), (12.984, 16.), (11.5, 17.5)]);
        let got = run(cx, NEST, &[("split", 2, 14.5)]);
        let want = "    a.trim(0.000, 3.000).at(\"0:01\").shift(0.5),\n    a.trim(3.000, 6.000).at(4).shift(0.5)\n  )";
        assert!(got.0.contains(want), "{}", got.0);
        assert_eq!((&got.1[2..], got.2), (&[(11.5, 14.5), (14.5, 17.5)][..], 1));
        let got = run(cx, NEST, &[("twin", 2, 0.)]);
        assert!(got.0.contains("\n    a.trim(0, 6).at(7).shift(0.5)\n"), "{}", got.0);
        assert_eq!(got.1[2..], [(11.5, 17.5), (17.5, 23.5)]);
        let got = run(cx, NEST, &[("copy", 2, 0.), ("lay", 0, 2.5)]);
        assert_eq!(got.0, format!("stack({NEST}, a.trim(0, 6).at(2.5))"));
        assert_eq!((&got.1[2..], got.2), (&[(11.5, 17.5), (2.5, 8.5)][..], 1));
        let got = run(cx, "cat(\n  v.trim(0, 4).at(2),\n  v.trim(4, 6)\n)", &[("split", 0, 3.)]);
        assert!(
            got.0.contains("  v.trim(0.000, 1.000).at(2),\n  v.trim(1.000, 4.000).at(3),\n"),
            "{}",
            got.0
        );
        assert_eq!(got.1, [(2., 3.), (3., 6.), (4., 6.)]);

        let got = run(cx, "cat(\n  v.trim(0, 3),\n  v.trim(5, 6),\n)", &[("split", 0, 38. / 30.)]);
        let want = "  v.trim(0.000, 1.266),\n  v.trim(1.266, 3.000),\n  v.trim(5, 6),";
        assert!(got.0.contains(want), "{}", got.0);
        assert_eq!(got.1, [(0., 1.266), (1.266, 3.), (3., 4.)]);
        let got = run(
            cx,
            "cat(\n  v.trim(0, 2).speed(0.5),\n  v.trim(5, 6),\n)",
            &[("split", 0, 31. / 30.)],
        );
        assert_eq!(got.1, [(0., 1.), (1., 4.), (4., 5.)]);
        assert_eq!(got.3, "split at 0:01.00, the nearest cut that keeps every frame");
        let bare = "cat(\n  v.trim(0, 2),\n  v\n)";
        let got = run(cx, bare, &[("split", 1, 6.)]);
        assert_eq!(
            (got.0.as_str(), got.2),
            ("cat(\n  v.trim(0, 2),\n  v.trim(0.000, 4.000),\n  v.trim(4.000, 10.000)\n)", 1)
        );
        assert_eq!(got.1, [(0., 2.), (2., 6.), (6., 12.)]);
        let pair = "cat(\n  v,\n  v,\n)";
        let got = run(cx, pair, &[("split", 1, 13.)]);
        assert_eq!(
            (got.0.as_str(), got.2),
            ("cat(\n  v,\n  v.trim(0.000, 3.000),\n  v.trim(3.000, 10.000),\n)", 1)
        );
        assert_eq!(got.1, [(0., 10.), (10., 13.), (13., 20.)]);
        let got = run(cx, pair, &[("delete", 0, 0.)]);
        assert_eq!((got.0.as_str(), got.1, got.2), ("cat(\n  v,\n)", vec![(0., 10.)], 1));
        let got = run(cx, "stack(\n  v,\n  a\n)", &[("twin", 1, 0.)]);
        assert_eq!((got.0.as_str(), got.2), ("stack(\n  v,\n  a,\n  a.at(10)\n)", 1));
        assert_eq!(got.1, [(0., 10.), (0., 10.), (10., 20.)]);
        same(
            run(cx, "cat(v, v,\n  v\n)", &[("split", 2, 23.)]),
            "cat(v, v,\n  v\n)",
            "line 1 holds more than v, edit",
        );

        let more = "holds more than";
        let flat = "cat(v, a.trim(0, 2).at(1))";
        for verb in ["split", "twin", "copy", "cut", "delete"] {
            same(
                run(cx, flat, &[(verb, 0, 1.)]),
                flat,
                &format!("line 1 {more} v, edit the script to "),
            );
            same(
                run(cx, flat, &[(verb, 1, 1.5)]),
                flat,
                &format!("line 3 {more} a, edit the script to "),
            );
        }
        assert_eq!(view.update(cx, |o, _| o.copied.clone()), "a.trim(0, 6).at(\"0:01\").shift(0.5)");
        same(run(cx, "stack(v.trim(0, 4))", &[("twin", 0, 1.)]), "stack(v.trim(0, 4))", more);
        same(run(cx, "stack(v.trim(0, 4))", &[("delete", 0, 1.)]), "stack(v.trim(0, 4))", more);
        same(
            run(cx, "stack(\n  v.trim(0, 4), // intro\n)", &[("cut", 0, 1.)]),
            "stack(\n  v.trim(0, 4), // intro\n)",
            more,
        );
        let plain = "is not a plain item of cat( ) or stack( ), edit the script to";
        let list = "stack(...[\n  v.trim(0, 4),\n])";
        same(run(cx, list, &[("split", 0, 1.)]), list, plain);
        let late = "const k = 1\nconst v = video(\"v.mp4\")\nstack(\n  v.trim(0, 4).at(k),\n)";
        same(run(cx, late, &[("twin", 0, 1.)]), &late[late.find("stack").unwrap()..], plain);
        let set = "const v = video(\"v.mp4\")\n  .at(2)\nstack(\n  v.trim(0, 4),\n)";
        same(run(cx, set, &[("split", 0, 3.)]), &set[set.find("stack").unwrap()..], plain);
        let inside = "put the playhead inside a video or sound clip to split it";
        for (z, t) in [(3, 3.), (0, 5.), (0, 4.), (0, 0.)] {
            same(run(cx, SRC, &[("split", z, t)]), SRC, inside);
        }
        let held = "stack(\n  v.for(3),\n)";
        same(run(cx, held, &[("split", 0, 1.)]), held, inside);
        view.update(cx, |o, _| o.copied.clear());
        same(run(cx, SRC, &[("lay", 0, 1.)]), SRC, "nothing copied yet");
        view.update(cx, |o, _| o.copied = "voice_9.at(1)".into());
        same(
            run(cx, SRC, &[("lay", 0, 1.)]),
            SRC,
            "voice_9 is no longer defined in the script, nothing pasted",
        );
        let only = "stack(\n  v.trim(0, 4),\n)";
        for verb in ["delete", "cut"] {
            same(run(cx, only, &[(verb, 0, 1.)]), only, "v is the last clip, edit the script to ");
        }
        let spread = "stack(\n  v.trim(\n    0, 4),\n  v.trim(4, 8).at(\n    1),\n)";
        for (verb, z) in [("split", 0), ("twin", 1)] {
            same(run(cx, spread, &[(verb, z, 2.)]), spread, "holds more than v, edit the script to");
        }
    }

    // A script reloaded from disk while the clip menu is open, after an edit made in another program,
    // should close the menu, so its commands cannot land on whichever clip now has the old number.
    #[gpui::test]
    fn reload_closes_menu(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, "reload");
        let src = format!("{DEF}{SRC}");
        let script = view.update(cx, |o, cx| {
            fs::write(&o.script, &src).unwrap();
            o.watch(cx);
            o.script.clone()
        });
        cx.run_until_parked();
        view.update(cx, |o, _| (o.sel, o.menu) = (Some(1), "clip"));
        fs::write(&script, src.replace("    v.trim(0, 4),\n", "")).unwrap();
        view.update(cx, |o, cx| o.watch(cx));
        cx.run_until_parked();
        assert_eq!(view.update(cx, |o, _| (o.menu, o.lines.len())), ("", src.lines().count() - 1));
    }

    // The right-click menu on a timeline clip and its keyboard shortcuts should do the same things,
    // each one undoable, and dragging a clip sideways should shift it. While text is selected in the
    // script, the same keys act on the text instead.
    #[gpui::test]
    fn menu(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, "menu");
        let lines: Vec<String> = format!("{DEF}{SRC}").lines().map(String::from).collect();
        view.update(cx, |o, _| o.lines = lines.clone());
        sync(&view, cx);
        cx.run_until_parked();
        let spot = |cx: &mut VisualTestContext, t: f64, lane: f32| {
            view.update_in(cx, |o, window, cx| {
                cx.notify();
                let h = f32::from(window.viewport_size().height);
                point(px(o.lead() + (t / o.per_px(window)) as f32), px(h - 28. - o.dock + RULER + lane))
            })
        };
        let (first, word, moved, far) = (spot(cx, 1., 84.), spot(cx, 3., 28.), spot(cx, 4., 28.), point(px(200.), px(10.)));
        let click = |cx: &mut VisualTestContext, at: Point<Pixels>, count: usize| {
            let button = if count == 2 { MouseButton::Right } else { MouseButton::Left };
            cx.simulate_mouse_move(at, None, Modifiers::default());
            cx.simulate_mouse_down(at, button, Modifiers::default());
            cx.simulate_mouse_up(at, button, Modifiers::default());
        };
        let twice = |cx: &mut VisualTestContext| {
            view.update(cx, |o, _| (o.t, o.sel) = (2., None));
            click(cx, first, 1);
            click(cx, first, 2);
            assert_eq!(
                view.update(cx, |o, _| (o.menu, o.sel, o.pin, o.past.len())),
                ("clip", Some(0), first, 0)
            );
        };
        let state = |cx: &mut VisualTestContext| view.update(cx, |o, _| (o.lines.join("\n"), o.past.len(), o.menu));
        let undo = |cx: &mut VisualTestContext| {
            cx.simulate_keystrokes("cmd-z");
            assert_eq!(view.update(cx, |o, _| (o.lines.clone(), o.past.len())), (lines.clone(), 0));
        };
        let all = lines.join("\n");
        let cut = all.replace("    v.trim(0, 4),\n", "");
        let wants = [
            (all.replace("v.trim(0, 4),", "v.trim(0.000, 2.000),\n    v.trim(2.000, 4.000),"), 1),
            (cut.clone(), 1),
            (all.clone(), 0),
            (all.replace(".for(3),\n", ".for(3),\n  v.trim(0, 4).at(2),\n"), 1),
            (all.replace("v.trim(0, 4),", "v.trim(0, 4),\n    v.trim(0, 4),"), 1),
            (cut, 1),
        ];

        click(cx, first, 1);
        assert_eq!(
            view.update(cx, |o, _| (o.sel, o.menu, o.lines.clone())),
            (Some(0), "", lines.clone())
        );
        cx.simulate_mouse_move(word, None, Modifiers::default());
        cx.simulate_mouse_down(word, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(moved, MouseButton::Left, Modifiers::default());
        let got = state(cx);
        assert!(
            got.0.contains("  text(\"hi\").at(2).for(3).shift(1.0000),") && got.1 == 1,
            "{}",
            got.0
        );
        cx.run_until_parked();
        undo(cx);
        sync(&view, cx);

        twice(cx);
        cx.simulate_keystrokes("escape");
        assert_eq!(
            view.update(cx, |o, _| (o.menu, o.mouse, o.lines.clone())),
            ("", true, lines.clone())
        );
        twice(cx);
        click(cx, far, 1);
        assert_eq!(state(cx), (all.clone(), 0, ""));

        for (i, want) in wants.iter().enumerate() {
            twice(cx);
            let y = if i < 5 { 26. * i as f32 - 157. } else { -18. };
            click(cx, point(first.x + px(30.), first.y + px(y)), 1);
            assert_eq!(state(cx), (want.0.clone(), want.1, ""), "item {i}");
            if i == 1 || i == 2 {
                assert_eq!(view.update(cx, |o, _| o.copied.clone()), "v.trim(0, 4)");
                assert_eq!(cx.read_from_clipboard().and_then(|i| i.text()).unwrap(), "v.trim(0, 4)");
            }
            if i == 1 {
                cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                view.update(cx, |o, _| o.copied.clear());
            }
            if want.1 == 1 {
                undo(cx);
            }
        }
        for (key, i) in [
            ("s", 0),
            ("shift-s", 0),
            ("cmd-x", 1),
            ("cmd-c", 2),
            ("cmd-v", 3),
            ("cmd-d", 4),
            ("backspace", 5),
            ("delete", 5),
        ] {
            view.update(cx, |o, _| (o.t, o.sel) = (2., Some(0)));
            cx.simulate_keystrokes(key);
            assert_eq!(state(cx), (wants[i].0.clone(), wants[i].1, ""), "{key}");
            if wants[i].1 == 1 {
                undo(cx);
            }
        }
        assert_eq!(cx.read_from_clipboard().and_then(|i| i.text()).unwrap(), "v.trim(0, 4)");

        view.update(cx, |o, _| o.sel = Some(3));
        cx.simulate_keystrokes("s");
        assert_eq!(state(cx), (all.clone(), 0, ""));
        assert!(view.update(cx, |o, _| o.note.starts_with("put the playhead inside")));
        view.update(cx, |o, _| (o.mouse, o.sel) = (false, Some(0)));
        cx.simulate_keystrokes("s cmd-d");
        assert_eq!(state(cx), (all.clone(), 0, ""));

        view.update(cx, |o, _| (o.mouse, o.anchor, o.cursor) = (true, Some((4, 6)), Some((4, 10))));
        cx.simulate_keystrokes("cmd-c cmd-d");
        assert_eq!(cx.read_from_clipboard().and_then(|i| i.text()).unwrap(), "trim");
        assert_eq!(
            view.update(cx, |o, _| (o.lines.clone(), o.copied.clone())),
            (lines.clone(), "v.trim(0, 4)".into())
        );
        cx.simulate_keystrokes("cmd-x");
        assert_eq!(view.update(cx, |o, _| o.lines[4].clone()), "    v.(0, 4),");
        cx.simulate_keystrokes("cmd-v");
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), lines);
        cx.simulate_keystrokes("s");
        assert_eq!(view.update(cx, |o, _| o.lines[4].clone()), "    v.trims(0, 4),");
    }
}
