use crate::{
    Asset, Origami, bitmap,
    keys::stamp,
    view::{BG, BLUE, BORDER, DIM, GREEN, PANEL, TEXT},
};
use gpui::{
    App, Bounds, Context, Div, MouseButton, MouseDownEvent, MouseMoveEvent, ObjectFit, PathBuilder, Pixels, RenderImage, StyledImage,
    Window, canvas, div, img, point, prelude::*, px, rgb, rgba,
};
use origami_engine::{Player, Preview, footprint};
use origami_lang::{Clip, Kind, Timeline};
use std::{sync::Arc, time::Instant};

const WIDE: f32 = 278.;

pub struct Peek {
    row: usize,
    tl: Arc<Timeline>,
    t: f64,
    run: Option<(Instant, f64, Option<Player>)>,
    image: Option<Arc<RenderImage>>,
    shown: Option<i64>,
    rough: bool,
    busy: bool,
}

impl Peek {
    fn new(row: usize, tl: Timeline) -> Self {
        Self {
            row,
            tl: Arc::new(tl),
            t: 0.,
            run: None,
            image: None,
            shown: None,
            rough: false,
            busy: false,
        }
    }
}

impl Origami {
    pub fn glance(&mut self, row: usize, cx: &mut Context<Self>) {
        let linked = |c: &&Clip| {
            let last = (c.line as usize).saturating_sub(1);
            let chain = self.lines.get(row + 1..=last);
            matches!(c.kind, Kind::Video | Kind::Sound)
                && last >= row
                && chain.is_some_and(|l| l.iter().all(|l| l.trim_start().starts_with('.')))
        };
        if let Some(c) = self.tl.clips.iter().find(linked) {
            let clip = Clip {
                start: 0.,
                end: c.end - c.start,
                z: 0,
                ..c.clone()
            };
            let tl = Timeline {
                fps: self.tl.fps,
                size: self.tl.size,
                duration: clip.end,
                preview: self.tl.preview.clone(),
                proxy: self.tl.proxy,
                clips: vec![clip],
                files: vec![],
            };
            return self.peek = Some(Peek::new(row, tl));
        }
        let line = self.lines[row].trim_start();
        let named = ["const ", "let ", "var "].iter().find_map(|k| line.strip_prefix(k));
        let Some(name) = named
            .filter(|_| line.contains("video(") || line.contains("sound("))
            .and_then(|l| l.split('=').next())
        else {
            return;
        };
        let (script, src) = (self.script.clone(), format!("{}\n{}\n", self.lines[..=row].join("\n"), name.trim()));
        cx.spawn(async move |this, cx| {
            let tl = cx
                .background_executor()
                .spawn(async move { origami_lang::peek(&script, &src, footprint) })
                .await;
            this.update(cx, |this, cx| {
                this.peek = tl.ok().filter(|tl| tl.duration > 0.).map(|tl| Peek::new(row, tl));
                cx.notify();
            })
        })
        .detach();
    }

    pub fn blink(&mut self, cx: &mut Context<Self>) {
        if let Some(image) = self.peek.take().and_then(|p| p.image) {
            cx.drop_image(image, None);
        }
    }

