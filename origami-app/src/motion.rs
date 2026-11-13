use crate::{
    Origami,
    keys::link,
    knob::{TRACK, close, rail},
    view::{BAR, BLUE, BORDER, DIM, INK, PANEL, RULER, TEXT, tint},
};
use gpui::{
    App, Bounds, Context, Div, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, Window, canvas, div, point, prelude::*, px,
    relative, rgb, rgba,
};
use origami_lang::{Clip, Kind, Part, Val};

pub const LEAD: f32 = 140.;
const STRIP: f32 = 22.;
const CARD: f32 = 182.;
const EASES: [&str; 8] = [
    "linear",
    "step",
    "slowIn",
    "slowOut",
    "slowInSlowOut",
    "springOut",
    "overshootOut",
    "bounceOut",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub time: String,
    pub value: f64,
    pub ease: String,
}

#[derive(Clone, Copy)]
pub struct Slip {
    from: Point<Pixels>,
    gain: [f64; 3],
    lo: f64,
    hi: f64,
}

pub struct Track {
    name: &'static str,
    label: String,
    row: usize,
    curves: Vec<Vec<f32>>,
    keys: Option<Vec<Key>>,
    lo: f64,
    hi: f64,
}

fn split(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut quote, mut from) = (vec![], 0, None, 0);
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (_, '"' | '\'') => quote = Some(c),
            (_, '(' | '[' | '{') => depth += 1,
            (_, ')' | ']' | '}') => depth -= 1,
            (_, ',') if depth == 0 => {
                out.push(&s[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[from..]);
    out
}

fn share(time: &str) -> Option<f64> {
    time.trim_matches(['"', '\'']).strip_suffix('%')?.parse().ok()
}

pub fn parse(arg: &str) -> Option<Vec<Key>> {
    let inner = arg.trim().strip_prefix("keys(")?.strip_suffix(')')?;
    let key = |k: &str| {
        let f = split(k.trim().strip_prefix('[')?.strip_suffix(']')?);
        let time = f[0].trim();
        Some(Key {
            time: time.parse::<f64>().ok().or(share(time)).map(|_| time.to_string())?,
            value: f.get(1)?.trim().parse().ok()?,
            ease: f.get(2).map_or("", |e| e.trim()).into(),
        })
    };
    split(inner).into_iter().map(key).collect()
}

pub fn print(keys: &[Key]) -> String {
    let key = |k: &Key| format!("[{}, {}{}{}]", k.time, k.value, if k.ease.is_empty() { "" } else { ", " }, k.ease);
    format!("keys({})", keys.iter().map(key).collect::<Vec<_>>().join(", "))
}

pub fn secs(time: &str, len: f64) -> f64 {
    match (share(time), time.parse::<f64>().unwrap_or(0.)) {
        (Some(p), _) => p / 100. * len,
        (_, t) if t < 0. => len + t,
        (_, t) => t,
    }
}

fn round(v: f64, places: i32) -> f64 {
    (v * 10f64.powi(places)).round() / 10f64.powi(places)
}

fn stamp(old: &str, t: f64, len: f64) -> String {
    match share(old) {
        Some(_) => format!("\"{}%\"", round(t / len * 100., 1)),
        None if old.starts_with('-') => round((t - len).min(-0.001), 3).to_string(),
        None => round(t, 3).to_string(),
    }
}

pub fn spot(line: &str, op: &str, n: usize) -> Option<(usize, usize)> {
    let open = line.find(&format!(".{op}("))? + op.len() + 1;
    let args = split(&line[open + 1..close(line.as_bytes(), open)?]);
    let (arg, a) = (args.get(n)?, open + 1 + args[..n].iter().map(|s| s.len() + 1).sum::<usize>());
    let pad = arg.len() - arg.trim_start().len();
    Some((a + pad, a + pad + arg.trim().len()))
}

fn rekey(line: &mut String, (a, b): (usize, usize), f: impl FnOnce(&mut Vec<Key>)) {
    if let Some(mut keys) = parse(&line[a..b]) {
        f(&mut keys);
        line.replace_range(a..b, &print(&keys));
    }
}

pub fn nudge(keys: &mut [Key], i: usize, frames: f64, value: f64, len: f64, fps: f64) {
    let at = |k: Option<&Key>| k.map(|k| secs(&k.time, len));
    let (lo, hi) = (
        at(i.checked_sub(1).and_then(|p| keys.get(p))).map_or(0., |t| t + 1. / fps),
        at(keys.get(i + 1)).map_or(len, |t| t - 1. / fps),
    );
    let Some(k) = keys.get_mut(i) else { return };
    if frames != 0. {
        k.time = stamp(&k.time, (secs(&k.time, len) + frames / fps).min(hi).max(lo), len);
    }
    k.value = value;
}

pub fn plant(keys: &mut Vec<Key>, t: f64, value: f64, len: f64) -> usize {
    let i = keys.iter().position(|k| secs(&k.time, len) > t).unwrap_or(keys.len());
    let (time, ease) = (round(t, 3).to_string(), String::new());
    keys.insert(i, Key { time, value, ease });
    i
}

pub fn grow(old: &str, n: usize, now: f64) -> String {
    let mut args: Vec<String> = split(old).into_iter().map(|a| a.trim().to_string()).collect();
    args.retain(|a| !a.is_empty());
    args.resize(args.len().max(n + 1), "null".into());
    let v = if args[n] == "null" { now.to_string() } else { args[n].clone() };
    args[n] = format!("keys([0, {v}])");
    args.join(", ")
}

fn limit(name: &str, v: f64, lo: f64, hi: f64) -> f64 {
    let v = round(v, if hi - lo >= 50. { 0 } else { 2 });
    if matches!(name, "opacity" | "shown") { v.clamp(0., 1.) } else { v }
}

fn op(name: &str) -> (&str, usize) {
    match name {
        "x" | "y" => ("pos", (name == "y") as usize),
        _ => (name, 0),
    }
}

fn part<'a>(p: &'a Part, name: &str) -> &'a Val {
    match name {
        "opacity" => &p.opacity,
        "x" => &p.x,
        "y" => &p.y,
        "scale" => &p.scale,
        _ => &p.rotate,
    }
}

