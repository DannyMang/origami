use crate::{
    Origami,
    complete::faces,
    motion::{ATTRS, Cord, FOLLOWS, Form, Link, Pull, Row, STILL, curve, name, secs},
    view::{BAR, BG, BLUE, BORDER, DIM, MUTED, ORANGE, PANEL, RULER, TEXT, tint},
};
use gpui::{
    App, Bounds, Context, Div, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, ScrollWheelEvent, Window, canvas, deferred, div,
    fill, point, prelude::*, px, relative, rgb, rgba, size,
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
const FACES: usize = 12;

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

pub fn field(text: String) -> Div {
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

pub fn stem(path: &str) -> String {
    Path::new(path).file_stem().unwrap_or_default().to_string_lossy().to_string()
}

pub fn title(attr: &str) -> &str {
    ATTRS.iter().find(|a| a.0 == attr).map_or(attr, |a| a.1)
}

pub fn hit(slots: &[(u32, &'static str, f32)], y: f32) -> Option<(u32, &'static str)> {
    let mut top = 0.;
    let under = |s: &&(u32, &'static str, f32)| {
        top += s.2;
        (top - s.2..top).contains(&y)
    };
    slots.iter().find(under).map(|s| (s.0, s.1))
}

fn top(slots: &[(u32, &'static str, f32)], at: (u32, &str)) -> f32 {
    slots.iter().take_while(|s| (s.0, s.1) != at).map(|s| s.2).sum()
}

fn sheet(left: f32, bottom: f32) -> Div {
    div()
        .absolute()
        .left(px(left))
        .bottom(px(bottom))
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
}

fn option(label: String) -> Div {
    div()
        .h(px(20.))
        .px_2()
        .flex()
        .items_center()
        .rounded(px(3.))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(BAR)))
        .child(label)
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

    pub fn slots(&self) -> Vec<(u32, &'static str, f32)> {
        let mut out = vec![];
        for c in &self.tl.clips {
            out.push((c.z, "", LAYER));
            if self.open.contains(&c.z) {
                let rows = self.rows(c);
                out.extend(rows.iter().map(|r| (c.z, r.name, ATTR)));
                out.extend((rows.len()..count(c.kind)).map(|_| (c.z, "-", ATTR)));
            }
        }
        out
    }

    pub fn tall(&self) -> f32 {
        self.slots().iter().map(|s| s.2).sum()
    }

    fn floor(&self, window: &Window) -> f32 {
        f32::from(window.viewport_size().height) - 28. - self.dock + RULER - self.lift
    }

    pub fn haul(&mut self, at: Point<Pixels>, window: &Window) {
        let y = f32::from(at.y) - self.floor(window);
        let over = hit(&self.slots(), y).filter(|_| f32::from(at.x) < NAMES);
        if let Some(c) = &mut self.cord {
            (c.at, c.over) = (at, over);
        }
    }

    fn layer(&self, c: &Clip, cx: &mut Context<Self>) -> Div {
        let (z, on, open) = (c.z, self.sel == Some(c.z), self.open.contains(&c.z));
        let (glyph, kind) = match c.kind {
            Kind::Video => ("\u{25b6}", "video"),
            Kind::Sound => ("\u{266a}", "sound"),
            Kind::Image => ("\u{25a3}", "image"),
            Kind::Text => ("T", "text"),
            Kind::Color => ("\u{25a0}", "color"),
            Kind::Rect => ("\u{25ad}", "rect"),
            Kind::Line => ("\u{2571}", "line"),
            Kind::Path => ("\u{270e}", "path"),
            Kind::Group => ("\u{25a6}", "group"),
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
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.unfurl(z)))
                    .child(if open { "\u{25be}" } else { "\u{25b8}" }),
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

    fn attr(&self, at: (u32, &'static str), label: &'static str, lit: (bool, bool), control: Div, gem: Div, cx: &mut Context<Self>) -> Div {
        let aim = self
            .cord
            .filter(|c| c.to != at)
            .map(|c| (c.over == Some(at), self.bound(c.to, at).is_ok()));
        let (lit, linked, tied) = (
            lit.0 || self.cord.is_some_and(|c| c.to == at),
            lit.1,
            lit.1 || aim.is_some_and(|a| a.1),
        );
        let pull = move |this: &mut Self, e: &MouseDownEvent, _: &mut Window| {
            this.cord = Some(Cord {
                to: at,
                from: e.position,
                at: e.position,
                over: None,
            })
        };
        let dot = div()
            .size(px(7.))
            .rounded_full()
            .border_1()
            .border_color(rgb(if tied { BLUE } else { DIM }))
            .when(linked, |d| d.bg(rgb(BLUE)));
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
            .map(|d| match aim {
                Some((true, true)) => d.bg(rgb(BAR)).border_1().border_color(rgb(BLUE)),
                Some((_, false)) => d.opacity(0.35),
                _ => d,
            })
            .capture_any_mouse_down(cx.listener(move |this, _, _, _| {
                if this.mouse && this.sel != Some(at.0) {
                    (this.sel, this.key) = (Some(at.0), None);
                }
            }))
            .child(
                div()
                    .w(px(14.))
                    .h_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(dot)
                    .when(FOLLOWS.contains(&at.1), |d| {
                        d.cursor_pointer().on_mouse_down(MouseButton::Left, Self::tap(cx, pull))
                    }),
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
        let (z, code) = (c.z, |text: String| div().text_color(rgb(DIM)).child(format!("{text}  code")));
        let chip = |name: &'static str, l: &Link, step: f64, cx: &mut Context<Self>| {
            let label = format!("\u{2190} {}.{}", l.clip, title(l.attr));
            let menu = move |this: &mut Self, _: &MouseDownEvent, _: &mut Window| {
                let shut = this.shut == "chip" && this.tie == Some((z, name));
                (this.menu, this.tie) = (if shut { "" } else { "chip" }, Some((z, name)));
            };
            let chip = div()
                .h(px(15.))
                .px(px(6.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(BLUE))
                .bg(rgba(BLUE << 8 | 0x26))
                .text_size(px(10.))
                .text_color(rgb(BLUE))
                .min_w_0()
                .overflow_hidden()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, Self::tap(cx, menu))
                .child(label);
            div().gap_1().child(chip).children(l.wire.map(|(mul, n)| {
                let (sign, gain) = match (mul, n < 0.) {
                    (true, _) => ("\u{d7}", 0.01),
                    (_, true) => ("\u{2212}", step),
                    _ => ("+", step),
                };
                let tug =
                    move |this: &mut Self, e: &MouseDownEvent, _: &mut Window| this.seize(e.position, [0., gain, 0.], Pull::Wire(name, n));
                field(format!("{sign} {}", if mul { n } else { n.abs() }))
                    .px(px(4.))
                    .flex_none()
                    .text_size(px(10.))
                    .cursor_col_resize()
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, tug))
            }))
        };
        let number = |row: &Row, cx: &mut Context<Self>| {
            let (name, a) = (row.name, ATTRS.iter().find(|a| a.0 == row.name).unwrap_or(&ATTRS[0]));
            let (text, keyed) = (format!("{:.*}", a.5, row.now), matches!(row.form, Form::Keyed(_)));
            let control = match &row.form {
                Form::Code => code(text),
                Form::Linked(l) => chip(name, l, a.2, cx),
                _ => div()
                    .cursor_col_resize()
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, e, _| this.tug(name, e.position)))
                    .children((a.4 == 1.).then(|| slider(row.now as f32, 64., BLUE)))
                    .child(field(text)),
            };
            let key = match row.form.free() && !STILL.contains(&name) {
                false => gem(false, BORDER),
                true => gem(keyed, if keyed { BLUE } else { DIM })
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.pin(name))),
            };
            let lit = (
                self.key.is_some_and(|k| (k.0, k.1) == (z, name)),
                matches!(row.form, Form::Linked(_)),
            );
            self.attr((z, name), a.1, lit, control, key, cx)
        };
        let mut out: Vec<Div> = rows.iter().map(|r| number(r, cx)).collect();
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
            self.attr((z, "-"), name, (false, false), control, gem(false, BORDER), cx)
        };
        if let Kind::Rect | Kind::Line = c.kind {
            out.push(swatch("color", cx));
        }
        if c.kind == Kind::Text {
            let row = (c.home as usize).saturating_sub(1);
            let font = div().child(
                self.typeface(row)
                    .w(px(120.))
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.fonting(row))),
            );
            out.push(swatch("color", cx));
            out.push(self.attr((z, "-"), "font", (false, false), font, gem(false, BORDER), cx));
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
            let lit = row.is_some_and(|r| self.key.is_some_and(|k| (k.0, k.1) == (z, r.name)));
            let out = div()
                .relative()
                .h(px(ATTR))
                .flex_none()
                .border_b_1()
                .border_color(rgb(LINE))
                .when(lit, |d| d.bg(rgb(LINE)));
            if let Some(r) = row.filter(|r| matches!(r.form, Form::Linked(_))) {
                let (s, fps) = (curve(c, r.name), self.fps());
                let (lo, hi) = s.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
                let trace = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                    let x = |t: f64| b.origin.x + b.size.width * ((start + t - pan) / d) as f32;
                    let y = |v: f32| b.origin.y + px(ATTR - 5. - (v - lo) / (hi - lo).max(1e-6) * (ATTR - 10.));
                    let mut line = PathBuilder::stroke(px(1.2));
                    line.move_to(point(x(0.), y(s[0])));
                    for (i, v) in s.iter().enumerate() {
                        line.line_to(point(x(i as f64 / fps), y(*v)));
                    }
                    line.line_to(point(x(len), y(s[s.len() - 1])));
                    if let Ok(line) = line.build() {
                        window.paint_path(line, rgba(BLUE << 8 | 0xa0));
                    }
                };
                return out.child(track().child(canvas(|_, _, _| (), trace).absolute().size_full()));
            }
            let Some((name, keys)) = keys else { return out };
            let spots: Vec<f64> = keys.iter().map(|k| start + secs(&k.time, len)).collect();
            let picked = self.key.filter(|k| (k.0, k.1) == (z, name)).map(|k| k.2);
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
        let (mut cells, mut lanes, mut y, mut skip) = (vec![], vec![], 0., 0.);
        for c in &tl.clips {
            let (z, on, open) = (c.z, self.sel == Some(c.z), self.open.contains(&c.z));
            let rows = if open { self.rows(c) } else { vec![] };
            let tall = LAYER + if open { rows.len().max(count(c.kind)) as f32 * ATTR } else { 0. };
            y += tall;
            if y <= self.lift {
                skip = y;
                continue;
            }
            if y - tall >= self.lift + self.dock {
                break;
            }
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
            if open {
                cells.extend(self.cells(c, &rows, cx));
                lanes.extend(self.lanes(c, &rows, per, cx));
            }
        }
        let column = |rows: Vec<Div>| div().mt(px(skip - self.lift)).flex().flex_col().children(rows);
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

    pub fn faced(&self, row: usize) -> String {
        let made = |c: &&Clip| c.kind == Kind::Text && c.home as usize == row + 1;
        self.tl.clips.iter().find(made).map_or(String::new(), |c| stem(&c.font))
    }

    pub fn typeface(&self, row: usize) -> Div {
        let name = Some(self.faced(row)).filter(|n| !n.is_empty());
        field(name.unwrap_or("default".into()))
            .justify_between()
            .overflow_hidden()
            .cursor_pointer()
            .child(div().text_color(rgb(MUTED)).child("\u{25be}"))
    }

    pub fn shelf(&self) -> Vec<String> {
        let (all, want) = (self.script.parent().map_or(vec![], faces), self.sift.0.to_lowercase());
        let fits = |n: &String| if n.is_empty() { "default" } else { n }.to_lowercase().contains(&want);
        [String::new()].into_iter().chain(all).filter(fits).collect()
    }

    fn fount(&self, window: &Window) -> Option<(usize, f32, f32)> {
        let h = f32::from(window.viewport_size().height);
        let tall = FACES as f32 * 20. + 52.;
        let code = |(row, x, y): (usize, f32, f32)| (row, x, if y + 24. + tall > h - 28. { h - y } else { h - y - 24. - tall });
        let motion = || {
            let c = self.opened().filter(|c| self.motion && c.kind == Kind::Text)?;
            let y = self.floor(window) + top(&self.slots(), (c.z, "-")) + ATTR;
            Some(((c.home as usize).checked_sub(1)?, 128., h - y))
        };
        (self.menu == "font").then(|| self.roost(window).map(code).or_else(motion))?
    }

    pub fn narrow(&mut self, k: &str, held: bool, window: &Window) -> bool {
        let Some((row, ..)) = self.fount(window) else { return false };
        if held {
            self.menu = "";
            return false;
        }
        match k {
            "escape" => self.menu = "",
            "backspace" => drop(self.sift.0.pop()),
            "space" => self.sift.0.push(' '),
            "enter" => {
                if let Some(name) = self.shelf().first().filter(|_| !self.sift.0.is_empty()).cloned() {
                    self.face(row, &name);
                }
                self.menu = "";
            }
            k if k.chars().count() == 1 => self.sift.0.push_str(k),
            _ => {}
        }
        self.sift.1 = 0.;
        true
    }

    pub fn fonts(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let (row, left, bottom) = self.fount(window)?;
        let (all, now, want) = (self.shelf(), self.faced(row), &self.sift.0);
        let first = (self.sift.1 as usize).min(all.len().saturating_sub(FACES));
        let items = all.iter().skip(first).take(FACES).map(|name| {
            let pick = name.clone();
            option(if name.is_empty() { "default".into() } else { name.clone() })
                .flex_none()
                .whitespace_nowrap()
                .overflow_hidden()
                .when(stem(name) == now, |d| d.bg(rgba(BLUE << 8 | 0x40)))
                .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.face(row, &pick)))
        });
        let items: Vec<Div> = items.collect();
        let find = match want.is_empty() {
            true => field("type to filter".into()).text_color(rgb(DIM)),
            false => field(format!("{want}\u{2502}")),
        };
        let note = match all.len() {
            0 => "no font matches \u{b7} backspace widens".to_string(),
            n if n > FACES => format!("{}\u{2013}{} of {n} \u{b7} scroll for more", first + 1, first + FACES),
            n => format!("{n} fonts"),
        };
        let roll = cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
            let last = this.shelf().len().saturating_sub(FACES) as f32;
            this.sift.1 = (this.sift.1 - f32::from(e.delta.pixel_delta(px(20.)).y) / 20.).clamp(0., last);
            cx.stop_propagation();
            cx.notify();
        });
        Some(
            sheet(left, bottom)
                .w(px(230.))
                .on_scroll_wheel(roll)
                .child(find.h(px(20.)).mb_1())
                .child(div().h(px(FACES as f32 * 20.)).flex().flex_col().children(items))
                .child(
                    div()
                        .h(px(18.))
                        .px_2()
                        .flex()
                        .items_center()
                        .text_size(px(11.))
                        .text_color(rgb(DIM))
                        .child(note),
                ),
        )
    }

    pub fn wires(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let at = self.tie.filter(|_| self.motion && ["follow", "chip"].contains(&self.menu))?;
        let me = self.tl.clips.iter().find(|c| c.z == at.0)?;
        let up = f32::from(window.viewport_size().height) - self.floor(window) - top(&self.slots(), at);
        if self.menu == "chip" {
            let (_, rows) = self.tied(at)?;
            let wired = rows
                .iter()
                .any(|r| r.name == at.1 && matches!(&r.form, Form::Linked(l) if l.wire.is_some()));
            let item = |label: &str, act: fn(&mut Self, (u32, &'static str)), cx: &mut Context<Self>| {
                option(label.into()).on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| act(this, at)))
            };
            let maths = match wired {
                true => vec![item("remove the maths", |this, at| this.wrap(at, None), cx)],
                false => vec![
                    item("add an offset", |this, at| this.wrap(at, Some((false, 0.))), cx),
                    item("multiply", |this, at| this.wrap(at, Some((true, 1.))), cx),
                ],
            };
            let menu = sheet(128., up)
                .child(item("go to source", Self::trace, cx))
                .child(item("break", Self::sever, cx));
            return Some(menu.children(maths));
        }
        let (text, sites) = self.sites();
        let above = self
            .tl
            .clips
            .iter()
            .zip(sites)
            .filter(|(c, _)| c.home < me.home && c.kind != Kind::Sound);
        let mut named: Vec<(&Clip, String)> = above.filter_map(|(c, f)| Some((c, name(&text, f?, c.kind)?.into()))).collect();
        named.dedup_by_key(|n| n.0.home);
        let both = ["pos"].into_iter().filter(|_| at.1.len() == 1);
        let lines = named.iter().map(|(c, name)| {
            let from = c.z;
            let picks = both.clone().chain(FOLLOWS[..5].iter().copied()).map(|a| {
                option(title(a).into())
                    .text_color(rgb(MUTED))
                    .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.bind(at, (from, a))))
            });
            let picks: Vec<Div> = picks.collect();
            div()
                .h(px(22.))
                .px_1()
                .flex()
                .items_center()
                .child(div().size(px(10.)).flex_none().rounded(px(2.)).bg(rgb(tint(c.kind))))
                .child(div().w(px(84.)).flex_none().pl_2().overflow_hidden().child(name.clone()))
                .children(picks)
        });
        let lines: Vec<Div> = lines.collect();
        let head = match lines.is_empty() {
            true => "no named clip is defined above this one in the script".to_string(),
            false => format!("{} follows", title(at.1)),
        };
        Some(
            sheet(48., up)
                .child(div().h(px(18.)).px_2().text_size(px(11.)).text_color(rgb(DIM)).child(head))
                .children(lines),
        )
    }

    pub fn cable(&self) -> Option<Div> {
        let c = self.cord.filter(|c| !c.still())?;
        let draw = move |_, _, window: &mut Window, _: &mut App| {
            let (mut line, bow) = (
                PathBuilder::stroke(px(1.5)),
                point((c.at.x - c.from.x).abs() / 2. + px(24.), px(0.)),
            );
            line.move_to(c.from);
            line.cubic_bezier_to(c.at, c.from + bow, c.at - bow);
            if let Ok(line) = line.build() {
                window.paint_path(line, rgb(BLUE));
            }
        };
        Some(
            div()
                .absolute()
                .size_full()
                .child(deferred(canvas(|_, _, _| (), draw).absolute().size_full())),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The time ruler in the Motion tab should pick tick spacing that suits the zoom level, so labels
    // are neither crowded nor sparse.
    #[test]
    fn ticks() {
        assert_eq!((tick(18. / 630., 64.), tick(60. / 600., 64.)), ((2., 4), (10., 4)));
        assert_eq!((tick(0.3 / 900., 64.), tick(3600. / 600., 64.)), ((0.05, 5), (500., 5)));
        assert_eq!((tick(1. / 64., 64.), tick(0.75 / 210., 40.)), ((1., 4), (0.2, 4)));
    }

    // Given how far down the Motion tab the pointer is, find the clip row or property row under it,
    // and nothing when it is outside the list.
    #[test]
    fn finds_the_row_under_the_pointer() {
        let slots = [
            (0, "", LAYER),
            (1, "", LAYER),
            (1, "x", ATTR),
            (1, "y", ATTR),
            (1, "-", ATTR),
            (2, "", LAYER),
        ];
        assert_eq!(
            (hit(&slots, 0.), hit(&slots, 19.9), hit(&slots, 20.)),
            (Some((0, "")), Some((0, "")), Some((1, "")))
        );
        assert_eq!(
            (hit(&slots, 41.), hit(&slots, 60.), hit(&slots, 80.)),
            (Some((1, "x")), Some((1, "y")), Some((1, "-")))
        );
        assert_eq!(
            (hit(&slots, 100.), hit(&slots, 117.), hit(&slots, -1.)),
            (Some((2, "")), None, None)
        );
        assert_eq!(
            (top(&slots, (1, "y")), top(&slots, (2, "")), top(&slots, (9, ""))),
            (59., 97., 117.)
        );
    }

    // Scrolling the Motion tab should stop at the top and at the bottom of its content.
    #[test]
    fn sinks() {
        assert_eq!(
            (sink(-5., 300., 200.), sink(500., 300., 200.), sink(50., 300., 200.)),
            (0., 100., 50.)
        );
        assert_eq!((sink(50., 100., 200.), sink(f32::MAX, 231., 170.)), (0., 61.));
    }

    // The font row of a text call opens a scrolling list of this Mac's fonts that typing narrows and that
    // shows the current font, each pick writing one undo step into that call's options (adding them when
    // there are none, replacing a variable only there, removing the font for default, never writing a
    // font name that is not plain text), while escape changes nothing, options the app cannot rewrite get
    // a note instead, and the list also opens from the Motion tab.
    #[gpui::test]
    fn picks_fonts(cx: &mut gpui::TestAppContext) {
        use gpui::{Modifiers, VisualTestContext};
        let dir = std::env::temp_dir().join("origami-font-test");
        std::fs::create_dir_all(&dir).unwrap();
        let src = "let face = \"Menlo\"
let a = text(\"daniel\", { font: face, size: 24, background: \"#0008\" }).for(2)
let b = text(\"text\").at(1).for(3).pos(0.5, 0.5)
let look = { size: 20 }
let c = caption(\"c\", look).for(1)
stack(a, b, c)";
        std::fs::write(dir.join("edit.js"), src).unwrap();
        let evil = "Zed\", size: (() => { throw 1 })(), x: \"";
        std::fs::write(dir.join(format!("{evil}.ttf")), "").unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), false, origami_engine::footprint).unwrap();
        let lines: Vec<String> = src.lines().map(String::from).collect();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (dir.join("edit.js"), lines.clone(), std::sync::Arc::new(tl));
            window.focus(&app.focus);
            app
        });
        let click = |cx: &mut VisualTestContext, x: f32, y: f32| {
            cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::default());
            cx.simulate_mouse_down(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
        };
        let open = |cx: &mut VisualTestContext, row: usize| {
            let at = view.update_in(cx, |o, window, cx| {
                (o.cursor, o.menu, o.shut) = (Some((row, 10)), "", "");
                o.grasp();
                cx.notify();
                o.roost(window).unwrap()
            });
            cx.run_until_parked();
            click(cx, at.1 + 20., at.2 + 12.);
        };
        let state = |cx: &mut VisualTestContext, row: usize| view.update(cx, |o, _| (o.lines[row].clone(), o.past.len(), o.menu));
        open(cx, 2);
        assert_eq!(
            view.update(cx, |o, _| (o.menu, o.shelf().len() > 20, o.shelf()[0].clone())),
            ("font", true, String::new())
        );
        let list = view.update_in(cx, |o, window, _| {
            let (_, left, bottom) = o.fount(window).unwrap();
            point(px(left + 20.), px(f32::from(window.viewport_size().height) - bottom - 100.))
        });
        cx.simulate_event(ScrollWheelEvent {
            position: list,
            delta: gpui::ScrollDelta::Lines(point(0., -3.)),
            ..Default::default()
        });
        assert_eq!(view.update(cx, |o, _| o.sift.1), 3.);
        cx.simulate_keystrokes("m e n l x backspace");
        assert_eq!(
            view.update(cx, |o, _| (o.sift.0.clone(), o.shelf())),
            ("menl".into(), vec!["Menlo".to_string()])
        );
        cx.simulate_keystrokes("enter");
        let typed = "let b = text(\"text\", { font: \"Menlo\" }).at(1).for(3).pos(0.5, 0.5)";
        assert_eq!(state(cx, 2), (typed.into(), 1, ""));
        open(cx, 1);
        assert_eq!(
            view.update(cx, |o, _| (o.faced(1), o.shelf()[o.sift.1 as usize + 3].clone())),
            ("Menlo".into(), "Menlo".into())
        );
        cx.simulate_keystrokes("escape");
        assert_eq!(state(cx, 1), (lines[1].clone(), 1, ""));
        view.update(cx, |o, _| o.face(1, evil));
        assert_eq!(state(cx, 1), (lines[1].clone(), 1, ""));
        open(cx, 1);
        cx.simulate_keystrokes("c o u r i e r");
        let first = view.update_in(cx, |o, window, _| {
            let (_, left, bottom) = o.fount(window).unwrap();
            (left + 20., f32::from(window.viewport_size().height) - bottom - 292. + 39.)
        });
        click(cx, first.0, first.1);
        let named = "let a = text(\"daniel\", { font: \"Courier\", size: 24, background: \"#0008\" }).for(2)";
        assert_eq!(state(cx, 1), (named.into(), 2, "font"));
        assert_eq!(
            view.update(cx, |o, _| (o.lines[0].clone(), o.knobbed)),
            (lines[0].clone(), Some((1, 8)))
        );
        open(cx, 1);
        cx.simulate_keystrokes("d e f a u l t");
        click(cx, first.0, first.1);
        let plain = "let a = text(\"daniel\", { size: 24, background: \"#0008\" }).for(2)";
        assert_eq!(state(cx, 1), (plain.into(), 3, "font"));
        cx.simulate_keystrokes("cmd-z cmd-z cmd-z");
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), lines);
        open(cx, 4);
        let (note, menu) = view.update(cx, |o, _| (o.note.clone(), o.menu));
        assert_eq!(
            (note.as_str(), menu),
            ("line 5 cannot be rewritten safely, edit the script to change its font", "")
        );
        let motion = view.update_in(cx, |o, window, _| {
            (o.knobbed, o.motion, o.sel, o.shut) = (None, true, Some(o.tl.clips[1].z), "");
            o.fonting(2);
            o.fount(window).map(|f| (f.0, f.1))
        });
        assert_eq!(motion, Some((2, 128.)));
    }
}
