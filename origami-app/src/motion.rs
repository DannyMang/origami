use crate::{
    Origami,
    knob::close,
    stage::{drag, rest},
};
use gpui::{Pixels, Point};
use origami_engine::{place, tone};
use origami_lang::{Clip, Kind, Val};
use std::mem;

pub const EASES: [&str; 9] = [
    "linear",
    "step",
    "slowIn",
    "slowOut",
    "slowInSlowOut",
    "springOut",
    "overshootOut",
    "bounceOut",
    "bezier",
];
pub const ATTRS: [(&str, &str, f64, f64, f64, usize); 10] = [
    ("x", "pos x", 0.001, f64::MIN, f64::MAX, 3),
    ("y", "pos y", 0.001, f64::MIN, f64::MAX, 3),
    ("scale", "scale", 0.01, 0.01, f64::MAX, 2),
    ("rotate", "rotate", 1., f64::MIN, f64::MAX, 0),
    ("opacity", "opacity", 1. / 56., 0., 1., 2),
    ("shown", "shown", 1. / 56., 0., 1., 2),
    ("size", "size", 1., 8., 300., 0),
    ("width", "width", 1., 1., f64::MAX, 0),
    ("height", "height", 1., 1., f64::MAX, 0),
    ("radius", "radius", 1., 0., f64::MAX, 0),
];
pub const STILL: [&str; 4] = ["size", "width", "height", "radius"];
const MAKERS: [(&str, Kind); 7] = [
    ("video", Kind::Video),
    ("image", Kind::Image),
    ("text", Kind::Text),
    ("caption", Kind::Text),
    ("color", Kind::Color),
    ("rect", Kind::Rect),
    ("line", Kind::Line),
];
const SOFT: [f64; 4] = [0.33, 0., 0.67, 1.];

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub time: String,
    pub value: f64,
    pub ease: String,
}

