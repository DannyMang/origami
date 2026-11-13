use crate::{
    Origami,
    complete::faces,
    motion::{ATTRS, Form, Pull, Row, STILL, secs},
    view::{BAR, BG, BLUE, BORDER, DIM, MUTED, ORANGE, PANEL, RULER, TEXT, tint},
};
use gpui::{
    App, Bounds, Context, Div, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, Window, canvas, div, fill, point, prelude::*, px,
    relative, rgb, rgba, size,
};
use origami_engine::tone;
use origami_lang::{Clip, Kind};
use std::path::Path;

pub const LEAD: f32 = 312.;
pub const FOOT: f32 = 22.;
pub const LINE: u32 = 0x363c46;
const NAMES: f32 = LEAD - 12.;
const LAYER: f32 = 20.;
const ATTR: f32 = 19.;
const ZOOM: f32 = 72.;
const FACES: usize = 10;

pub fn tick(per: f64, gap: f64) -> (f64, i64) {
    let (want, unit) = (per * gap, 10f64.powf((per * gap).log10().floor()));
    let steps = [(1., 4), (2., 4), (5., 5), (10., 4)].map(|(m, parts)| (m * unit, parts));
    steps.into_iter().find(|s| s.0 >= want).unwrap_or(steps[3])
}

pub fn sink(lift: f32, tall: f32, room: f32) -> f32 {
    lift.min(tall - room).max(0.)
}

pub fn track() -> Div {
    div().absolute().top_0().bottom_0().left(px(12.)).right(px(12.))
}

pub fn lozenge(window: &mut Window, p: Point<Pixels>, r: f32, ink: u32, solid: bool) {
    let (r, mut path) = (px(r), if solid { PathBuilder::fill() } else { PathBuilder::stroke(px(1.2)) });
    path.add_polygon(
        &[point(p.x, p.y - r), point(p.x + r, p.y), point(p.x, p.y + r), point(p.x - r, p.y)],
        true,
    );
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(ink));
    }
}

fn gem(solid: bool, ink: u32) -> Div {
    let draw = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| lozenge(window, b.center(), 4.5, ink, solid);
    div().size(px(12.)).flex_none().child(canvas(|_, _, _| (), draw).size_full())
}

fn slider(part: f32, wide: f32, ink: u32) -> Div {
    let knob = div()
        .absolute()
        .top(px(2.))
        .left(px(part.clamp(0., 1.) * (wide - 8.)))
        .size(px(8.))
        .rounded_full()
        .bg(rgb(TEXT));
    let line = div()
        .absolute()
        .top(px(5.))
        .left_0()
        .right_0()
        .h(px(2.))
        .rounded(px(1.))
        .bg(rgb(ink));
    div().relative().w(px(wide)).h(px(12.)).flex_none().child(line).child(knob)
}

