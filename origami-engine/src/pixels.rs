use crate::{fx, lut::spread};
use std::thread;

pub fn spin((x, y): (f64, f64), k: f64, deg: f64) -> (f64, f64) {
    let (s, c) = deg.to_radians().sin_cos();
    (k * (x * c - y * s), k * (x * s + y * c))
}

pub fn blit(
    out: &mut [u8],
    (pw, ph): (usize, usize),
    src: &[u8],
    (sw, sh): (usize, usize),
    k: f64,
    deg: f64,
    (cx, cy): (f64, f64),
    a: f64,
    mul: bool,
) {
    if !(k > 0. && a > 0.) || out.is_empty() {
        return;
    }
    let ((sin, cos), (hw, hh)) = (deg.to_radians().sin_cos(), (sw as f64 / 2., sh as f64 / 2.));
    let (rx, ry) = (
        k * (hw * cos.abs() + hh * sin.abs()) + 1.,
        k * (hw * sin.abs() + hh * cos.abs()) + 1.,
    );
    let span = |c: f64, r: f64, n: usize| ((c - r).clamp(0., n as f64) as usize, (c + r).ceil().clamp(0., n as f64) as usize);
    let ((x0, x1), (y0, y1)) = (span(cx, rx, pw), span(cy, ry, ph));
    let uv = |dx: f64, dy: f64| ((dx * cos + dy * sin) / k + hw - 0.5, (dy * cos - dx * sin) / k + hh - 0.5);
    let cols: Vec<f64> = match sin == 0. {
        true => (x0..x1).map(|x| uv(x as f64 + 0.5 - cx, 0.).0).collect(),
        false => vec![],
    };
    let paint = |from: usize, rows: &mut [u8]| {
        for (y, row) in (y0 + from / (pw * 4)..).zip(rows.chunks_exact_mut(pw * 4)) {
            let dy = y as f64 + 0.5 - cy;
            let flat = uv(0., dy).1;
            for (x, d) in (x0..x1).zip(row[x0 * 4..].chunks_exact_mut(4)) {
                let (u, v) = match cols.get(x - x0) {
                    Some(&u) => (u, flat),
                    None => uv(x as f64 + 0.5 - cx, dy),
                };
                let sum = fx::taps(src, (sw, sh), u, v);
                if sum[3] == 0. {
                    continue;
                }
                for i in 0..3 {
                    let ink = if mul {
                        sum[i] * (d[i] as f64 + 255. - d[3] as f64) / 255.
                    } else {
                        sum[i]
                    };
                    d[i] = (ink * a + d[i] as f64 * (1. - sum[3] * a)) as u8;
                }
                d[3] = (d[3] as f64 + (255. - d[3] as f64) * sum[3] * a) as u8;
            }
        }
    };
    let rows = &mut out[y0 * pw * 4..y1 * pw * 4];
    if (x1 - x0) * (y1 - y0) < 1 << 16 {
        paint(0, rows)
    } else {
        spread(rows, pw * 4, paint)
    }
}

pub fn straight(mut px: Vec<u8>) -> Vec<u8> {
    spread(&mut px, 4, |_, part| {
        for p in part.chunks_exact_mut(4).filter(|p| p[3] > 0 && p[3] < 255) {
            for i in 0..3 {
                p[i] = (p[i] as u32 * 255 / p[3] as u32).min(255) as u8;
            }
        }
    });
    px
}

pub fn nudge(px: &[u8], w: usize, (fx, fy): (f64, f64)) -> Vec<u8> {
    let (mut out, n, cut) = (px.to_vec(), w * 4, |f: f64| (f as usize, (f.fract() * 256.) as u32));
    let ((kx, a), (ky, b), h) = (cut(fx), cut(fy), px.len() / n.max(1));
    if kx + ky == 0 && a + b == 0 {
        return out;
    }
    let mix = |t: u8, u: u8, l: u8, v: u8| {
        let (top, low) = (t as u32 * (256 - a) + u as u32 * a, l as u32 * (256 - a) + v as u32 * a);
        ((top * (256 - b) + low * b) >> 16) as u8
    };
    let band = h.div_ceil(thread::available_parallelism().map_or(1, usize::from)).max(1) * n;
    thread::scope(|s| {
        for (j, rows) in out.chunks_mut(band).enumerate() {
            s.spawn(move || {
                for (y, row) in rows.chunks_exact_mut(n).enumerate() {
                    let [top, low] = [0, 1].map(|d| &px[(j * band / n + y + ky + d).min(h - 1) * n..][..n]);
                    let m = (w - 1).saturating_sub(kx) * 4;
                    let ((head, tail), (i, k)) = (row.split_at_mut(m), (n - 4 - m, n - m));
                    for ((((p, t), u), l), v) in head.iter_mut().zip(&top[i..]).zip(&top[k..]).zip(&low[i..]).zip(&low[k..]) {
                        *p = mix(*t, *u, *l, *v);
                    }
                    for (x, p) in (m / 4..).zip(tail.chunks_exact_mut(4)) {
                        let (i, k) = ((x + kx).min(w - 1) * 4, (x + kx + 1).min(w - 1) * 4);
                        for c in 0..4 {
                            p[c] = mix(top[i + c], top[k + c], low[i + c], low[k + c]);
                        }
                    }
                }
            });
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // A sidecar picture is shifted by the drawn size of one full-size pixel, which is more than a
    // pixel when the clip is drawn larger than its file: whole pixels should be carried and the rest
    // blended, holding the edges, and nothing should move when there is nothing to shift.
    #[test]
    fn nudges_by_any_amount() {
        let px: Vec<u8> = (0..12).flat_map(|i| [i as u8 * 20; 4]).collect();
        let at = |out: &[u8], x: usize, y: usize| out[(y * 4 + x) * 4];
        assert_eq!(nudge(&px, 4, (0., 0.)), px);
        let two = nudge(&px, 4, (2., 0.));
        assert_eq!([0, 1, 2, 3].map(|x| at(&two, x, 0)), [40, 60, 60, 60]);
        let far = nudge(&px, 4, (1.5, 1.));
        assert_eq!([at(&far, 0, 0), at(&far, 2, 1), at(&far, 3, 2)], [110, 220, 220]);
        let tiny = nudge(&px, 4, (0.5, 0.25));
        assert_eq!([at(&tiny, 0, 0), at(&tiny, 3, 2)], [30, 220]);
    }

    // A layer drawn on whole pixels shows its own pixels, blends its half-clear ones, leaves the picture
    // under its clear ones as it was, and multiplies into the picture when blended so.
    #[test]
    fn whole_pixel_layers_keep_their_pixels() {
        let src = [[10, 20, 30, 255], [0, 0, 0, 0], [200, 100, 50, 128], [40, 80, 120, 255]].concat();
        let drawn = |mul: bool| {
            let mut out = [90, 90, 90, 255].repeat(16);
            blit(&mut out, (4, 4), &src, (2, 2), 1., 0., (2., 2.), 1., mul);
            [(0, 0), (1, 1), (2, 1), (1, 2), (2, 2)].map(|(x, y)| out[(y * 4 + x) * 4..][..4].to_vec())
        };
        assert_eq!(
            drawn(false),
            [
                [90, 90, 90, 255],
                [10, 20, 30, 255],
                [90, 90, 90, 255],
                [145, 95, 69, 255],
                [40, 80, 120, 255]
            ]
        );
        assert_eq!(
            drawn(true),
            [
                [90, 90, 90, 255],
                [3, 7, 10, 255],
                [90, 90, 90, 255],
                [80, 62, 53, 255],
                [14, 28, 42, 255]
            ]
        );
    }
}