#[derive(Clone, Copy)]
pub enum Pull {
    Key,
    Field(&'static str, f64),
    Bend(bool, f64, f64),
    Zoom(f64),
    Place([Option<f64>; 2], [f64; 2]),
}

#[derive(Clone, Copy)]
pub struct Slip {
    from: Point<Pixels>,
    gain: [f64; 3],
    lo: f64,
    hi: f64,
    pull: Pull,
}

#[derive(PartialEq)]
pub enum Form {
    Plain,
    Keyed(Vec<Key>),
    Code,
}

pub struct Row {
    pub name: &'static str,
    pub form: Form,
    pub now: f64,
}

type Call = (usize, usize, usize);

struct Slot {
    a: usize,
    b: usize,
    pre: String,
    post: &'static str,
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

fn here(keys: &[Key], t: f64, len: f64, fps: f64) -> Option<usize> {
    keys.iter().position(|k| (secs(&k.time, len) - t).abs() < 0.5 / fps)
}

fn without(mut keys: Vec<Key>, i: usize) -> String {
    let k = keys.remove(i);
    if keys.is_empty() { k.value.to_string() } else { print(&keys) }
}

pub fn toggle(old: &str, t: f64, now: f64, len: f64, fps: f64) -> String {
    let Some(mut keys) = parse(old) else {
        return format!("keys([{}, {now}])", round(t, 3));
    };
    match here(&keys, t, len, fps) {
        Some(i) => return without(keys, i),
        None => plant(&mut keys, t, now, len),
    };
    print(&keys)
}

pub fn assign(old: &str, t: f64, v: f64, len: f64, fps: f64) -> String {
    let Some(mut keys) = parse(old) else { return v.to_string() };
    match here(&keys, t, len, fps) {
        Some(i) => keys[i].value = v,
        None => drop(plant(&mut keys, t, v, len)),
    }
    print(&keys)
}

pub fn tidy(name: &str, v: f64) -> f64 {
    let fit = |a: &(&str, &str, f64, f64, f64, usize)| round(v.clamp(a.3, a.4), a.5 as i32);
    ATTRS.iter().find(|a| a.0 == name).map_or(v, fit)
}

pub fn bend(ease: &str) -> Option<[f64; 4]> {
    if ease.is_empty() || ease.trim_matches(['"', '\'']) == "linear" {
        return Some([0.33, 0.33, 0.67, 0.67]);
    }
    let args = split(ease.strip_prefix("bezier(")?.strip_suffix(')')?);
    let v: Option<Vec<f64>> = args.into_iter().map(|x| x.trim().parse().ok()).collect();
    v?.try_into().ok()
}

pub fn handle(a: (f64, f64), b: (f64, f64), v: [f64; 4], lead: bool) -> (f64, f64) {
    let i = if lead { 0 } else { 2 };
    (a.0 + v[i] * (b.0 - a.0), a.1 + v[i + 1] * (b.1 - a.1))
}

pub fn bent(ease: &str, lead: bool, a: (f64, f64), b: (f64, f64), at: (f64, f64)) -> String {
    let (mut v, i) = (bend(ease).unwrap_or(SOFT), if lead { 0 } else { 2 });
    v[i] = round(((at.0 - a.0) / (b.0 - a.0).max(1e-9)).clamp(0., 1.), 2);
    if b.1 != a.1 {
        v[i + 1] = round((at.1 - a.1) / (b.1 - a.1), 2);
    }
    format!("bezier({}, {}, {}, {})", v[0], v[1], v[2], v[3])
}

fn bounds(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let (lo, hi) = values.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    if hi - lo < 1e-9 { (lo - 0.5, hi + 0.5) } else { (lo, hi) }
}

pub fn curve(c: &Clip, name: &str) -> Vec<f32> {
    match c.vals().find(|v| v.0 == name).map(|v| v.1) {
        Some(Val::Frames(_, s)) => s.clone(),
        Some(Val::Fixed(v)) => vec![*v as f32],
        None => vec![],
    }
}

fn op(name: &str) -> (&str, usize) {
    match name {
        "x" | "y" => ("pos", (name == "y") as usize),
        _ => (name, 0),
    }
}

fn chain(s: &str, from: usize, kind: Kind) -> Option<Vec<Call>> {
    let (b, word) = (s.as_bytes(), |c: &u8| c.is_ascii_alphanumeric() || *c == b'_');
    let row = from + s.get(from..)?.find('\n').unwrap_or(s.len() - from);
    let name = |open: usize| open - b[..open].iter().rev().take_while(|c| word(c)).count();
    let made = |open: &usize| b[*open] == b'(' && MAKERS.contains(&(&s[name(*open)..*open], kind));
    let mut opens = (from..row).filter(made);
    let (open, None) = (opens.next()?, opens.next()) else {
        return None;
    };
    let mut out = vec![(name(open), open, close(b, open)?)];
    loop {
        let tail = s[out[out.len() - 1].2 + 1..].trim_start();
        let (at, n) = (
            s.len() - tail.len() + 1,
            tail.strip_prefix('.').map_or(0, |t| t.bytes().take_while(word).count()),
        );
        if n == 0 || b.get(at + n) != Some(&b'(') {
            return Some(out);
        }
        out.push((at, at + n, close(b, at + n)?));
    }
}

fn arg(s: &str, open: usize, end: usize, n: usize) -> Option<(usize, usize)> {
    let args = split(&s[open + 1..end]);
    let (arg, a) = (args.get(n)?, open + 1 + args[..n].iter().map(|s| s.len() + 1).sum::<usize>());
    let a = a + arg.len() - arg.trim_start().len();
    (!arg.trim().is_empty()).then_some((a, a + arg.trim().len()))
}

fn tail(s: &str, a: usize, b: usize) -> (usize, &'static str) {
    let kept = s[a..b].trim_end();
    let gap = match kept {
        "" => "",
        k if k.ends_with(',') => " ",
        _ => ", ",
    };
    (a + kept.len(), gap)
}

fn slot(s: &str, calls: &[Call], name: &str) -> Option<Slot> {
    let word = |c: &Call| &s[c.0..c.1];
    let new = |a, b, pre: String, post| Some(Slot { a, b, pre, post });
    let (maker, last) = (calls[0], calls[calls.len() - 1]);
    if let ("width" | "height", "rect") = (name, word(&maker)) {
        let (a, b) = arg(s, maker.1, maker.2, (name == "height") as usize)?;
        return new(a, b, String::new(), "");
    }
    if let "size" | "color" | "font" | "background" | "radius" | "width" = name {
        let at = match word(&maker) {
            "rect" => 2,
            "line" => 4,
            _ => 1,
        };
        let Some((a, b)) = arg(s, maker.1, maker.2, at) else {
            let (p, gap) = tail(s, maker.1 + 1, maker.2);
            return new(p, p, format!("{gap}{{ {name}: "), " }");
        };
        if !s[a..b].starts_with('{') || !s[a..b].ends_with('}') {
            return None;
        }
        let mut at = a + 1;
        for e in split(&s[a + 1..b - 1]) {
            if let Some((k, v)) = e.split_once(':').filter(|(k, _)| k.trim().trim_matches(['"', '\'']) == name) {
                let from = at + k.len() + 1 + v.len() - v.trim_start().len();
                return new(from, from + v.trim().len(), String::new(), "");
            }
            at += e.len() + 1;
        }
        let (p, gap) = tail(s, a + 1, b - 1);
        let (gap, post) = (if gap.is_empty() { " " } else { gap }, if b - a == 2 { " " } else { "" });
        return new(p, p, format!("{gap}{name}: "), post);
    }
    let (op, n) = op(name);
    let mut found = calls[1..].iter().filter(|c| word(c) == op);
    match (found.next(), found.next()) {
        (Some(_), Some(_)) => None,
        (Some(&(_, open, end)), _) => match arg(s, open, end, n) {
            Some((a, b)) => new(a, b, String::new(), ""),
            None => {
                let (p, gap) = tail(s, open + 1, end);
                new(p, p, if n == 1 && gap.is_empty() { "null, " } else { gap }.into(), "")
            }
        },
        (None, _) => {
            let line = s[..last.0].rfind('\n').map_or(0, |i| i + 1);
            let pad = Some(&s[line..last.0.saturating_sub(1)]).filter(|p| calls.len() > 1 && p.trim().is_empty());
            let (call, post) = match (name, word(&maker)) {
                ("y", _) => ("pos(null, ".into(), ")"),
                ("x", "caption") => ("pos(".into(), ", 0.84)"),
                _ => (format!("{op}("), ")"),
            };
            new(
                last.2 + 1,
                last.2 + 1,
                format!("{}.{call}", pad.map_or(String::new(), |p| format!("\n{p}"))),
                post,
            )
        }
    }
}

fn read(s: &str, from: usize, kind: Kind, name: &str) -> Option<(Slot, bool)> {
    let calls = chain(s, from, kind)?;
    Some((slot(s, &calls, name)?, &s[calls[0].0..calls[0].1] == "caption"))
}

pub fn amend(s: &str, from: usize, kind: Kind, name: &str, f: impl FnOnce(&str) -> String) -> Option<String> {
    let (slot, _) = read(s, from, kind, name)?;
    let old = s[slot.a..slot.b].trim();
    let new = Some(f(old)).filter(|new| new != old)?;
    Some(format!("{}{}{new}{}{}", &s[..slot.a], slot.pre, slot.post, &s[slot.b..]))
}

impl Origami {
    fn offset(&self, c: &Clip) -> Option<usize> {
        let row = (c.home as usize).checked_sub(1).filter(|r| *r < self.lines.len())?;
        Some(self.lines[..row].iter().map(|l| l.len() + 1).sum())
    }