    pub fn gaze(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.peek.as_mut() else { return };
        if let Some((since, t0, player)) = &p.run {
            let t = player.as_ref().map_or(t0 + since.elapsed().as_secs_f64(), Player::time);
            match t < p.tl.duration && !player.as_ref().is_some_and(Player::done) {
                true => p.t = t,
                false => (p.run, p.t) = (None, 0.),
            }
            cx.notify();
        }
        let fps = p.tl.fps.max(1) as f64;
        let want = (p.t * fps).round() as i64;
        if p.busy || p.shown == Some(want) && !(p.rough && !self.eyeing) || !p.tl.clips.iter().any(|c| c.video) {
            return;
        }
        p.busy = true;
        let (scout, tl, fast) = (self.scout.clone(), p.tl.clone(), self.eyeing && p.tl.proxy);
        let (w, h) = Preview::size(&tl, (WIDE * self.dense) as f64, false);
        cx.spawn(async move |this, cx| {
            let job = tl.clone();
            let draw = async move {
                let mut scout = scout.lock().unwrap();
                scout.fast = fast;
                (scout.frame(&job, want as f64 / fps, (w, h)), scout.rough)
            };
            let (px, rough) = cx.background_executor().spawn(draw).await;
            this.update(cx, |this, cx| {
                let image = bitmap(px, w, h);
                let old = match this.peek.as_mut().filter(|p| Arc::ptr_eq(&p.tl, &tl)) {
                    Some(p) => {
                        (p.busy, p.shown, p.rough) = (false, Some(want), rough);
                        p.image.replace(image)
                    }
                    None => Some(image),
                };
                if let Some(old) = old {
                    cx.drop_image(old, None);
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn eye(&mut self) {
        let Some(p) = self.peek.as_mut() else { return };
        p.run = match p.run.take() {
            Some(_) => None,
            None => {
                self.player = None;
                Some((Instant::now(), p.t, Player::new(&p.tl, p.t, 1.).ok()))
            }
        };
    }

    fn perch(&self, window: &Window) -> Option<(f32, f32)> {
        let p = self.peek.as_ref()?;
        let (hidden, top) = (self.hidden(), self.top as usize);
        if p.row < top || hidden.get(p.row) != Some(&false) {
            return None;
        }
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let y = self.head() + (top..p.row).filter(|i| !hidden[*i]).count() as f32 * self.row();
        let tall = if p.tl.clips.iter().any(|c| c.video) { 230. } else { 130. };
        Some((
            (w - self.side + self.gutter()).min(w - WIDE - 26.),
            if y + self.row() + tall > h - 28. {
                y - tall
            } else {
                y + self.row()
            },
        ))
    }

    pub fn scan(&mut self, x: Pixels, window: &Window) {
        let Some((left, _)) = self.perch(window) else { return };
        if let Some(p) = self.peek.as_mut() {
            let end = (p.tl.duration - 1. / p.tl.fps.max(1) as f64).max(0.);
            (p.run, p.t) = (None, (((f32::from(x) - left - 11.) / WIDE) as f64 * p.tl.duration).clamp(0., end));
        }
    }

    pub fn peep(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let (left, top) = self.perch(window)?;
        let c = self.peek.as_ref()?.tl.clips.first()?.clone();
        let bars: Vec<f32> = match self.asset(c.src.clone(), &c.src, None).filter(|_| !c.video) {
            Some(Asset::Wave(w)) => {
                let (a, b) = ((c.src_in * 20.) as usize, ((c.src_in + c.end * c.speed) * 20.) as usize);
                let part = &w[a.min(w.len())..b.clamp(a, w.len())];
                part.chunks((part.len() / 139).max(1))
                    .map(|c| c.iter().fold(0., |m: f32, &p| m.max(p)))
                    .collect()
            }
            _ => vec![],
        };
        let p = self.peek.as_ref()?;
        let done = (p.t / p.tl.duration.max(0.001)) as f32;
        let wave = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            let w = b.size.width / bars.len().max(1) as f32;
            for (i, bar) in bars.iter().enumerate() {
                let (x, tall) = (b.origin.x + w * i as f32, b.size.height * bar.max(0.04));
                let y = b.origin.y + (b.size.height - tall) / 2.;
                let mut shape = PathBuilder::fill();
                shape.add_polygon(
                    &[point(x, y), point(x + w * 0.6, y), point(x + w * 0.6, y + tall), point(x, y + tall)],
                    true,
                );
                if let Ok(shape) = shape.build() {
                    let ink = if (i as f32) < done * bars.len() as f32 { BLUE } else { GREEN };
                    window.paint_path(shape, rgba(ink << 8 | 0xc0));
                }
            }
        };
        let media = match (c.video, &p.image) {
            (true, Some(i)) => div().h(px(156.)).child(img(i.clone()).size_full().object_fit(ObjectFit::Cover)),
            (true, None) => div().h(px(156.)).bg(rgb(BG)),
            _ => div().h(px(56.)).child(canvas(|_, _, _| (), wave).size_full()),
        };
        let play = div()
            .size(px(24.))
            .flex_none()
            .rounded_full()
            .bg(rgb(BORDER))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(10.))
            .cursor_pointer()
            .child(if p.run.is_some() { "||" } else { "\u{25b6}" })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.eye();
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        let bar = div()
            .h(px(14.))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.eyeing = true;
                    this.scan(e.position.x, window);
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
                    .child(div().h_full().w(px(done * WIDE)).rounded(px(2.)).bg(rgb(BLUE)))
                    .child(
                        div()
                            .absolute()
                            .top(px(-4.))
                            .left(px(done * WIDE - 6.))
                            .size(px(12.))
                            .rounded_full()
                            .bg(rgb(BLUE)),
                    ),
            );
        let about = match c.video {
            true => format!("{}\u{d7}{}{}", c.w, c.h, if c.audio { " \u{b7} sound" } else { "" }),
            false => c.src.rsplit('/').next().unwrap_or_default().to_string(),
        };
        Some(
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(WIDE + 22.))
                .p(px(10.))
                .flex()
                .flex_col()
                .gap_2()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(6.))
                .text_size(px(12.))
                .text_color(rgb(TEXT))
                .occlude()
                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                    if this.eyeing {
                        this.scan(e.position.x, window);
                        cx.notify();
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.release();
                        cx.notify();
                    }),
                )
                .child(media.rounded(px(3.)).overflow_hidden())
                .child(bar)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(play)
                        .child(format!("{} / {}", stamp(p.t), stamp(p.tl.duration)))
                        .child(div().flex_1())
                        .child(div().text_color(rgb(DIM)).whitespace_nowrap().child(about)),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, point};

