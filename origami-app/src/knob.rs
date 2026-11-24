use crate::{
    Origami, bitmap,
    keys::secs,
    view::{BLUE, BORDER, DIM, GREEN, INK, PANEL, RED, TEXT},
};
use gpui::{Context, Div, MouseButton, MouseDownEvent, Pixels, Point, RenderImage, Window, div, img, prelude::*, px, rgb, rgba};
use origami_engine::tone;
use origami_lang::Clip;
use std::{
    borrow::Cow,
    sync::{Arc, LazyLock},
};

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

static DISC: LazyLock<Arc<RenderImage>> = LazyLock::new(|| bitmap(disc(), 300, 300));
static STRIPS: LazyLock<[Arc<RenderImage>; 3]> = LazyLock::new(|| [0, 1, 2].map(|kind| bitmap(strip(kind), 150, 150)));

const PAD: f32 = 200.;
pub const TRACK: f32 = 160.;
const TYPED: [&str; 2] = ["text", "caption"];
const CURVES: [&str; 6] = ["curve", "hueVsSat", "hueVsHue", "hueVsLuma", "lumaVsSat", "satVsSat"];

fn range(call: &str, label: &str) -> Option<(f64, f64)> {
    Some(match (call, label) {
        ("saturation", "0") | ("look", "1") | (_, "saturation") => (0., 200.),
        ("exposure", "0") => (-5., 5.),
        ("fadedFilm", "0") => (0., 100.),
        ("temperature" | "tint" | "contrast" | "highlights" | "shadows" | "whites" | "blacks", "0") => (-100., 100.),
        ("sharpen" | "vibrance" | "tintBalance", "0") => (-100., 100.),
        (_, "hue") => (0., 360.),
        ("vignette", "amount") => (-3., 3.),
        ("displace", "amount") => (0., 100.),
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
        ("pos", _) => (0., 1.),
        ("trim" | "fade" | "for" | "at" | "shift", _) | ("rect", "0" | "1") | (_, "radius" | "width") => (0., 0.),
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

fn disc() -> Vec<u8> {
    let mut px = vec![0; 300 * 300 * 4];
    for (i, p) in px.chunks_exact_mut(4).enumerate() {
        let (dx, dy) = ((i % 300) as f32 - 149.5, 149.5 - (i / 300) as f32);
        let (far, hue) = (dx.hypot(dy) / 150., dy.atan2(dx).to_degrees().rem_euclid(360.) / 60.);
        p[..3].copy_from_slice(&paint(hue, far, 1.));
        p[3] = (((1. - far) * 150.).clamp(0., 1.) * 255.) as u8;
    }
    px
}

fn strip(kind: usize) -> Vec<u8> {
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

pub fn close(s: &[u8], open: usize) -> Option<usize> {
    let (mut depth, mut quote, mut i) = (0, 0u8, open);
    while i < s.len() {
        match (s[i], s.get(i + 1)) {
            (b'\\', _) if quote != 0 => i += 1,
            (c, _) if quote != 0 => quote = if c == quote { 0 } else { quote },
            (b'/', Some(b'/')) => i = s[i..].iter().position(|c| *c == b'\n').map_or(s.len(), |n| i + n),
            (b'/', Some(b'*')) => i = s[i + 2..].windows(2).position(|w| w == b"*/").map_or(s.len(), |n| i + n + 3),
            (b'"' | b'\'' | b'`', _) => quote = s[i],
            (b'(', _) => depth += 1,
            (b')', _) if depth == 1 => return Some(i),
            (b')', _) => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn ink(rgb: f64, strength: f64) -> String {
    match strength < 1. {
        true => format!("\"#{:06x}{:02x}\"", rgb as u32, (strength.max(0.) * 255.).round() as u32),
        false => format!("\"#{:06x}\"", rgb as u32),
    }
}

fn scan(line: &str, call: &str, from: usize, end: usize) -> Vec<Knob> {
    let s = line.as_bytes();
    let (mut out, mut i, mut arg, mut key, mut square, mut pair) = (vec![], from, 0, String::new(), 0, 0);
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    while i < end {
        let c = s[i];
        let from = i;
        let mut found = None;
        match c {
            b'(' => i = close(&s[..end], i).unwrap_or(end),
            b'[' => square += 1,
            b']' => square -= 1,
            b',' if square == 0 && key.is_empty() => arg += 1,
            b'}' => key.clear(),
            b'"' | b'\'' => {
                i = line[i + 1..end].find(c as char).map_or(end, |n| i + 1 + n);
                let text = &line[from + 1..i.min(end)];
                if let Some([r, g, b, a]) = tone(text).filter(|_| text.starts_with('#')) {
                    let rgb = u32::from_be_bytes([0, r, g, b]) as f64;
                    for (label, value, hi) in [("#", rgb, 16777215.), ("strength", a as f64 / 255., 1.)] {
                        out.push(Knob {
                            label: label.into(),
                            span: (from, i + 1),
                            value,
                            lo: 0.,
                            hi,
                        });
                    }
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
            let label: Cow<str> = match () {
                _ if square > 0 => ["x", "y"][pair % 2].into(),
                _ if !key.is_empty() => key.as_str().into(),
                _ => arg.to_string().into(),
            };
            pair += (square > 0) as usize;
            if let Some((lo, hi)) = range(call, &label) {
                out.push(Knob {
                    label: label.into(),
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

pub fn calls(line: &str) -> impl Iterator<Item = (String, usize, usize, Vec<Knob>)> + '_ {
    let (s, mut fuel) = (line.as_bytes(), 16 * line.len());
    let word = |c: &u8| c.is_ascii_alphanumeric() || *c == b'_';
    (0..s.len()).filter(move |i| s[*i] == b'(').filter_map(move |open| {
        let end = (fuel > 0).then(|| close(s, open)).flatten();
        fuel = fuel.saturating_sub(end.unwrap_or(s.len()) - open);
        let (end, name) = (end?, open - s[..open].iter().rev().take_while(|c| word(c)).count());
        Some((
            line[name..open].to_string(),
            name,
            end,
            scan(line, &line[name..open], open + 1, end),
        ))
    })
}

pub fn knobs(line: &str, at: usize) -> Option<(String, usize, usize, Vec<Knob>)> {
    let on = calls(line).filter(|c| c.1 <= at && at <= c.2 && live(&c.0, &c.3));
    let from = line.len() - line.trim_start().len();
    let loose = || (String::new(), from, line.len().saturating_sub(1), scan(line, "", from, line.len()));
    let (call, name, end, mut found) = on.max_by_key(|c| c.1).unwrap_or_else(loose);
    let dye = found.iter().find(|k| k.label == "#" && k.span.0 <= at && at < k.span.1);
    if let Some(span) = dye.map(|k| k.span) {
        found.retain(|k| !["#", "strength"].contains(&k.label.as_str()) || k.span == span);
    }
    live(&call, &found).then_some((call, name, end, found))
}

fn live(call: &str, found: &[Knob]) -> bool {
    !found.is_empty() || TYPED.contains(&call)
}

fn bound(found: &mut Vec<Knob>, top: Option<f64>) {
    for k in found.iter_mut().filter(|k| k.label != "#") {
        k.hi = top.unwrap_or(k.hi);
    }
    found.retain(|k| k.hi > k.lo);
}

fn limit(call: &str, i: usize, found: &[Knob]) -> (f64, f64) {
    match (call, i, found) {
        ("trim", 0, [a, b]) => (a.lo, a.hi.min(b.value - 0.04)),
        ("trim", 1, [a, b]) => (b.lo.max(a.value + 0.04), b.hi),
        _ => (found[i].lo, found[i].hi),
    }
}

fn print(call: &str, found: &[Knob], i: usize, v: f64) -> String {
    let (k, (lo, hi)) = (&found[i], limit(call, i, found));
    match k.label.as_str() {
        "#" => ink(v, found[i + 1].value),
        "strength" => ink(found[i - 1].value, v.min(1.)),
        _ => show(v.min(hi).max(lo), k.lo, if ["trim", "fade"].contains(&call) { k.lo } else { k.hi }),
    }
}

fn part(k: &Knob) -> f32 {
    ((k.value - k.lo) / (k.hi - k.lo)).clamp(0., 1.) as f32
}

fn graft(line: &str, found: &[Knob], x: f64, y: f64) -> (String, usize) {
    let (j, text) = (
        found.chunks_exact(2).filter(|p| p[0].value < x).count(),
        format!("[{}, {}]", show(x, 0., 1.), show(y, 0., 1.)),
    );
    let mut line = line.to_string();
    match found.get(j * 2) {
        Some(k) => line.insert_str(line[..k.span.0].rfind('[').unwrap_or(k.span.0), &(text + ", ")),
        None => {
            let end = found.last().map_or(0, |k| k.span.1);
            line.insert_str(end + line[end..].find(']').map_or(0, |i| i + 1), &format!(", {text}"));
        }
    }
    (line, j)
}

fn prune(line: &str, found: &[Knob], j: usize) -> String {
    let (a, b) = (found[j * 2].span.0, found[j * 2 + 1].span.1);
    let (mut a, mut b) = (line[..a].rfind('[').unwrap_or(a), b + line[b..].find(']').map_or(0, |i| i + 1));
    match line[b..].starts_with(", ") {
        true => b += 2,
        false if line[..a].ends_with(", ") => a -= 2,
        false => {}
    }
    format!("{}{}", &line[..a], &line[b..])
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
    pub fn owns(&self, c: &Clip, row: usize) -> bool {
        let last = (c.line as usize).saturating_sub(1);
        let chain = self.lines.get(row + 1..=last);
        last >= row && chain.is_some_and(|l| l.iter().all(|l| l.trim_start().starts_with('.')))
    }

    pub fn board(&self, row: usize, at: usize) -> Option<(String, usize, usize, Vec<Knob>)> {
        let (call, name, end, mut found) = knobs(self.lines.get(row)?, at)?;
        let clip = self.tl.clips.iter().find(|c| self.owns(c, row));
        let top = match call.as_str() {
            "trim" => clip.map(|c| c.total),
            "fade" => clip.map(|c| c.end - c.start),
            _ => None,
        };
        bound(&mut found, top);
        live(&call, &found).then_some((call, name, end, found))
    }

    pub fn panel(&self) -> Option<(usize, String, usize, Vec<Knob>)> {
        let (row, at) = self.knobbed?;
        let (call, name, _, found) = self.board(row, at)?;
        Some((row, call, name, found))
    }

    pub fn lined(&self, row: usize) -> Option<(usize, usize)> {
        let line = self.lines.get(row)?;
        let (r, col) = self.hover.filter(|h| h.0 == row)?;
        let at = line.char_indices().nth(col).map_or(line.len(), |x| x.0);
        let (_, name, end, _) = self.board(row, at).filter(|_| r == row)?;
        let cols = |b: usize| line[..b].chars().count();
        Some((cols(name), cols(end) + 1))
    }

    pub fn grasp(&mut self) {
        let Some((row, col)) = self.cursor else { return };
        let Some(line) = self.lines.get(row) else { return };
        let at = line.char_indices().nth(col).map_or(line.len(), |x| x.0);
        self.knobbed = self.board(row, at).map(|k| (row, k.1));
    }

    fn origin(&self, window: &Window) -> Option<(f32, f32, usize, bool)> {
        let (row, call, name, found) = self.panel()?;
        let y = self.line_y(row)?;
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let pad = found.iter().any(|k| k.label == "hue" || k.label == "#") || CURVES.contains(&call.as_str());
        let rows = if CURVES.contains(&call.as_str()) { 1 } else { found.len() } + TYPED.contains(&call.as_str()) as usize;
        let tall = rows as f32 * 24. + 20. + if pad { PAD + 10. } else { 0. };
        let col = self.lines[row][..name].chars().count() as f32;
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

    pub fn roost(&self, window: &Window) -> Option<(usize, f32, f32)> {
        let ((x, y, n, pad), (row, call, ..)) = (self.origin(window)?, self.panel()?);
        let y = y + 11. + n as f32 * 24. + if pad { PAD + 10. } else { 0. };
        TYPED.contains(&call.as_str()).then_some((row, x + 95., y))
    }

    fn set(&mut self, mut values: Vec<(usize, f64)>) {
        let Some((row, call, _, found)) = self.panel() else { return };
        values.sort_by_key(|v| std::cmp::Reverse(v.0));
        for (i, v) in values {
            self.lines[row].replace_range(found[i].span.0..found[i].span.1, &print(&call, &found, i, v));
        }
        self.store();
    }

    pub fn turn(&mut self, at: Point<Pixels>, window: &Window) {
        let (Some(turn), Some((x, y, _, _)), Some((_, _, _, found))) = (self.turning, self.origin(window), self.panel()) else {
            return;
        };
        let (dx, dy) = (f32::from(at.x) - x - 11. - PAD / 2., f32::from(at.y) - y - 11. - PAD / 2.);
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
                let (Some(kx), Some(ky)) = (found.get(j * 2), found.get(j * 2 + 1)) else {
                    return;
                };
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
        let curve = CURVES.contains(&call.as_str());
        let mut points: Vec<(f32, f32)> = found
            .chunks_exact(2)
            .filter(|_| curve)
            .map(|p| (part(&p[0]), part(&p[1])))
            .collect();
        let marks: Vec<Div> = match curve {
            true => {
                let args = &self.lines[row][name..];
                let ink = match ["\"red\"", "\"green\"", "\"blue\""]
                    .iter()
                    .position(|c| call == "curve" && args.contains(c))
                {
                    Some(i) => [RED, GREEN, BLUE][i],
                    None => TEXT,
                };
                let back = ["hueVsSat", "hueVsHue", "hueVsLuma", "lumaVsSat", "satVsSat"]
                    .iter()
                    .position(|c| *c == call);
                let back = back.map(|i| {
                    div()
                        .absolute()
                        .inset_0()
                        .child(img(STRIPS[i.saturating_sub(2)].clone()).size_full())
                });
                let handle = |p: &(f32, f32)| dot(p.0, p.1, 14.).bg(rgb(ink)).border_2().border_color(rgb(INK));
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
                let wheel = div().absolute().inset_0().child(img(DISC.clone()).size_full());
                vec![wheel, dot(fx, fy, 12.).border_2().border_color(rgb(INK)).bg(rgb(0xffffff))]
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
                    this.knobbed = this.kept;
                    let Some(((x, y, _, _), (row, call, _, found))) = this.origin(window).zip(this.panel()) else {
                        return;
                    };
                    let (fx, fy) = (
                        (f32::from(e.position.x) - x - 11.) / PAD,
                        1. - (f32::from(e.position.y) - y - 11.) / PAD,
                    );
                    let near = |p: &[Knob]| (part(&p[0]) - fx).hypot(part(&p[1]) - fy);
                    let nearest = found.chunks_exact(2).enumerate().min_by(|a, b| near(a.1).total_cmp(&near(b.1)));
                    let hit = nearest.filter(|n| near(n.1) < 0.07).map(|n| n.0);
                    let curve = CURVES.contains(&call.as_str());
                    this.hold(Turn::Wheel);
                    match (curve, hit) {
                        (false, _) => {}
                        (_, Some(j)) if e.click_count == 2 => {
                            if found.len() > 4 {
                                this.lines[row] = prune(&this.lines[row], &found, j);
                            }
                            this.turning = None;
                            this.store();
                            return cx.stop_propagation();
                        }
                        (_, Some(j)) => this.turning = Some(Turn::Point(j)),
                        (_, None) => {
                            let at = |k: &Knob, f: f32| k.lo + (f.clamp(0., 1.) as f64) * (k.hi - k.lo);
                            let (line, j) = graft(&this.lines[row], &found, at(&found[0], fx), at(&found[1], fy));
                            (this.lines[row], this.turning) = (line, Some(Turn::Point(j)));
                        }
                    }
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
                false => (k.value.clamp(k.lo, k.hi), part(k), k.label.clone(), show(k.value, k.lo, k.hi)),
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
        let rows: Vec<Div> = match curve {
            true => vec![
                div()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(rgb(DIM))
                    .child("drag a point \u{b7} click to add \u{b7} double-click to remove"),
            ],
            false => rows.collect(),
        };
        let open = move |this: &mut Self, _: &MouseDownEvent, _: &mut Window| {
            this.knobbed = this.kept;
            this.fonting(row);
        };
        let face = TYPED.contains(&call.as_str()).then(|| {
            div()
                .h(px(24.))
                .flex()
                .items_center()
                .child(div().w(px(84.)).text_color(rgb(DIM)).child("font"))
                .child(self.typeface(row).w(px(TRACK)).h(px(18.)))
                .on_mouse_down(MouseButton::Left, Self::tap(cx, open))
        });
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
                .children(rows)
                .children(face),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(line: &str, at: usize) -> Vec<(String, f64)> {
        knobs(line, at).map_or(vec![], |k| k.3.into_iter().map(|k| (k.label, k.value)).collect())
    }

    // Pointing at a value offers the right drag controls (one per number in a call, a colour and strength
    // for colours, points for curves) that write back in the script's own form, while names, keyframes
    // and values that follow another clip offer none.
    #[test]
    fn finds() {
        let tied = "  .pos(title.pos.x, add(title2.pos.y, -0.06)).opacity(mul(title.opacity, 2)).scale(a1.scale)";
        assert!(calls(tied).all(|c| c.3.is_empty()));
        assert_eq!(labels("  clip.temperature(-10),", 12), [("0".into(), -10.)]);
        assert_eq!(labels("a.trim(\"0:10\", 30).speed(1.5)", 4), [("0".into(), 10.), ("1".into(), 30.)]);
        assert_eq!(labels("a.trim(\"0:10\", 30).speed(1.5)", 22), [("0".into(), 1.5)]);
        assert_eq!(
            labels("c.wheel(\"shadows\", { hue: 210, amount: 0.2, luma: -0.1 })", 3),
            [("hue".into(), 210.), ("amount".into(), 0.2), ("luma".into(), -0.1)]
        );
        assert_eq!(labels("c.curve(\"luma\", [[0, 0], [0.5, 0.6]])", 3).len(), 4);
        assert!(labels("video(\"a2.mp4\")", 2).is_empty());
        let line = "c.curve(\"red\", [[0, 0], [0.45, 0.53], [1, 1]])";
        let found = knobs(line, 3).unwrap().3;
        assert_eq!(
            graft(line, &found, 0.2, 0.3),
            ("c.curve(\"red\", [[0, 0], [0.2, 0.3], [0.45, 0.53], [1, 1]])".into(), 1)
        );
        assert_eq!(
            graft(line, &found, 1.5, 0.3).0,
            "c.curve(\"red\", [[0, 0], [0.45, 0.53], [1, 1], [1.5, 0.3]])"
        );
        assert_eq!(prune(line, &found, 1), "c.curve(\"red\", [[0, 0], [1, 1]])");
        assert_eq!(prune(line, &found, 2), "c.curve(\"red\", [[0, 0], [0.45, 0.53]])");
        assert_eq!(
            labels("color(\"#282c33\").for(5)", 2),
            [("#".into(), 0x282c33 as f64), ("strength".into(), 1.)]
        );
        let tag = "text(\"a\", { size: 28, color: \"#ffffff\", background: \"#0008\" })";
        assert_eq!(labels(tag, 2).len(), 5);
        assert_eq!(
            labels(tag, tag.find("\"#0008").unwrap()),
            [("size".into(), 28.), ("#".into(), 0.), ("strength".into(), 136. / 255.)]
        );
        let plate = slides("  background: \"#0008\",", 14, None);
        let set = |i: usize, v: f64| print("", &plate, i, v);
        assert_eq!(
            (set(1, 0.25), set(1, 1.), set(1, -3.)),
            ("\"#00000040\"".into(), "\"#000000\"".into(), "\"#00000000\"".into())
        );
        assert_eq!(
            (set(0, 0xff8000 as f64), print("fade", &slides("a.fade(1, 2)", 3, Some(4.)), 1, 9.)),
            ("\"#ff800088\"".into(), "4".into())
        );
        assert_eq!(labels("  background: \"#0008\",", 14)[1], ("strength".into(), 136. / 255.));
        assert_eq!(labels("  size: 28, font: \"Menlo\"", 3), [("size".into(), 28.)]);
        assert!(labels("  [0, 0.5], [1, 2]", 3).is_empty() && labels("", 0).is_empty() && labels("const n = 5", 3).is_empty());
        assert_eq!(
            (ink(255., 1.), ink(255., 0.5), ink(0., 0.)),
            ("\"#0000ff\"".into(), "\"#0000ff80\"".into(), "\"#00000000\"".into())
        );
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

    fn slides(line: &str, at: usize, top: Option<f64>) -> Vec<Knob> {
        let mut found = knobs(line, at).map_or(vec![], |k| k.3);
        bound(&mut found, top);
        found
    }

    // Drag controls stop at sensible limits, a trim never going past the clip's length or crossing its
    // other end and sliders staying inside their range, and values with no known range get no slider.
    #[test]
    fn bounds() {
        for free in ["a.for(5)", "a.at(2)", "a.shift(-1)", "a.loop(3)", "a.trim(0, 9)", "a.fade(1)"] {
            assert!(slides(free, 3, None).is_empty(), "{free}");
        }
        let trim = slides("a.trim(5, 99)", 3, Some(40.));
        assert_eq!((trim[1].lo, trim[1].hi, part(&trim[1])), (0., 40., 1.));
        assert_eq!(limit("trim", 0, &trim), (0., 40.));
        assert_eq!(limit("trim", 1, &trim), (5.04, 40.));
        assert_eq!(limit("trim", 0, &slides("a.trim(5, 9)", 3, Some(40.))), (0., 8.96));
        let hot = slides("a.temperature(250)", 3, None);
        assert_eq!((limit("temperature", 0, &hot), part(&hot[0])), ((-100., 100.), 1.));
        assert_eq!(part(&slides("a.temperature(-250)", 3, None)[0]), 0.);
        let pushed = slides("a.displace({ amount: 50 })", 3, None);
        assert_eq!((part(&pushed[0]), print("displace", &pushed, 0, 80.)), (0.5, "80".into()));
    }

    // On the curve pad, dragging a point moves it, clicking empty space adds one and double-clicking a
    // point removes it, each rewriting the curve in the script, and a drag let go outside the window
    // ends there as one undo step.
    #[gpui::test]
    fn pad(cx: &mut gpui::TestAppContext) {
        use gpui::{Modifiers, MouseButton, point};
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.script = "/tmp/origami-pad-test.js".into();
            app.lines = vec![
                "stack(".into(),
                "  color(\"#203040\").for(2).curve(\"red\", [[0, 0], [0.45, 0.53], [1, 1]]),".into(),
                ")".into(),
            ];
            app
        });
        let base = view.update_in(cx, |o, window, cx| {
            o.cursor = Some((1, 30));
            o.grasp();
            cx.notify();
            o.origin(window).map(|p| (p.0 + 11., p.1 + 11.))
        });
        let (bx, by) = base.unwrap();
        cx.run_until_parked();
        let at = |fx: f32, fy: f32| point(px(bx + fx * PAD), px(by + (1. - fy) * PAD));
        cx.simulate_mouse_move(at(0.45, 0.53), None, Modifiers::default());
        cx.simulate_mouse_down(at(0.45, 0.53), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(0.6, 0.3), Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(at(0.6, 0.3), MouseButton::Left, Modifiers::default());
        let line = view.update(cx, |o, _| o.lines[1].clone());
        assert!(line.contains("[0.6, 0.3]"), "{line}");
        cx.simulate_mouse_down(at(0.2, 0.8), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(0.2, 0.8), MouseButton::Left, Modifiers::default());
        let line = view.update(cx, |o, _| o.lines[1].clone());
        assert!(line.contains("[[0, 0], [0.2, 0.8], [0.6, 0.3], [1, 1]]"), "{line}");
        let twice = |position| gpui::MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 2,
            first_mouse: false,
        };
        cx.simulate_event(twice(at(0.2, 0.8)));
        cx.simulate_mouse_up(at(0.2, 0.8), MouseButton::Left, Modifiers::default());
        let line = view.update(cx, |o, _| o.lines[1].clone());
        assert!(line.contains("[[0, 0], [0.6, 0.3], [1, 1]]"), "{line}");
        let past = view.update(cx, |o, _| o.past.len());
        cx.simulate_mouse_down(at(0.6, 0.3), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(0.7, 0.4), Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(point(px(-40.), px(-40.)), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(0.9, 0.9), None, Modifiers::default());
        let (line, now) = view.update(cx, |o, _| (o.lines[1].clone(), o.past.len()));
        assert!(line.contains("[[0, 0], [0.7, 0.4], [1, 1]]") && now == past + 1, "{line}");
    }
}