    pub fn frame(&self, c: &Clip) -> f64 {
        ((self.t - c.start).clamp(0., c.end - c.start) * self.fps()).round()
    }

    fn centre(&self, c: &Clip, name: &str) -> f64 {
        let (w, h) = (self.tl.size.0 as f64, self.tl.size.1 as f64);
        let (.., x, y) = place(c, (w, h), 1.);
        round(if name == "x" { x / w } else { y / h }, 3)
    }

    pub fn rows(&self, c: &Clip) -> Vec<Row> {
        let (text, frame) = (self.lines.join("\n"), self.frame(c) as usize);
        let fits = |a: &&(&str, &str, f64, f64, f64, usize)| match (c.kind, a.0) {
            (Kind::Sound, _) => false,
            (Kind::Text, "shown" | "size") | (Kind::Rect, "width" | "height" | "radius") | (Kind::Line, "width") => true,
            (_, name) => !STILL.contains(&name) && name != "shown",
        };
        let row = |&(name, ..): &(&'static str, &str, f64, f64, f64, usize)| {
            let val = c.vals().find(|v| v.0 == name).map(|v| v.1);
            let live = match name {
                "width" if c.kind == Kind::Rect => c.w as f64,
                "size" | "width" => c.size,
                "height" => c.h as f64,
                "radius" => c.radius,
                _ => val.map_or(0., |v| v.at(frame)),
            };
            let site = self.offset(c).and_then(|from| read(&text, from, c.kind, name));
            let (form, now) = match site.as_ref().map(|(s, caption)| (text[s.a..s.b].trim(), *caption)) {
                None => (Form::Code, live),
                Some(("" | "null", caption)) => match (name, val) {
                    (name, _) if STILL.contains(&name) => (Form::Plain, live),
                    ("x" | "y", None) => (Form::Plain, self.centre(c, name)),
                    ("y", Some(Val::Fixed(v))) if caption && *v == 0.84 => (Form::Plain, live),
                    ("rotate", Some(Val::Fixed(v))) if *v == 0. => (Form::Plain, live),
                    ("scale" | "opacity" | "shown", Some(Val::Fixed(v))) if *v == 1. => (Form::Plain, live),
                    _ => (Form::Code, live),
                },
                Some((old, _)) => match (old.parse(), parse(old)) {
                    (Ok(v), _) => (Form::Plain, v),
                    (_, Some(keys)) => (Form::Keyed(keys), live),
                    _ => (Form::Code, live),
                },
            };
            Row { name, form, now }
        };
        ATTRS.iter().filter(fits).map(row).collect()
    }

