use crate::{
    Origami,
    stage::{rest, share},
    view::{BLUE, INK, TEXT},
    voice::top,
};
use gpui::{Bounds, Context, Div, PathBuilder, Pixels, Point, Window, point, prelude::*, px, quad, rgb, rgba, size};

fn drawn(lines: &mut Vec<String>, points: &[(f64, f64)]) -> usize {
    let taken = |n: &String| lines.iter().any(|l| l.starts_with(&format!("const {n} ")));
    let name = (1..)
        .map(|n| if n == 1 { "shape".into() } else { format!("shape{n}") })
        .find(|n| !taken(n));
    let code = |l: &String| top(l) && !l.trim().is_empty() && !l.starts_with("//");
    let (at, pair) = (lines.iter().rposition(code).unwrap_or(lines.len()), |p: &(f64, f64)| {
        format!("[{}, {}]", rest(p.0, 0., 0.), rest(p.1, 0., 0.))
    });
    let rows = points
        .chunks(3)
        .map(|c| format!("  {},", c.iter().map(pair).collect::<Vec<_>>().join(", ")));
    let head = format!("const {} = path([", name.unwrap_or_default());
    let made = [head].into_iter().chain(rows).chain(["], { curve: true })".into(), String::new()]);
    lines.splice(at..at, made);
    at
}

fn named(lines: &[String], name: &str) -> Option<usize> {
    let made = |l: &String| {
        let rest = ["const ", "let ", "var "].iter().find_map(|k| l.strip_prefix(k));
        rest.and_then(|r| r.strip_prefix(name))
            .is_some_and(|r| r.trim_start().starts_with('='))
    };
    lines.iter().position(|l| !name.is_empty() && made(l))
}

fn owner(lines: &[String], row: usize) -> Option<String> {
    let head = lines.get(..=row)?.iter().rev().find(|l| top(l) && !l.trim().is_empty())?;
    let rest = ["const ", "let ", "var "].iter().find_map(|k| head.strip_prefix(k))?;
    Some(rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect())
}

fn pairs(lines: &[String]) -> Vec<(f64, f64)> {
    let end = lines.iter().position(|l| l.starts_with(']')).unwrap_or(0);
    let text = lines[..=end.min(lines.len() - 1)].join(" ");
    let pair = |s: &str| {
        let (x, y) = s.split(']').next()?.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    };
    text.split('[').filter_map(pair).collect()
}

pub fn trail(points: &[(f64, f64)], sheet: Bounds<Pixels>, window: &mut Window, closed: bool) {
    let to = |&(x, y): &(f64, f64)| sheet.origin + point(sheet.size.width * x as f32, sheet.size.height * y as f32);
    let spots: Vec<Point<Pixels>> = points.iter().map(to).collect();
    let mut line = PathBuilder::stroke(px(1.5));
    line.add_polygon(&spots, closed);
    if let Some(line) = line.build().ok().filter(|_| spots.len() > 1) {
        window.paint_path(line, rgb(BLUE));
    }
    for (i, at) in spots.iter().enumerate() {
        let r = if i == 0 { 5. } else { 3. };
        let b = Bounds::new(*at - point(px(r), px(r)), size(px(2. * r), px(2. * r)));
        window.paint_quad(quad(
            b,
            px(if i == 0 { r } else { 0. }),
            rgb(TEXT),
            px(1.),
            rgb(INK),
            Default::default(),
        ));
    }
}

impl Origami {
    pub fn quill(&mut self) {
        (self.pen, self.trace) = (false, self.trace.take().xor(Some(vec![])));
        if self.trace.is_some() {
            self.note = "click around the shape \u{b7} click the first point to close \u{b7} esc cancels".into();
        }
    }