fn field(text: String) -> Div {
    div()
        .h(px(15.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(3.))
        .bg(rgb(BG))
        .border_1()
        .border_color(rgb(BORDER))
        .child(text)
}

fn stem(path: &str) -> String {
    Path::new(path).file_stem().unwrap_or_default().to_string_lossy().to_string()
}

fn count(kind: Kind) -> usize {
    match kind {
        Kind::Sound => 0,
        Kind::Text => 10,
        Kind::Rect => 9,
        Kind::Line => 7,
        _ => 5,
    }
}

impl Origami {
    pub fn tap(
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &MouseDownEvent, &mut Window) + 'static,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, e, window, cx| {
            if this.mouse {
                f(this, e, window);
                cx.stop_propagation();
                cx.notify();
            }
        })
    }

    pub fn tall(&self) -> f32 {
        self.tl.clips.len() as f32 * LAYER + self.opened().map_or(0, |c| count(c.kind)) as f32 * ATTR
    }

    fn layer(&self, c: &Clip, cx: &mut Context<Self>) -> Div {
        let (z, on) = (c.z, self.sel == Some(c.z));
        let (glyph, kind) = match c.kind {
            Kind::Video => ("\u{25b6}", "video"),
            Kind::Sound => ("\u{266a}", "sound"),
            Kind::Image => ("\u{25a3}", "image"),
            Kind::Text => ("T", "text"),
            Kind::Color => ("\u{25a0}", "color"),
            Kind::Rect => ("\u{25ad}", "rect"),
            Kind::Line => ("\u{2571}", "line"),
        };
        let slot = |w: f32| div().w(px(w)).flex_none();
        div()
            .h(px(LAYER))
            .flex_none()
            .flex()
            .items_center()
            .px(px(10.))
            .border_b_1()
            .border_color(rgb(LINE))
            .when(on, |d| d.bg(rgb(BAR)))
            .whitespace_nowrap()
            .overflow_hidden()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.pick(z)))
            .child(
                slot(14.)
                    .text_color(rgb(if on { TEXT } else { DIM }))
                    .child(if on { "\u{25be}" } else { "\u{25b8}" }),
            )
            .child(slot(18.).child(div().size(px(10.)).rounded(px(2.)).bg(rgb(tint(c.kind)))))
            .child(slot(20.).text_color(rgb(MUTED)).child(glyph))
            .child(div().min_w_0().overflow_hidden().child(c.label.clone()))
            .child(
                div()
                    .flex_none()
                    .pl_3()
                    .text_color(rgb(DIM))
                    .child(format!("{kind} \u{b7} line {}", c.home)),
            )
    }

    fn attr(&self, label: &'static str, lit: bool, control: Div, gem: Div) -> Div {
        div()
            .relative()
            .h(px(ATTR))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(34.))
            .pr(px(8.))
            .border_b_1()
            .border_color(rgb(LINE))
            .when(lit, |d| {
                d.bg(rgb(LINE))
                    .child(div().absolute().left_0().top_0().bottom_0().w(px(2.)).bg(rgb(BLUE)))
            })
            .child(
                div()
                    .w(px(14.))
                    .flex_none()
                    .child(div().size(px(7.)).rounded_full().border_1().border_color(rgb(DIM))),
            )
            .child(div().w(px(80.)).flex_none().child(label))
            .child(
                control
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(11.))
                    .whitespace_nowrap(),
            )
            .child(gem)
    }

    fn cells(&self, c: &Clip, rows: &[Row], cx: &mut Context<Self>) -> Vec<Div> {
        let code = |text: String| div().text_color(rgb(DIM)).child(format!("{text}  code"));
        let number = |row: &Row| {
            let (name, a) = (row.name, ATTRS.iter().find(|a| a.0 == row.name).unwrap_or(&ATTRS[0]));
            let (text, keyed) = (format!("{:.*}", a.5, row.now), matches!(row.form, Form::Keyed(_)));
            let control = match row.form {
                Form::Code => code(text),
                _ => div()
                    .cursor_col_resize()
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, e, _| this.tug(name, e.position)))
                    .children((a.4 == 1.).then(|| slider(row.now as f32, 64., BLUE)))
                    .child(field(text)),
            };
            let key = match row.form == Form::Code || STILL.contains(&name) {
                true => gem(false, BORDER),
                false => gem(keyed, if keyed { BLUE } else { DIM })
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.pin(name))),
            };
            self.attr(a.1, self.key.is_some_and(|k| k.1 == name), control, key)
        };
        let mut out: Vec<Div> = rows.iter().map(number).collect();
        let swatch = |name: &'static str, cx: &mut Context<Self>| {
            let (none, set) = if name == "color" {
                ([255; 4], &c.color)
            } else {
                ([0; 4], &c.background)
            };
            let control = match self.option(c, name).map(|v| (tone(&v), v)) {
                Some((ink, v)) if ink.is_some() || v.is_empty() => div()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.dye(name)))
                    .child(
                        div()
                            .size(px(13.))
                            .flex_none()
                            .rounded(px(3.))
                            .bg(rgba(u32::from_be_bytes(ink.unwrap_or(none))))
                            .border_1()
                            .border_color(rgb(BORDER)),
                    )
                    .child(div().text_color(rgb(MUTED)).child(match (v.is_empty(), name) {
                        (false, _) => v.to_uppercase(),
                        (_, "color") => "#FFFFFF".into(),
                        _ => "none".into(),
                    })),
                _ => code(set.clone()),
            };
            self.attr(name, false, control, gem(false, BORDER))
        };
        if let Kind::Rect | Kind::Line = c.kind {
            out.push(swatch("color", cx));
        }
        if c.kind == Kind::Text {
            let font = match self.option(c, "font") {
                Some(name) => div().child(
                    field(if name.is_empty() { "default".into() } else { stem(&name) })
                        .w(px(120.))
                        .justify_between()
                        .overflow_hidden()
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Left,
                            Self::tap(cx, |this, _, _| this.menu = if this.shut == "font" { "" } else { "font" }),
                        )
                        .child(div().text_color(rgb(MUTED)).child("\u{25be}")),
                ),
                None => code(stem(&c.font)),
            };
            out.push(swatch("color", cx));
            out.push(self.attr("font", false, font, gem(false, BORDER)));
            out.push(swatch("background", cx));
        }
        out
    }

    fn lanes(&self, c: &Clip, rows: &[Row], per: f64, cx: &mut Context<Self>) -> Vec<Div> {
        let (d, pan, z, start, len) = (self.span(), self.pan, c.z, c.start, c.end - c.start);
        let lane = |row: Option<&Row>| {
            let keys = row.and_then(|r| match &r.form {
                Form::Keyed(keys) => Some((r.name, keys)),
                _ => None,
            });
            let lit = row.is_some_and(|r| self.key.is_some_and(|k| k.1 == r.name));
            let out = div()
                .relative()
                .h(px(ATTR))
                .flex_none()
                .border_b_1()
                .border_color(rgb(LINE))
                .when(lit, |d| d.bg(rgb(LINE)));
            let Some((name, keys)) = keys else { return out };
            let spots: Vec<f64> = keys.iter().map(|k| start + secs(&k.time, len)).collect();
            let picked = self.key.filter(|k| k.1 == name).map(|k| k.2);
            let hits = spots.iter().enumerate().map(|(i, &t)| {
                div()
                    .absolute()
                    .left(relative(((t - pan) / d) as f32))
                    .top(px(2.))
                    .ml(px(-7.))
                    .size(px(14.))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        Self::tap(cx, move |this, e, _| {
                            this.seek(t);
                            this.key = Some((z, name, i));
                            this.seize(e.position, [per, 0., 0.], Pull::Key);
                        }),
                    )
            });
            let hits: Vec<Div> = hits.collect();
            let draw = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                let at = |t: f64| point(b.origin.x + b.size.width * ((t - pan) / d) as f32, b.center().y);
                if let (Some(&first), Some(&last)) = (spots.first(), spots.last()) {
                    let bar = Bounds::from_corners(at(first) - point(px(0.), px(1.)), at(last) + point(px(0.), px(1.)));
                    window.paint_quad(fill(bar, rgba(BLUE << 8 | 0x73)));
                }
                for (i, &t) in spots.iter().enumerate() {
                    lozenge(window, at(t), 5., if picked == Some(i) { TEXT } else { BLUE }, true);
                }
            };
            out.child(track().child(canvas(|_, _, _| (), draw).absolute().size_full()).children(hits))
        };
        let extra = count(c.kind).saturating_sub(rows.len());
        rows.iter().map(Some).chain((0..extra).map(|_| None)).map(lane).collect()
    }

    pub fn scene(&self, per: f64, cx: &mut Context<Self>) -> Vec<Div> {
        let (d, pan, tl) = (self.span(), self.pan, self.tl.clone());
        let x = move |t: f64| relative(((t - pan) / d) as f32);
        let picked = self.opened().cloned();
        let rows = picked.as_ref().map_or(vec![], |c| self.rows(c));
        let (step, parts) = tick(per, 64.);
        let fine = step / parts as f64;
        let ticks = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            for i in (pan / fine).ceil() as i64..=((pan + d) / fine) as i64 {
                let (tall, ink) = match (i % parts == 0, parts == 4 && i % 2 == 0) {
                    (true, _) => (8., DIM),
                    (_, true) => (5., 0x5c6270),
                    _ => (3., 0x5c6270),
                };
                let at = b.origin.x + b.size.width * ((i as f64 * fine - pan) / d) as f32;
                let mark = Bounds::new(point(at.round(), b.bottom() - px(tall)), size(px(1.), px(tall)));
                window.paint_quad(fill(mark, rgb(ink)));
            }
        };
        let label = |t: f64, s: String| div().absolute().left(x(t)).ml(px(-3.)).top(px(1.)).child(s);
        let numbers = ((pan / step).ceil() as i64..=((pan + d) / step) as i64)
            .map(|i| i as f64 * step)
            .map(|t| label(t, ((t * 1e3).round() / 1e3).to_string()));
        let marks = self
            .marks
            .iter()
            .map(|(m, &t)| label(t, m.to_string()).ml(px(4.)).text_color(rgb(ORANGE)));
        let ruler = div()
            .relative()
            .flex_1()
            .overflow_hidden()
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.scrub(e.position.x, window);
                    cx.notify();
                }),
            )
            .child(
                track()
                    .child(canvas(|_, _, _| (), ticks).absolute().size_full())
                    .children(numbers)
                    .children(marks),
            );
        let names = || div().w(px(NAMES)).flex_none().border_r_1().border_color(rgb(BORDER));
        let head = div()
            .h(px(RULER))
            .flex_none()
            .flex()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                names()
                    .flex()
                    .items_center()
                    .pl(px(62.))
                    .text_size(px(11.))
                    .text_color(rgb(DIM))
                    .child("Name"),
            )
            .child(ruler);
        let (mut cells, mut lanes) = (vec![], vec![]);
        for c in &tl.clips {
            let (z, on) = (c.z, self.sel == Some(c.z));
            let bar = div()
                .absolute()
                .top(px(3.))
                .h(px(14.))
                .left(x(c.start))
                .w(relative(((c.end - c.start) / d) as f32))
                .px(px(6.))
                .rounded(px(3.))
                .border_1()
                .border_color(rgba(tint(c.kind) << 8 | if on { 0xff } else { 0xb3 }))
                .bg(rgba(tint(c.kind) << 8 | if on { 0x4d } else { 0x2e }))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(px(10.))
                .line_height(px(12.))
                .child(c.label.clone());
            cells.push(self.layer(c, cx));
            lanes.push(
                div()
                    .relative()
                    .h(px(LAYER))
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .when(on, |d| d.bg(rgb(BAR)))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.pick(z)))
                    .child(track().child(bar)),
            );
            if on {
                cells.extend(self.cells(c, &rows, cx));
                lanes.extend(self.lanes(c, &rows, per, cx));
            }
        }
        let column = |rows: Vec<Div>| div().mt(px(-self.lift)).flex().flex_col().children(rows);
        let time = div()
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.key = None;
                    cx.notify();
                }),
            )
            .child(match self.graph {
                true => self.plot(picked.as_ref(), &rows, per, cx),
                false => column(lanes),
            });
        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .text_size(px(12.))
            .child(names().overflow_hidden().child(column(cells)))
            .child(time);
        let side = |label: &'static str, graph: bool| {
            div()
                .h_full()
                .px(px(7.))
                .flex()
                .items_center()
                .cursor_pointer()
                .when(self.graph == graph, |d| d.bg(rgb(BORDER)).text_color(rgb(TEXT)))
                .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.graph = graph))
                .child(label)
        };
        let nudge = |label: &'static str, k: f64| {
            div()
                .w(px(14.))
                .flex()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.text_color(rgb(TEXT)))
                .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.magnify(k)))
                .child(label)
        };
        let depth = self.depth();
        let zoom = slider((self.zoom.ln() / depth) as f32, ZOOM, BORDER)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                Self::tap(cx, move |this, e, _| {
                    this.seize(e.position, [depth / (ZOOM - 8.) as f64, 0., 0.], Pull::Zoom(this.zoom))
                }),
            );
        let foot = div()
            .h(px(FOOT))
            .flex_none()
            .flex()
            .border_t_1()
            .border_color(rgb(BORDER))
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .whitespace_nowrap()
            .child(
                names()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pl(px(10.))
                    .pr(px(8.))
                    .child(format!("{} selected", picked.iter().count()))
                    .child(
                        div()
                            .h(px(16.))
                            .flex()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .overflow_hidden()
                            .text_color(rgb(DIM))
                            .child(side("Time Editor", false))
                            .child(side("Graph Editor", true)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .pr_3()
                    .child(nudge("\u{2212}", 0.5))
                    .child(zoom)
                    .child(nudge("+", 2.)),
            );
        let flag = |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            let (mut path, o) = (PathBuilder::fill(), b.origin);
            let at = |x: f32, y: f32| point(o.x + px(x), o.y + px(y));
            path.add_polygon(&[at(0., 0.), at(11., 0.), at(11., 7.), at(5.5, 12.), at(0., 7.)], true);
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(BLUE));
            }
        };
        let head_at = div()
            .absolute()
            .top(px(10.))
            .bottom_0()
            .left(x(self.t))
            .w(px(1.))
            .bg(rgb(BLUE))
            .child(canvas(|_, _, _| (), flag).absolute().left(px(-5.)).w(px(11.)).h(px(12.)));
        let over = div()
            .absolute()
            .top_0()
            .bottom(px(FOOT))
            .left(px(NAMES))
            .right_0()
            .overflow_hidden()
            .child(track().child(head_at));
        vec![head, body, foot, over]
    }

    pub fn fonts(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let c = self
            .opened()
            .filter(|c| self.menu == "font" && self.motion && c.kind == Kind::Text)?;
        let at = self.tl.clips.iter().position(|o| o.z == c.z)? as f32;
        let h = f32::from(window.viewport_size().height);
        let y = h - 28. - self.dock + RULER + (at + 1.) * LAYER + 8. * ATTR - self.lift;
        let (all, now) = (faces(self.script.parent()?), self.option(c, "font").unwrap_or_default());
        let items = all.iter().take(FACES).map(|name| {
            let pick = name.clone();
            div()
                .h(px(20.))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(3.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(BAR)))
                .when(*name == now, |d| d.bg(rgba(BLUE << 8 | 0x40)))
                .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.face(&pick)))
                .child(name.clone())
        });
        let items: Vec<Div> = items.collect();
        let note = match all.len() {
            0 => Some("no font files next to the script or in fonts/".to_string()),
            n if n > FACES => Some(format!("{} more: type the name in the script", n - FACES)),
            _ => None,
        };
        Some(
            div()
                .absolute()
                .left(px(128.))
                .bottom(px(h - y))
                .min_w(px(120.))
                .p_1()
                .flex()
                .flex_col()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(6.))
                .text_size(px(12.))
                .occlude()
                .children(items)
                .children(note.map(|n| div().px_2().text_size(px(11.)).text_color(rgb(DIM)).child(n))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks() {
        assert_eq!((tick(18. / 630., 64.), tick(60. / 600., 64.)), ((2., 4), (10., 4)));
        assert_eq!((tick(0.3 / 900., 64.), tick(3600. / 600., 64.)), ((0.05, 5), (500., 5)));
        assert_eq!((tick(1. / 64., 64.), tick(0.75 / 210., 40.)), ((1., 4), (0.2, 4)));
    }

    #[test]
    fn sinks() {
        assert_eq!(
            (sink(-5., 300., 200.), sink(500., 300., 200.), sink(50., 300., 200.)),
            (0., 100., 50.)
        );
        assert_eq!((sink(50., 100., 200.), sink(f32::MAX, 231., 170.)), (0., 61.));
    }
}
