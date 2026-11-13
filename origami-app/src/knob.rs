use crate::{
    Origami,
    view::{BLUE, BORDER, DIM, PANEL},
};
use gpui::{Context, Div, MouseButton, MouseDownEvent, Pixels, Point, Window, div, img, prelude::*, px, rgb, rgba};
use origami_lang::Clip;

pub struct Knob {
    pub label: String,
    pub span: (usize, usize),
    pub value: f64,
    pub lo: f64,
    pub hi: f64,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Turn {
    Slide(usize, f32, f64, f64),
    Wheel,
    Point(usize),
}

const PAD: f32 = 150.;
pub const TRACK: f32 = 160.;
const CURVES: [&str; 6] = ["curve", "hueVsSat", "hueVsHue", "hueVsLuma", "lumaVsSat", "satVsSat"];

fn range(call: &str, label: &str, value: f64) -> Option<(f64, f64)> {
    Some(match (call, label) {
        ("saturation", "0") | ("look", "1") | (_, "saturation") => (0., 200.),
        ("exposure", "0") => (-5., 5.),
        ("fadedFilm", "0") => (0., 100.),
        ("temperature" | "tint" | "contrast" | "highlights" | "shadows" | "whites" | "blacks", "0") => (-100., 100.),
        ("sharpen" | "vibrance" | "tintBalance", "0") => (-100., 100.),
        (_, "hue") => (0., 360.),
        ("vignette", "amount") => (-3., 3.),
        (_, "amount") => (0., 1.),
        (_, "luma") => (-1., 1.),
        (_, "midpoint" | "feather" | "range" | "soft" | "denoise") => (0., 100.),
        (_, "roundness" | "temperature" | "tint" | "contrast") => (-100., 100.),
        (_, "blur") => (0., 30.),
        (_, "size") => (8., 300.),
        ("speed", "0") => (0.1, 4.),
        ("volume", "0") => (0., 2.),
        ("scale", "0") => (0.1, 2.),
        ("opacity" | "shown", "0") => (0., 1.),
        ("rotate", "0") => (-360., 360.),
        ("loop", "0") => (1., 10.),
        ("pos", _) => (0., 1.),
        ("fade", _) => (0., 5.),
        ("trim" | "for" | "at", _) => (0., (value * 2.).max(10.)),
        ("shift", _) => (-10., 10.),
        ("curve", "x" | "y") | (_, "x") if CURVES.contains(&call) => (0., 1.),
        (_, "y") if CURVES.contains(&call) => (-1., 1.),
        _ => return None,
    })
}

fn paint(hue: f32, vivid: f32, light: f32) -> [u8; 3] {
    [1., 3., 5.].map(|n| {
        let k = (n + hue) % 6.;
        (255. * light * (1. - vivid.min(1.) * k.min(4. - k).clamp(0., 1.))) as u8
    })
}

pub fn disc() -> Vec<u8> {
    let mut px = vec![0; 300 * 300 * 4];
    for (i, p) in px.chunks_exact_mut(4).enumerate() {
        let (dx, dy) = ((i % 300) as f32 - 149.5, 149.5 - (i / 300) as f32);
        let (far, hue) = (dx.hypot(dy) / 150., dy.atan2(dx).to_degrees().rem_euclid(360.) / 60.);
        p[..3].copy_from_slice(&paint(hue, far, 1.));
        p[3] = (((1. - far) * 150.).clamp(0., 1.) * 255.) as u8;
    }
    px
}

pub fn strip(kind: usize) -> Vec<u8> {
    let mut px = vec![0; 150 * 150 * 4];
    for (i, p) in px.chunks_exact_mut(4).enumerate() {
        let x = (i % 150) as f32 / 149.;
        p[..3].copy_from_slice(&match kind {
            0 => paint(x * 6., 1., 1.),
            1 => paint(0., 0., x),
            _ => paint(0.5, x, 0.5 + x / 2.),
        });
        p[3] = if i / 150 >= 138 { 255 } else { 70 };
    }
    px
}

fn hsv(rgb: f64) -> (f32, f32, f32) {
    let [r, g, b] = [16, 8, 0].map(|s| (rgb as u32 >> s & 255) as f32 / 255.);
    let (max, d) = (r.max(g).max(b), r.max(g).max(b) - r.min(g).min(b));
    let hue = match () {
        _ if d == 0. => 0.,
        _ if max == r => ((g - b) / d).rem_euclid(6.),
        _ if max == g => (b - r) / d + 2.,
        _ => (r - g) / d + 4.,
    };
    (hue * 60., if max == 0. { 0. } else { d / max }, max)
}

fn pack(hue: f32, vivid: f32, light: f32) -> f64 {
    let [b, g, r] = paint(hue / 60., vivid, light).map(u32::from);
    (r << 16 | g << 8 | b) as f64
}

fn secs(s: &str) -> Option<f64> {
    s.split(':').try_fold(0., |a, x| x.parse::<f64>().ok().map(|x| a * 60. + x))
}

pub fn close(s: &[u8], open: usize) -> Option<usize> {
    let (mut depth, mut quote, mut i) = (0, 0u8, open);
    while i < s.len() {
        match s[i] {
            c if quote != 0 => quote = if c == quote { 0 } else { quote },
            b'"' | b'\'' => quote = s[i],
            b'(' => depth += 1,
            b')' if depth == 1 => return Some(i),
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn scan(line: &str, call: &str, open: usize, end: usize) -> Vec<Knob> {
    let s = line.as_bytes();
    let (mut out, mut i, mut arg, mut key, mut square, mut pair) = (vec![], open + 1, 0, String::new(), 0, 0);
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    while i < end {
        let c = s[i];
        let from = i;
        let mut found = None;
        match c {
            b'(' => i = close(s, i).unwrap_or(end),
            b'[' => square += 1,
            b']' => square -= 1,
            b',' if square == 0 && key.is_empty() => arg += 1,
            b'}' => key.clear(),
            b'"' | b'\'' => {
                i = line[i + 1..end].find(c as char).map_or(end, |n| i + 1 + n);
                let text = &line[from + 1..i.min(end)];
                let named = [("white", 0xffffff), ("black", 0)]
                    .into_iter()
                    .find(|n| n.0 == text && (key == "color" || call == "color"));
                let hex = text
                    .strip_prefix('#')
                    .filter(|h| h.len() == 6)
                    .and_then(|h| u32::from_str_radix(h, 16).ok());
                if let Some(rgb) = hex.or(named.map(|n| n.1)) {
                    out.push(Knob {
                        label: "#".into(),
                        span: (from, i + 1),
                        value: rgb as f64,
                        lo: 0.,
                        hi: 16777215.,
                    });
                }
                found = secs(text).filter(|_| text.contains(':'));
            }
            _ if word(c) && !c.is_ascii_digit() => {
                while i + 1 < end && word(s[i + 1]) {
                    i += 1;
                }
                if line[i + 1..end].trim_start().starts_with(':') {
                    key = line[from..=i].to_string();
                }
            }
            _ if c.is_ascii_digit() || c == b'-' && s.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                while i + 1 < end && (s[i + 1].is_ascii_digit() || s[i + 1] == b'.') {
                    i += 1;
                }
                found = line[from..=i].parse().ok();
            }
            _ => {}
        }
        if let Some(value) = found {
            let label = match () {
                _ if square > 0 => ["x", "y"][pair % 2].to_string(),
                _ if !key.is_empty() => key.clone(),
                _ => arg.to_string(),
            };
            pair += (square > 0) as usize;
            if let Some((lo, hi)) = range(call, &label, value) {
                out.push(Knob {
                    label,
                    span: (from, i + 1),
                    value,
                    lo,
                    hi,
                });
            }
        }
        i += 1;
    }
    out
}

pub fn calls(line: &str) -> Vec<(String, usize, usize, Vec<Knob>)> {
    let s = line.as_bytes();
    let word = |c: &u8| c.is_ascii_alphanumeric() || *c == b'_';
    let found = (0..s.len()).filter(|i| s[*i] == b'(').filter_map(|open| {
        let name = open - s[..open].iter().rev().take_while(|c| word(c)).count();
        let end = close(s, open)?;
        Some((line[name..open].to_string(), name, end, scan(line, &line[name..open], open, end)))
    });
    found.collect()
}

pub fn knobs(line: &str, at: usize) -> Option<(String, usize, usize, Vec<Knob>)> {
    let on = calls(line).into_iter().filter(|c| c.1 <= at && at <= c.2 && !c.3.is_empty());
    on.max_by_key(|c| c.1)
}

fn show(v: f64, lo: f64, hi: f64) -> String {
    match hi - lo >= 50. {
        true => format!("{}", v.round()),
        false => format!("{:.2}", v).trim_end_matches('0').trim_end_matches('.').to_string(),
    }
}

pub fn rail(label: String, start: f32, text: String) -> Div {
    div()
        .h(px(24.))
        .flex()
        .items_center()
        .cursor_pointer()
        .child(div().w(px(84.)).text_color(rgb(DIM)).child(label))
        .child(
            div()
                .relative()
                .w(px(TRACK))
                .h(px(4.))
                .rounded(px(2.))
                .bg(rgb(BORDER))
                .child(div().h_full().w(px(start * TRACK)).rounded(px(2.)).bg(rgba(BLUE << 8 | 0xa0)))
                .child(
                    div()
                        .absolute()
                        .top(px(-4.))
                        .left(px(start * TRACK - 6.))
                        .size(px(12.))
                        .rounded_full()
                        .bg(rgb(BLUE)),
                ),
        )
        .child(div().w(px(70.)).pl_3().child(text))
}

impl Origami {
    pub fn panel(&self) -> Option<(usize, String, usize, Vec<Knob>)> {
        let (row, at) = self.knobbed?;
        let (call, name, _, mut found) = knobs(self.lines.get(row)?, at)?;
        let linked = |c: &&Clip| {
            let last = (c.line as usize).saturating_sub(1);
            c.total > 0.
                && last >= row
                && self
                    .lines
                    .get(row + 1..=last)
                    .is_some_and(|l| l.iter().all(|l| l.trim_start().starts_with('.')))
        };
        let top = match call.as_str() {
            "trim" => self.tl.clips.iter().find(linked).map(|c| c.total),
            "for" | "at" => Some((self.tl.duration * 2.).max(10.)),
            _ => None,
        };
        for k in found.iter_mut().filter(|k| k.label != "#") {
            k.hi = top.unwrap_or(k.hi);
        }
        Some((row, call, name, found))
    }

    pub fn lined(&self, row: usize) -> Option<(usize, usize)> {
        let line = self.lines.get(row)?;
        let (r, col) = self.hover.filter(|h| h.0 == row)?;
        let at = line.char_indices().nth(col).map_or(line.len(), |x| x.0);
        let (_, name, end, _) = knobs(line, at).filter(|_| r == row)?;
        let cols = |b: usize| line[..b].chars().count();
        Some((cols(name), cols(end) + 1))
    }

    pub fn grasp(&mut self) {
        let Some((row, col)) = self.cursor else { return };
        let line = &self.lines[row];
        let at = line.char_indices().nth(col).map_or(line.len(), |x| x.0);
        self.knobbed = knobs(line, at).map(|k| (row, k.1));
    }

    fn origin(&self, window: &Window) -> Option<(f32, f32, usize, bool)> {
        let (row, call, name, found) = self.panel()?;
        let (hidden, top) = (self.hidden(), self.top as usize);
        if row < top || hidden.get(row) != Some(&false) {
            return None;
        }
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let pad = found.iter().any(|k| k.label == "hue" || k.label == "#") || CURVES.contains(&call.as_str());
        let tall = found.len() as f32 * 24. + 20. + if pad { PAD + 10. } else { 0. };
        let col = self.lines[row][..name].chars().count() as f32;
        let y = self.head() + (top..row).filter(|i| !hidden[*i]).count() as f32 * self.row();
        let x = (w - self.side + self.inset() + col * self.glyph()).min(w - 344.);
        Some((
            x,
            if y + self.row() + tall > h - 28. {
                y - tall
            } else {
                y + self.row()
            },
            found.len(),
            pad,
        ))
    }

    fn set(&mut self, mut values: Vec<(usize, f64)>) {
        let Some((row, call, _, found)) = self.panel() else { return };
        let timed = ["trim", "for", "at"].contains(&call.as_str());
        values.sort_by_key(|v| std::cmp::Reverse(v.0));
        for (i, v) in values {
            let k = &found[i];
            let text = match k.label == "#" {
                true => format!("\"#{:06x}\"", v as u32),
                false => show(v.clamp(k.lo, k.hi), k.lo, if timed { k.lo } else { k.hi }),
            };
            self.lines[row].replace_range(k.span.0..k.span.1, &text);
        }
        self.store();
    }

    pub fn turn(&mut self, at: Point<Pixels>, window: &Window) {
        let (Some(turn), Some((x, y, _, _)), Some((_, _, _, found))) = (self.turning, self.origin(window), self.panel()) else {
            return;
        };
        let (dx, dy) = (f32::from(at.x) - x - 10. - PAD / 2., f32::from(at.y) - y - 10. - PAD / 2.);
        let spot = |label: &str| found.iter().position(|k| k.label == label);
        match turn {
            Turn::Slide(i, x0, v0, _) if found[i].label == "#" => {
                let (hue, vivid, _) = hsv(found[i].value);
                self.set(vec![(
                    i,
                    pack(hue, vivid, (v0 as f32 + (f32::from(at.x) - x0) / TRACK).clamp(0., 1.)),
                )]);
            }
            Turn::Slide(i, x0, v0, wide) => self.set(vec![(i, v0 + ((f32::from(at.x) - x0) / TRACK) as f64 * wide)]),
            Turn::Wheel if spot("#").is_some() => {
                let (i, far) = (spot("#").unwrap_or(0), (dx.hypot(dy) / (PAD / 2.)).min(1.));
                let light = Some(hsv(found[i].value).2).filter(|v| *v > 0.02).unwrap_or(1.);
                self.set(vec![(i, pack((-dy).atan2(dx).to_degrees().rem_euclid(360.), far, light))]);
            }
            Turn::Wheel => {
                let (Some(hue), Some(amount)) = (spot("hue"), spot("amount")) else {
                    return;
                };
                let (angle, far) = ((-dy).atan2(dx).to_degrees().rem_euclid(360.), (dx.hypot(dy) / (PAD / 2.)).min(1.));
                self.set(vec![(hue, angle as f64), (amount, far as f64 * found[amount].hi)]);
            }
            Turn::Point(j) => {
                let (kx, ky) = (&found[j * 2], &found[j * 2 + 1]);
                let (fx, fy) = ((dx / PAD + 0.5).clamp(0., 1.) as f64, (0.5 - dy / PAD).clamp(0., 1.) as f64);
                self.set(vec![
                    (j * 2, kx.lo + fx * (kx.hi - kx.lo)),
                    (j * 2 + 1, ky.lo + fy * (ky.hi - ky.lo)),
                ]);
            }
        }
    }

    fn hold(&mut self, turn: Turn) {
        self.knobbed = self.kept;
        self.past.push(self.lines.clone());
        self.future.clear();
        self.turning = Some(turn);
    }

    pub fn knobs(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let ((x, y, _, pad), (row, call, name, found)) = (self.origin(window)?, self.panel()?);
        let dot = |fx: f32, fy: f32, size: f32| {
            div()
                .absolute()
                .left(px(fx * PAD - size / 2.))
                .top(px((1. - fy) * PAD - size / 2.))
                .size(px(size))
                .rounded_full()
        };
        let part = |k: &Knob| ((k.value - k.lo) / (k.hi - k.lo)).clamp(0., 1.) as f32;
        let curve = CURVES.contains(&call.as_str());
        let mut points: Vec<(f32, f32)> = found.chunks(2).filter(|_| curve).map(|p| (part(&p[0]), part(&p[1]))).collect();
        let marks: Vec<Div> = match curve {
            true => {
                let args = &self.lines[row][name..];
                let ink = match ["\"red\"", "\"green\"", "\"blue\""]
                    .iter()
                    .position(|c| call == "curve" && args.contains(c))
                {
                    Some(i) => [0xd07277, 0xa1c181, BLUE][i],
                    None => 0xdce0e5,
                };
                let back = ["hueVsSat", "hueVsHue", "hueVsLuma", "lumaVsSat", "satVsSat"]
                    .iter()
                    .position(|c| *c == call);
                let back = back.map(|i| {
                    div()
                        .absolute()
                        .inset_0()
                        .child(img(self.strips[i.saturating_sub(2)].clone()).size_full())
                });
                let handle = |p: &(f32, f32)| dot(p.0, p.1, 10.).bg(rgb(ink)).border_1().border_color(rgb(0x1b1f25));
                let handles: Vec<Div> = points.iter().map(handle).collect();
                let rule = |i: usize| {
                    div().absolute().bg(rgba(BORDER << 8 | 0x90)).map(|d| match i < 3 {
                        true => d.left(px(PAD * (i + 1) as f32 / 4.)).top_0().bottom_0().w(px(1.)),
                        false => d.top(px(PAD * (i - 2) as f32 / 4.)).left_0().right_0().h(px(1.)),
                    })
                };
                let grid = (0..6).map(rule);
                points.sort_by(|a, b| a.0.total_cmp(&b.0));
                let line = points.windows(2).flat_map(|w| {
                    (0..=20).map(move |i| {
                        let t = i as f32 / 20.;
                        dot(w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t, 2.).bg(rgb(ink))
                    })
                });
                back.into_iter().chain(grid).chain(line).chain(handles).collect()
            }
            false => {
                let spot = |label: &str| found.iter().find(|k| k.label == label);
                let (hue, far) = match spot("#") {
                    Some(k) => (hsv(k.value).0, hsv(k.value).1),
                    None => (spot("hue").map_or(0., |k| k.value as f32), spot("amount").map_or(0., part)),
                };
                let (fx, fy) = (hue.to_radians().cos() * far * 0.5 + 0.5, hue.to_radians().sin() * far * 0.5 + 0.5);
                let wheel = div().absolute().inset_0().child(img(self.disc.clone()).size_full());
                vec![wheel, dot(fx, fy, 12.).border_2().border_color(rgb(0x1b1f25)).bg(rgb(0xffffff))]
            }
        };
        let board = div()
            .relative()
            .size(px(PAD))
            .mb(px(10.))
            .border_1()
            .border_color(rgb(BORDER))
            .when(!curve, |d| d.rounded_full())
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    let Some(((x, y, _, _), (_, call, _, found))) = this.origin(window).zip(this.panel()) else {
                        return;
                    };
                    let (fx, fy) = (
                        (f32::from(e.position.x) - x - 10.) / PAD,
                        1. - (f32::from(e.position.y) - y - 10.) / PAD,
                    );
                    let part = |k: &Knob| ((k.value - k.lo) / (k.hi - k.lo)) as f32;
                    let near = |p: &[Knob]| (part(&p[0]) - fx).hypot(part(&p[1]) - fy);
                    let nearest = found.chunks(2).enumerate().min_by(|a, b| near(a.1).total_cmp(&near(b.1)));
                    this.hold(match CURVES.contains(&call.as_str()) {
                        true => Turn::Point(nearest.map_or(0, |n| n.0)),
                        false => Turn::Wheel,
                    });
                    this.turn(e.position, window);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .children(marks);
        let rows = found.iter().enumerate().map(|(i, k)| {
            let light = hsv(k.value).2;
            let (value, start, label, text) = match k.label == "#" {
                true => (light as f64, light, "light".into(), format!("#{:06x}", k.value as u32)),
                false => (k.value, part(k), k.label.clone(), show(k.value, k.lo, k.hi)),
            };
            let wide = k.hi - k.lo;
            rail(label, start, text).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.hold(Turn::Slide(i, f32::from(e.position.x), value, wide));
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
        });
        let rows: Vec<Div> = rows.collect();
        Some(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .p(px(10.))
                .flex()
                .flex_col()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(6.))
                .text_size(px(12.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.knobbed = this.kept;
                        cx.stop_propagation();
                    }),
                )
                .children(pad.then_some(board))
                .children(rows),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(line: &str, at: usize) -> Vec<(String, f64)> {
        knobs(line, at).map_or(vec![], |k| k.3.into_iter().map(|k| (k.label, k.value)).collect())
    }

    #[test]
    fn finds() {
        assert_eq!(labels("  clip.temperature(-10),", 12), [("0".into(), -10.)]);
        assert_eq!(labels("a.trim(\"0:10\", 30).speed(1.5)", 4), [("0".into(), 10.), ("1".into(), 30.)]);
        assert_eq!(labels("a.trim(\"0:10\", 30).speed(1.5)", 22), [("0".into(), 1.5)]);
        assert_eq!(
            labels("c.wheel(\"shadows\", { hue: 210, amount: 0.2, luma: -0.1 })", 3),
            [("hue".into(), 210.), ("amount".into(), 0.2), ("luma".into(), -0.1)]
        );
        assert_eq!(labels("c.curve(\"luma\", [[0, 0], [0.5, 0.6]])", 3).len(), 4);
        assert!(labels("video(\"a2.mp4\")", 2).is_empty());
        assert_eq!(labels("color(\"#282c33\").for(5)", 2), [("#".into(), 0x282c33 as f64)]);
        assert_eq!(
            pack(hsv(0x74ade8 as f64).0, hsv(0x74ade8 as f64).1, hsv(0x74ade8 as f64).2) as u32,
            0x74ade8
        );
        assert!(labels("clip.speed(rate)", 7).is_empty());
        assert_eq!(labels("a.opacity(0.5).rotate(-90)", 3), [("0".into(), 0.5)]);
        assert_eq!(labels("a.opacity(0.5).rotate(-90)", 16), [("0".into(), -90.)]);
        let keyed = "  .pos(null, keys([0, 0.36], [0.9, 0.3, \"slowOut\"])).scale(keys([0, 1], [\"100%\", 1.2, bezier(0.3, 0, 0.6, 1)]))";
        assert!((0..keyed.len()).all(|at| labels(keyed, at).is_empty()));
    }
}
