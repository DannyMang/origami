use crate::{
    lut::{Lut, Rgb, spread},
    text,
};
use lru::LruCache;
use origami_lang::{Grade, Vignette};
use std::{
    array,
    f64::consts::TAU,
    num::NonZeroUsize,
    sync::{Arc, LazyLock, Mutex},
};

type Cache<T> = LazyLock<Mutex<LruCache<String, T>>>;

static LUTS: Cache<Option<Arc<Lut>>> = LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(64).unwrap())));
static MASKS: Cache<Arc<Vec<u8>>> = LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(8).unwrap())));

fn luma(c: Rgb) -> f64 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn smooth(a: f64, b: f64, x: f64) -> f64 {
    if a >= b {
        return (x >= a) as u8 as f64;
    }
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

fn blend(a: Rgb, b: Rgb, k: f64) -> Rgb {
    array::from_fn(|i| a[i] + (b[i] - a[i]) * k)
}

fn saturate(c: Rgb, k: f64) -> Rgb {
    let y = luma(c);
    c.map(|v| y + (v - y) * k)
}

fn balance(c: Rgb, temperature: f64, tint: f64) -> Rgb {
    let gain = [1. + temperature / 400., 1. - tint / 400., 1. - temperature / 400.];
    array::from_fn(|i| c[i] * gain[i] / luma(gain))
}

fn contrast(c: Rgb, amount: f64) -> Rgb {
    c.map(|v| (v - 0.5) * 2f64.powf(amount / 100.) + 0.5)
}

fn hsv(c: Rgb) -> (f64, f64, f64) {
    let [r, g, b] = c.map(|v| v.clamp(0., 1.));
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let d = hi - lo;
    let h = match d {
        d if d <= 0. => 0.,
        _ if hi == r => ((g - b) / d).rem_euclid(6.),
        _ if hi == g => (b - r) / d + 2.,
        _ => (r - g) / d + 4.,
    };
    (h / 6., if hi > 0. { d / hi } else { 0. }, (hi + lo) / 2.)
}

fn push(hue: f64, amount: f64) -> Rgb {
    let c = [0., 120., 240.].map(|at| 0.5 + 0.5 * (hue - at).to_radians().cos());
    c.map(|v| (v - luma(c)) * amount / 2.)
}

fn turn(c: Rgb, turns: f64) -> Rgb {
    let (sin, cos) = (turns * TAU).sin_cos();
    let (mean, k) = ((c[0] + c[1] + c[2]) / 3., sin / 3f64.sqrt());
    let cross = [c[2] - c[1], c[0] - c[2], c[1] - c[0]];
    array::from_fn(|i| c[i] * cos + cross[i] * k + mean * (1. - cos))
}

fn spline(p: &[(f64, f64)], x: f64, none: f64) -> f64 {
    let (Some(first), Some(last)) = (p.first(), p.last()) else {
        return none;
    };
    let Some(i) = p.windows(2).position(|w| w[0].0 <= x && x < w[1].0) else {
        return if x < first.0 { first.1 } else { last.1 };
    };
    let slope = |i: usize| {
        if p[i + 1].0 > p[i].0 {
            (p[i + 1].1 - p[i].1) / (p[i + 1].0 - p[i].0)
        } else {
            0.
        }
    };
    let bend = |i: usize| match (i.checked_sub(1).map(slope), (i + 1 < p.len()).then(|| slope(i))) {
        (Some(a), Some(b)) if a * b > 0. => 2. * a * b / (a + b),
        (Some(_), Some(_)) => 0.,
        (a, b) => a.or(b).unwrap_or(0.),
    };
    let (h, t) = (p[i + 1].0 - p[i].0, (x - p[i].0) / (p[i + 1].0 - p[i].0));
    let (t2, t3) = (t * t, t * t * t);
    (2. * t3 - 3. * t2 + 1.) * p[i].1 + (t3 - 2. * t2 + t) * h * bend(i) + (3. * t2 - 2. * t3) * p[i + 1].1 + (t3 - t2) * h * bend(i + 1)
}

fn around(p: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let shift = |d: f64| p.iter().map(move |(x, y)| (x + d, *y));
    shift(-1.).chain(shift(0.)).chain(shift(1.)).collect()
}

fn pipe(g: &Grade) -> impl Fn(Rgb) -> Rgb + '_ {
    let input = g.input_lut.as_deref().and_then(Lut::read);
    let look = g.look.as_ref().and_then(|(path, k)| Some((Lut::read(path)?, k / 100.)));
    let (stops, split) = (2f64.powf(g.exposure / 2.2), 2f64.powf(-g.tint_balance / 100.));
    let tints = [g.shadow_tint, g.highlight_tint].map(|(hue, amount)| push(hue, amount));
    let wheels =
        [&g.wheel.shadows, &g.wheel.midtones, &g.wheel.highlights].map(|(hue, amount, luma)| push(*hue, *amount).map(|v| v + luma / 2.));
    let [hue_sat, hue_hue, hue_luma] = [&g.hue_vs_sat, &g.hue_vs_hue, &g.hue_vs_luma].map(|p| around(p));
    let key = g
        .hsl_secondary
        .as_ref()
        .map(|s| (s, hsv([0, 1, 2].map(|i| text::rgba(&s.color)[i] as f64 / 255.))));
    move |mut c| {
        if let Some(lut) = &input {
            c = lut.sample(c);
        }
        c = contrast(balance(c, g.temperature, g.tint).map(|v| v * stops), g.contrast);
        for (amount, weight) in [
            (g.highlights * 0.003, (|y| y * y) as fn(f64) -> f64),
            (g.shadows * 0.003, |y| (1. - y) * (1. - y)),
            (g.whites * 0.002, |y| y),
            (g.blacks * 0.002, |y| 1. - y),
        ] {
            let d = amount * weight(luma(c).clamp(0., 1.));
            c = c.map(|v| v + d);
        }
        c = saturate(c, g.saturation.unwrap_or(100.) / 100.);
        if let Some((lut, k)) = &look {
            c = blend(c, lut.sample(c), *k);
        }
        c = c.map(|v| v + g.faded_film / 100. * (0.15 - 0.3 * v));
        c = saturate(c, 1. + g.vibrance / 100. * (1. - hsv(c).1));
        let high = luma(c).clamp(0., 1.).powf(split);
        c = array::from_fn(|i| c[i] + tints[0][i] * (1. - high) + tints[1][i] * high);
        c = c.map(|v| spline(&g.curve.luma, v, v));
        c = array::from_fn(|i| spline([&g.curve.red, &g.curve.green, &g.curve.blue][i], c[i], c[i]));
        let ((h, s, _), y) = (hsv(c), luma(c));
        let more =
            (1. + spline(&hue_sat, h, 0.)) * (1. + spline(&g.luma_vs_sat, y.clamp(0., 1.), 0.)) * (1. + spline(&g.sat_vs_sat, s, 0.));
        let chroma = turn(c.map(|v| v - y), spline(&hue_hue, h, 0.));
        let y = y + spline(&hue_luma, h, 0.) * s;
        c = chroma.map(|v| y + (v - luma(chroma)) * more.max(0.));
        let y = luma(c).clamp(0., 1.);
        let weights = [(1. - y) * (1. - y), 2. * y * (1. - y), y * y];
        c = array::from_fn(|i| c[i] + (0..3).map(|w| wheels[w][i] * weights[w]).sum::<f64>());
        if let Some((s, (kh, ks, kl))) = &key {
            let near = |d: f64, unit: f64| 1. - smooth(s.range * unit, (s.range + s.soft) * unit, d);
            let (h, sat, l) = hsv(c);
            let around = (h - kh).abs().min(1. - (h - kh).abs()) * (sat.min(*ks) * 10.).min(1.);
            let mask = near(around, 0.005) * near((sat - ks).abs(), 0.015) * near((l - kl).abs(), 0.025);
            let lit = balance(c, s.temperature, s.tint).map(|v| v * 2f64.powf(s.exposure / 2.2));
            let fixed = saturate(contrast(lit, s.contrast), s.saturation / 100.);
            c = blend(c, fixed, mask);
        }
        c.map(|v| v.clamp(0., 1.))
    }
}

