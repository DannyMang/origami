use crate::{
    Origami,
    keys::stamp,
    view::{BLUE, BORDER, MUTED, PANEL},
};
use gpui::{Context, Div, MouseButton, MouseDownEvent, Pixels, Window, div, prelude::*, px, rgb, rgba};
use std::time::{Duration, Instant};

const WIDE: f32 = 480.;
const RAIL: f32 = 248.;

pub fn corners(out: bool) -> Div {
    let piece = |top: bool, left: bool| {
        let d = div().absolute().size(px(5.)).border_color(rgb(MUTED));
        let d = if top { d.top_0() } else { d.bottom_0() };
        let d = if left { d.left_0() } else { d.right_0() };
        let d = if top == out { d.border_t_1() } else { d.border_b_1() };
        if left == out { d.border_l_1() } else { d.border_r_1() }
    };
    div()
        .relative()
        .size(px(13.))
        .child(piece(true, true))
        .child(piece(true, false))
        .child(piece(false, true))
        .child(piece(false, false))
}

impl Origami {
    pub fn fullscreen(&mut self) {
        (self.full, self.stir) = (!self.full, Instant::now());
    }

    fn rail(window: &Window) -> f32 {
        (f32::from(window.viewport_size().width) - WIDE) / 2. + 116.
    }

    pub fn glide(&mut self, x: Pixels, window: &Window) {
        let part = ((f32::from(x) - Self::rail(window)) / RAIL).clamp(0., 1.);
        self.seek(part as f64 * self.tl.duration);
    }

    pub fn bar(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        if !self.scrubbing && self.stir.elapsed() > Duration::from_secs(2) {
            return None;
        }
        let done = (self.t / self.tl.duration.max(1e-3)) as f32;
        let button = |d: Div| d.w(px(24.)).px_0().justify_center().flex_none();
        let time = |t: f64| div().w(px(64.)).flex_none().flex().justify_center().child(stamp(t));
        let rail = div()
            .w(px(RAIL))
            .h(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.scrubbing = true;
                    this.glide(e.position.x, window);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(4.))
                    .rounded(px(2.))
                    .bg(rgb(BORDER))
                    .child(div().h_full().w(px(done * RAIL)).rounded(px(2.)).bg(rgb(BLUE)))
                    .child(
                        div()
                            .absolute()
                            .top(px(-4.))
                            .left(px(done * RAIL - 6.))
                            .size(px(12.))
                            .rounded_full()
                            .bg(rgb(BLUE)),
                    ),
            );
        let play = if self.player.is_some() { "||" } else { "\u{25b6}" };
        let bar = div()
            .w(px(WIDE))
            .h(px(40.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(10.))
            .bg(rgba(PANEL << 8 | 0xe6))
            .text_size(px(12.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(button(self.tool(play, cx, |t, _, _| t.toggle())))
            .child(time(self.t))
            .child(rail)
            .child(time(self.tl.duration))
            .child(button(self.tool(corners(false), cx, |t, _, _| t.fullscreen())));
        let left = (f32::from(window.viewport_size().width) - WIDE) / 2.;
        Some(div().absolute().bottom(px(28.)).left(px(left)).child(bar))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, point};
    use origami_engine::footprint;
    use std::sync::Arc;

    // Fullscreen preview should open and close from its button, the F key and Escape. Its bar should
    // scrub the playhead, stop when the button is let go even outside the window, and hide after a
    // few seconds without mouse movement.
    #[gpui::test]
    fn scrub(cx: &mut gpui::TestAppContext) {
        let path = std::env::temp_dir().join("origami-full-test.js");
        std::fs::write(&path, "stack(color(\"#203040\").for(4))\n").unwrap();
        let tl = origami_lang::eval(&path, false, footprint).unwrap();
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (path.clone(), vec!["stack(color(\"#203040\").for(4))".into()], Arc::new(tl));
            app
        });
        let (w, h, side) = view.update_in(cx, |o, window, _| {
            window.focus(&o.focus);
            (
                f32::from(window.viewport_size().width),
                f32::from(window.viewport_size().height),
                o.side,
            )
        });
        cx.run_until_parked();
        let at = |cx: &mut gpui::VisualTestContext| view.update(cx, |o, _| (o.full, o.scrubbing, o.t));
        let click = |cx: &mut gpui::VisualTestContext, x: f32, y: f32| {
            cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::default());
            cx.simulate_mouse_down(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
        };
        let rail = |f: f32| point(px((w - WIDE) / 2. + 116. + f * RAIL), px(h - 48.));
        let far = point(px(20.), px(30.));
        click(cx, w - side - 26., 51.);
        assert_eq!(at(cx), (true, false, 0.));
        cx.simulate_mouse_move(rail(0.5), None, Modifiers::default());
        cx.simulate_mouse_down(rail(0.5), MouseButton::Left, Modifiers::default());
        assert_eq!(at(cx), (true, true, 2.));
        cx.simulate_mouse_move(rail(0.25), Some(MouseButton::Left), Modifiers::default());
        assert_eq!(at(cx), (true, true, 1.));
        cx.simulate_mouse_up(rail(0.25), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(rail(0.9), None, Modifiers::default());
        assert_eq!(at(cx), (true, false, 1.));
        cx.simulate_mouse_down(rail(0.75), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(far, Some(MouseButton::Left), Modifiers::default());
        assert_eq!(at(cx), (true, true, 0.));
        cx.simulate_mouse_up(far, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(rail(0.9), None, Modifiers::default());
        assert_eq!(at(cx), (true, false, 0.));
        cx.simulate_mouse_down(rail(0.5), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(point(px(w / 2.), px(h + 30.)), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(rail(0.9), None, Modifiers::default());
        assert_eq!(at(cx), (true, false, 2.));
        click(cx, (w + WIDE) / 2. - 24., h - 48.);
        assert!(!at(cx).0);
        cx.simulate_keystrokes("f");
        assert!(at(cx).0);
        let shown = |cx: &mut gpui::VisualTestContext| view.update_in(cx, |o, window, cx| o.bar(window, cx).is_some());
        view.update(cx, |o, _| o.stir -= Duration::from_secs(3));
        assert!(!shown(cx));
        cx.simulate_mouse_move(far, None, Modifiers::default());
        assert!(shown(cx));
        cx.simulate_keystrokes("escape");
        assert!(!at(cx).0);
    }

    // In fullscreen the script and the timeline are hidden, so keys that would edit them do nothing,
    // while playback keys still work and Escape leaves.
    #[gpui::test]
    fn keys_leave_the_hidden_script_alone(cx: &mut gpui::TestAppContext) {
        let path = std::env::temp_dir().join("origami-full-keys-test.js");
        let src = "stack(\n  color(\"#203040\").for(4),\n  text(\"a\").for(4),\n)";
        std::fs::write(&path, src).unwrap();
        let tl = origami_lang::eval(&path, false, footprint).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (path.clone(), src.lines().map(String::from).collect(), Arc::new(tl));
            app.full = true;
            app.past.push(vec![]);
            window.focus(&app.focus);
            app
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("backspace delete s cmd-x cmd-d cmd-v cmd-z x");
        let state = |cx: &mut gpui::VisualTestContext| view.update(cx, |o, _| (o.lines.join("\n"), o.past.len(), o.full, o.t));
        assert_eq!(state(cx), (src.into(), 1, true, 0.));
        cx.simulate_keystrokes("right escape");
        assert_eq!(state(cx), (src.into(), 1, false, 1. / 30.));
    }
}