impl Origami {
    fn site(&self, c: &Clip, name: &str) -> Option<(usize, (usize, usize))> {
        let (op, n) = op(name);
        let row = match c.vals().find(|v| v.0 == name)?.1 {
            Val::Frames(line, _) => (*line as usize).checked_sub(1)?,
            Val::Fixed(_) => link(&self.lines, c.line, op)?,
        };
        Some((row, spot(self.lines.get(row)?, op, n)?))
    }

    pub fn tracks(&self, c: &Clip) -> Vec<Track> {
        let track = |name, label, row, curves, keys| Track {
            name,
            label,
            row,
            curves,
            keys,
            lo: 0.,
            hi: 0.,
        };
        let own = c.vals().filter_map(|(name, val)| {
            let site = self.site(c, name);
            let keys = site.and_then(|(row, (a, b))| parse(&self.lines[row][a..b]));
            let (row, curve) = match val {
                Val::Frames(line, s) => (*line as usize, s.clone()),
                Val::Fixed(v) => (keys.as_ref().and(site)?.0 + 1, vec![*v as f32]),
            };
            let label = if op(name).0 == "pos" { format!("pos {name}") } else { name.into() };
            Some(track(name, label, row, vec![curve], keys))
        });
        let each = ["opacity", "x", "y", "scale", "rotate"].into_iter().filter_map(|name| {
            let moving = c.parts.iter().filter_map(|p| match part(p, name) {
                Val::Frames(line, s) => Some((*line as usize, s.clone())),
                Val::Fixed(_) => None,
            });
            let moving: Vec<_> = moving.collect();
            let row = link(&self.lines, c.line, "each").map_or(moving.first()?.0, |r| r + 1);
            let by = ["char", "word", "line"].into_iter().find(|by| {
                let call = format!(".each(\"{by}\"");
                self.lines.get(row.wrapping_sub(1)).is_some_and(|l| l.contains(&call))
            });
            let label = format!("{name} \u{b7} per {}", by.unwrap_or("part"));
            Some(track("", label, row, moving.into_iter().map(|m| m.1).collect(), None))
        });
        let mut out: Vec<Track> = own.chain(each).collect();
        for t in &mut out {
            let keyed = t.keys.iter().flatten().map(|k| k.value);
            let all = t.curves.iter().flatten().map(|v| *v as f64).chain(keyed);
            let (lo, hi) = all.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
            let held = self.slip.zip(self.key).filter(|(_, k)| k.0 == c.z && k.1 == t.name);
            (t.lo, t.hi) = match held {
                Some((s, _)) => (s.lo, s.hi),
                _ if t.name == "shown" || t.label.starts_with("opacity") => (0., 1.),
                _ if hi - lo < 1e-9 => (lo - 0.5, hi + 0.5),
                _ => (lo, hi),
            };
        }
        out
    }

