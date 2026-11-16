use crate::{
    Origami,
    stage::{rest, share},
    view::{BLUE, TEXT},
    voice::placed,
};
use gpui::{Context, Div, FontWeight, Pixels, Point, prelude::*, px, rgb, rgba};

const WORD: &str = "text";

fn typed(lines: &mut Vec<String>, t: f64, (x, y): (f64, f64)) -> (usize, usize) {
    let at = (t * 100.).round() / 100.;
    let item = format!("text(\"{WORD}\").at({at}).for(3).pos({}, {})", rest(x, 0., 0.), rest(y, 0., 0.));
    let row = placed(lines, 0, &item, &["stack("]);
    let from = lines[row].rfind(&item).unwrap_or(0);
    (row, lines[row][..from].chars().count() + 6)
}

impl Origami {
    pub fn pen(&mut self) {
        self.pen = !self.pen;
        if self.pen {
            self.note = "click the preview where the text goes \u{b7} esc cancels".into();
        }
    }

    pub fn print(&mut self, at: Point<Pixels>) {
        let sheet = self.sheet();
        if !sheet.contains(&at) {
            return self.note = "click inside the picture to place the text".into();
        }
        let (mut row, mut col) = (0, 0);
        self.change(|this| (row, col) = typed(&mut this.lines, this.t, share(sheet, at)));
        (self.pen, self.sel) = (false, Some(self.tl.clips.len() as u32));
        (self.anchor, self.cursor) = (Some((row, col)), Some((row, col + WORD.len())));
        self.reveal(row);
        self.note = "added text \u{b7} type the words, cmd+S applies them \u{b7} cmd+Z removes it".into();
    }

    pub fn typer(&self, cx: &mut Context<Self>) -> Div {
        self.tool("T", cx, |t, _, _| t.pen())
            .text_size(px(15.))
            .font_weight(FontWeight::BOLD)
            .when(self.pen, |d| d.bg(rgba(BLUE << 8 | 0x40)).text_color(rgb(TEXT)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext, point};
    use std::{env, fs, mem, sync::Arc};

    // Text placed with the type tool should be written as one new line at the playhead and at the
    // clicked spot, rounded to three decimals: inside the script's stack when it ends in one, and
    // otherwise by wrapping the last clip or cat in a stack. The words are reported by where they
    // start, counted in characters.
    #[test]
    fn lands() {
        let on = |src: &str, t: f64, at: (f64, f64)| {
            let mut lines: Vec<String> = src.lines().map(String::from).collect();
            let (row, col) = typed(&mut lines, t, at);
            let word: String = lines[row].chars().skip(col - 1).take(6).collect();
            assert_eq!(word, "\"text\"");
            lines.join("\n")
        };
        assert_eq!(
            on("stack(\n  shot,\n  title\n)", 1.504, (0.12345, 0.9996)),
            "stack(\n  shot,\n  title,\n  text(\"text\").at(1.5).for(3).pos(0.123, 1),\n)"
        );
        assert_eq!(
            on("cat(\n  a,\n  b,\n)", 0., (0.5, 0.25)),
            "stack(cat(\n  a,\n  b,\n), text(\"text\").at(0).for(3).pos(0.5, 0.25))"
        );
        assert_eq!(
            on("video(\"\u{e9}t\u{e9}.mp4\").trim(0, 5)", 2., (0., 0.5)),
            "stack(video(\"\u{e9}t\u{e9}.mp4\").trim(0, 5), text(\"text\").at(2).for(3).pos(0, 0.5))"
        );
        assert_eq!(
            on("stack(a).mute()", 2., (0.5, 0.5)),
            "stack(stack(a).mute(), text(\"text\").at(2).for(3).pos(0.5, 0.5))"
        );
        assert_eq!(on("", 2., (0.5, 0.5)), "text(\"text\").at(2).for(3).pos(0.5, 0.5)");
    }

    // The T button sits beside the speed button. Clicking it or pressing t arms the type tool, and
    // clicking it again or pressing escape disarms it. While it is armed, a click in the picture adds
    // a text clip there at the playhead, selects it and selects its placeholder word in the script so
    // typing replaces it; one undo removes the clip. A click beside the picture adds nothing. It works
    // the same in keyboard mode, and does nothing on the launch screen.
    #[gpui::test]
    fn types(cx: &mut TestAppContext) {
        let path = env::temp_dir().join("origami-pen-test.js");
        let src = "stack(\n  color(\"#203040\").for(4),\n)";
        fs::write(&path, src).unwrap();
        let tl = origami_lang::eval(&path, false, origami_engine::footprint).unwrap();
        let lines: Vec<String> = src.lines().map(String::from).collect();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl, app.t) = (path.clone(), lines.clone(), Arc::new(tl), 1.5);
            window.focus(&app.focus);
            app
        });
        cx.run_until_parked();
        let click = |cx: &mut VisualTestContext, x: f32, y: f32| {
            cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::default());
            cx.simulate_mouse_down(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
        };
        let (mid, bar) = view.update_in(cx, |o, window, _| {
            let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
            ((w - o.side) / 2., h - 28. - o.dock - 17.)
        });
        let (mut x, mut pace, mut rate) = (mid + 100., 0., 1.);
        while !view.update(cx, |o, _| o.pen) {
            x += 4.;
            assert!(x < 2. * mid, "no T button");
            click(cx, x, bar);
            if view.update(cx, |o, _| mem::replace(&mut rate, o.rate) != o.rate) {
                pace = x;
            }
        }
        assert!(pace > 0. && x - pace < 20., "{pace} {x}");
        click(cx, x, bar);
        assert!(!view.update(cx, |o, _| o.pen));
        click(cx, x, bar);
        cx.simulate_keystrokes("escape");
        assert_eq!(view.update(cx, |o, _| (o.pen, o.mouse)), (false, true));

