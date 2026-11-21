use crate::lut::spread;
use origami_lang::Fx;
use std::{f64::consts::FRAC_PI_2, sync::Arc};

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

fn blur(px: &[u8], (w, h): (usize, usize), r: usize) -> Vec<u8> {
    let pre = |p: &[u8]| [0, 1, 2, 3].map(|i| if i < 3 { p[i] as f32 * p[3] as f32 / 255. } else { p[3] as f32 });
    let (mut a, n) = (px.chunks_exact(4).flat_map(pre).collect::<Vec<f32>>(), (2 * r + 1) as f32);
    let mut b = vec![0.; a.len()];
    for _ in 0..3 {
        spread(&mut b, w * 4, |from, rows| {
            for (y, row) in (from / (w * 4)..).zip(rows.chunks_exact_mut(w * 4)) {
                let (src, mut sum) = (&a[y * w * 4..][..w * 4], [0.; 4]);
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
        let mut sum = vec![0.; w * 4];
        for y in 0..h + r {
            for (i, s) in sum.iter_mut().enumerate() {
                *s += if y < h { b[y * w * 4 + i] } else { 0. } - if y > 2 * r { b[(y - 2 * r - 1) * w * 4 + i] } else { 0. };
                if y >= r {
                    a[(y - r) * w * 4 + i] = *s / n;
                }
            }
        }
    }
    let post = |p: &[f32]| [0, 1, 2, 3].map(|i| if i < 3 && p[3] > 0. { p[i] * 255. / p[3] } else { p[i] }.min(255.) as u8);
    a.chunks_exact(4).flat_map(post).collect()
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

pub fn paint(fx: &[Fx], mut px: Arc<Vec<u8>>, size: (usize, usize), i: usize, k: f64) -> Arc<Vec<u8>> {
    let whole = |v: &origami_lang::Val| v.at(i).floor() as i64;
    for f in fx {
        let made = match f {
            Fx::Lens(fov) if fov.at(i) != 0. => lens(&px, size, fov.at(i)),
            Fx::Blur(r) if r.at(i) * k >= 0.5 => blur(&px, size, (r.at(i) * k).round() as usize),
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
            Fx::Displace { amount, size: cell, seed } => displace(&px, size, k, amount.at(i), *cell, whole(seed)),
        };
        px = Arc::new(made);
    }
    px
}
