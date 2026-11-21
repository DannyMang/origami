use crate::{
    Origami,
    motion::{Form, Pull, amend, assign, secs},
    view::{INK, TEXT},
};
use gpui::{Bounds, PathBuilder, Pixels, Point, Window, point, px, quad, rgb, size};
use origami_engine::{place, seat};
use origami_lang::{Clip, Kind, Val};

type Quad = ((f64, f64), (f64, f64), f64);

pub struct Marks {
    corners: [(f64, f64); 4],
    path: Vec<(f64, f64)>,
    keys: Vec<(f64, f64)>,
}

pub fn letterbox(room: Bounds<Pixels>, pad: f32, (iw, ih): (usize, usize)) -> Bounds<Pixels> {
    let (w, h, iw, ih) = (
        f32::from(room.size.width),
        f32::from(room.size.height),
        iw.max(1) as f32,
        ih.max(1) as f32,
    );
    let k = ((w - 2. * pad) / iw).min((h - 2. * pad) / ih).max(0.);
    let at = room.origin + point(px((w - iw * k) / 2.), px((h - ih * k) / 2.));
    Bounds::new(at, size(px(iw * k), px(ih * k)))
}

pub fn share(sheet: Bounds<Pixels>, at: Point<Pixels>) -> (f64, f64) {
    let part = |a: Pixels, from: Pixels, wide: Pixels| (f32::from(a - from) / f32::from(wide).max(1e-6)) as f64;
    (
        part(at.x, sheet.origin.x, sheet.size.width),
        part(at.y, sheet.origin.y, sheet.size.height),
    )
}

fn inside(((cx, cy), (w, h), deg): Quad, (x, y): (f64, f64)) -> bool {
    let ((sin, cos), (dx, dy)) = (deg.to_radians().sin_cos(), (x - cx, y - cy));
    (dx * cos + dy * sin).abs() <= w / 2. && (dy * cos - dx * sin).abs() <= h / 2.
}

pub fn rest(from: f64, by: f64, wide: f64) -> f64 {
    ((from + by).clamp(-wide / 2., 1. - wide / 2.) * 1e3).round() / 1e3
}

pub fn drag(text: &str, from: usize, kind: Kind, to: [Option<f64>; 2], (t, len, fps): (f64, f64, f64)) -> String {
    let mut out = text.to_string();
    for (name, v) in ["x", "y"].into_iter().zip(to) {
        if let Some(new) = v.and_then(|v| amend(&out, from, kind, name, |old| assign(old, t, v, len, fps))) {
            out = new;
        }
    }
    out
}

fn loose(c: &Clip) -> bool {
    c.video
        && match c.kind {
            Kind::Text | Kind::Rect | Kind::Line | Kind::Path => true,
            Kind::Color | Kind::Sound => false,
            _ => c.scale != Val::Fixed(1.) || c.pos != (None, None),
        }
}

pub fn draw(marks: &Marks, sheet: Bounds<Pixels>, window: &mut Window) {
    let to = |&(x, y): &(f64, f64)| sheet.origin + point(sheet.size.width * x as f32, sheet.size.height * y as f32);
    let dot = |window: &mut Window, at: Point<Pixels>, r: f32, rim: f32| {
        let b = Bounds::new(at - point(px(r), px(r)), size(px(2. * r), px(2. * r)));
        window.paint_quad(quad(b, px(r), rgb(TEXT), px(rim), rgb(INK), Default::default()));
    };
    let (mut gap, path) = (0., marks.path.iter().map(to).collect::<Vec<_>>());
    for w in path.windows(2) {
        let (dx, dy) = (f32::from(w[1].x - w[0].x), f32::from(w[1].y - w[0].y));
        let far = dx.hypot(dy);
        while gap < far {
            dot(window, w[0] + point(px(dx * gap / far), px(dy * gap / far)), 1., 0.);
            gap += 6.;
        }
        gap -= far;
    }
    for k in &marks.keys {
        dot(window, to(k), 4., 1.);
    }
    let mut line = PathBuilder::stroke(px(1.));
    line.add_polygon(&marks.corners.each_ref().map(to), true);
    if let Ok(line) = line.build() {
        window.paint_path(line, rgb(TEXT));
    }
}