        let spot = |cx: &mut VisualTestContext, x: f32, y: f32| {
            let sheet = view.update(cx, |o, _| o.sheet());
            let at = sheet.origin + point(sheet.size.width * x, sheet.size.height * y);
            click(cx, f32::from(at.x), f32::from(at.y));
            view.update(cx, |o, _| (o.lines.join("\n"), o.pen, o.past.len()))
        };
        let want = src.replace(",\n)", ",\n  text(\"text\").at(1.5).for(3).pos(0.25, 0.75),\n)");
        for mouse in [true, false] {
            view.update(cx, |o, _| o.mouse = mouse);
            cx.simulate_keystrokes("t");
            assert!(view.update(cx, |o, _| o.pen));
            assert_eq!(spot(cx, 0.5, -0.01), (src.to_string(), true, 0));
            assert_eq!(spot(cx, 0.25, 0.75), (want.clone(), false, 1));
            assert_eq!(fs::read_to_string(&path).unwrap(), format!("{want}\n"));
            assert_eq!(
                view.update(cx, |o, _| (o.sel, o.chosen(), o.picked())),
                (Some(1), Some(((2, 8), (2, 12))), "text".into())
            );
            cx.simulate_keystrokes("h i");
            assert_eq!(
                view.update(cx, |o, _| o.lines[2].clone()),
                "  text(\"hi\").at(1.5).for(3).pos(0.25, 0.75),"
            );
            cx.simulate_keystrokes("cmd-z cmd-z cmd-z escape");
            assert_eq!(view.update(cx, |o, _| (o.lines.clone(), o.past.len())), (lines.clone(), 0));
        }

        let (view, cx) = cx.add_window_view(|window, cx| {
            let app = crate::fresh(cx.focus_handle(), vec![]);
            window.focus(&app.focus);
            app
        });
        cx.simulate_keystrokes("t");
        assert!(!view.update(cx, |o, _| o.pen));
    }
}
