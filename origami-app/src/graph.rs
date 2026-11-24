use crate::{
    Origami,
    knob::{TRACK, rail},
    motion::{ATTRS, EASES, Form, Key, Pull, Row, bend, curve, handle, secs},
    scene::{FOOT, LEAD, lozenge, tick, track},
    view::{BAR, BLUE, DIM, GREEN, MUTED, ORANGE, PANEL, PURPLE, RED, RULER, TEXT, card},
};
use gpui::{
    App, Bounds, Context, Div, MouseButton, PathBuilder, Pixels, Window, canvas, div, fill, point, prelude::*, px, relative, rgb, rgba,
    size,
};
use origami_lang::Clip;

const CARD: f32 = 204.;
const TOP: f32 = 22.;
const BASE: f32 = 18.;
const HUES: [u32; 6] = [BLUE, GREEN, ORANGE, PURPLE, RED, MUTED];

impl Origami {
    pub fn plot(&self, c: Option<&Clip>, rows: &[Row], per: f64, cx: &mut Context<Self>) -> Div {
        let keyed = rows.iter().filter_map(|r| {
            let keys = match &r.form {
                Form::Keyed(keys) => &keys[..],
                Form::Linked(_) => &[],
                _ => return None,
            };
            Some((r.name, keys, HUES[ATTRS.iter().position(|a| a.0 == r.name)? % 6]))
        });
        let keyed: Vec<(&'static str, &[Key], u32)> = keyed.collect();
        let Some(c) = c.filter(|_| !keyed.is_empty()) else {
            let hint = "select a clip and click a diamond to key a property: its curve shows here";
            return div().p_3().text_size(px(11.)).text_color(rgb(DIM)).child(hint);
        };
        let (d, pan, fps, z, start, len) = (self.span(), self.pan, self.fps(), c.z, c.start, c.end - c.start);
        let ((lo, hi), room) = (self.reach(c, rows), (self.dock - RULER - FOOT - TOP - BASE).max(1.));
        let y = move |v: f64| TOP + (1. - (v - lo) / (hi - lo)) as f32 * room;
        let x = move |t: f64| relative(((t - pan) / d) as f32);
        let gain = [per, 0., (lo - hi) / room as f64];
        let (rise, _) = tick((hi - lo) / room as f64, 40.);
        let levels = ((lo / rise).ceil() as i64..=(hi / rise).floor() as i64).map(|i| i as f64 * rise);
        let levels: Vec<f64> = levels.collect();
        let spot = |k: &Key| (secs(&k.time, len), k.value);
        let held = self.key.and_then(|k| Some((keyed.iter().find(|r| r.0 == k.1)?.1, k.1, k.2)));
        let arms = held.into_iter().flat_map(|(keys, name, i)| {
            [false, true].into_iter().filter_map(move |lead| {
                let j = i + lead as usize;
                let (a, b) = (spot(keys.get(j.checked_sub(1)?)?), spot(keys.get(j)?));
                Some((name, i, lead, spot(keys.get(i)?), handle(a, b, bend(&keys[j].ease)?, lead)))
            })
        });
        let arms: Vec<_> = arms.collect();
        let dots = arms.iter().map(|&(name, i, lead, _, (t, v))| {
            div()
                .absolute()
                .left(x(start + t))
                .top(px(y(v) - 4.5))
                .ml(px(-4.5))
                .size(px(9.))
                .rounded_full()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(TEXT))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    Self::tap(cx, move |this, e, _| {
                        this.key = Some((z, name, i));
                        this.seize(e.position, gain, Pull::Bend(lead, t, v));
                    }),
                )
        });
        let dots: Vec<Div> = dots.collect();
        let hits = keyed.iter().flat_map(|&(name, keys, _)| {
            let cx = &*cx;
            keys.iter().map(spot).enumerate().map(move |(i, (t, v))| {
                div()
                    .absolute()
                    .left(x(start + t))
                    .top(px(y(v) - 7.))
                    .ml(px(-7.))
                    .size(px(14.))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        Self::tap(cx, move |this, e, _| {
                            this.seek(start + t);
                            this.key = Some((z, name, i));
                            this.seize(e.position, gain, Pull::Key);
                        }),
                    )
            })
        });
        let hits: Vec<Div> = hits.collect();
        let curves = keyed.iter().map(|&(name, keys, ink)| {
            let picked = self.key.filter(|k| k.1 == name).map(|k| k.2);
            (curve(c, name), keys.iter().map(spot).collect::<Vec<_>>(), ink, picked)
        });
        let curves: Vec<_> = curves.collect();
        let (step, _) = tick(per, 64.);
        let (rules, links) = (levels.clone(), arms.iter().map(|a| (a.3, a.4)).collect::<Vec<_>>());
        let draw = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            let at = |t: f64, v: f64| point(b.origin.x + b.size.width * ((start + t - pan) / d) as f32, b.origin.y + px(y(v)));
            for i in (pan / step).ceil() as i64..=((pan + d) / step) as i64 {
                let x = at(i as f64 * step - start, 0.).x.round();
                window.paint_quad(fill(Bounds::new(point(x, b.origin.y), size(px(1.), b.size.height)), rgb(0x343a44)));
            }
            for &v in &rules {
                let line = Bounds::new(
                    point(b.origin.x - px(12.), at(0., v).y.round()),
                    size(b.size.width + px(24.), px(1.)),
                );
                window.paint_quad(fill(line, rgb(BAR)));
            }
            for (samples, keys, ink, picked) in &curves {
                let mut line = PathBuilder::stroke(px(1.5));
                line.move_to(at(0., samples.first().map_or(0., |v| *v as f64)));
                for (i, v) in samples.iter().enumerate() {
                    line.line_to(at(i as f64 / fps, *v as f64));
                }
                line.line_to(at(len, samples.last().map_or(0., |v| *v as f64)));
                if let Ok(line) = line.build() {
                    window.paint_path(line, rgb(*ink));
                }
                for (i, &(t, v)) in keys.iter().enumerate() {
                    let on = *picked == Some(i);
                    lozenge(window, at(t, v), if on { 6.5 } else { 5. }, *ink, true);
                    if on {
                        lozenge(window, at(t, v), 4.5, TEXT, true);
                    }
                }
            }
            for &(key, arm) in &links {
                let mut line = PathBuilder::stroke(px(1.));
                line.move_to(at(key.0, key.1));
                line.line_to(at(arm.0, arm.1));
                if let Ok(line) = line.build() {
                    window.paint_path(line, rgb(TEXT));
                }
            }
        };
        let places = if rise < 1. { 2 } else { 0 };
        let labels = levels.iter().map(|&v| {
            div()
                .absolute()
                .left(px(5.))
                .top(px(y(v) - 14.))
                .text_size(px(10.))
                .text_color(rgb(DIM))
                .child(format!("{v:.places$}"))
        });
        let labels: Vec<Div> = labels.collect();
        let legend = keyed.iter().map(|&(name, _, ink)| {
            let label = ATTRS.iter().find(|a| a.0 == name).map_or(name, |a| a.1);
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().w(px(14.)).h(px(2.)).bg(rgb(ink)))
                .child(label)
        });
        div()
            .size_full()
            .child(
                track()
                    .child(canvas(|_, _, _| (), draw).absolute().size_full())
                    .children(hits)
                    .children(dots),
            )
            .children(labels)
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(52.))
                    .flex()
                    .gap_4()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .whitespace_nowrap()
                    .children(legend),
            )
    }

    pub fn keyed(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let (c, (z, name, i)) = (self.opened().filter(|_| !self.full && self.motion)?, self.key?);
        let rows = self.rows(c);
        let row = rows.iter().find(|r| r.name == name)?;
        let Form::Keyed(keys) = &row.form else { return None };
        let (k, len, (lo, hi)) = (keys.get(i)?, c.end - c.start, self.swing(c, row));
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let at = secs(&k.time, len);
        let x = LEAD + ((c.start + at - self.pan) / self.per_px(window)) as f32 - 24.;
        let y = h - self.dock - 66. - CARD;
        let slider = |label: &str, part: f64, text: String, gain: [f64; 3]| {
            rail(label.into(), part.clamp(0., 1.) as f32, text).on_mouse_down(
                MouseButton::Left,
                Self::tap(cx, move |this, e, _| {
                    this.key = Some((z, name, i));
                    this.seize(e.position, gain, Pull::Key);
                }),
            )
        };
        let eases = EASES.iter().map(|&e| {
            let on = match e {
                "linear" => k.ease.is_empty(),
                "bezier" => k.ease.starts_with("bezier("),
                e => k.ease.trim_matches(['"', '\'']) == e,
            };
            div()
                .w(relative(0.5))
                .h(px(22.))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(3.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(BAR)))
                .when(on, |d| d.bg(rgba(BLUE << 8 | 0x40)))
                .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.ease(e)))
                .child(e)
        });
        let eases: Vec<Div> = eases.collect();
        Some(
            card()
                .absolute()
                .left(px(x.min(w - self.side - 344.).max(8.)))
                .top(px(y))
                .h(px(CARD))
                .p(px(10.))
                .flex()
                .flex_col()
                .text_size(px(12.))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .h(px(18.))
                        .text_color(rgb(DIM))
                        .child(format!("{name}  key {} of {}", i + 1, keys.len())),
                )
                .child(slider(
                    "time",
                    at / len,
                    k.time.trim_matches(['"', '\'']).into(),
                    [len / TRACK as f64, 0., 0.],
                ))
                .child(slider(
                    "value",
                    (k.value - lo) / (hi - lo),
                    k.value.to_string(),
                    [0., (hi - lo) / TRACK as f64, 0.],
                ))
                .child(div().pt_1().w(px(314.)).flex().flex_wrap().children(eases)),
        )
    }
}
