use crate::{Asset, Mode, Origami, keys::stamp};
use gpui::{
    App, Bounds, ClipboardItem, Context, Div, FontWeight, HighlightStyle, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, ObjectFit, ParentElement, PathBuilder, Pixels, Render, ScrollWheelEvent, Styled, StyledImage, StyledText, Window,
    canvas, deferred, div, img, point, prelude::*, px, relative, rgb, rgba,
};
use origami_engine::Preview;
use origami_lang::{Clip, Kind, Timeline};
use std::{mem, ops::Range, time::Instant};

pub const BG: u32 = 0x282c33;
pub const PANEL: u32 = 0x2f343e;
pub const BAR: u32 = 0x3b414d;
pub const BORDER: u32 = 0x464b57;
pub const TEXT: u32 = 0xdce0e5;
const MUTED: u32 = 0xa9afbc;
pub const DIM: u32 = 0x878a98;
pub const BLUE: u32 = 0x74ade8;
pub const GREEN: u32 = 0xa1c181;
const PURPLE: u32 = 0xb477cf;
const ORANGE: u32 = 0xbf956a;
const RED: u32 = 0xd07277;
const CODE: u32 = 0xacb2be;
const COMMENT: u32 = 0x5d636f;
const GUTTER: u32 = 0x4e5a5f;
pub const INK: u32 = 0x1b1f25;
const DOCS: &str = "https://example.com/origami/docs";
pub const HEAD: f32 = 48.;
pub const RULER: f32 = 22.;
const HELP: &str =
    "mouse: drag the ruler to move the playhead, click a clip to select, drag to move, drag edges to trim, delete removes it, cmd+z undo, cmd+shift+z redo, f full screen
click the script to type in it, cmd+s saves and applies, esc leaves     scroll zooms the timeline     drag pane edges to resize     cmd+ cmd-  code size     cmd+0  reset layout     cmd+f  search
Motion tab: click a key to select it, drag it to change time and value, double-click a curve to add a key, delete removes it, + add animates a property
fold code with the gutter arrows, cmd+opt+[ and cmd+opt+], or za zM zR in vim keys     esc  vim keys, i  back
h l  frame        H L  second       w b  clip boundary    0 $ gg G  start, end
j k  layer        < >  shift clip   x  delete clip    space  play, pause    m' + letter  marks
v    select       y    yank code    gd  open in editor    / n N  search    .  repeat
:1:05  :goto 1:05  :f 1800  :export out.mp4 [--dry-run]  :e file.js  :set loop  :ir  :q";

fn tab(label: impl IntoElement) -> Div {
    div()
        .h(px(34.))
        .flex_none()
        .flex()
        .items_center()
        .px_3()
        .bg(rgb(PANEL))
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(label)
}

fn tokens(s: &str) -> Vec<(Range<usize>, HighlightStyle)> {
    let (mut out, mut i) = (vec![], 0);
    while i < s.len() {
        let rest = &s[i..];
        let c = rest.chars().next().unwrap();
        let until = |f: fn(char) -> bool| rest.find(f).unwrap_or(rest.len());
        let (n, color) = match c {
            _ if rest.starts_with("//") => (rest.len(), COMMENT),
            '"' | '\'' => (rest[1..].find(c).map_or(rest.len(), |j| j + 2), GREEN),
            _ if c.is_ascii_digit() => (until(|c| !c.is_ascii_digit() && c != '.'), ORANGE),
            _ if c.is_alphabetic() || c == '_' => {
                let n = until(|c| !c.is_alphanumeric() && c != '_');
                let key = ["const", "let", "var", "function", "return"].contains(&&rest[..n]);
                (
                    n,
                    if key {
                        PURPLE
                    } else if rest[n..].starts_with('(') {
                        BLUE
                    } else {
                        CODE
                    },
                )
            }
            _ => (until(|c| c.is_alphanumeric() || "\"'_/".contains(c)).max(c.len_utf8()), CODE),
        };
        out.push((i..i + n, rgb(color).into()));
        i += n;
    }
    out
}

