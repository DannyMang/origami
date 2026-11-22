use crate::lut::spread;
use origami_lang::{Fx, Val};
use std::{f64::consts::FRAC_PI_2, sync::Arc, thread};

fn hash(x: i64, y: i64, seed: i64) -> f64 {
    let mut n = (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
        ^ (seed as u64).wrapping_mul(0x1656_67b1_9e37_79f9);
    n = (n ^ (n >> 31)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n = (n ^ (n >> 29)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((n ^ (n >> 32)) & 0xff_ffff) as f64 / 0xff_ffff as f64
}

fn field(x: f64, y: f64, seed: i64) -> f64 {
    let ((ix, iy), ease) = ((x.floor(), y.floor()), |f: f64| f * f * (3. - 2. * f));
    let ((fx, fy), at) = ((ease(x - ix), ease(y - iy)), |ox, oy| hash(ix as i64 + ox, iy as i64 + oy, seed));
    let (top, low) = (at(0, 0) + (at(1, 0) - at(0, 0)) * fx, at(0, 1) + (at(1, 1) - at(0, 1)) * fx);
    top + (low - top) * fy
}

fn fractal(x: f64, y: f64, seed: i64, layers: i64, gain: f64, ratio: f64) -> f64 {
    let (mut sum, mut a, mut g) = (0., 1., 1.);
    for k in 0..layers {
        sum += a * (field(x * g, y * g, seed * 31 + k) - 0.5);
        (a, g) = (a * gain, g * ratio);
    }
    sum
}

fn sample(px: &[u8], (w, h): (usize, usize), u: f64, v: f64) -> [u8; 4] {
    let ((ux, vy), mut sum) = ((u.floor(), v.floor()), [0.; 4]);
    let (fx, fy) = (u - ux, v - vy);
    for (ox, oy, m) in [
        (0, 0, (1. - fx) * (1. - fy)),
        (1, 0, fx * (1. - fy)),
        (0, 1, (1. - fx) * fy),
        (1, 1, fx * fy),
    ] {
        let (sx, sy) = (ux as i64 + ox, vy as i64 + oy);
        if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
            continue;
        }
        let s = &px[(sy as usize * w + sx as usize) * 4..][..4];
        let m = m * s[3] as f64 / 255.;
        for i in 0..3 {
            sum[i] += s[i] as f64 * m;
        }
        sum[3] += m;
    }
    match sum[3] > 0. {
        true => [sum[0] / sum[3], sum[1] / sum[3], sum[2] / sum[3], sum[3] * 255.].map(|v| v as u8),
        false => [0; 4],
    }
}

fn each(px: &[u8], w: usize, f: impl Fn(f64, f64, &[u8]) -> [u8; 4] + Sync) -> Vec<u8> {
    let mut out = vec![0; px.len()];
    spread(&mut out, w * 4, |from, rows| {
        for (y, row) in (from / (w * 4)..).zip(rows.chunks_exact_mut(w * 4)) {
            for (x, d) in row.chunks_exact_mut(4).enumerate() {
                d.copy_from_slice(&f(x as f64 + 0.5, y as f64 + 0.5, &px[(y * w + x) * 4..][..4]));
            }
        }
    });
    out
}

fn lens(px: &[u8], (w, h): (usize, usize), fov: f64) -> Vec<u8> {
    let (f, (cx, cy)) = (w as f64 / 2. / (fov.abs().to_radians() / 2.).tan(), (w as f64 / 2., h as f64 / 2.));
    each(px, w, |x, y, _| {
        let (dx, dy) = (x - cx, y - cy);
        let turn = dx.hypot(dy) / f;
        if fov > 0. && turn >= FRAC_PI_2 {
            return [0; 4];
        }
        let far = if fov > 0. { turn.tan() } else { turn.atan() };
        let k = if turn > 0. { far / turn } else { 1. };
        sample(px, (w, h), cx + dx * k - 0.5, cy + dy * k - 0.5)
    })
}

fn down(mut rows: Vec<&mut [f32]>, r: usize, n: f32) {
    let (h, m) = (rows.len(), rows.first().map_or(0, |row| row.len()));
    let (mut sum, mut ring) = (vec![0.; m], vec![0.; (r + 1).min(h) * m]);
    for y in 0..h + r {
        for i in 0..m {
            let add = if y < h { rows[y][i] } else { 0. };
            sum[i] += add - if y > 2 * r { ring[(y - 2 * r - 1) % (r + 1) * m + i] } else { 0. };
            if y >= r {
                ring[(y - r) % (r + 1) * m + i] = rows[y - r][i];
                rows[y - r][i] = sum[i] / n;
            }
        }
    }
}

fn blur(px: &[u8], w: usize, r: usize) -> Vec<u8> {
    let pre = |p: &[u8]| [0, 1, 2, 3].map(|i| if i < 3 { p[i] as f32 * p[3] as f32 / 255. } else { p[3] as f32 });
    let (mut a, n, cols) = (
        vec![0.; px.len()],
        (2 * r + 1) as f32,
        (w * 4).div_ceil(thread::available_parallelism().map_or(1, usize::from)),
    );
    spread(&mut a, 4, |from, part| {
        for (d, p) in part.chunks_exact_mut(4).zip(px[from..].chunks_exact(4)) {
            d.copy_from_slice(&pre(p));
        }
    });
    for _ in 0..3 {
        spread(&mut a, w * 4, |_, rows| {
            let mut src = vec![0.; w * 4];
            for row in rows.chunks_exact_mut(w * 4) {
                let mut sum = [0.; 4];
                src.copy_from_slice(row);
                for x in 0..w + r {
                    for c in 0..4 {
                        sum[c] += if x < w { src[x * 4 + c] } else { 0. } - if x > 2 * r { src[(x - 2 * r - 1) * 4 + c] } else { 0. };
                        if x >= r {
                            row[(x - r) * 4 + c] = sum[c] / n;
                        }
                    }
                }
            }
        });
        let mut parts: Vec<Vec<&mut [f32]>> = vec![];
        for row in a.chunks_exact_mut(w * 4) {
            for (j, part) in row.chunks_mut(cols).enumerate() {
                match parts.get_mut(j) {
                    Some(rows) => rows.push(part),
                    None => parts.push(vec![part]),
                }
            }
        }
        thread::scope(|s| parts.into_iter().for_each(|rows| drop(s.spawn(move || down(rows, r, n)))));
    }
    let post = |p: &[f32]| [0, 1, 2, 3].map(|i| if i < 3 && p[3] > 0. { p[i] * 255. / p[3] } else { p[i] }.min(255.) as u8);
    let mut out = vec![0; px.len()];
    spread(&mut out, 4, |from, part| {
        for (d, p) in part.chunks_exact_mut(4).zip(a[from..].chunks_exact(4)) {
            d.copy_from_slice(&post(p));
        }
    });
    out
}

fn depth(px: &[u8], (w, h): (usize, usize), most: f32) -> Vec<f32> {
    let mut d: Vec<f32> = px.chunks_exact(4).map(|p| if p[3] < 128 { 0. } else { most }).collect();
    let near = |d: &[f32], x: usize, y: usize, ox: i64, oy: i64, step: f32| {
        let (nx, ny) = (x as i64 + ox, y as i64 + oy);
        let inside = nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64;
        if inside { d[ny as usize * w + nx as usize] + step } else { step }
    };
    let diag = std::f32::consts::SQRT_2;
    for y in 0..h {
        for x in 0..w {
            let v = [(-1, 0, 1.), (0, -1, 1.), (-1, -1, diag), (1, -1, diag)].map(|(ox, oy, s)| near(&d, x, y, ox, oy, s));
            d[y * w + x] = v.into_iter().fold(d[y * w + x], f32::min);
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let v = [(1, 0, 1.), (0, 1, 1.), (1, 1, diag), (-1, 1, diag)].map(|(ox, oy, s)| near(&d, x, y, ox, oy, s));
            d[y * w + x] = v.into_iter().fold(d[y * w + x], f32::min);
        }
    }
    d
}

fn roughen(px: &[u8], (w, h): (usize, usize), k: f64, border: f64, scale: f64, sharp: f64, seed: i64) -> Vec<u8> {
    let (deep, soft, cell) = (depth(px, (w, h), (border * k) as f32 + 1.), (0.25 / sharp).min(1.), 0.1 * scale * k);
    each(px, w, |x, y, p| {
        let edge = ((deep[(y as usize) * w + x as usize] as f64 - 0.5) / (border * k).max(1e-6)).clamp(0., 1.);
        let n = 0.5 + fractal(x / cell, y / cell, seed, 2, 0.5, 2.) / 1.5;
        let m = ((edge - n * (1. - soft)) / soft).clamp(0., 1.);
        [p[0], p[1], p[2], (p[3] as f64 * m) as u8]
    })
}

fn displace(px: &[u8], (w, h): (usize, usize), k: f64, amount: f64, size: f64, seed: i64) -> Vec<u8> {
    each(px, w, |x, y, _| {
        let (u, v) = (x / (size * k), y / (size * k));
        let (dx, dy) = (field(u, v, seed * 2) * 2. - 1., field(u, v, seed * 2 + 1) * 2. - 1.);
        sample(px, (w, h), x + dx * amount * k - 0.5, y + dy * amount * k - 0.5)
    })
}

pub fn now(fx: &[Fx], i: usize) -> Vec<Fx> {
    let (at, whole) = (|v: &Val| Val::Fixed(v.at(i)), |v: &Val| Val::Fixed(v.at(i).floor()));
    let one = |f: &Fx| {
        let mut f = f.clone();
        match &mut f {
            Fx::Lens(v) | Fx::Blur(v) => *v = at(v),
            Fx::Noise(_, seed) | Fx::Fractal { seed, .. } | Fx::Roughen { seed, .. } => *seed = whole(seed),
            Fx::Displace { amount, seed, .. } => (*amount, *seed) = (at(amount), whole(seed)),
            Fx::Levels(..) => {}
        }
        f
    };
    fx.iter().map(one).collect()
}

pub fn paint(fx: &[Fx], mut px: Arc<Vec<u8>>, size: (usize, usize), k: f64) -> Arc<Vec<u8>> {
    let whole = |v: &Val| v.at(0) as i64;
    for f in fx.iter().filter(|_| size.0 * size.1 > 0) {
        let made = match f {
            Fx::Lens(fov) if fov.at(0) != 0. => lens(&px, size, fov.at(0)),
            Fx::Blur(r) if r.at(0) * k >= 0.5 => blur(&px, size.0, (r.at(0) * k).round() as usize),
            Fx::Lens(_) | Fx::Blur(_) => continue,
            Fx::Levels(black, white, alpha) => {
                let map = |v: u8| ((v as f64 / 255. - black) / (white - black).max(1e-6) * 255.).clamp(0., 255.) as u8;
                each(&px, size.0, |_, _, p| match alpha {
                    true => [p[0], p[1], p[2], map(p[3])],
                    false => [map(p[0]), map(p[1]), map(p[2]), p[3]],
                })
            }
            Fx::Noise(amount, seed) => {
                let seed = whole(seed);
                each(&px, size.0, |x, y, p| {
                    let add = (hash((x / k) as i64, (y / k) as i64, seed) - 0.5) * 510. * amount;
                    [0, 1, 2, 3].map(|c| if c < 3 { (p[c] as f64 + add).clamp(0., 255.) as u8 } else { p[3] })
                })
            }
            Fx::Fractal { scale, seed } => {
                let (cell, seed) = (scale * k * 2. / 3., whole(seed));
                each(&px, size.0, |x, y, p| {
                    let v = ((0.5 + fractal(x / cell, y / cell, seed, 6, 0.7, 1. / 0.56)) * 255.).clamp(0., 255.) as u8;
                    [v, v, v, p[3]]
                })
            }
            Fx::Roughen {
                border,
                scale,
                sharp,
                seed,
            } => roughen(&px, size, k, *border, *scale, *sharp, whole(seed)),
            Fx::Displace { amount, size: cell, seed } => displace(&px, size, k, amount.at(0), *cell, whole(seed)),
        };
        px = Arc::new(made);
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    // A blur gives the pixels of blurring every row and then every column three times over with running
    // sums, also for radii that reach past the picture's edges.
    #[test]
    fn blurs_rows_then_columns() {
        let alpha = |i: usize| {
            if i % 7 == 3 {
                0
            } else if i % 3 > 0 {
                255
            } else {
                i * 29 % 256
            }
        };
        let px: Vec<u8> = (0..20)
            .flat_map(|i| [i * 37 % 256, (i * 91 + 20) % 256, (i * 53 + 7) % 256, alpha(i)].map(|v| v as u8))
            .collect();
        let mark = |v: Vec<u8>| {
            (
                v.iter().map(|&b| b as usize).sum::<usize>(),
                (1..).zip(&v).map(|(i, &b)| i * b as usize).sum::<usize>(),
            )
        };
        assert_eq!(
            [1, 2, 6].map(|r| mark(blur(&px, 5, r))),
            [(9466, 388895), (8457, 340500), (7960, 318820)]
        );
    }

    // Effects on a picture with no width or no height give it back as it was instead of stopping.
    #[test]
    fn empty_pictures_pass_through() {
        let all = r#"[{"lens":30},{"blur":4},{"levels":[0.1,0.9,false]},{"noise":[0.2,1]},{"fractal":{"scale":50,"seed":1}},
            {"roughen":{"border":4,"scale":20,"sharp":1,"seed":0}},{"displace":{"amount":10,"size":50,"seed":0}}]"#;
        let fx: Vec<Fx> = serde_json::from_str(all).unwrap();
        for size in [(0, 3), (3, 0), (0, 0)] {
            let px = Arc::new(vec![]);
            assert!(Arc::ptr_eq(&paint(&fx, px.clone(), size, 1.), &px));
        }
    }
}
