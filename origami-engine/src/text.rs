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

pub fn rgb(s: &str) -> [u8; 3] {
    let hex = match s {
        "white" => "ffffff",
        "black" => "000000",
        s => s.trim_start_matches('#'),
    };
    let v = u32::from_str_radix(hex, 16).unwrap_or(0).to_be_bytes();
    [v[1], v[2], v[3]]
}

pub struct Glyph {
    pub i: usize,
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub px: Arc<Vec<u8>>,
}

pub fn glyphs(c: &Clip, k: f64) -> (Vec<Glyph>, usize, usize) {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    let font = face(&c.font);
    layout.append(&[&*font], &TextStyle::new(&c.src, (c.size * c.scale.max() * k) as f32, 0));
    let w = layout.glyphs().iter().map(|g| g.x as usize + g.width).max().unwrap_or(0);
    let [r, g, b] = rgb(&c.color);
    let glyph = |p: &GlyphPosition| Glyph {
        i: c.src[..p.byte_offset].chars().count(),
        x: p.x as usize,
        y: p.y as usize,
        w: p.width,
        h: p.height,
        px: Arc::new(font.rasterize_config(p.key).1.into_iter().flat_map(|a| [b, g, r, a]).collect()),
    };
    (layout.glyphs().iter().map(glyph).collect(), w, layout.height().ceil() as usize)
}

pub fn raster(c: &Clip, k: f64) -> (Vec<u8>, usize, usize) {
    let (glyphs, w, h) = glyphs(c, k);
    let [r, g, b] = rgb(&c.color);
    let mut px = [b, g, r, 0].repeat(w * h);
    for g in glyphs {
        for (i, a) in g.px.chunks_exact(4).enumerate() {
            let (x, y) = (g.x + i % g.w, g.y + i / g.w);
            if x < w && y < h {
                px[(y * w + x) * 4 + 3] = a[3];
            }
        }
    }
    (px, w, h)
}
