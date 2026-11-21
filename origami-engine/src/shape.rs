use crate::{
    lut::spread,
    text::{self, rgba},
};
use origami_lang::{Clip, Kind, Val};

pub fn outline(c: &Clip, i: usize) -> Vec<(f64, f64)> {
    let p: Vec<(f64, f64)> = c.points.iter().map(|(x, y)| (x.at(i), y.at(i))).collect();
    if !c.curve {
        return p;
    }
    let (n, smooth) = (p.len(), |i: usize| {
        let (k, t) = (i / 12, (i % 12) as f64 / 12.);
        let [a, b, d, e] = [0, 1, 2, 3].map(|o| p[(k + p.len() + o - 1) % p.len()]);
        let mix = |a: f64, b: f64, d: f64, e: f64| {
            0.5 * (2. * b + (d - a) * t + (2. * a - 5. * b + 4. * d - e) * t * t + (3. * b - a - 3. * d + e) * t * t * t)
        };
        (mix(a.0, b.0, d.0, e.0), mix(a.1, b.1, d.1, e.1))
    });
    (0..n * 12).map(smooth).collect()
}

pub fn bounds(c: &Clip) -> [f64; 4] {
    let long = |v: &Val| match v {
        Val::Fixed(_) => 1,
        Val::Frames(_, s) => s.len(),
    };
    let frames = c.points.iter().map(|(x, y)| long(x).max(long(y))).max().unwrap_or(1);
    let wide = |b: [f64; 4], p: (f64, f64)| [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)];
    (0..frames)
        .flat_map(|i| outline(c, i))
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], wide)
}

pub fn extent(c: &Clip, (w, h): (f64, f64), k: f64) -> (usize, usize) {
    let [x1, y1, x2, y2] = c.ends;
    let (tw, th) = match c.kind {
        Kind::Path => {
            let [x0, y0, x1, y1] = bounds(c);
            ((x1 - x0) * w + 2., (y1 - y0) * h + 2.)
        }
        Kind::Line => ((x2 - x1).abs() * w + c.size * k, (y2 - y1).abs() * h + c.size * k),
        _ => (c.w as f64 * k, c.h as f64 * k),
    };
    let side = |v: f64| (v * c.scale.max()).ceil().max(1.) as usize;
    (side(tw), side(th))
}

