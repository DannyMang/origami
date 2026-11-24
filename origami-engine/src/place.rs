use crate::{shape, text};
use origami_lang::{Clip, Kind, Val};

fn spot(c: &Clip, (w, h): (f64, f64), (tw, th): (f64, f64)) -> (f64, f64, f64, f64) {
    let (at, m) = (
        |p: &Option<Val>, w, tw, m| p.as_ref().map_or(m * w - tw / 2., |x| x.at(0) * w),
        mid(c),
    );
    let (x, y) = (at(&c.pos.0, w, tw, m.0), at(&c.pos.1, h, th, m.1));
    (tw.round(), th.round(), x.round(), y.round())
}

fn mid(c: &Clip) -> (f64, f64) {
    match (c.kind, c.ends) {
        (Kind::Line, [x1, y1, x2, y2]) => ((x1 + x2) / 2., (y1 + y2) / 2.),
        (Kind::Path, _) => {
            let [x1, y1, x2, y2] = shape::bounds(c);
            ((x1 + x2) / 2., (y1 + y2) / 2.)
        }
        _ => (0.5, 0.5),
    }
}

pub fn grown(c: &Clip, i: usize) -> f64 {
    c.scale.at(i) / if c.kind == Kind::Group { 1. } else { c.scale.max() }
}

pub fn seat(c: &Clip, (w, h): (f64, f64), (tw, th): (f64, f64), i: usize) -> (f64, f64) {
    let (q, m) = (grown(c, i), mid(c));
    let at = |p: &Option<Val>, w: f64, tw: f64, m: f64| p.as_ref().map_or(m * w, |x| x.at(i) * w + tw * q / 2.);
    (at(&c.pos.0, w, tw, m.0), at(&c.pos.1, h, th, m.1))
}

pub fn place(c: &Clip, (w, h): (f64, f64), k: f64) -> (f64, f64, f64, f64) {
    let (bw, bh) = (w * c.scale.max(), h * c.scale.max());
    let r = (bw / c.w as f64).min(bh / c.h as f64);
    match c.kind {
        Kind::Text => {
            let (_, tw, th) = text::glyphs(c, k, false);
            spot(c, (w, h), (tw as f64, th as f64))
        }
        Kind::Rect | Kind::Line | Kind::Path => {
            let (tw, th) = shape::extent(c, (w, h), k);
            spot(c, (w, h), (tw as f64, th as f64))
        }
        _ if c.w > 0 => spot(c, (w, h), (c.w as f64 * r, c.h as f64 * r)),
        _ => (w, h, 0., 0.),
    }
}

pub fn footprint(c: &Clip, size: (f64, f64)) -> (f64, f64) {
    let (w, h, ..) = place(c, size, 1.);
    (w, h)
}

pub fn plain(c: &Clip) -> bool {
    let fixed = |(n, v): (&str, &Val)| n == "opacity" || matches!(v, Val::Fixed(_));
    c.parts.is_empty()
        && c.blend.is_empty()
        && c.step == 0.
        && c.into == 0
        && c.kind != Kind::Group
        && c.matte.is_none()
        && c.fx.is_empty()
        && c.points.iter().all(|p| matches!(p, (Val::Fixed(_), Val::Fixed(_))))
        && c.rotate == Val::Fixed(0.)
        && c.shown == Val::Fixed(1.)
        && c.vals().all(fixed)
}

pub fn live(c: &Clip, t: f64) -> bool {
    c.start <= t + 1e-6 && t + 1e-6 < c.end
}

pub fn alpha(c: &Clip, t: f64) -> f64 {
    let side = |d: f64, f: f64| if f > 0. { d / f } else { 1. };
    side(t - c.start, c.fade.0).min(side(c.end - t, c.fade.1)).clamp(0., 1.)
}