    pub fn option(&self, c: &Clip, name: &str) -> Option<String> {
        let text = self.lines.join("\n");
        let (s, _) = read(&text, self.offset(c)?, c.kind, name)?;
        let old = text[s.a..s.b].trim();
        let quoted = |q: char| old.len() > 1 && old.starts_with(q) && old.ends_with(q) && old.matches(q).count() == 2;
        match old {
            "" => Some(String::new()),
            _ if quoted('"') || quoted('\'') => Some(old[1..old.len() - 1].into()),
            _ => None,
        }
    }

    pub fn swing(&self, c: &Clip, row: &Row) -> (f64, f64) {
        match &row.form {
            Form::Keyed(keys) if !matches!(row.name, "opacity" | "shown") => {
                bounds(keys.iter().map(|k| k.value).chain(curve(c, row.name).into_iter().map(f64::from)))
            }
            _ => (0., 1.),
        }
    }

    pub fn reach(&self, c: &Clip, rows: &[Row]) -> (f64, f64) {
        let keyed = rows.iter().filter(|r| matches!(r.form, Form::Keyed(_)));
        let ends: Vec<f64> = keyed.flat_map(|r| <[f64; 2]>::from(self.swing(c, r))).collect();
        match self.slip {
            Some(s) => (s.lo, s.hi),
            None if ends.is_empty() => (0., 1.),
            None => bounds(ends.into_iter()),
        }
    }

    fn amend(&mut self, c: &Clip, name: &str, f: impl FnOnce(&str) -> String) {
        let text = self.offset(c).and_then(|from| amend(&self.lines.join("\n"), from, c.kind, name, f));
        if let Some(text) = text {
            self.lines = text.split('\n').map(String::from).collect();
        }
    }

    fn shape(&mut self, c: &Clip, name: &str, f: impl FnOnce(&mut Vec<Key>)) {
        let rekey = |mut keys: Vec<Key>| {
            f(&mut keys);
            print(&keys)
        };
        self.amend(c, name, |old| parse(old).map_or(old.into(), rekey));
    }

    fn held(&self) -> Option<(Clip, &'static str, usize)> {
        let (_, name, i) = self.key.filter(|_| self.motion)?;
        Some((self.opened()?.clone(), name, i))
    }

    pub fn pick(&mut self, z: u32) {
        (self.sel, self.key) = (Some(z).filter(|z| self.sel != Some(*z)), None);
        if let Some(row) = self.opened().map(|c| (c.home as usize).saturating_sub(1)) {
            self.reveal(row);
        }
    }

    pub fn seize(&mut self, from: Point<Pixels>, gain: [f64; 3], pull: Pull) {
        let (lo, hi) = self.opened().map_or((0., 1.), |c| self.reach(c, &self.rows(c)));
        (self.slip, self.base) = (Some(Slip { from, gain, lo, hi, pull }), self.lines.clone());
    }

    fn live(&self, name: &str) -> Option<(Clip, Row)> {
        let c = self.opened()?.clone();
        let row = self.rows(&c).into_iter().find(|r| r.name == name && r.form != Form::Code)?;
        Some((c, row))
    }

    pub fn tug(&mut self, name: &'static str, from: Point<Pixels>) {
        if let (Some((_, row)), Some(a)) = (self.live(name), ATTRS.iter().find(|a| a.0 == name)) {
            self.seize(from, [0., a.2, 0.], Pull::Field(name, row.now));
        }
    }