    // Dragging along the bar of the hover preview should move its playhead, keep following the drag
    // even when the pointer leaves the bar, and stop once the button is released.
    #[gpui::test]
    fn scrub(cx: &mut gpui::TestAppContext) {
        let path = std::env::temp_dir().join("origami-peek-test.js");
        std::fs::write(&path, "stack(color(\"#203040\").for(4))\n").unwrap();
        let tl = origami_lang::eval(&path, false, footprint).unwrap();
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines) = (path.clone(), vec!["stack(color(\"#203040\").for(4))".into()]);
            app.peek = Some(Peek::new(0, tl));
            app
        });
        let (left, top) = view.update_in(cx, |o, window, _| o.perch(window)).unwrap();
        cx.run_until_parked();
        let bar = |f: f32| point(px(left + 11. + f * WIDE), px(top + 11. + 156. + 8. + 7.));
        let at = |cx: &mut gpui::VisualTestContext| view.update(cx, |o, _| (o.peek.as_ref().unwrap().t, o.eyeing));
        cx.simulate_mouse_move(bar(0.5), None, Modifiers::default());
        cx.simulate_mouse_down(bar(0.5), MouseButton::Left, Modifiers::default());
        assert_eq!(at(cx).1, true);
        cx.simulate_mouse_move(bar(0.25), Some(MouseButton::Left), Modifiers::default());
        assert!((at(cx).0 - 1.).abs() < 0.05, "{:?}", at(cx));
        cx.simulate_mouse_up(bar(0.25), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(bar(0.9), None, Modifiers::default());
        cx.simulate_mouse_move(point(px(left - 300.), px(top + 300.)), None, Modifiers::default());
        assert!((at(cx).0 - 1.).abs() < 0.05 && !at(cx).1, "{:?}", at(cx));
        cx.simulate_mouse_down(bar(0.5), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(
            point(px(left - 300.), px(top + 300.)),
            Some(MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(point(px(left - 300.), px(top + 300.)), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(bar(0.9), None, Modifiers::default());
        assert!(!at(cx).1 && at(cx).0 < 0.05, "{:?}", at(cx));
    }
}