    pub fn dot(&mut self, at: Point<Pixels>) {
        let sheet = self.sheet();
        let Some(points) = self.trace.as_mut().filter(|_| sheet.contains(&at)) else {
            return self.note = "click inside the picture to draw the shape".into();
        };
        let first = points
            .first()
            .map(|&(x, y)| sheet.origin + point(sheet.size.width * x as f32, sheet.size.height * y as f32));
        let near = |p: Point<Pixels>| f32::from(p.x - at.x).hypot(f32::from(p.y - at.y)) < 10.;
        if points.len() < 3 || !first.is_some_and(near) {
            return points.push(share(sheet, at));
        }
        let (points, mut row, was) = (self.trace.take().unwrap_or_default(), 0, self.lines.len());
        self.change(|this| row = drawn(&mut this.lines, &points));
        self.reveal(row);
        self.flash(row, self.lines.len() - was - 1);
        self.note = format!(
            "{} is a shape \u{b7} add it to the stack, or mask with clip.matte(...)",
            &self.lines[row][6..].split(' ').next().unwrap_or_default()
        );
    }

    pub fn refer(&mut self) {
        let (word, at) = (self.picked(), self.cursor.map_or(0, |c| c.0));
        let name = match named(&self.lines, &word) {
            Some(_) => word,
            None => owner(&self.lines, at).unwrap_or_default(),
        };
        let Some(row) = named(&self.lines, &name) else {
            return;
        };
        let placed = self.tl.clips.iter().filter(|c| c.label == name);
        let first = placed.min_by(|a, b| a.start.total_cmp(&b.start)).map(|c| (c.z, c.start));
        let shape = Some(pairs(&self.lines[row..])).filter(|p| p.len() > 2 && self.lines[row].contains("path("));
        if first.is_none() && shape.is_none() {
            return;
        }
        if let Some((z, start)) = first {
            if self.sel != Some(z) {
                self.pick(z);
            }
            self.seek(start);
        }
        self.ghost = shape;
        if self.cursor.is_some_and(|c| c.0 != row) {
            self.reveal(row);
            self.flash(row, 1);
        }
    }