pub fn lut(g: &Grade) -> Option<Arc<Lut>> {
    let key = format!(
        "{:?}",
        Grade {
            vignette: None,
            ..g.clone()
        }
    );
    let bake = || {
        let (pipe, round) = (pipe(g), |c: Rgb| c.map(|v| v + 0.5 / 255.));
        let (lut, same) = (Lut::bake(33, |c| round(pipe(c))), Lut::bake(33, round));
        let moved = lut.t.iter().zip(&same.t).any(|(a, b)| (0..3).any(|i| (a[i] - b[i]).abs() > 1e-6));
        moved.then(|| Arc::new(lut))
    };
    LUTS.lock().unwrap().get_or_insert(key, bake).clone()
}

pub fn mask(v: &Vignette, w: usize, h: usize) -> Arc<Vec<u8>> {
    let draw = || {
        let (round, tone) = (v.roundness / 100., if v.amount > 0. { 255 } else { 0 });
        let (p, r) = (2. + 6. * (-round).max(0.), w.max(h) as f64 / 2.);
        let (sx, sy) = (w as f64 / 2., h as f64 / 2.);
        let (sx, sy) = (sx + (r - sx) * round.max(0.), sy + (r - sy) * round.max(0.));
        let (edge, soft, top) = (
            v.midpoint / 100. * 1.4,
            v.feather / 100. * 0.7,
            (v.amount.abs() / 3.).min(1.) * 255.,
        );
        let alpha = |i: usize| {
            let (dx, dy) = (
                ((i % w) as f64 + 0.5 - w as f64 / 2.) / sx,
                ((i / w) as f64 + 0.5 - h as f64 / 2.) / sy,
            );
            let d = (dx.abs().powf(p) + dy.abs().powf(p)).powf(1. / p);
            (top * smooth(edge - soft, edge + soft, d)).round() as u8
        };
        let mut px = vec![0; w * h * 4];
        spread(&mut px, 4, |from, part| {
            for (i, p) in (from / 4..).zip(part.chunks_exact_mut(4)) {
                p.copy_from_slice(&[tone, tone, tone, alpha(i)]);
            }
        });
        Arc::new(px)
    };
    MASKS.lock().unwrap().get_or_insert(format!("{v:?}|{w}x{h}"), draw).clone()
}