    pub fn pin(&mut self, name: &'static str) {
        let Some((c, row)) = self.live(name).filter(|_| !STILL.contains(&name)) else {
            return;
        };
        let (len, fps) = (c.end - c.start, self.fps());
        let (t, now) = (
            self.frame(&c) / fps,
            if row.form == Form::Plain { row.now } else { tidy(name, row.now) },
        );
        self.change(|this| this.amend(&c, name, |old| toggle(old, t, now, len, fps)));
        self.key = None;
    }

    pub fn dye(&mut self, name: &'static str) {
        let Some(c) = self.opened().cloned() else { return };
        let new = if name == "color" { "\"#ffffff\"" } else { "\"#00000088\"" };
        match self.option(&c, name).map(|c| (c.is_empty(), tone(&c))) {
            Some((true, _)) => self.change(|this| this.amend(&c, name, |_| new.into())),
            Some((_, Some(_))) => {}
            _ => return,
        }
        let text = self.lines.join("\n");
        let Some((s, _)) = self.offset(&c).and_then(|from| read(&text, from, c.kind, name)) else {
            return;
        };
        let (row, col) = (
            text[..s.a].matches('\n').count(),
            s.a - text[..s.a].rfind('\n').map_or(0, |i| i + 1),
        );
        let board = self.board(row, col).filter(|k| k.3.iter().any(|k| k.label == "#"));
        self.knobbed = board.map(|_| (row, col));
        if self.knobbed.is_some() {
            self.reveal(row);
        }
    }

    pub fn face(&mut self, name: &str) {
        if let Some(c) = self.opened().cloned().filter(|c| self.option(c, "font").is_some()) {
            self.change(|this| this.amend(&c, "font", |_| format!("\"{name}\"")));
        }
    }

    pub fn slide(&mut self, at: Point<Pixels>) {
        let Some(s) = self.slip else { return };
        let (dx, dy) = (f32::from(at.x - s.from.x) as f64, f32::from(at.y - s.from.y) as f64);
        if let Pull::Zoom(from) = s.pull {
            self.zoom = (from.ln() + dx * s.gain[0]).exp();
            return self.magnify(1.);
        }
        let Some(c) = self.opened().cloned() else { return };
        let (len, fps) = (c.end - c.start, self.fps());
        let old = mem::replace(&mut self.lines, self.base.clone());
        let spot = |k: &Key| (secs(&k.time, len), k.value);
        match (s.pull, self.key) {
            (Pull::Field(name, from), _) => {
                let (t, v) = (self.frame(&c) / fps, tidy(name, from + dx * s.gain[1]));
                self.amend(&c, name, |old| assign(old, t, v, len, fps));
            }
            (Pull::Place(from, size), _) if (dx, dy) != (0., 0.) => {
                let to = [0, 1].map(|n| from[n].map(|v| rest(v, [dx, dy][n] * s.gain[n], size[n])));
                if let Some(at) = self.offset(&c) {
                    let text = drag(&self.lines.join("\n"), at, c.kind, to, (self.frame(&c) / fps, len, fps));
                    self.lines = text.split('\n').map(String::from).collect();
                }
            }
            (Pull::Key, Some((_, name, i))) => self.shape(&c, name, |keys| {
                let (was, by) = (keys.get(i).map_or(0., |k| k.value), dx * s.gain[1] + dy * s.gain[2]);
                let v = if by == 0. { was } else { tidy(name, was + by) };
                nudge(keys, i, (dx * s.gain[0] * fps).round(), v, len, fps);
            }),
            (Pull::Bend(lead, t, v), Some((_, name, i))) => self.shape(&c, name, |keys| {
                let j = i + lead as usize;
                if (1..keys.len()).contains(&j) {
                    let to = (t + dx * s.gain[0], v + dy * s.gain[2]);
                    keys[j].ease = bent(&keys[j].ease, lead, spot(&keys[j - 1]), spot(&keys[j]), to);
                }
            }),
            _ => {}
        }
        if self.lines != old {
            self.store();
        }
    }

    pub fn cull(&mut self) -> bool {
        let Some((c, name, i)) = self.held() else {
            return self.motion;
        };
        let cut = |old: &str| parse(old).filter(|k| i < k.len()).map_or(old.into(), |k| without(k, i));
        self.change(|this| this.amend(&c, name, cut));
        self.key = None;
        true
    }

