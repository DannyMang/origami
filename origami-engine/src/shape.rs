use crate::text::{self, rgba};
use origami_lang::{Clip, Kind};

pub fn extent(c: &Clip, (w, h): (f64, f64), k: f64) -> (usize, usize) {
    let [x1, y1, x2, y2] = c.ends;
    let (tw, th) = match c.kind {
        Kind::Line => ((x2 - x1).abs() * w + c.size * k, (y2 - y1).abs() * h + c.size * k),
        _ => (c.w as f64 * k, c.h as f64 * k),
    };
    let side = |v: f64| (v * c.scale.max()).ceil().max(1.) as usize;
    (side(tw), side(th))
}

pub fn image(c: &Clip, size: (f64, f64), k: f64) -> (Vec<u8>, usize, usize) {
    match c.kind {
        Kind::Text => return text::raster(c, k),
        Kind::Color => return (text::flat(&c.src, (size.0 * size.1) as usize), size.0 as usize, size.1 as usize),
        _ => {}
    }
    let ((tw, th), s, [r, g, b, a]) = (extent(c, size, k), c.scale.max() * k, rgba(&c.color));
    let ((hw, hh), half, [x1, y1, x2, y2]) = ((tw as f64 / 2., th as f64 / 2.), c.size * s / 2., c.ends);
    let (round, (ex, ey)) = (
        (c.radius * s).min(hw).min(hh),
        ((hw - half) * (x2 - x1).signum(), (hh - half) * (y2 - y1).signum()),
    );
    let far = |x: f64, y: f64| match c.kind {
        Kind::Line => {
            let t = ((x * ex + y * ey) / (ex * ex + ey * ey).max(1e-9)).clamp(-1., 1.);
            (x - t * ex).hypot(y - t * ey) - half
        }
        _ => {
            let (px, py) = (x.abs() - hw + round, y.abs() - hh + round);
            px.max(0.).hypot(py.max(0.)) + px.max(py).min(0.) - round
        }
    };
    let cover = |i: usize| (0.5 - far((i % tw) as f64 + 0.5 - hw, (i / tw) as f64 + 0.5 - hh)).clamp(0., 1.);
    let px = (0..tw * th).flat_map(|i| [b, g, r, (a as f64 * cover(i)).round() as u8]);
    (px.collect(), tw, th)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::clip;

    // A rectangle should fill its whole box, with the corners cut away when a radius is set, and
    // should shrink correctly when drawn for the smaller preview.
    #[test]
    fn rect_covers_inside_its_corners() {
        let (px, w, h) = image(&clip("rect", r#","radius":8"#), (200., 100.), 1.);
        let at = |x: usize, y: usize| px[(y * w + x) * 4 + 3];
        assert_eq!((w, h, &px[..3]), (40, 20, &[0, 0, 255][..]));
        assert_eq!((at(0, 0), at(20, 0), at(0, 10), at(20, 10), at(39, 19)), (0, 128, 128, 128, 0));
        assert!((1..128).contains(&at(2, 2)) && at(3, 3) == 128);
        let sharp = image(&clip("rect", ""), (200., 100.), 0.5);
        assert_eq!((sharp.1, sharp.2), (20, 10));
        assert!(sharp.0.chunks_exact(4).all(|p| p[3] == 128));
        let sum = |c: &Clip| {
            image(c, (200., 100.), 1.)
                .0
                .chunks_exact(4)
                .map(|p| p[3] as f64 / 128.)
                .sum::<f64>()
        };
        let cut = (4. - std::f64::consts::PI) * 64.;
        assert!((sum(&clip("rect", "")) - 800.).abs() < 1e-9 && (sum(&clip("rect", r#","radius":8"#)) - 800. + cut).abs() < 2.);
    }

    // A line should be drawn from one end to the other at the right thickness, with nothing drawn away
    // from it.
    #[test]
    fn line_runs_between_its_ends() {
        let c = clip("line", r#","ends":[0.2,0.8,0.7,0.3]"#);
        let (px, w, h) = image(&c, (200., 100.), 1.);
        let at = |x: usize, y: usize| px[(y * w + x) * 4 + 3];
        assert_eq!((w, h), (106, 56));
        assert_eq!(
            (at(3, 52), at(102, 3), at(53, 28), at(3, 3), at(102, 52), at(0, 55)),
            (128, 128, 128, 0, 0, 0)
        );
        let flat = clip("line", r#","ends":[0.5,0.5,0.1,0.5]"#);
        let (px, w, h) = image(&flat, (200., 100.), 1.);
        assert_eq!((w, h, px[3], px[(3 * w + 40) * 4 + 3]), (86, 6, 0, 128));
    }
}