pub fn paint(g: &Grade, px: Arc<Vec<u8>>, (w, h): (usize, usize), out: Vec<u8>) -> Arc<Vec<u8>> {
    let (lut, mask) = (lut(g), g.vignette.as_ref().map(|v| mask(v, w, h)));
    if lut.is_none() && mask.is_none() {
        return px;
    }
    let mut out = lut.map_or_else(|| px.to_vec(), |l| l.apply(&px, out));
    if let Some(m) = &mask {
        spread(&mut out, 4, |from, part| {
            for (d, m) in part.chunks_exact_mut(4).zip(m[from.min(m.len())..].chunks_exact(4)) {
                let a = m[3] as u32;
                for i in 0..3 {
                    d[i] = (((d[i] as u32 * (255 - a) + m[i] as u32 * a + 128) * 257) >> 16) as u8;
                }
            }
        });
    }
    Arc::new(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(json: &str) -> Grade {
        serde_json::from_str(json).unwrap()
    }

    fn grey(json: &str) -> f64 {
        luma(pipe(&with(json))([0.5; 3]))
    }

    // A grade with every setting left at its default should do nothing at all: no colour table is
    // built and the picture is untouched.
    #[test]
    fn defaults_are_identity() {
        assert!(lut(&Grade::default()).is_none());
        assert!(
            lut(&with(
                r#"{"saturation":100,"curve":{"luma":[[0,0],[1,1]]},"wheel":{"midtones":[40,0,0]}}"#
            ))
            .is_none()
        );
    }

    // Each colour slider should push the picture the way its name says: more exposure is brighter,
    // more temperature is warmer, zero saturation is grey, and so on.
    #[test]
    fn sliders_move_the_right_way() {
        for k in ["exposure", "highlights", "shadows", "whites", "blacks"] {
            assert!(
                grey(&format!(r#"{{"{k}":1}}"#)) > 0.5 && grey(&format!(r#"{{"{k}":-1}}"#)) < 0.5,
                "{k}"
            );
        }
        let run = |json: &str, c| pipe(&with(json))(c);
        let [r, _, b] = run(r#"{"temperature":50}"#, [0.5; 3]);
        assert!(r > 0.5 && b < 0.5);
        let [r, g, _] = run(r#"{"tint":50}"#, [0.5; 3]);
        assert!(r > 0.5 && g < 0.5);
        assert!(run(r#"{"contrast":50}"#, [0.7; 3])[0] > 0.7 && run(r#"{"contrast":50}"#, [0.3; 3])[0] < 0.3);
        let [r, g, b] = run(r#"{"saturation":0}"#, [1., 0., 0.]);
        assert!(r == g && g == b);
        assert!(run(r#"{"saturation":150}"#, [0.6, 0.4, 0.4])[0] > 0.6);
        assert!(run(r#"{"vibrance":50}"#, [0.6, 0.4, 0.4])[0] > 0.6);
        assert!(run(r#"{"fadedFilm":50}"#, [0.; 3])[0] > 0. && run(r#"{"fadedFilm":50}"#, [1.; 3])[0] < 1.);
    }

    // A colour table saved as a .cube file should read back the same and is read once however its path
    // is spelled, a size so large that its cube overflows is refused, a look at strength 0 should change
    // nothing, and at 100 should apply in full.
    #[test]
    fn cube_round_trips_and_look_blends() {
        let lut = Lut::bake(5, |[r, g, b]| [b, r * 0.5, g * g]);
        let back = Lut::parse(&lut.text()).unwrap();
        assert!(lut.t.iter().zip(&back.t).all(|(a, b)| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)));
        let line = Lut::parse("TITLE \"t\"\nLUT_1D_SIZE 2\n0 0 0\n1 0.5 0.25\n").unwrap();
        assert_eq!(line.sample([0.5; 3]), [0.5, 0.25, 0.125]);
        assert_eq!(Lut::parse("LUT_3D_SIZE 4194304\n"), None);
        let file = crate::tests::scratch("cube").join("look.cube");
        std::fs::write(&file, lut.text()).unwrap();
        let spelled = file.parent().unwrap().join(".//look.cube");
        assert!(Arc::ptr_eq(
            &Lut::read(&file.to_string_lossy()).unwrap(),
            &Lut::read(&spelled.to_string_lossy()).unwrap()
        ));
        let look = |k: f64| Grade {
            look: Some((file.to_string_lossy().into(), k)),
            ..Grade::default()
        };
        assert!(super::lut(&look(0.)).is_none());
        let c = [0.25, 0.5, 0.75];
        let (full, [r, g, b]) = (pipe(&look(100.))(c), lut.sample(c));
        assert!((full[0] - r).abs() < 1e-6 && (full[1] - g).abs() < 1e-6 && (full[2] - b).abs() < 1e-6);
    }

    // Applying a colour table to a whole picture should give exactly the bytes that sampling the
    // table at each pixel gives, for any table and any pixel, keep each pixel's alpha and leave
    // nothing of the picture it writes over.
    #[test]
    fn applying_matches_sampling() {
        let mut seed = 7u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for n in [2, 5, 33] {
            let mut lut = Lut::bake(n, |c| c);
            lut.t
                .iter_mut()
                .flatten()
                .for_each(|v| *v = next() as f32 / (1u32 << 31) as f32 * 2. - 0.5);
            let px: Vec<u8> = (0..4 * 50_000).map(|_| next() as u8).collect();
            let want = px.chunks_exact(4).flat_map(|s| {
                let [r, g, b] = lut
                    .sample([s[2], s[1], s[0]].map(|v| v as f64 / 255.))
                    .map(|v| (v as f32 * 255.) as u8);
                [b, g, r, s[3]]
            });
            assert!(lut.apply(&px, vec![7; px.len()]).into_iter().eq(want), "{n}");
        }
    }
}