    pub fn growable(&self, focus: Option<&Clip>) -> Vec<&'static str> {
        let Some(c) = focus.filter(|c| self.motion && c.kind != Kind::Sound) else {
            return vec![];
        };
        let (tracks, text) = (self.tracks(c), c.kind == Kind::Text);
        let all = ["opacity", "scale", "pos x", "pos y", "rotate", "shown"].into_iter();
        let free = |p: &&str| (*p != "shown" || text) && tracks.iter().all(|t| t.name != p.trim_start_matches("pos "));
        all.filter(free).collect()
    }

    pub fn animate(&mut self, label: &str) {
        let Some(c) = self.focused().cloned() else { return };
        let name = label.trim_start_matches("pos ");
        let now = match c.vals().find(|v| v.0 == name) {
            Some((_, Val::Fixed(v))) if !matches!(name, "opacity" | "scale") => *v,
            Some(_) => 1.,
            None => 0.,
        };
        let (op, n) = op(name);
        let was = self.past.len();
        self.patch(c.line, op, |old| grow(old, n, now));
        self.note = match self.past.len() == was {
            true => format!("line {} has no room for {label}, edit the script", c.line),
            false => String::new(),
        };
        self.sel = Some(c.z);
    }

    fn held(&self) -> Option<(Clip, &'static str, usize)> {
        let (z, name, i) = self.key.filter(|_| self.motion)?;
        Some((self.tl.clips.iter().find(|c| c.z == z)?.clone(), name, i))
    }

    fn shape(&mut self, c: &Clip, name: &str, f: impl FnOnce(&mut Vec<Key>)) {
        if let Some((row, span)) = self.site(c, name) {
            rekey(&mut self.lines[row], span, f);
        }
    }

    fn seize(&mut self, key: (u32, &'static str, usize), slip: Slip) {
        (self.key, self.sel, self.slip, self.base) = (Some(key), Some(key.0), Some(slip), self.lines.clone());
    }

    pub fn slide(&mut self, at: Point<Pixels>) {
        let (Some(s), Some((c, name, i))) = (self.slip, self.held()) else {
            return;
        };
        let (dx, dy) = (f32::from(at.x - s.from.x) as f64, f32::from(at.y - s.from.y) as f64);
        let (len, fps) = (c.end - c.start, self.fps());
        let old = std::mem::replace(&mut self.lines, self.base.clone());
        self.shape(&c, name, |keys| {
            let v = keys.get(i).map_or(0., |k| k.value) + dx * s.gain[1] + dy * s.gain[2];
            nudge(keys, i, (dx * s.gain[0] * fps).round(), limit(name, v, s.lo, s.hi), len, fps);
        });
        if self.lines != old {
            self.store();
        }
    }

    fn sow(&mut self, name: &'static str, t: f64) {
        let Some(c) = self.focused().cloned() else { return };
        let Some((_, val)) = c.vals().find(|v| v.0 == name) else {
            return;
        };
        let (len, fps) = (c.end - c.start, self.fps());
        let frame = ((t - c.start).clamp(0., len) * fps).round();
        let mut at = 0;
        self.change(|this| {
            this.shape(&c, name, |keys| {
                at = plant(keys, frame / fps, round(val.at(frame as usize), 2), len)
            })
        });
        (self.key, self.sel) = (Some((c.z, name, at)), Some(c.z));
    }

    pub fn cull(&mut self) -> bool {
        let Some((c, name, i)) = self.held() else {
            return self.motion;
        };
        self.change(|this| {
            this.shape(&c, name, |keys| {
                if keys.len() > 1 && i < keys.len() {
                    keys.remove(i);
                }
            })
        });
        self.key = None;
        true
    }

    fn ease(&mut self, ease: &str) {
        let Some((c, name, i)) = self.held() else { return };
        let ease = if ease == "linear" { String::new() } else { format!("\"{ease}\"") };
        self.change(|this| {
            this.shape(&c, name, |keys| {
                if let Some(k) = keys.get_mut(i) {
                    k.ease = ease;
                }
            })
        });
    }

    fn tall(&self, rows: usize) -> f32 {
        ((self.dock - RULER - STRIP) / rows.max(1) as f32).clamp(26., 44.)
    }

    pub fn curves(&self, focus: Option<&Clip>, cx: &mut Context<Self>) -> Vec<Div> {
        let hint = |s: String| div().pl_3().pt_3().text_size(px(11.)).text_color(rgb(DIM)).child(s);
        let Some(c) = focus else {
            return vec![hint("put the playhead on a clip, or select one in Timeline".into())];
        };
        let (d, pan, fps, z) = (self.span(), self.pan, self.fps(), c.z);
        let (start, end, tracks) = (c.start, c.end, self.tracks(c));
        let tall = self.tall(tracks.len());
        let x = move |t: f64| relative(((t - pan) / d) as f32);
        let span = move || {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(x(start))
                .w(relative(((end - start) / d) as f32))
        };
        let label = |name: String, note: String| {
            div()
                .w(px(LEAD))
                .flex_none()
                .pl_3()
                .flex()
                .flex_col()
                .justify_center()
                .text_size(px(11.))
                .whitespace_nowrap()
                .child(name)
                .child(div().text_color(rgb(DIM)).child(note))
        };
        let name = div()
            .px_1()
            .text_size(px(11.))
            .whitespace_nowrap()
            .child(format!("{}  line {}", c.label, c.line));
        let bar = span()
            .top(px(3.))
            .bottom(px(3.))
            .rounded(px(3.))
            .border_1()
            .border_color(rgb(TEXT))
            .bg(rgba(tint(c.kind) << 8 | 0x55))
            .overflow_hidden()
            .child(name);
        let strip = div()
            .h(px(STRIP))
            .flex_none()
            .flex()
            .pr_3()
            .child(div().w(px(LEAD)).flex_none())
            .child(div().relative().flex_1().overflow_hidden().child(bar));
        let empty = tracks.is_empty().then(|| match c.kind {
            Kind::Sound => hint(format!("{} is sound, it has nothing to animate here", c.label)),
            _ => hint(format!(
                "nothing on {} is animated yet: pick a property under + add, or write .opacity(keys([0, 0], [1, 1])) on line {}",
                c.label, c.line
            )),
        });
        let rows = tracks.into_iter().map(|t| {
            let picked = self.key.filter(|k| k.0 == z && k.1 == t.name && t.keys.is_some()).map(|k| k.2);
            let (lo, hi, name) = (t.lo, t.hi, t.name);
            let y = move |v: f64| 6. + (1. - (v - lo) / (hi - lo)) as f32 * (tall - 13.);
            let spots = t.keys.iter().flatten().map(|k| (start + secs(&k.time, end - start), k.value));
            let spots: Vec<(f64, f64)> = spots.collect();
            let hits = spots.iter().enumerate().map(|(i, &(t, v))| {
                div()
                    .absolute()
                    .left(x(t))
                    .top(px(y(v) - 7.))
                    .ml(px(-7.))
                    .size(px(14.))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            if !this.mouse {
                                return;
                            }
                            let gain = [this.per_px(window), 0., (lo - hi) / (tall - 13.) as f64];
                            this.seek(t);
                            this.seize(
                                (z, name, i),
                                Slip {
                                    from: e.position,
                                    gain,
                                    lo,
                                    hi,
                                },
                            );
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            });
            let hits: Vec<Div> = hits.collect();
            let (curves, keyed) = (t.curves, t.keys.is_some());
            let draw = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
                let at = |t: f64, v: f64| point(b.origin.x + b.size.width * ((t - pan) / d) as f32, b.origin.y + px(y(v)));
                for (n, s) in curves.iter().enumerate().take(12).rev() {
                    let mut line = PathBuilder::stroke(px(1.5));
                    line.move_to(at(start, s[0] as f64));
                    for (i, v) in s.iter().enumerate() {
                        line.line_to(at(start + i as f64 / fps, *v as f64));
                    }
                    line.line_to(at(end, s[s.len() - 1] as f64));
                    if let Ok(line) = line.build() {
                        window.paint_path(line, rgba(BLUE << 8 | if n == 0 { 0xd0 } else { 0x38 }));
                    }
                }
                for (i, &(t, v)) in spots.iter().enumerate() {
                    let ink = if picked == Some(i) { TEXT } else { BLUE };
                    for (r, color) in [(px(6.5), INK), (px(4.5), ink)] {
                        let (p, mut gem) = (at(t, v), PathBuilder::fill());
                        gem.add_polygon(
                            &[point(p.x, p.y - r), point(p.x + r, p.y), point(p.x, p.y + r), point(p.x - r, p.y)],
                            true,
                        );
                        if let Ok(gem) = gem.build() {
                            window.paint_path(gem, rgb(color));
                        }
                    }
                }
            };
            let plot = div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(7.))
                .right(px(7.))
                .child(span().bg(rgba(BAR << 8 | 0x70)))
                .child(canvas(|_, _, _| (), draw).absolute().size_full())
                .children(hits);
            let lane = div()
                .relative()
                .flex_1()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(-7.))
                        .right(px(-7.))
                        .overflow_hidden()
                        .child(plot),
                )
                .when(keyed, |lane| {
                    lane.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            this.key = None;
                            if e.click_count == 2 && this.mouse {
                                this.sow(name, this.at(e.position.x, window));
                            }
                            cx.notify();
                        }),
                    )
                });
            let note = format!("line {}{}", t.row, if keyed { "" } else { " \u{b7} code" });
            div()
                .h(px(tall))
                .flex_none()
                .flex()
                .pr_3()
                .border_t_1()
                .border_color(rgba(BORDER << 8 | 0x80))
                .child(label(t.label, note))
                .child(lane)
        });
        let rows: Vec<Div> = rows.collect();
        [strip].into_iter().chain(empty).chain(rows).collect()
    }

    pub fn keyed(&self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let (c, name, i) = self.held().filter(|h| !self.full && self.focused().is_some_and(|f| f.z == h.0.z))?;
        let tracks = self.tracks(&c);
        let row = tracks.iter().position(|t| t.name == name)?;
        let (t, keys) = (&tracks[row], tracks[row].keys.as_ref()?);
        let (k, len, z) = (keys.get(i)?, c.end - c.start, c.z);
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let at = secs(&k.time, len);
        let x = LEAD + ((c.start + at - self.pan) / self.per_px(window)) as f32 - 24.;
        let y = h - self.dock - 66. - CARD;
        let (lo, hi) = (t.lo, t.hi);
        let slider = |label: &str, part: f64, text: String, gain: [f64; 3]| {
            rail(label.into(), part.clamp(0., 1.) as f32, text).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    let from = e.position;
                    this.seize((z, name, i), Slip { from, gain, lo, hi });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
        };
        let eases = EASES.iter().map(|&e| {
            let on = k.ease.trim_matches(['"', '\'']) == e || k.ease.is_empty() && e == "linear";
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.ease(e);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(e)
        });
        let eases: Vec<Div> = eases.collect();
        Some(
            div()
                .absolute()
                .left(px(x.min(w - self.side - 344.).max(8.)))
                .top(px(y))
                .h(px(CARD))
                .p(px(10.))
                .flex()
                .flex_col()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(6.))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::rewrite;
    use std::{fs, path::Path};

    fn keys(s: &str) -> Vec<Key> {
        parse(s).unwrap()
    }

    #[test]
    fn reads_and_writes_keys() {
        let src = "keys([0, 0], [0.9, 1, \"slowOut\"], [-0.6, 1], [\"100%\", 0, bezier(0.3, 0, 0.6, 1)])";
        let k = keys(src);
        assert_eq!(
            (k.len(), k[1].time.as_str(), k[1].value, k[1].ease.as_str()),
            (4, "0.9", 1., "\"slowOut\"")
        );
        assert_eq!((k[3].time.as_str(), k[3].ease.as_str()), ("\"100%\"", "bezier(0.3, 0, 0.6, 1)"));
        assert_eq!(print(&k), src);
        let at: Vec<f64> = k.iter().map(|k| secs(&k.time, 4.)).collect();
        assert_eq!(at, [0., 0.9, 3.4, 4.]);
        for none in [
            "t => t",
            "fade",
            "keys([0, half])",
            "delay(keys([0, 0]), 1)",
            "keys([a, 1])",
            "keys()",
        ] {
            assert_eq!(parse(none), None, "{none}");
        }
    }

    #[test]
    fn finds_the_argument() {
        let line = "  .pos(null, keys([0, 0.36], [0.9, 0.3, \"slowOut\"])).scale(1.5)";
        let (a, b) = spot(line, "pos", 1).unwrap();
        assert_eq!(&line[a..b], "keys([0, 0.36], [0.9, 0.3, \"slowOut\"])");
        assert_eq!(spot(line, "pos", 0).map(|(a, b)| &line[a..b]), Some("null"));
        assert_eq!(spot(line, "scale", 0).map(|(a, b)| &line[a..b]), Some("1.5"));
        assert_eq!((spot(line, "pos", 2), spot(line, "opacity", 0)), (None, None));
    }

    #[test]
    fn moves_a_key() {
        let mut k = keys("keys([0, 0], [0.9, 1, \"slowOut\"], [-0.6, 1], [\"100%\", 0])");
        nudge(&mut k, 1, 3., 0.5, 4., 30.);
        assert_eq!(print(&k), "keys([0, 0], [1, 0.5, \"slowOut\"], [-0.6, 1], [\"100%\", 0])");
        nudge(&mut k, 1, 900., 0.5, 4., 30.);
        assert_eq!(k[1].time, "3.367");
        nudge(&mut k, 2, 0., 0.25, 4., 30.);
        assert_eq!((k[2].time.as_str(), k[2].value), ("-0.6", 0.25));
        nudge(&mut k, 2, 6., 1., 4., 30.);
        assert_eq!(k[2].time, "-0.4");
        nudge(&mut k, 3, -6., 0., 4., 30.);
        assert_eq!(k[3].time, "\"95%\"");
        nudge(&mut k, 0, -5., 0., 4., 30.);
        assert_eq!(k[0].time, "0");
        nudge(&mut k, 2, 90., 1., 4., 30.);
        assert_eq!(k[2].time, "-0.233");
        assert_eq!(
            (
                limit("opacity", 1.234, 0., 1.),
                limit("rotate", 91.6, 0., 180.),
                limit("scale", 1.234, 1., 2.)
            ),
            (1., 92., 1.23)
        );
    }

    #[test]
    fn adds_and_removes() {
        let mut k = keys("keys([0, 0], [\"50%\", 1], [-0.5, 0])");
        assert_eq!(plant(&mut k, 1., 0.5, 4.), 1);
        assert_eq!(plant(&mut k, 3.9, 0.2, 4.), 4);
        assert_eq!(print(&k), "keys([0, 0], [1, 0.5], [\"50%\", 1], [-0.5, 0], [3.9, 0.2])");
        assert_eq!(grow("", 0, 1.), "keys([0, 1])");
        assert_eq!(grow("0.5", 0, 1.), "keys([0, 0.5])");
        assert_eq!(grow("null, 0.56", 0, 0.), "keys([0, 0]), 0.56");
        assert_eq!(grow("0.3", 1, 0.84), "0.3, keys([0, 0.84])");
        assert_eq!(grow("", 1, 0.84), "null, keys([0, 0.84])");
    }

    #[test]
    fn rewrites_still_evaluate() {
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../demo");
        let Ok(src) = fs::read_to_string(demo.join("motion.js")) else {
            return;
        };
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        let edit = |lines: &mut Vec<String>, row: usize, op: &str, n: usize, f: &dyn Fn(&mut Vec<Key>)| {
            let span = spot(&lines[row], op, n).unwrap();
            rekey(&mut lines[row], span, f);
        };
        edit(&mut lines, 8, "opacity", 0, &|k| nudge(k, 1, 6., 0.8, 4.6, 30.));
        edit(&mut lines, 8, "opacity", 0, &|k| nudge(k, 2, -3., 1., 4.6, 30.));
        edit(&mut lines, 7, "pos", 1, &|k| k[1].ease = "\"bounceOut\"".into());
        edit(&mut lines, 3, "scale", 0, &|k| {
            plant(k, 2., 1.1, 6.);
        });
        edit(&mut lines, 4, "opacity", 0, &|k| drop(k.remove(0)));
        rewrite(&mut lines, 5, "rotate", |old| grow(old, 0, 0.));
        rewrite(&mut lines, 9, "pos", |old| grow(old, 0, 0.));
        rewrite(&mut lines, 12, "pos", |old| grow(old, 1, 0.84));
        rewrite(&mut lines, 16, "pos", |old| grow(old, 1, 0.));
        assert_eq!(lines[8], "  .opacity(keys([0, 0], [1.1, 0.8], [-0.7, 1], [\"100%\", 0]))");
        assert_eq!(lines[7], "  .pos(keys([0, 0]), keys([0, 0.36], [0.9, 0.3, \"bounceOut\"]))");
        assert_eq!(lines[3], "  .scale(keys([0, 1], [2, 1.1], [\"100%\", 1.2, \"slowInSlowOut\"]))");
        assert_eq!(lines[4], "  .opacity(keys([\"100%\", 0])).rotate(keys([0, 0]))");
        assert_eq!(lines[11], "  .shown(keys([0, 0], [1.6, 1])).pos(null, keys([0, 0.84]))");
        assert_eq!(lines[14], "  .pos(null, keys([0, 0.56]))");
        let path = demo.join("motion.rewritten.js");
        fs::write(&path, lines.join("\n")).unwrap();
        let tl = origami_lang::eval(&path, false);
        fs::remove_file(&path).unwrap();
        let tl = tl.unwrap();
        assert!(matches!(tl.clips[1].opacity, Val::Frames(9, _)));
        assert_eq!(
            (tl.clips[0].opacity.clone(), tl.clips[0].rotate.clone()),
            (Val::Fixed(0.), Val::Fixed(0.))
        );
    }
}