impl Origami {
    pub fn sheet(&self) -> Bounds<Pixels> {
        let size = (self.tl.size.0 as usize, self.tl.size.1 as usize);
        letterbox(self.stage, if self.full { 0. } else { 12. }, size)
    }

    fn quad(&self, c: &Clip) -> Quad {
        let (size, i) = ((self.tl.size.0 as f64, self.tl.size.1 as f64), self.frame(c) as usize);
        let ((tw, th, ..), q) = (place(c, size, 1.), c.scale.at(i) / c.scale.max());
        (seat(c, size, (tw, th), i), (tw * q, th * q), c.rotate.at(i))
    }

    pub fn turned(&self, c: &Clip) -> impl Fn((f64, f64)) -> (f64, f64) + use<> {
        let ((w, h), ((cx, cy), _, deg)) = ((self.tl.size.0 as f64, self.tl.size.1 as f64), self.quad(c));
        let (sin, cos) = deg.to_radians().sin_cos();
        move |(x, y)| ((cx + x * cos - y * sin) / w, (cy + x * sin + y * cos) / h)
    }

    pub fn hit(&self, at: Point<Pixels>) -> Option<Clip> {
        let (x, y) = share(self.sheet(), at);
        let at = (x * self.tl.size.0 as f64, y * self.tl.size.1 as f64);
        let hit = |c: &&&Clip| loose(c) && inside(self.quad(c), at);
        self.under().iter().rev().find(hit).map(|c| (*c).clone())
    }

    pub fn clasp(&mut self, at: Point<Pixels>) -> bool {
        let (sheet, (w, h)) = (self.sheet(), (self.tl.size.0 as f64, self.tl.size.1 as f64));
        let Some(c) = self.hit(at) else {
            return false;
        };
        if self.sel != Some(c.z) {
            self.pick(c.z);
        }
        let (rows, (_, wide, _)) = (self.rows(&c), self.quad(&c));
        let from = ["x", "y"].map(|n| rows.iter().find(|r| r.name == n && r.form.free()).map(|r| r.now));
        let gain = [sheet.size.width, sheet.size.height].map(|s| 1. / f32::from(s).max(1.) as f64);
        self.seize(at, [gain[0], gain[1], 0.], Pull::Place(from, [wide.0 / w, wide.1 / h]));
        true
    }