pub fn tint(kind: Kind) -> u32 {
    match kind {
        Kind::Video | Kind::Image => BLUE,
        Kind::Sound => GREEN,
        Kind::Text => PURPLE,
        Kind::Color => DIM,
    }
}

impl Origami {
    fn timecode(&self) -> String {
        let (f, s) = ((self.t * self.fps()).round() as u64, self.t as u64);
        format!(
            "{:02}:{:02}:{:02}:{:02}",
            s / 3600,
            s / 60 % 60,
            s % 60,
            f % self.tl.fps.max(1) as u64
        )
    }

    fn clip(&mut self, c: &Clip, focused: bool, per: f64, cx: &mut Context<Self>) -> Div {
        let (len, d, held) = (c.end - c.start, self.span(), c.clone());
        let wide = len / per;
        let (a, b) = (c.start, c.end);
        let edge = || div().absolute().top_0().bottom_0().w(px(8.)).cursor_col_resize();
        let color = tint(c.kind);
        let rim = if focused { rgb(TEXT) } else { rgba(color << 8 | 0x80) };
        let body = match c.kind {
            Kind::Video | Kind::Image => {
                let n = ((b - a) / d * 12.).ceil().clamp(1., 48.) as usize;
                let grid = 2f64.powf((len * c.speed / n as f64).max(0.1).log2().floor());
                let at = |i| {
                    if c.kind == Kind::Image {
                        0.
                    } else {
                        ((c.src_in + (i as f64 + 0.5) / n as f64 * len * c.speed) / grid).floor() * grid
                    }
                };
                let thumb = |this: &mut Self, t: f64| match this.asset(format!("{}@{t:.1}", c.src), &c.src, Some(t)) {
                    Some(Asset::Thumb(i)) => div()
                        .flex_1()
                        .h_full()
                        .child(img(i.clone()).size_full().object_fit(ObjectFit::Cover)),
                    _ => div().flex_1(),
                };
                Some(div().children((0..n).map(|i| thumb(self, at(i))).collect::<Vec<_>>()))
            }
            Kind::Sound => {
                let a = (c.src_in * 20.) as usize;
                let b = (((c.src_in + len * c.speed) * 20.) as usize).max(a);
                let bars: Vec<f32> = match self.asset(c.src.clone(), &c.src, None) {
                    Some(Asset::Wave(w)) => w[a.min(w.len())..b.min(w.len())]
                        .chunks(((b - a) / 160).max(1))
                        .map(|c| c.iter().fold(0., |m: f32, &p| m.max(p)))
                        .collect(),
                    _ => vec![],
                };
                let draw = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                    let w = b.size.width / bars.len().max(1) as f32;
                    let edge = |i: usize, side: f32| {
                        let y = b.origin.y + b.size.height * (0.5 + side * bars[i].max(0.04) / 2.);
                        [point(b.origin.x + w * i as f32, y), point(b.origin.x + w * (i + 1) as f32, y)]
                    };
                    let top = (0..bars.len()).flat_map(|i| edge(i, -1.));
                    let bottom = (0..bars.len()).rev().flat_map(|i| edge(i, 1.).into_iter().rev());
                    let mut shape = PathBuilder::fill();
                    shape.add_polygon(&top.chain(bottom).collect::<Vec<_>>(), true);
                    if let Ok(shape) = shape.build() {
                        window.paint_path(shape, rgba(GREEN << 8 | 0xb0));
                    }
                };
                Some(div().child(canvas(|_, _, _| (), draw).size_full()))
            }
            _ => None,
        };
        div()
            .absolute()
            .top(px(3.))
            .bottom(px(3.))
            .left(relative(((a - self.pan) / d) as f32))
            .w(relative(((b - a) / d) as f32))
            .rounded(px(3.))
            .map(|d| match wide < 3. {
                true => d.bg(rim),
                false => d
                    .border_1()
                    .border_color(rim)
                    .bg(rgba(color << 8 | if focused { 0x55 } else { 0x24 })),
            })
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    this.grab(&held, e.position.x, window);
                    cx.stop_propagation();
                }),
            )
            .children(body.map(|b| b.flex().size_full()))
            .map(|d| match wide < 16. {
                true => d.cursor_col_resize(),
                false => d.child(edge().left_0()).child(edge().right_0()),
            })
            .children((wide > 5.).then(|| {
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(4.))
                    .px_1()
                    .rounded(px(3.))
                    .bg(rgba(BG << 8 | 0xd9))
                    .text_size(px(11.))
                    .whitespace_nowrap()
                    .child(c.label.clone())
            }))
    }

    fn tool(&self, label: &'static str, cx: &mut Context<Self>, f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static) -> Div {
        div()
            .h(px(26.))
            .px(px(8.))
            .flex()
            .items_center()
            .rounded(px(4.))
            .text_color(rgb(MUTED))
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(BAR)).text_color(rgb(TEXT)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    f(this, window, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(label)
    }

    fn grip(&self, wide: bool, cx: &mut Context<Self>) -> Div {
        let grip = div().absolute().on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.pull = Some(wide);
                cx.stop_propagation();
            }),
        );
        match wide {
            true => grip.top_0().bottom_0().left(px(-3.)).w(px(6.)).cursor_col_resize(),
            false => grip.left_0().right_0().top(px(-3.)).h(px(6.)).cursor_row_resize(),
        }
    }

    fn menu(&self, label: &'static str, items: Vec<Div>, cx: &mut Context<Self>) -> Div {
        let list = div()
            .absolute()
            .bottom(px(26.))
            .right_0()
            .p_1()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(6.))
            .text_size(px(13.))
            .children(items);
        self.tool(label, cx, move |t, _, _| t.menu = if t.shut == label { "" } else { label })
            .relative()
            .children((self.menu == label).then(|| deferred(list)))
    }

    fn tools(&self, tl: &Timeline, focus: Option<&Clip>, cx: &mut Context<Self>) -> Div {
        let pace = match self.rate {
            0.5 => "0.5x",
            2. => "2x",
            _ => "1x",
        };
        let add = vec![
            self.tool("text", cx, |t, _, _| t.write("text")),
            self.tool("caption", cx, |t, _, _| t.write("caption")),
        ];
        let more = vec![
            self.tool("zoom in", cx, |t, _, _| t.magnify(2.)),
            self.tool("zoom out", cx, |t, _, _| t.magnify(0.5)),
            self.tool("full screen", cx, |t, w, _| t.fullscreen(w)),
            self.tool("bigger code", cx, |t, _, _| t.scale(1.)),
            self.tool("smaller code", cx, |t, _, _| t.scale(-1.)),
            self.tool("reset layout", cx, |t, _, _| t.reset()),
            self.tool("export", cx, |t, _, cx| t.run("export out.mp4", cx)),
        ];
        let side = || div().flex_1().flex().items_center().text_size(px(20.));
        let grow = self
            .growable(focus)
            .into_iter()
            .map(|p| self.tool(p, cx, move |t, _, _| t.animate(p)));
        let grow = grow.collect();
        let page = |label: &'static str, motion: bool| {
            div()
                .cursor_pointer()
                .text_color(rgb(if self.motion == motion { TEXT } else { DIM }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.motion = motion;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(label)
        };
        tab(div().flex().gap_4().child(page("Timeline", false)).child(page("Motion", true)))
            .relative()
            .child(
                side()
                    .pl_4()
                    .child(self.tool("\u{21a9}", cx, |t, _, _| t.undo()))
                    .child(self.tool("\u{21aa}", cx, |t, _, _| t.redo()))
                    .child(self.tool("\u{2715}", cx, |t, _, _| t.delete())),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .text_size(px(16.))
                    .child(self.tool("|\u{25c0}", cx, |t, _, _| t.step(-1.)))
                    .child(self.tool(if self.player.is_some() { "||" } else { "\u{25b6}" }, cx, |t, _, _| t.toggle()))
                    .child(self.tool("\u{25b6}|", cx, |t, _, _| t.step(1.))),
            )
            .child(
                side()
                    .justify_end()
                    .child(self.tool(pace, cx, |t, _, _| t.pace()).text_size(px(13.)))
                    .child(match self.motion {
                        true => self.menu("+ add", grow, cx).text_size(px(13.)),
                        false => self.menu("+", add, cx),
                    })
                    .child(self.menu("\u{22ef}", more, cx))
                    .child(div().pl_3().text_size(px(13.)).text_color(rgb(MUTED)).child(format!(
                        "{} / {}",
                        self.timecode(),
                        stamp(tl.duration)
                    ))),
            )
    }

    fn timeline(&mut self, tl: &Timeline, focus: Option<&Clip>, per: f64, cx: &mut Context<Self>) -> Div {
        let (d, pan) = (self.span(), self.pan);
        let x = |t: f64| relative(((t - pan) / d) as f32);
        let mut lanes: Vec<(bool, Vec<&Clip>)> = vec![];
        let heard = |c: &&Clip| c.kind == Kind::Video && c.audio && c.gain > 0.;
        let sound = |c: &Clip| Clip {
            kind: Kind::Sound,
            video: false,
            ..c.clone()
        };
        let linked: Vec<Clip> = tl.clips.iter().filter(heard).map(sound).collect();
        for c in linked.iter().chain(&tl.clips) {
            let free = |(v, l): &(bool, Vec<&Clip>)| *v == c.video && l.iter().all(|o| o.end <= c.start + 1e-9 || c.end <= o.start + 1e-9);
            match lanes.iter().position(free) {
                Some(i) => lanes[i].1.push(c),
                None => lanes.push((c.video, vec![c])),
            }
        }
        let (mut rows, audio): (Vec<_>, Vec<_>) = lanes.into_iter().partition(|l| l.0);
        let videos = rows.len();
        rows.reverse();
        rows.extend(audio);
        let tall = |video: bool| if video { 56. } else { 46. };
        let fit = ((self.dock - 30.) / rows.iter().map(|r| tall(r.0)).sum::<f32>()).min(1.);
        let step = [1., 2., 5., 10., 15., 30., 60., 120., 300., 600.]
            .into_iter()
            .find(|s| d / s <= 10.)
            .unwrap_or(1800.);
        let ticks = ((pan / step).ceil() as usize..=((pan + d) / step) as usize).map(|i| i as f64 * step);
        let lead = self.lead();
        let label = |s: String| {
            div()
                .w(px(lead))
                .flex_none()
                .pl_3()
                .flex()
                .items_center()
                .text_size(px(11.))
                .text_color(rgb(DIM))
                .child(s)
        };
        let mark = |t: f64, s: String| {
            div()
                .absolute()
                .left(x(t))
                .pl_1()
                .border_l_1()
                .border_color(rgb(BORDER))
                .text_size(px(10.))
                .text_color(rgb(DIM))
                .child(s)
        };
        let rows: Vec<Div> = match self.motion {
            true => self.curves(focus, cx),
            false => rows
                .into_iter()
                .enumerate()
                .map(|(i, (video, clips))| {
                    let name = if video {
                        format!("V{}", videos - i)
                    } else {
                        format!("A{}", i - videos + 1)
                    };
                    let clips: Vec<Div> = clips
                        .into_iter()
                        .filter(|c| c.end > pan && c.start < pan + d)
                        .map(|c| self.clip(c, focus.is_some_and(|f| f.z == c.z), per, cx))
                        .collect();
                    div()
                        .h(px(tall(video) * fit))
                        .flex_none()
                        .flex()
                        .pr_3()
                        .child(label(name))
                        .child(div().relative().flex_1().overflow_hidden().children(clips))
                })
                .collect(),
        };
        let k = ((self.t - pan) / d) as f32;
        let wedge = |w: f32| div().w(px(w)).h(px(3.)).bg(rgb(TEXT));
        let head = (0. ..=1.).contains(&k).then(|| {
            div()
                .absolute()
                .top_0()
                .left(relative(k))
                .ml(px(-6.))
                .w(px(13.))
                .flex()
                .flex_col()
                .items_center()
                .child(wedge(13.))
                .child(wedge(9.))
                .child(wedge(5.))
                .child(wedge(1.))
        });
        let select = match self.mode {
            Mode::Visual(a) => Some(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(x(a.min(self.t)))
                    .w(relative(((a - self.t).abs() / d) as f32))
                    .bg(rgba(BLUE << 8 | 0x30)),
            ),
            _ => None,
        };
        div()
            .relative()
            .flex_none()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(self.tools(tl, focus, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .h(px(self.dock))
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, window, cx| {
                        this.scroll(e, window);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .h(px(RULER))
                            .flex_none()
                            .flex()
                            .pr_3()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                                    this.scrub(e.position.x, window);
                                    cx.notify();
                                }),
                            )
                            .child(label(String::new()))
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .overflow_hidden()
                                    .children(ticks.map(|t| mark(t, format!("{}:{:02}", t as u64 / 60, t as u64 % 60))))
                                    .children(
                                        self.marks
                                            .iter()
                                            .map(|(m, &t)| mark(t, m.to_string()).top(px(10.)).text_color(rgb(ORANGE))),
                                    ),
                            ),
                    )
                    .children(rows)
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(self.lead()))
                            .right_3()
                            .overflow_hidden()
                            .children(select)
                            .child(div().absolute().top_0().bottom_0().left(x(self.t)).w(px(1.)).bg(rgb(TEXT))),
                    )
                    .child(div().absolute().top_0().left(px(self.lead())).right_3().children(head)),
            )
            .child(self.grip(false, cx))
    }

    fn script(&self, focus: Option<&Clip>, rows: usize, cx: &mut Context<Self>) -> Div {
        let active = focus.map_or(0, |c| c.line as usize);
        let caret = self.cursor;
        let span = self.chosen();
        let state = match self.dirty {
            true => "unsaved changes".into(),
            false if self.saved.is_empty() => String::new(),
            false => format!("saved {}", self.saved),
        };
        let name = self.script.file_name().unwrap_or_default().to_string_lossy().to_string();
        let (regions, hidden, issues, found) = (self.regions(), self.hidden(), self.issues(), self.hits());
        let row = |(i, s): (usize, &String)| {
            let on = i + 1 == active;
            let region = regions.iter().find(|r| r.start == i);
            let shut = region.is_some() && self.folded.contains(s);
            let arrow = div()
                .w(px(1.5 * self.glyph()))
                .flex_none()
                .text_color(rgb(DIM))
                .cursor_pointer()
                .children(region.map(|_| if shut { "\u{25b8}" } else { "\u{25be}" }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.fold(i, None);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                );
            let number = div()
                .w(px(3.5 * self.glyph()))
                .flex_none()
                .text_color(rgb(match on {
                    _ if i + 1 == self.bad as usize => RED,
                    true => TEXT,
                    false => GUTTER,
                }))
                .child(format!("{:>3}", i + 1));
            div()
                .relative()
                .h(px(self.row()))
                .flex()
                .items_center()
                .whitespace_nowrap()
                .when(on, |d| d.bg(rgb(PANEL)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                        if !this.mouse {
                            return;
                        }
                        this.blink(cx);
                        this.place(e.position, window);
                        (this.anchor, this.picking) = (this.cursor, e.click_count == 1);
                        this.widen(e.click_count);
                        if e.click_count == 3 {
                            this.glance(i, cx);
                        }
                        this.grasp();
                        let hit = this.tl.clips.iter().filter(|c| c.line as usize == i + 1).map(|c| c.start);
                        if let Some(t) = hit.min_by(f64::total_cmp) {
                            this.seek(t);
                        }
                        cx.notify();
                    }),
                )
                .children(
                    found[found.partition_point(|h| h.0 < i)..]
                        .iter()
                        .take_while(|h| h.0 == i)
                        .map(|h| {
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(px(self.inset() + h.1 as f32 * self.glyph()))
                                .w(px(h.2 as f32 * self.glyph()))
                                .bg(rgba(ORANGE << 8 | 0x48))
                        }),
                )
                .children(span.filter(|(a, b)| a.0 <= i && i <= b.0).map(|(a, b)| {
                    let from = if i == a.0 { a.1 } else { 0 };
                    let to = if i == b.0 { b.1 } else { s.chars().count() + 1 };
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(self.inset() + from as f32 * self.glyph()))
                        .w(px((to - from) as f32 * self.glyph()))
                        .bg(rgba(BLUE << 8 | 0x50))
                }))
                .children(issues.iter().filter(|x| x.row == i).flat_map(|x| {
                    let (from, wide) = (
                        self.inset() + x.span.0 as f32 * self.glyph(),
                        (x.span.1 - x.span.0) as f32 * self.glyph(),
                    );
                    (0..(wide / 3.) as usize).map(move |n| {
                        div()
                            .absolute()
                            .bottom(px(if n % 2 == 0 { 1. } else { 3. }))
                            .left(px(from + n as f32 * 3.))
                            .size(px(2.))
                            .bg(rgb(RED))
                    })
                }))
                .children(self.lined(i).map(|(a, b)| {
                    div()
                        .absolute()
                        .bottom(px(2.))
                        .left(px(self.inset() + a as f32 * self.glyph()))
                        .w(px((b - a) as f32 * self.glyph()))
                        .h(px(1.))
                        .bg(rgb(BLUE))
                }))
                .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, window, cx| {
                    let at = Some(this.spot(e.position, window)).filter(|_| this.mouse);
                    if this.hover != at {
                        (this.hover, this.since, this.tipped) = (at, Instant::now(), false);
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .ml(px(self.inset()))
                        .flex_none()
                        .child(StyledText::new(s.clone()).with_highlights(tokens(s))),
                )
                .children(region.filter(|_| shut).map(|r| {
                    let note = div().px_3().flex().items_center().text_color(rgb(DIM)).child(r.note.clone());
                    note.when(r.run, |n| n.absolute().top_0().bottom_0().right_0().bg(rgb(BG)))
                }))
                .children(caret.filter(|c| c.0 == i).map(|c| {
                    div()
                        .absolute()
                        .left(px(self.inset() + c.1 as f32 * self.glyph()))
                        .top(px(self.row() * 0.14))
                        .w(px(2.))
                        .h(px(self.row() * 0.72))
                        .bg(rgb(BLUE))
                }))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left_0()
                        .w(px(self.gutter()))
                        .pl_3()
                        .flex()
                        .items_center()
                        .bg(rgb(if on { PANEL } else { BG }))
                        .child(number)
                        .child(arrow),
                )
        };
        div()
            .w(px(self.side))
            .relative()
            .flex_none()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(BORDER))
            .child(
                tab(name)
                    .gap_2()
                    .children(self.dirty.then(|| div().size(px(7.)).rounded_full().bg(rgb(TEXT))))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(if self.dirty { ORANGE } else { DIM }))
                            .child(state),
                    ),
            )
            .children(self.clash.then(|| {
                let pick = |label: &'static str, mine: bool| {
                    div()
                        .px_2()
                        .rounded(px(3.))
                        .bg(rgb(BAR))
                        .text_color(rgb(TEXT))
                        .cursor_pointer()
                        .child(label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                (this.clash, this.dirty) = (false, mine);
                                match mine {
                                    true => this.store(),
                                    false => this.load(cx),
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                };
                div()
                    .h(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(12.))
                    .child(div().text_color(rgb(ORANGE)).child("changed on disk while you were editing"))
                    .child(div().flex_1())
                    .child(pick("keep mine", true))
                    .child(pick("use disk", false))
            }))
            .children(self.finding.as_ref().map(|q| {
                let hits = found.len();
                let step = |label: &'static str, on: bool| {
                    div()
                        .px_2()
                        .rounded(px(3.))
                        .text_color(rgb(MUTED))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(BAR)).text_color(rgb(TEXT)))
                        .child(label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.hunt(Some(on));
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                };
                let mode = |label: &'static str, i: usize| {
                    div()
                        .px_1()
                        .rounded(px(3.))
                        .text_size(px(12.))
                        .text_color(rgb(if self.modes[i] { TEXT } else { MUTED }))
                        .when(self.modes[i], |d| d.bg(rgba(BLUE << 8 | 0x40)))
                        .cursor_pointer()
                        .child(label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.flip(i);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        )
                };
                let bad = self.seeker.is_none() && !q.is_empty();
                div()
                    .h(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .bg(rgb(PANEL))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(if q.is_empty() {
                        div().text_color(rgb(DIM)).child("Search\u{2026}")
                    } else {
                        div().when(bad, |d| d.text_color(rgb(RED))).child(q.clone())
                    })
                    .child(div().w(px(1.)).h(px(14.)).bg(rgb(BLUE)))
                    .child(div().flex_1())
                    .child(mode("Aa", 0))
                    .child(mode("wd", 1))
                    .child(mode(".*", 2))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(DIM))
                            .child(format!("{}/{hits}", (self.hit + 1).min(hits))),
                    )
                    .child(step("\u{2039}", false))
                    .child(step("\u{203a}", true))
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .pt_2()
                    .text_size(px(self.font))
                    .when(self.mouse, |d| d.cursor_text())
                    .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                        let by = e.delta.pixel_delta(px(22.));
                        match by.x.abs() > by.y.abs() {
                            true => this.sway(f32::from(by.x)),
                            false => this.roll(f32::from(by.y) / this.row()),
                        }
                        cx.notify();
                    }))
                    .children(
                        self.lines
                            .iter()
                            .enumerate()
                            .skip(self.top as usize)
                            .filter(|(i, _)| !hidden[*i])
                            .take(rows)
                            .map(row),
                    ),
            )
            .child(self.grip(true, cx))
    }

    fn status(&self, focus: Option<&Clip>, cx: &mut Context<Self>) -> Div {
        let mode = match &self.mode {
            Mode::Command(s) => format!(":{s}"),
            Mode::Search(s) => format!("/{s}"),
            Mode::Visual(_) => "-- VISUAL --".into(),
            Mode::Normal if self.cursor.is_some() => "-- EDIT --".into(),
            Mode::Normal if self.mouse => "-- MOUSE --".into(),
            Mode::Normal => "-- NORMAL --".into(),
        };
        let (color, info) = match focus {
            _ if !self.err.is_empty() => (RED, self.err.clone()),
            _ if !self.note.is_empty() => (TEXT, self.note.clone()),
            Some(c) => (MUTED, format!("{}  line {}", self.code(c, c.start, c.end), c.line)),
            None => (MUTED, String::new()),
        };
        let keys = format!("{}{}", self.count, self.pending.map(String::from).unwrap_or_default());
        div()
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .bg(rgb(BAR))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    (this.mouse, this.mode) = (!this.mouse, Mode::Normal);
                                    cx.notify();
                                }),
                            )
                            .child(mode),
                    )
                    .children((!self.mouse).then(|| div().text_color(rgb(DIM)).child("i or click here for mouse")))
                    .child(div().text_color(rgb(color)).child(info)),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .text_color(rgb(MUTED))
                    .child(keys)
                    .child(if self.looping { "loop" } else { "" })
                    .child(div().text_color(rgb(TEXT)).child(self.timecode())),
            )
    }

    fn launch(&self, cx: &mut Context<Self>) -> Div {
        let button = |label: &'static str| {
            div()
                .h(px(36.))
                .px(px(18.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .cursor_pointer()
                .child(label)
        };
        let recent = self.recent.iter().filter(|p| p.exists()).map(|p| {
            let (path, name) = (p.clone(), p.file_name().unwrap_or_default().to_string_lossy().to_string());
            let dir = p
                .parent()
                .and_then(|d| d.file_name())
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            div()
                .w(px(420.))
                .h(px(30.))
                .px_3()
                .flex()
                .items_center()
                .justify_between()
                .gap_4()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(PANEL)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.open(path.clone());
                        cx.notify();
                    }),
                )
                .child(name)
                .child(div().text_size(px(11.)).text_color(rgb(DIM)).child(dir))
        });
        let recent: Vec<Div> = recent.collect();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(28.))
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Menlo")
            .child(div().text_size(px(22.)).child("origami"))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .text_size(px(14.))
                    .child(
                        button("Create new file")
                            .bg(rgb(BLUE))
                            .text_color(rgb(INK))
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.create(cx))),
                    )
                    .child(
                        button("Docs")
                            .border_1()
                            .border_color(rgb(BORDER))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.open_url(DOCS)),
                    ),
            )
            .children((!recent.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .text_size(px(13.))
                    .child(div().px_3().pb_1().text_size(px(11.)).text_color(rgb(DIM)).child("Recent"))
                    .children(recent)
            }))
            .child(
                div()
                    .absolute()
                    .bottom(px(20.))
                    .text_size(px(12.))
                    .text_color(rgb(DIM))
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.open_url("https://x.com/danielung19"))
                    .child("created by daniel ung"),
            )
    }
}

