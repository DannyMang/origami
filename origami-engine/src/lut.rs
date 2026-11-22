use lru::LruCache;
use std::{
    array, fs,
    num::NonZeroUsize,
    os::unix::fs::MetadataExt,
    sync::{Arc, LazyLock, Mutex},
    thread,
    time::SystemTime,
};

type Files = LruCache<(u64, u64, Option<SystemTime>), Option<Arc<Lut>>>;

static FILES: LazyLock<Mutex<Files>> = LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(32).unwrap())));

pub type Rgb = [f64; 3];

#[derive(Debug, PartialEq)]
pub struct Lut {
    pub n: usize,
    pub cube: bool,
    pub t: Vec<[f32; 3]>,
}

fn mix<const N: usize>(a: [f32; N], b: [f32; N], f: f32) -> [f32; N] {
    array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
}

pub fn spread<T: Send>(out: &mut [T], unit: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let band = out.len().div_ceil(unit).div_ceil(64).max(1) * unit;
    let parts = Mutex::new(out.chunks_mut(band).enumerate());
    let next = || parts.lock().unwrap().next();
    thread::scope(|s| {
        for _ in 0..thread::available_parallelism().map_or(1, usize::from) {
            s.spawn(|| {
                while let Some((i, part)) = next() {
                    f(i * band, part)
                }
            });
        }
    });
}

impl Lut {
    pub fn bake(n: usize, f: impl Fn(Rgb) -> Rgb + Sync) -> Self {
        let at = |i: usize| (i % n) as f64 / (n - 1) as f64;
        let mut t = vec![[0.; 3]; n * n * n];
        spread(&mut t, 1, |from, part| {
            for (i, c) in (from..).zip(part) {
                *c = f([at(i), at(i / n), at(i / n / n)]).map(|v| v as f32);
            }
        });
        Self { n, cube: true, t }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let (mut n, mut cube, mut t) = (0, true, vec![]);
        for line in text.lines().map(str::trim) {
            let mut words = line.split_whitespace();
            let (Some(head), rest) = (words.next(), words.flat_map(str::parse::<f32>).collect::<Vec<_>>()) else {
                continue;
            };
            match (head, &rest[..]) {
                ("LUT_3D_SIZE", [size]) => n = *size as usize,
                ("LUT_1D_SIZE", [size]) => (n, cube) = (*size as usize, false),
                ("DOMAIN_MIN", lo) if lo != [0.; 3] => return None,
                ("DOMAIN_MAX", hi) if hi != [1.; 3] => return None,
                (r, [g, b]) => t.extend(r.parse().ok().map(|r| [r, *g, *b])),
                _ => {}
            }
        }
        (n > 1 && Some(t.len()) == if cube { n.checked_pow(3) } else { Some(n) }).then_some(Self { n, cube, t })
    }

    pub fn read(path: &str) -> Option<Arc<Self>> {
        let m = fs::metadata(path).ok()?;
        let key = (m.dev(), m.ino(), m.modified().ok());
        let load = || Self::parse(&origami_lang::load(path.as_ref()).ok()?).map(Arc::new);
        FILES.lock().unwrap().get_or_insert(key, load).clone()
    }

    pub fn text(&self) -> String {
        let head = format!("LUT_{}D_SIZE {}\n", if self.cube { 3 } else { 1 }, self.n);
        self.t.iter().fold(head, |s, [r, g, b]| s + &format!("{r:.6} {g:.6} {b:.6}\n"))
    }

    fn cell(&self, v: f64) -> (usize, f32) {
        let x = v.clamp(0., 1.) * (self.n - 1) as f64;
        let i = (x as usize).min(self.n - 2);
        (i, (x - i as f64) as f32)
    }

    fn blend(&self, [(r, fr), (g, fg), (b, fb)]: [(usize, f32); 3]) -> [f32; 3] {
        let (n, t) = (self.n, &self.t);
        let row = |i: usize| mix(t[i], t[i + 1], fr);
        let face = |i: usize| mix(row(i), row(i + n), fg);
        let i = r + (g + b * n) * n;
        mix(face(i), face(i + n * n), fb)
    }

    pub fn sample(&self, c: Rgb) -> Rgb {
        let at = c.map(|v| self.cell(v));
        if self.cube {
            return self.blend(at).map(f64::from);
        }
        array::from_fn(|k| {
            let (i, f) = at[k];
            (self.t[i][k] + (self.t[i + 1][k] - self.t[i][k]) * f) as f64
        })
    }

    pub fn apply(&self, px: &[u8], mut out: Vec<u8>) -> Vec<u8> {
        let (n, t) = (self.n, self.t.iter().map(|&[r, g, b]| [b, g, r, 1.]).collect::<Vec<_>>());
        let [b, g, r] =
            [n * n, n, 1].map(|step| -> [(usize, f32); 256] { array::from_fn(|v| self.cell(v as f64 / 255.)).map(|(i, f)| (i * step, f)) });
        out.resize(px.len(), 0);
        spread(&mut out, 4, |from, part| {
            for (p, s) in part.chunks_exact_mut(4).zip(px[from..].chunks_exact(4)) {
                let [(i, fb), (j, fg), (k, fr)] = [b[s[0] as usize], g[s[1] as usize], r[s[2] as usize]];
                let c = &t[i + j + k..][..n * n + n + 2];
                let row = |i: usize| mix(c[i], c[i + 1], fr);
                let face = |i: usize| mix(row(i), row(i + n), fg);
                let (c, scale) = (mix(face(0), face(n * n), fb), [255., 255., 255., s[3] as f32]);
                p.copy_from_slice(&array::from_fn::<_, 4, _>(|k| (c[k] * scale[k]) as u8));
            }
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A colour table file rewritten again and again reads back its new table each time and keeps at most
    // 32 readings in memory, not one for every version it ever had.
    #[test]
    fn rewritten_cubes_stay_bounded() {
        let file = crate::tests::scratch("recube").join("look.cube");
        for i in 0..40 {
            let lut = Lut::bake(2, |c| c.map(|v| v * i as f64 / 40.));
            fs::write(&file, lut.text()).unwrap();
            assert_eq!(Lut::read(&file.to_string_lossy()).as_deref(), Lut::parse(&lut.text()).as_ref());
        }
        assert!(FILES.lock().unwrap().len() <= 32);
    }
}