    pub fn marks(&self) -> Option<Marks> {
        let c = self.opened().filter(|c| loose(c) && c.start <= self.t && self.t < c.end)?;
        let ((w, h), (len, fps)) = ((self.tl.size.0 as f64, self.tl.size.1 as f64), (c.end - c.start, self.fps()));
        let ((tw, th, ..), (_, (bw, bh), _), turn) = (place(c, (w, h), 1.), self.quad(c), self.turned(c));
        let corner = |(x, y): (f64, f64)| turn((x * bw / 2., y * bh / 2.));
        let at = |i: usize| {
            let (x, y) = seat(c, (w, h), (tw, th), i);
            (x / w, y / h)
        };
        let rows = self.rows(c);
        let keyed = rows.iter().filter(|r| ["x", "y"].contains(&r.name)).filter_map(|r| match &r.form {
            Form::Keyed(keys) => Some(keys),
            _ => None,
        });
        let keys: Vec<(f64, f64)> = keyed.flatten().map(|k| at((secs(&k.time, len) * fps).round() as usize)).collect();
        let frames = if keys.is_empty() { 0 } else { (len * fps).ceil() as usize };
        Some(Marks {
            corners: [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)].map(corner),
            path: (0..frames).map(at).collect(),
            keys,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    // The preview picture is fitted inside its panel with empty bars beside or above it, and a pointer
    // position on screen maps to the matching spot in the picture, even outside it.
    #[test]
    fn pointer_maps_through_the_letterbox() {
        let wide = letterbox(room(0., 68., 1000., 520.), 10., (800, 400));
        assert_eq!(wide, room(10., 83., 980., 490.));
        let tall = letterbox(room(10., 20., 400., 900.), 0., (960, 540));
        assert_eq!(tall, room(10., 357.5, 400., 225.));
        assert_eq!(letterbox(room(0., 0., 10., 10.), 12., (960, 540)).size, size(px(0.), px(0.)));
        let at = |b: Bounds<Pixels>, x: f32, y: f32| share(b, point(px(x), px(y)));
        assert_eq!(
            (at(tall, 10., 357.5), at(tall, 410., 582.5), at(tall, 210., 470.)),
            ((0., 0.), (1., 1.), (0.5, 0.5))
        );
        assert_eq!(at(tall, 110., 301.25), (0.25, -0.25));
        let portrait = letterbox(room(0., 0., 800., 600.), 0., (540, 960));
        assert_eq!((portrait, at(portrait, 400., 150.)), (room(231.25, 0., 337.5, 600.), (0.5, 0.25)));
    }

    // Clicking in the preview should count as hitting a clip only inside its box, including when the
    // box is rotated.
    #[test]
    fn hits_turned_boxes() {
        let flat = ((100., 50.), (80., 20.), 0.);
        assert!(inside(flat, (139., 59.)) && !inside(flat, (141., 50.)) && !inside(flat, (100., 61.)));
        let turned = ((100., 50.), (80., 20.), 90.);
        assert!(inside(turned, (100., 89.)) && inside(turned, (109., 12.)) && !inside(turned, (139., 50.)));
        let tilted = ((0., 0.), (100., 10.), 45.);
        assert!(inside(tilted, (30., 30.)) && !inside(tilted, (30., -30.)));
    }

    // A clip dragged in the preview cannot be dropped completely outside the frame, and its position
    // is rounded to three decimals.
    #[test]
    fn drops_stay_in_the_frame() {
        assert_eq!(
            (rest(0.4, 0.12345, 0.2), rest(0.4, -9., 0.2), rest(0.4, 9., 0.2)),
            (0.523, -0.1, 0.9)
        );
        assert_eq!((rest(0.1, 0., 0.), rest(0.5, 2., 3.)), (0.1, -0.5));
    }

    // Dragging a clip in the preview rewrites only the moved parts of its .pos(...) in the script, as a
    // plain position when it has no keys or a new or updated key at the playhead when it does, and a
    // drag that changes nothing leaves the script alone.
    #[test]
    fn drags_rewrite_pos() {
        let src = "const shot = color(\"#203040\").for(6)
const tag = text(\"holger\", {
  background: \"#0008\",
}).for(6)
const keyed = text(\"k\").for(6)
  .pos(keys([0, 0.11], [3, 0.27]), 0.4)
const both = rect(40, 20).pos(keys([0, 0.1], [\"50%\", 0.2]), keys([0, 0.5]))
const note = caption(\"typed\").pos(null, t => t)
stack(shot, tag, keyed, both, note)";
        let on = |row: usize, kind, to, t| {
            let from = src.lines().take(row).map(|l| l.len() + 1).sum();
            let out = drag(src, from, kind, to, (t, 6., 30.));
            let changed: Vec<String> = out
                .lines()
                .zip(src.lines())
                .filter(|(a, b)| a != b)
                .map(|(a, _)| a.into())
                .collect();
            assert_eq!(out.lines().count(), src.lines().count());
            changed.join("|")
        };
        let to = [Some(0.3), Some(0.62)];
        assert_eq!(on(1, Kind::Text, to, 2.), "}).for(6).pos(0.3, 0.62)");
        assert_eq!(on(4, Kind::Text, to, 3.), "  .pos(keys([0, 0.11], [3, 0.3]), 0.62)");
        assert_eq!(on(4, Kind::Text, to, 1.5), "  .pos(keys([0, 0.11], [1.5, 0.3], [3, 0.27]), 0.62)");
        assert_eq!(
            on(6, Kind::Rect, to, 3.),
            "const both = rect(40, 20).pos(keys([0, 0.1], [\"50%\", 0.3]), keys([0, 0.5], [3, 0.62]))"
        );
        assert_eq!(
            on(7, Kind::Text, [Some(0.3), None], 0.),
            "const note = caption(\"typed\").pos(0.3, t => t)"
        );
        assert_eq!(on(7, Kind::Text, [None, None], 0.), "");
        assert_eq!(on(4, Kind::Text, [Some(0.27), Some(0.4)], 3.), "");
    }
}