pub fn image(c: &Clip, size: (f64, f64), k: f64, i: usize) -> (Vec<u8>, usize, usize) {
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
    if c.kind == Kind::Path {
        let ([x0, y0, x1, y1], grow) = (bounds(c), c.scale.max());
        let to = |p: &(f64, f64)| {
            (
                (p.0 - (x0 + x1) / 2.) * size.0 * grow + hw,
                (p.1 - (y0 + y1) / 2.) * size.1 * grow + hh,
            )
        };
        let (pts, mut cover) = (outline(c, i).iter().map(to).collect::<Vec<_>>(), vec![0f32; tw * th]);
        for row in 0..th * 4 {
            let y = (row as f64 + 0.5) / 4.;
            let sides = pts.iter().zip(pts.iter().cycle().skip(1)).filter(|(a, b)| (a.1 <= y) != (b.1 <= y));
            let mut xs: Vec<f64> = sides.map(|(a, b)| a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0)).collect();
            xs.sort_by(f64::total_cmp);
            for span in xs.chunks_exact(2) {
                for x in span[0].max(0.) as usize..span[1].ceil().min(tw as f64) as usize {
                    let part = (span[1].min(x as f64 + 1.) - span[0].max(x as f64)).clamp(0., 1.);
                    cover[row / 4 * tw + x] += part as f32 / 4.;
                }
            }
        }
        let px = cover.iter().flat_map(|v| [b, g, r, (a as f64 * *v as f64).round() as u8]);
        return (px.collect(), tw, th);
    }
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
    let mut px = vec![0; tw * th * 4];
    spread(&mut px, tw * 4, |from, rows| {
        for (i, p) in (from / 4..).zip(rows.chunks_exact_mut(4)) {
            p.copy_from_slice(&[b, g, r, (a as f64 * cover(i)).round() as u8]);
        }
    });
    (px, tw, th)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::clip;

    // A rectangle should fill its whole box, with the corners cut away when a radius is set, and
    // should shrink correctly when drawn for the smaller preview.
    #[test]
    fn rect_covers_inside_its_corners() {
        let (px, w, h) = image(&clip("rect", r#","radius":8"#), (200., 100.), 1., 0);
        let at = |x: usize, y: usize| px[(y * w + x) * 4 + 3];
        assert_eq!((w, h, &px[..3]), (40, 20, &[0, 0, 255][..]));
        assert_eq!((at(0, 0), at(20, 0), at(0, 10), at(20, 10), at(39, 19)), (0, 128, 128, 128, 0));
        assert!((1..128).contains(&at(2, 2)) && at(3, 3) == 128);
        let sharp = image(&clip("rect", ""), (200., 100.), 0.5, 0);
        assert_eq!((sharp.1, sharp.2), (20, 10));
        assert!(sharp.0.chunks_exact(4).all(|p| p[3] == 128));
        let sum = |c: &Clip| {
            image(c, (200., 100.), 1., 0)
                .0
                .chunks_exact(4)
                .map(|p| p[3] as f64 / 128.)
                .sum::<f64>()
        };
        let cut = (4. - std::f64::consts::PI) * 64.;
        assert!((sum(&clip("rect", "")) - 800.).abs() < 1e-9 && (sum(&clip("rect", r#","radius":8"#)) - 800. + cut).abs() < 2.);
    }

    // A path fills the inside of its points and nothing outside, and when its points move over time
    // its box is big enough for every frame while each frame draws the shape where it is then.
    #[test]
    fn path_fills_where_its_points_are() {
        let still = clip("path", r#","points":[[0.1,0.2],[0.3,0.2],[0.3,0.6],[0.1,0.6]]"#);
        let (px, w, h) = image(&still, (200., 100.), 1., 0);
        assert_eq!((w, h, px[3], px[(20 * w + 20) * 4 + 3]), (42, 42, 0, 128));
        let moving = clip(
            "path",
            r#","points":[[[1,[0.1,0.5]],0.2],[[1,[0.3,0.7]],0.2],[[1,[0.3,0.7]],0.6],[[1,[0.1,0.5]],0.6]]"#,
        );
        assert_eq!(bounds(&moving), [0.10000000149011612, 0.2, 0.699999988079071, 0.6]);
        let at = |i: usize, x: usize| {
            let (px, w, _) = image(&moving, (200., 100.), 1., i);
            px[(20 * w + x) * 4 + 3]
        };
        assert_eq!((at(0, 20), at(0, 100), at(1, 20), at(1, 100)), (128, 0, 0, 128));
    }

    // A line should be drawn from one end to the other at the right thickness, with nothing drawn away
    // from it.
    #[test]
    fn line_runs_between_its_ends() {
        let c = clip("line", r#","ends":[0.2,0.8,0.7,0.3]"#);
        let (px, w, h) = image(&c, (200., 100.), 1., 0);
        let at = |x: usize, y: usize| px[(y * w + x) * 4 + 3];
        assert_eq!((w, h), (106, 56));
        assert_eq!(
            (at(3, 52), at(102, 3), at(53, 28), at(3, 3), at(102, 52), at(0, 55)),
            (128, 128, 128, 0, 0, 0)
        );
        let flat = clip("line", r#","ends":[0.5,0.5,0.1,0.5]"#);
        let (px, w, h) = image(&flat, (200., 100.), 1., 0);
        assert_eq!((w, h, px[3], px[(3 * w + 40) * 4 + 3]), (86, 6, 0, 128));
    }
}
