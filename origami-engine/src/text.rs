use fontdue::{
    Font, FontSettings,
    layout::{CoordinateSystem, Layout, TextStyle},
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

pub fn raster(c: &Clip, k: f64) -> (Vec<u8>, usize, usize) {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    let font = face(&c.font);
    layout.append(&[&*font], &TextStyle::new(&c.src, (c.size * c.scale * k) as f32, 0));
    let w = layout.glyphs().iter().map(|g| g.x as usize + g.width).max().unwrap_or(0);
    let h = layout.height().ceil() as usize;
    let [r, g, b] = rgb(&c.color);
    let mut px = [b, g, r, 0].repeat(w * h);
    for g in layout.glyphs() {
        let (m, cover) = font.rasterize_config(g.key);
        for (i, a) in cover.iter().enumerate() {
            let (x, y) = (g.x as usize + i % m.width, g.y as usize + i / m.width);
            if x < w && y < h {
                px[(y * w + x) * 4 + 3] = *a;
            }
        }
    }
    (px, w, h)
}