impl Render for Origami {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.script.as_os_str().is_empty() {
            return self.launch(cx);
        }
        if let Some(x) = self.push.filter(|_| mem::take(&mut self.again)) {
            self.drag_to(x, window);
        }
        self.settle(window);
        let tl = self.tl.clone();
        let focus = self.focused().cloned();
        let dir = self
            .script
            .parent()
            .and_then(|p| p.file_name())
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let help = div().absolute().inset_0().flex().items_center().justify_center().child(
            div()
                .p_4()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(6.))
                .children(HELP.lines().map(|l| div().child(l.to_string()))),
        );
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let rows = ((h - self.head() - 28.) / self.row()).ceil().max(0.) as usize;
        let (w, h) = if self.full {
            (w, h)
        } else {
            (w - self.side - 24., h - self.dock - 154.)
        };
        let (iw, ih) = Preview::size(&tl);
        let (iw, ih) = (iw.max(1) as f32, ih.max(1) as f32);
        let fit = (w / iw).min(h / ih).max(0.);
        let stage = div()
            .relative()
            .flex_1()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.mouse {
                        this.toggle();
                        cx.notify();
                    }
                }),
            )
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .children(self.image.clone().map(|i| img(i).w(px(iw * fit)).h(px(ih * fit))));
        let root = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(if self.full { 0 } else { BG }))
            .text_color(rgb(TEXT))
            .text_size(px(13.))
            .font_family("Menlo")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| this.key(e, window, cx)))
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                (this.cursor, this.anchor) = (None, None);
                this.kept = this.knobbed.take();
                this.offers.1.clear();
                this.shut = mem::take(&mut this.menu);
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| {
                let out = f32::from(e.position.x) < f32::from(window.viewport_size().width) - this.side;
                if out && this.hover.take().is_some() {
                    cx.notify();
                }
                if this.eyeing {
                    this.scan(e.position.x, window);
                    cx.notify();
                } else if let Some(wide) = this.pull {
                    this.resize(wide, e.position.x, e.position.y, window);
                    cx.notify();
                } else if this.turning.is_some() {
                    this.turn(e.position, window);
                    cx.notify();
                } else if this.slip.is_some() {
                    this.slide(e.position);
                    cx.notify();
                } else if this.picking {
                    this.place(e.position, window);
                    cx.notify();
                } else if this.scrubbing || this.drag.is_some() {
                    this.drag_to(e.position.x, window);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.release();
                    cx.notify();
                }),
            );
        if self.full {
            return root.child(stage);
        }
        root.child(
            div()
                .h(px(34.))
                .flex_none()
                .flex()
                .items_center()
                .pl(px(80.))
                .gap_2()
                .bg(rgb(BAR))
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(dir)
                .child(div().text_color(rgb(DIM)).child("origami")),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(tab("Program"))
                        .child(stage)
                        .child(self.timeline(&tl, focus.as_ref(), self.per_px(window), cx)),
                )
                .child(self.script(focus.as_ref(), rows, cx)),
        )
        .child(self.status(focus.as_ref(), cx))
        .children(self.popup(window).map(|list| {
            list.occlude().on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                this.skim(f32::from(e.delta.pixel_delta(px(22.)).y) / this.row());
                cx.notify();
            }))
        }))
        .children(self.knobs(window, cx))
        .children(self.keyed(window, cx))
        .children(self.tip(window).map(|(tip, text)| {
            tip.occlude().cursor_pointer().on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    this.note = "copied".into();
                    cx.notify();
                }),
            )
        }))
        .children(self.peep(window, cx))
        .children(self.help.then_some(help))
    }
}
