use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, GlyphPosition, Layout, TextStyle},
};
use origami_lang::Clip;
use std::{
    collections::HashMap,
    fs,
    sync::{Arc, LazyLock, Mutex},
};

static FONTS: LazyLock<Mutex<HashMap<String, Arc<Font>>>> = LazyLock::new(Mutex::default);

fn face(path: &str) -> Arc<Font> {
    let load = |p: &str| Font::from_bytes(fs::read(p).ok()?, FontSettings::default()).ok().map(Arc::new);
    let plain = "/System/Library/Fonts/Helvetica.ttc";
    let mut fonts = FONTS.lock().unwrap();
    let font = fonts
        .entry(path.into())
        .or_insert_with(|| load(path).or_else(|| load(plain)).unwrap());
    font.clone()
}

pub fn tone(s: &str) -> Option<[u8; 4]> {
    let hex = match s {
        "white" => "ffffff",
        "black" => "000000",
        s => s.trim_start_matches('#'),
    };
    let wide: String = match hex.len() {
        3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
        _ => hex.into(),
    };
    let v = u32::from_str_radix(&wide, 16).ok()?.to_be_bytes();
    match wide.len() {
        6 => Some([v[1], v[2], v[3], 255]),
        8 => Some(v),
        _ => None,
    }
}

pub fn rgba(s: &str) -> [u8; 4] {
    tone(s).unwrap_or([0, 0, 0, 255])
}

pub fn flat(color: &str, n: usize) -> Vec<u8> {
    let [r, g, b, a] = rgba(color);
    [b, g, r, a].repeat(n)
}

fn over(d: &mut [u8], s: &[u8]) {
    let (sa, da) = (s[3] as f64 / 255., d[3] as f64 / 255.);
    let a = sa + da * (1. - sa);
    for i in 0..3 {
        d[i] = ((s[i] as f64 * sa + d[i] as f64 * da * (1. - sa)) / a.max(1e-9)).round() as u8;
    }
    d[3] = (a * 255.).round() as u8;
}

pub struct Glyph {
    pub i: usize,
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub px: Arc<Vec<u8>>,
}

pub fn glyphs(c: &Clip, k: f64, ink: bool) -> (Vec<Glyph>, usize, usize) {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    let (font, size) = (face(&c.font), c.size * c.scale.max() * k);
    layout.append(&[&*font], &TextStyle::new(&c.src, size as f32, 0));
    let rows: Vec<(usize, usize)> = layout.lines().into_iter().flatten().map(|l| (l.glyph_start, l.glyph_end)).collect();
    let wide = |r: &(usize, usize)| {
        let row = layout.glyphs().get(r.0..=r.1).into_iter().flatten();
        row.map(|g| g.x as usize + g.width).max().unwrap_or(0)
    };
    let (w, [r, g, b, a]) = (rows.iter().map(wide).max().unwrap_or(0), rgba(&c.color));
    let (lean, pad) = (
        ["left", "", "right"].iter().position(|a| *a == c.align).unwrap_or(1),
        if c.background.is_empty() {
            (0, 0)
        } else {
            ((size * 0.3) as usize, (size * 0.12) as usize)
        },
    );
    let glyph = |(n, p): (usize, &GlyphPosition)| Glyph {
        i: c.src[..p.byte_offset].chars().count(),
        x: p.x as usize + pad.0 + rows.iter().find(|r| n <= r.1).map_or(0, |r| (w - wide(r)) * lean / 2),
        y: p.y as usize + pad.1,
        w: p.width,
        h: p.height,
        px: Arc::new(
            ink.then(|| font.rasterize_config(p.key).1)
                .into_iter()
                .flatten()
                .flat_map(|v| [b, g, r, (v as u32 * a as u32 / 255) as u8])
                .collect(),
        ),
    };
    (
        layout.glyphs().iter().enumerate().map(glyph).collect(),
        w + 2 * pad.0,
        layout.height().ceil() as usize + 2 * pad.1,
    )
}

pub fn raster(c: &Clip, k: f64) -> (Vec<u8>, usize, usize) {
    let (glyphs, w, h) = glyphs(c, k, true);
    let mut px = flat(&c.background, w * h);
    if c.background.is_empty() {
        px.chunks_exact_mut(4).for_each(|p| p.copy_from_slice(&[0; 4]));
    }
    for g in glyphs {
        for (i, s) in g.px.chunks_exact(4).enumerate() {
            let (x, y) = (g.x + i % g.w, g.y + i / g.w);
            if x < w && y < h {
                over(&mut px[(y * w + x) * 4..][..4], s);
            }
        }
    }
    (px, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Colours written as names or hex codes, with or without transparency, should be read correctly,
    // and anything unrecognised rejected. Drawing a see-through colour over another should mix the two
    // properly.
    #[test]
    fn colours_carry_alpha() {
        let all = [
            "white",
            "black",
            "#f80",
            "#f808",
            "#ff8800",
            "#ff880080",
            "ff8800",
            "red",
            "#12",
            "",
        ]
        .map(tone);
        let (o, half) = ([255, 136, 0, 255], [255, 136, 0, 136]);
        assert_eq!(all[..5], [[255; 4], [0, 0, 0, 255], o, half, o].map(Some));
        assert_eq!(all[5..], [Some([255, 136, 0, 128]), Some(o), None, None, None]);
        assert_eq!((rgba("red"), flat("#0008", 2)), ([0, 0, 0, 255], vec![0, 0, 0, 136, 0, 0, 0, 136]));
        let mut d = [0, 0, 0, 128];
        over(&mut d, &[255, 255, 255, 255]);
        assert_eq!(d, [255; 4]);
        over(&mut d, &[9, 9, 9, 0]);
        assert_eq!(d, [255; 4]);
        let mut clear = [7, 7, 7, 0];
        over(&mut clear, &[10, 20, 30, 51]);
        assert_eq!(clear, [10, 20, 30, 51]);
    }
}