    pub fn ease(&mut self, ease: &str) {
        let Some((c, name, i)) = self.held() else { return };
        let ease = match ease {
            "linear" => String::new(),
            "bezier" => format!("bezier({}, {}, {}, {})", SOFT[0], SOFT[1], SOFT[2], SOFT[3]),
            e => format!("\"{e}\""),
        };
        self.change(|this| {
            this.shape(&c, name, |keys| {
                if let Some(k) = keys.get_mut(i) {
                    k.ease = ease;
                }
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const SRC: &str = "const shot = color(\"#203040\").for(6)
const title = text(\"ORIGAMI\", { size: 180 }).at(0.4).for(4.6)
  .pos(null, keys([0, 0.36], [0.9, 0.3, \"slowOut\"]))
  .opacity(keys([0, 0], [0.9, 1], [-0.6, 1], [\"100%\", 0]))
const tag = text(\"holger\", {
  font: \"Menlo\",
}).for(6).scale(1.5)
const note = caption(\"typed\").for(4)
stack(shot, title.shift(1), tag, note, text(\"inline\").for(2))";

    fn keys(s: &str) -> Vec<Key> {
        parse(s).unwrap()
    }

    fn edit(src: &str, row: usize, kind: Kind, name: &str, f: impl FnOnce(&str) -> String) -> String {
        let from = src.lines().take(row).map(|l| l.len() + 1).sum();
        amend(src, from, kind, name, f).unwrap_or(src.into())
    }

    fn set(src: &str, row: usize, name: &str, new: &str) -> String {
        edit(src, row, Kind::Text, name, |_| new.into())
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
                tidy("opacity", 1.234),
                tidy("rotate", 91.6),
                tidy("scale", -3.),
                tidy("size", 900.),
                tidy("x", -0.12345)
            ),
            (1., 92., 0.01, 300., -0.123)
        );
    }

    #[test]
    fn toggles_a_key_at_the_playhead() {
        assert_eq!(toggle("", 1.5, 0.5, 4., 30.), "keys([1.5, 0.5])");
        assert_eq!(toggle("0.8", 0., 0.8, 4., 30.), "keys([0, 0.8])");
        let two = "keys([0, 0], [\"50%\", 1])";
        assert_eq!(toggle(two, 1., 0.5, 4., 30.), "keys([0, 0], [1, 0.5], [\"50%\", 1])");
        assert_eq!(toggle(two, 2.01, 0.5, 4., 30.), "keys([0, 0])");
        assert_eq!(toggle("keys([0, 0.25])", 0., 9., 4., 30.), "0.25");
        assert_eq!(assign(two, 2., 0.7, 4., 30.), "keys([0, 0], [\"50%\", 0.7])");
        assert_eq!(assign(two, 3.9, 0.2, 4., 30.), "keys([0, 0], [\"50%\", 1], [3.9, 0.2])");
        assert_eq!(assign("null", 3.9, 0.2, 4., 30.), "0.2");
    }

    #[test]
    fn patches_the_chain() {
        let row = |s: &str, n: usize| s.lines().nth(n).unwrap().to_string();
        let on = |row: usize, kind, name: &str, new: &str| edit(SRC, row, kind, name, |_| new.into());
        assert_eq!(
            row(&on(0, Kind::Color, "opacity", "0.5"), 0),
            "const shot = color(\"#203040\").for(6).opacity(0.5)"
        );
        assert_eq!(row(&on(1, Kind::Text, "scale", "2"), 4), "  .scale(2)");
        assert_eq!(
            row(&on(1, Kind::Text, "x", "0.1"), 2),
            "  .pos(0.1, keys([0, 0.36], [0.9, 0.3, \"slowOut\"]))"
        );
        let moved = edit(SRC, 1, Kind::Text, "y", |old| assign(old, 0.9, 0.5, 4.6, 30.));
        assert_eq!(row(&moved, 2), "  .pos(null, keys([0, 0.36], [0.9, 0.5, \"slowOut\"]))");
        assert_eq!(row(&on(4, Kind::Text, "scale", "2"), 6), "}).for(6).scale(2)");
        assert_eq!(row(&on(4, Kind::Text, "y", "0.2"), 6), "}).for(6).scale(1.5).pos(null, 0.2)");
        assert_eq!(
            row(&on(7, Kind::Text, "x", "0.3"), 7),
            "const note = caption(\"typed\").for(4).pos(0.3, 0.84)"
        );
        assert_eq!(
            row(&on(8, Kind::Text, "rotate", "9"), 8),
            "stack(shot, title.shift(1), tag, note, text(\"inline\").for(2).rotate(9))"
        );
        assert_eq!(on(8, Kind::Video, "rotate", "9"), SRC);
        assert_eq!(on(1, Kind::Text, "scale", ""), SRC);
        let twice = "text(\"a\").scale(2).scale(3)";
        assert_eq!(edit(twice, 0, Kind::Text, "scale", |_| "4".into()), twice);
        assert_eq!(set("text(\"a\").pos(0.3)", 0, "y", "0.6"), "text(\"a\").pos(0.3, 0.6)");
        assert_eq!(set("text(\"a\").pos()", 0, "y", "0.6"), "text(\"a\").pos(null, 0.6)");
    }

    #[test]
    fn rewrites_text_options() {
        let row = |s: &str, n: usize| s.lines().nth(n).unwrap().to_string();
        assert_eq!(
            row(&set(SRC, 1, "size", "96"), 1),
            "const title = text(\"ORIGAMI\", { size: 96 }).at(0.4).for(4.6)"
        );
        assert_eq!(
            row(&set(SRC, 1, "color", "\"#ffcc00\""), 1),
            "const title = text(\"ORIGAMI\", { size: 180, color: \"#ffcc00\" }).at(0.4).for(4.6)"
        );
        assert_eq!(row(&set(SRC, 4, "font", "\"Minecraft\""), 5), "  font: \"Minecraft\",");
        assert_eq!(row(&set(SRC, 4, "size", "28"), 5), "  font: \"Menlo\", size: 28");
        assert_eq!(
            row(&set(SRC, 7, "font", "\"Minecraft\""), 7),
            "const note = caption(\"typed\", { font: \"Minecraft\" }).for(4)"
        );
        assert_eq!(set("text(\"a\", {})", 0, "size", "9"), "text(\"a\", { size: 9 })");
        assert_eq!(set("text(\"a\", look)", 0, "size", "9"), "text(\"a\", look)");
        assert_eq!(
            row(&set(SRC, 4, "background", "\"#0008\""), 5),
            "  font: \"Menlo\", background: \"#0008\""
        );
        let wide = "const tag = text(\"a\", {\n  size: 28,\n  color: \"#fff\",\n  background: \"#0008\"\n})\n  .pos(0.1, 0.2)";
        assert_eq!(row(&set(wide, 0, "color", "\"#ffcc0080\""), 2), "  color: \"#ffcc0080\",");
        assert_eq!(row(&set(wide, 0, "background", "\"#102030\""), 3), "  background: \"#102030\"");
        assert_eq!(
            row(&set(wide, 0, "font", "\"Menlo\""), 3),
            "  background: \"#0008\", font: \"Menlo\""
        );
        assert_eq!(row(&set(wide, 0, "y", "0.5"), 5), "  .pos(0.1, 0.5)");
    }

    #[test]
    fn rewrites_shapes() {
        let src =
            "stack(\n  rect(400, 120, { color: \"#0008\" }).for(2),\n  line(0.1, 0.5, 0.9, 0.5),\n  rect(9, 9, {\n    radius: 2,\n  }))";
        let on = |row: usize, kind, name: &str, new: &str| edit(src, row, kind, name, |_| new.into());
        let row = |s: &str, n: usize| s.lines().nth(n).unwrap().to_string();
        assert_eq!(
            row(&on(1, Kind::Rect, "width", "500"), 1),
            "  rect(500, 120, { color: \"#0008\" }).for(2),"
        );
        assert_eq!(
            row(&on(1, Kind::Rect, "height", "90"), 1),
            "  rect(400, 90, { color: \"#0008\" }).for(2),"
        );
        assert_eq!(
            row(&on(1, Kind::Rect, "radius", "16"), 1),
            "  rect(400, 120, { color: \"#0008\", radius: 16 }).for(2),"
        );
        assert_eq!(
            row(&on(1, Kind::Rect, "x", "0.2"), 1),
            "  rect(400, 120, { color: \"#0008\" }).for(2).pos(0.2),"
        );
        assert_eq!(
            row(&on(2, Kind::Line, "width", "8"), 2),
            "  line(0.1, 0.5, 0.9, 0.5, { width: 8 }),"
        );
        assert_eq!(
            row(&on(2, Kind::Line, "color", "\"#ff000080\""), 2),
            "  line(0.1, 0.5, 0.9, 0.5, { color: \"#ff000080\" }),"
        );
        assert_eq!(row(&on(3, Kind::Rect, "radius", "7"), 4), "    radius: 7,");
        assert_eq!(row(&on(3, Kind::Rect, "color", "\"#fff\""), 4), "    radius: 2, color: \"#fff\"");
        assert_eq!(on(2, Kind::Rect, "width", "8"), src);
        assert_eq!((tidy("width", -4.), tidy("radius", -4.), tidy("height", 80.4)), (1., 0., 80.));
    }

    #[test]
    fn handles_round_trip() {
        let (a, b) = ((1., 0.2), (3., 0.6));
        assert_eq!(bend(""), bend("\"linear\""));
        assert_eq!(
            (bend("\"slowOut\""), bend("bezier(0.3, 0, 0.6, 1)")),
            (None, Some([0.3, 0., 0.6, 1.]))
        );
        let lead = handle(a, b, [0.25, 0.5, 0.75, 1.5], true);
        let last = handle(a, b, [0.25, 0.5, 0.75, 1.5], false);
        assert_eq!((lead, last), ((1.5, 0.4), (2.5, 0.8)));
        let ease = bent(&bent("\"slowOut\"", true, a, b, lead), false, a, b, last);
        assert_eq!(ease, "bezier(0.25, 0.5, 0.75, 1.5)");
        assert_eq!(handle(a, b, bend(&ease).unwrap(), false), last);
        assert_eq!(bent("", true, a, b, (9., 0.3)), "bezier(1, 0.25, 0.67, 0.67)");
        assert_eq!(bent("", false, a, (3., 0.2), (0., 5.)), "bezier(0.33, 0.33, 0, 0.67)");
    }

    #[test]
    fn rewrites_still_evaluate() {
        let mut src = SRC.to_string();
        let bend = |old: &str| {
            let ease = bent("", false, (0., 0.36), (0.9, 0.3), (0.6, 0.3));
            print(&[Key {
                ease,
                ..keys(old)[1].clone()
            }])
        };
        let edits: [(usize, &str, &dyn Fn(&str) -> String); 12] = [
            (7, "y", &|_| "0.7".into()),
            (7, "size", &|_| "60".into()),
            (4, "size", &|_| "28".into()),
            (4, "shown", &|old| toggle(old, 0.5, 1., 6., 30.)),
            (1, "opacity", &|old| toggle(old, 2., 0.8, 4.6, 30.)),
            (1, "opacity", &|old| assign(old, 0.9, 0.5, 4.6, 30.)),
            (1, "y", &bend),
            (1, "rotate", &|old| toggle(old, 0., 0., 4.6, 30.)),
            (1, "rotate", &|old| toggle(old, 0., 0., 4.6, 30.)),
            (1, "scale", &|old| toggle(old, 1., 1., 4.6, 30.)),
            (1, "x", &|_| "0.25".into()),
            (1, "color", &|_| "\"#ffcc00\"".into()),
        ];
        for (row, name, f) in edits {
            src = edit(&src, row, Kind::Text, name, f);
        }
        let dir = std::env::temp_dir().join("origami-motion-rewrites");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edit.js"), &src).unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), true, origami_engine::footprint).unwrap();
        let (title, tag, note) = (&tl.clips[1], &tl.clips[2], &tl.clips[3]);
        assert!(src.contains("  .pos(0.25, keys([0.9, 0.3, bezier(0.33, 0.33, 0.67, 1)]))"), "{src}");
        assert!(
            src.contains("  .opacity(keys([0, 0], [0.9, 0.5], [2, 0.8], [-0.6, 1], [\"100%\", 0]))\n  .rotate(0)\n  .scale(keys([1, 1]))")
        );
        assert_eq!(
            (title.home, title.line, title.color.as_str(), title.rotate.clone()),
            (2, 11, "#ffcc00", Val::Fixed(0.))
        );
        assert_eq!(
            (title.pos.clone(), title.opacity.at(27)),
            ((Some(Val::Fixed(0.25)), Some(Val::Fixed(0.3))), 0.5)
        );
        assert_eq!(
            (tag.size, tag.shown.clone(), tag.scale.clone()),
            (28., Val::Fixed(1.), Val::Fixed(1.5))
        );
        assert_eq!((note.size, note.pos.1.clone(), note.home), (60., Some(Val::Fixed(0.7)), 10));
    }
}