    pub fn tracer(&self, cx: &mut Context<Self>) -> Div {
        self.tool("\u{270e}", cx, |t, _, _| t.quill())
            .text_size(px(15.))
            .when(self.trace.is_some(), |d| d.bg(rgba(BLUE << 8 | 0x40)).text_color(rgb(TEXT)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext};
    use std::{env, fs, sync::Arc};

    // The pencil button after the T button arms the shape tool. While it is armed, clicks in the picture
    // drop points and clicks beside it do nothing, escape throws the points away, and clicking the first
    // point after three or more writes the shape into the script as one undo step, disarms the tool and
    // lights the new lines for two seconds. Double-clicking the shape's name outlines it on the picture
    // until escape, and once the shape is in the timeline also moves the playhead to where it first
    // appears, as does a double or triple click anywhere else in its definition. A name that is neither
    // a shape nor in the timeline does nothing.
    #[gpui::test]
    fn draws(cx: &mut TestAppContext) {
        let path = env::temp_dir().join("origami-quill-test.js");
        let src = "stack(\n  color(\"#203040\").for(4),\n)";
        fs::write(&path, src).unwrap();
        let tl = origami_lang::eval(&path, false, origami_engine::footprint).unwrap();
        let lines: Vec<String> = src.lines().map(String::from).collect();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (path.clone(), lines.clone(), Arc::new(tl));
            window.focus(&app.focus);
            app
        });
        cx.run_until_parked();
        let click = |cx: &mut VisualTestContext, x: f32, y: f32| cx.simulate_click(point(px(x), px(y)), Modifiers::default());
        let (mid, bar) = view.update_in(cx, |o, window, _| {
            let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
            ((w - o.side) / 2., h - 28. - o.dock - 17.)
        });
        let mut x = mid + 100.;
        while view.update(cx, |o, _| o.trace.is_none()) {
            x += 4.;
            assert!(x < 2. * mid, "no pencil button");
            click(cx, x, bar);
            view.update(cx, |o, _| o.pen = false);
        }
        let spot = |cx: &mut VisualTestContext, x: f32, y: f32| {
            let sheet = view.update(cx, |o, _| o.sheet());
            let at = sheet.origin + point(sheet.size.width * x, sheet.size.height * y);
            click(cx, f32::from(at.x), f32::from(at.y));
            view.update(cx, |o, _| o.trace.as_ref().map(Vec::len))
        };
        assert_eq!(
            (spot(cx, 0.5, -0.05), spot(cx, 0.2, 0.2), spot(cx, 0.8, 0.2)),
            (Some(0), Some(1), Some(2))
        );
        assert_eq!(spot(cx, 0.2, 0.2), Some(3));
        cx.simulate_keystrokes("escape");
        assert_eq!(view.update(cx, |o, _| (o.trace.clone(), o.lines.clone())), (None, lines.clone()));
        view.update(cx, |o, _| o.quill());
        assert_eq!(
            (spot(cx, 0.25, 0.25), spot(cx, 0.75, 0.25), spot(cx, 0.5, 0.75)),
            (Some(1), Some(2), Some(3))
        );
        assert_eq!(spot(cx, 0.25, 0.25), None);
        let want = "const shape = path([\n  [0.25, 0.25], [0.75, 0.25], [0.5, 0.75],\n], { curve: true })\n\nstack(\n  color(\"#203040\").for(4),\n)";
        assert_eq!(view.update(cx, |o, _| (o.lines.join("\n"), o.past.len())), (want.to_string(), 1));
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("{want}\n"));
        assert_eq!(view.update(cx, |o, _| o.lit().map(|l| (l.1, l.2))), Some((0, 3)));
        view.update(cx, |o, _| o.glow.as_mut().unwrap().0 -= std::time::Duration::from_secs(3));
        assert_eq!(view.update(cx, |o, _| o.lit()), None);
        let refer = |cx: &mut VisualTestContext| {
            view.update(cx, |o, _| {
                (o.anchor, o.cursor, o.t) = (Some((0, 6)), Some((0, 11)), 0.);
                o.refer();
                (o.ghost.clone(), o.t)
            })
        };
        assert_eq!(refer(cx), (Some(vec![(0.25, 0.25), (0.75, 0.25), (0.5, 0.75)]), 0.));
        view.update(cx, |o, _| {
            o.lines.push("const wide = 1920".into());
            (o.anchor, o.cursor, o.ghost) = (Some((7, 6)), Some((7, 10)), None);
            o.refer();
            assert_eq!((o.ghost.clone(), o.lit()), (None, None));
            o.lines.pop();
        });
        view.update(cx, |o, _| {
            o.lines.insert(6, "  shape.at(1).for(2),".into());
            fs::write(&o.script, o.lines.join("\n")).unwrap();
            o.tl = Arc::new(origami_lang::eval(&o.script, false, origami_engine::footprint).unwrap());
        });
        assert_eq!(refer(cx), (Some(vec![(0.25, 0.25), (0.75, 0.25), (0.5, 0.75)]), 1.));
        for (row, from, to) in [(0, 0, 5), (0, 14, 18), (1, 0, 40), (3, 0, 19)] {
            let seen = view.update(cx, |o, _| {
                (o.anchor, o.cursor, o.ghost, o.t) = (Some((row, from)), Some((row, to)), None, 0.);
                o.refer();
                (o.ghost.as_ref().map(Vec::len), o.t)
            });
            assert_eq!(seen, (Some(3), 1.), "{row} {from}");
        }
        view.update(cx, |o, _| {
            o.lines.remove(6);
        });
        cx.simulate_keystrokes("escape");
        assert_eq!(view.update(cx, |o, _| o.ghost.clone()), None);
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), lines);
    }

    // A shape drawn with the pen is written as a named path above the script's last statement, three
    // points to a line and rounded to three decimals, and a second shape gets its own name.
    #[test]
    fn lands_above_the_last_statement() {
        let mut lines: Vec<String> = "const a = color(\"#000\")\n\nstack(\n  a,\n)".lines().map(String::from).collect();
        let points = [(0.1, 0.2), (0.5, 0.12345), (0.9, 0.2), (0.5, 1.2)];
        assert_eq!(drawn(&mut lines, &points), 2);
        assert_eq!(
            lines.join("\n"),
            "const a = color(\"#000\")\n\nconst shape = path([\n  [0.1, 0.2], [0.5, 0.123], [0.9, 0.2],\n  [0.5, 1],\n], { curve: true })\n\nstack(\n  a,\n)"
        );
        assert_eq!(drawn(&mut lines, &points[..3]), 7);
        assert_eq!(lines[7], "const shape2 = path([");
        let mut empty = vec![];
        assert_eq!((drawn(&mut empty, &points[..3]), empty[0].as_str()), (0, "const shape = path(["));
    }
}
