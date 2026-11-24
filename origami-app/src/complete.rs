use crate::{
    Origami,
    view::{BLUE, BORDER, DIM, PANEL},
};
use gpui::{Div, Styled, Window, div, prelude::*, px, rgb, rgba};
use std::{collections::HashSet, fs, mem, path::Path, sync::LazyLock};

pub struct Def {
    pub name: &'static str,
    pub args: &'static str,
    pub method: bool,
    pub about: &'static str,
}

const fn global(name: &'static str, args: &'static str, about: &'static str) -> Def {
    Def {
        name,
        args,
        method: false,
        about,
    }
}

const fn method(name: &'static str, args: &'static str, about: &'static str) -> Def {
    Def {
        name,
        args,
        method: true,
        about,
    }
}

pub const DEFS: &[Def] = &[
    global(
        "settings",
        "{ fps, size, preview, proxy }",
        "Sets frame rate and output size. preview is \"auto\", \"full\", \"half\" or \"quarter\"; proxy: false scrubs without camera sidecar files.",
    ),
    global("video", "path", "Loads a video file."),
    global("sound", "path", "Loads an audio file."),
    global("image", "path", "Loads a still image."),
    global(
        "text",
        "words, { size, color, font, background, align }",
        "Draws text, centered. background is a colour behind the words; align is \"left\", \"center\" or \"right\".",
    ),
    global(
        "caption",
        "words, { size, color, font, background, align }",
        "Draws smaller text near the bottom.",
    ),
    global("captions", "path", "Loads every caption from an .srt file."),
    global(
        "color",
        "color",
        "Fills the frame with a color: \"#rgb\", \"#rrggbb\", or with transparency \"#rgba\", \"#rrggbbaa\".",
    ),
    global(
        "rect",
        "width, height, { color, radius }",
        "Draws a rectangle, centered. Size and corner radius in pixels.",
    ),
    global(
        "line",
        "x1, y1, x2, y2, { color, width }",
        "Draws a line between two points given as fractions of the frame. Width in pixels.",
    ),
    global(
        "path",
        "points, { color, curve }",
        "Draws a filled shape through points given as fractions of the frame. curve: true rounds it through every point.",
    ),
    global("cat", "...clips", "Plays clips one after another."),
    global("stack", "...clips", "Plays clips at the same time. Later clips draw on top."),
    global(
        "keys",
        "[time, value, ease], ...",
        "Animates through values. Time in seconds, negative from the end, or \"50%\".",
    ),
    global("delay", "value, seconds", "Starts an animated value later."),
    global("bezier", "x1, y1, x2, y2", "A custom ease for a key."),
    global(
        "add",
        "value, number",
        "Adds a number to a followed value: add(title.pos.y, -0.06).",
    ),
    global("mul", "value, number", "Multiplies a followed value: mul(title.scale, 2)."),
    method("trim", "start, end", "Keeps the source between two times."),
    method("for", "duration", "Sets how long the clip lasts."),
    method("at", "time", "Sets when the clip starts inside its group."),
    method("shift", "seconds", "Moves the clip later, or earlier if negative."),
    method("speed", "factor", "Plays faster or slower."),
    method("volume", "factor", "Scales loudness."),
    method("mute", "", "Silences the clip."),
    method("fade", "in, out", "Fades in and out, in seconds."),
    method("loop", "count", "Repeats the clip."),
    method("scale", "factor", "Resizes the picture. A number, a function of time, or keys."),
    method(
        "pos",
        "x, y",
        "Places the top-left corner, as fractions of the frame. Each takes a number, a function of time, or keys.",
    ),
    method("opacity", "amount", "Fades the picture. 0 to 1, a function of time, or keys."),
    method("rotate", "degrees", "Turns the picture. A number, a function of time, or keys."),
    method(
        "merge",
        "",
        "Draws a stack or cat as one picture, so methods change that picture as a whole.",
    ),
    method(
        "matte",
        "clip, mode",
        "Shows the clip only through another clip. mode is \"alpha\", \"alphaInverted\", \"luma\" or \"lumaInverted\".",
    ),
    method(
        "blend",
        "mode",
        "How the clip mixes with what is under it: \"normal\" or \"multiply\".",
    ),
    method(
        "fps",
        "rate",
        "Plays the clip at its own lower frame rate, so movement steps. Above 0, up to 240.",
    ),
    method(
        "shown",
        "share",
        "Shows the first share of the letters. 0 to 1, a function of time, or keys.",
    ),
    method(
        "each",
        "\"char\", (i, n) => ({ opacity, x, y, scale, rotate })",
        "Animates every char, word or line of the text on its own.",
    ),
    method("temperature", "amount", "Cooler (negative) or warmer (positive). -100 to 100."),
    method("tint", "amount", "Green (negative) or magenta (positive). -100 to 100."),
    method("saturation", "amount", "Overall colour strength. 0 to 200, default 100."),
    method("exposure", "stops", "Overall brightness in stops. -5 to 5."),
    method("contrast", "amount", "Pushes midtones apart or together. -100 to 100."),
    method("highlights", "amount", "Brightens or darkens bright areas. -100 to 100."),
    method("shadows", "amount", "Brightens or darkens dark areas. -100 to 100."),
    method("whites", "amount", "Sets the white clipping point. -100 to 100."),
    method("blacks", "amount", "Sets the black clipping point. -100 to 100."),
    method("look", "name, intensity", "Applies a look from a .cube file or a built-in name."),
    method("inputLut", "path", "Applies a technical LUT before every other colour step."),
    method("fadedFilm", "amount", "Lifts blacks and lowers whites. 0 to 100."),
    method("sharpen", "amount", "Not built yet: accepted, does nothing."),
    method("vibrance", "amount", "Saturation that favours dull colours. -100 to 100."),
    method("shadowTint", "{ hue, amount }", "Tints the shadows."),
    method("highlightTint", "{ hue, amount }", "Tints the highlights."),
    method(
        "tintBalance",
        "amount",
        "Moves the split between shadow and highlight tint. -100 to 100.",
    ),
    method("curve", "channel, points", "Tone curve on luma, red, green or blue."),
    method("hueVsSat", "points", "Changes saturation for a range of hues."),
    method("hueVsHue", "points", "Shifts a range of hues to another hue."),
    method("hueVsLuma", "points", "Changes brightness for a range of hues."),
    method("lumaVsSat", "points", "Changes saturation by brightness."),
    method("satVsSat", "points", "Changes saturation by existing saturation."),
    method(
        "wheel",
        "range, { hue, amount, luma }",
        "Colour offset in shadows, midtones or highlights.",
    ),
    method("hslSecondary", "{ key, refine, correction }", "Corrects one colour range only."),
    method(
        "duotone",
        "dark, light",
        "Repaints the picture in two colours by brightness: black becomes the first, white the second.",
    ),
    method(
        "vignette",
        "{ amount, midpoint, roundness, feather }",
        "Darkens or lightens the edges.",
    ),
    method(
        "lens",
        "degrees",
        "Bends the picture like a wide lens: positive curves it into a barrel, negative pinches. -170 to 170, a function of time, or keys.",
    ),
    method(
        "blur",
        "radius",
        "Softens the picture. Radius in pixels, 0 and up, a function of time, or keys.",
    ),
    method(
        "levels",
        "black, white, \"alpha\"",
        "Stretches the tones between black and white, shares from 0 to 1. \"alpha\" works on how solid the picture is.",
    ),
    method(
        "noise",
        "amount, seed",
        "Adds grain. Amount 0 to 1; the seed picks the pattern, and a function of time makes it move.",
    ),
    method(
        "fractal",
        "{ scale, seed }",
        "Replaces the colours with soft grey clouds. scale is the biggest clouds in pixels.",
    ),
    method(
        "roughen",
        "{ border, scale, sharpness, seed }",
        "Eats into the edges with a ragged line, like torn paper or ink.",
    ),
    method(
        "displace",
        "{ amount, size, seed }",
        "Pushes every pixel a short way so the picture ripples. amount and size in pixels.",
    ),
];

const SHOWN: usize = 8;
const LOOKS: &str = "Contents/Lumetri/LUTs/Creative";

pub type Offer = (String, &'static str, &'static str);

static INSTALLED: LazyLock<Vec<String>> = LazyLock::new(origami_lang::fonts);

fn ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

fn word(s: &[char]) -> &[char] {
    &s[s.len() - s.iter().rev().take_while(|c| ident(**c)).count()..]
}

fn starts(name: &str, typed: &str) -> bool {
    name != typed && name.to_lowercase().starts_with(&typed.to_lowercase())
}

fn names(dir: &Path) -> Vec<(String, bool)> {
    let entries = fs::read_dir(dir).into_iter().flatten().flatten();
    let mut out: Vec<_> = entries
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path().is_dir()))
        .filter(|(n, _)| !n.starts_with('.'))
        .collect();
    out.sort();
    out
}

fn typeface(name: &str) -> bool {
    [".ttf", ".otf", ".ttc"].iter().any(|e| name.to_lowercase().ends_with(e))
}

pub fn faces(dir: &Path) -> Vec<String> {
    let found = ["", "fonts"].into_iter().flat_map(|sub| {
        let files = names(&dir.join(sub)).into_iter().filter(|(n, d)| !d && typeface(n));
        files.map(move |(n, _)| match &n[..n.len() - 4] {
            stem if stem.contains('.') => Path::new(sub).join(&n).to_string_lossy().to_string(),
            stem => stem.to_string(),
        })
    });
    let mut out: Vec<String> = found.collect();
    out.dedup();
    let rest: Vec<String> = INSTALLED.iter().filter(|n| !out.contains(n)).cloned().collect();
    out.extend(rest);
    out
}

fn premiere() -> Vec<String> {
    let adobe = |dir: &Path| names(dir).into_iter().filter(|(n, _)| n.starts_with("Adobe Premiere Pro "));
    let apps = adobe(Path::new("/Applications")).flat_map(|(year, _)| {
        let year = Path::new("/Applications").join(year);
        adobe(&year).map(move |(app, _)| year.join(app).join(LOOKS))
    });
    apps.flat_map(|dir| names(&dir))
        .filter_map(|(n, _)| n.strip_suffix(".cube").map(String::from))
        .collect()
}

pub fn kind(lines: &[String], name: &str) -> Option<&'static str> {
    let made = |n: &str| match n {
        "video" => Some("Video"),
        "sound" => Some("Audio"),
        "image" | "color" | "rect" | "line" | "path" => Some("Shape"),
        "text" | "caption" | "captions" => Some("Text"),
        "cat" | "stack" => Some("Group"),
        _ => None,
    };
    let mut name = name.to_string();
    for _ in 0..6 {
        if let Some(k) = made(&name) {
            return Some(k);
        }
        let defined = |l: &&String| {
            let words: Vec<&str> = l.split(|c| !ident(c)).filter(|w| !w.is_empty()).collect();
            words.first().is_some_and(|w| ["let", "const", "var"].contains(w)) && words.get(1) == Some(&name.as_str())
        };
        let line = lines.iter().find(defined)?;
        name = line.split_once('=')?.1.split(|c| !ident(c)).find(|w| !w.is_empty())?.to_string();
    }
    None
}

pub fn fits(method: &str, kind: Option<&str>) -> bool {
    let at = |name: &str| DEFS.iter().position(|d| d.name == name).unwrap_or(0);
    match kind {
        Some("Video" | "Audio" | "Shape") if ["shown", "each"].contains(&method) => false,
        Some(k) if method == "merge" && k != "Group" => false,
        Some("Audio") => at(method) < at("scale"),
        Some("Shape" | "Text") => !["trim", "speed", "volume", "mute"].contains(&method),
        _ => true,
    }
}

fn root(lines: &[String], row: usize, head: &[char]) -> String {
    let mut i = head.len();
    loop {
        let mut depth = 0;
        while i > 0 && (depth > 0 || head[i - 1] == ')') {
            depth += (head[i - 1] == ')') as i32 - (head[i - 1] == '(') as i32;
            i -= 1;
        }
        let name = word(&head[..i]);
        i -= name.len();
        if i == 0 || head[i - 1] != '.' {
            let above = lines[..row].iter().rev().find(|l| !l.trim_start().starts_with('.'));
            return match (name.is_empty() && head[..i].iter().all(|c| c.is_whitespace()), above) {
                (true, Some(l)) => {
                    let l: Vec<char> = l.trim_end().trim_end_matches(',').chars().collect();
                    root(&lines[..0], 0, &l)
                }
                _ => String::from_iter(name),
            };
        }
        i -= 1;
    }
}

fn inside(head: &[char]) -> String {
    let mut depth = 0;
    for i in (0..head.len()).rev() {
        match head[i] {
            ')' => depth += 1,
            '(' if depth == 0 => return String::from_iter(word(&head[..i])),
            '(' => depth -= 1,
            _ => {}
        }
    }
    String::new()
}

fn vars(lines: &[String]) -> Vec<String> {
    let (mut out, mut seen) = (vec![], HashSet::new());
    for line in lines {
        let words: Vec<&str> = line.split(|c| !ident(c)).filter(|w| !w.is_empty()).collect();
        for pair in words.windows(2) {
            if ["let", "const", "var"].contains(&pair[0]) && seen.insert(pair[1]) {
                out.push(pair[1].to_string());
            }
        }
    }
    out
}

pub fn offers(lines: &[String], (row, col): (usize, usize), dir: &Path) -> (usize, Vec<Offer>) {
    let chars: Vec<char> = lines.get(row).map_or(vec![], |l| l.chars().collect());
    let head = &chars[..col.min(chars.len())];
    let mut quote = None;
    for (i, &c) in head.iter().enumerate() {
        match quote {
            Some((q, _)) if q == c => quote = None,
            None if c == '"' || c == '\'' => quote = Some((c, i)),
            None if c == '/' && head.get(i + 1) == Some(&'/') => return (0, vec![]),
            _ => {}
        }
    }
    if let Some((_, q)) = quote {
        let before = String::from_iter(&head[..q]);
        if before.trim_end().ends_with("align:") {
            let typed = String::from_iter(&head[q + 1..]);
            let sides = ["left", "center", "right"].into_iter().filter(|n| starts(n, &typed));
            return (q + 1, sides.map(|n| (n.to_string(), "", "")).collect());
        }
        let font = before.trim_end().ends_with("font:");
        let Some(call) = before.trim_end().strip_suffix(if font { ':' } else { '(' }) else {
            return (0, vec![]);
        };
        let call = String::from_iter(word(&call.chars().collect::<Vec<_>>()));
        let look = call == "look";
        let look = look || call == "inputLut";
        if !look && !["video", "sound", "image", "captions", "font"].contains(&call.as_str()) {
            return (0, vec![]);
        }
        let start = head.iter().rposition(|c| *c == '/').filter(|s| *s > q).unwrap_or(q) + 1;
        let (folder, typed) = (String::from_iter(&head[q + 1..start]), String::from_iter(&head[start..]));
        let found = names(&dir.join(&folder)).into_iter();
        let named = font && folder.is_empty();
        let files = found.filter(|(n, d)| {
            *d || if font {
                !named && typeface(n)
            } else {
                !look || n.ends_with(".cube")
            }
        });
        let files = files.map(|(n, d)| if d { n + "/" } else { n });
        let builtin = match folder.is_empty() {
            true if look => premiere(),
            true if font => faces(dir),
            _ => vec![],
        };
        let fit = files.chain(builtin).filter(|n| starts(n, &typed));
        return (start, fit.map(|n| (n, "", "")).collect());
    }
    let typed = word(head);
    let start = head.len() - typed.len();
    let dot = start > 0 && head[start - 1] == '.';
    let owner = word(&head[..start.saturating_sub(1)]);
    let number = |w: &[char]| w.first().is_some_and(char::is_ascii_digit);
    let spread = dot && start > 1 && head[start - 2] == '.';
    if chars.get(head.len()).is_some_and(|c| ident(*c)) || number(typed) || dot && number(owner) || spread || !dot && typed.is_empty() {
        return (0, vec![]);
    }
    let (typed, axis) = (String::from_iter(typed), owner == ['p', 'o', 's']);
    let owner = root(lines, row, &head[..start.saturating_sub(1)]);
    let owner = kind(lines, &owner).filter(|_| dot);
    let fed = ["pos", "scale", "rotate", "opacity", "shown", "add", "mul"].contains(&inside(&head[..start]).as_str());
    if fed && owner.is_some_and(|k| !["Group", "Audio"].contains(&k)) {
        let reads: &[&str] = match axis {
            true => &["x", "y"],
            false => &["pos", "pos.x", "pos.y", "scale", "rotate", "opacity"],
        };
        let fit = reads.iter().filter(|n| starts(n, &typed));
        return (start, fit.map(|n| (n.to_string(), "", "")).collect());
    }
    let defs = DEFS
        .iter()
        .filter(|d| d.method == dot && (starts(d.name, &typed) || d.name == typed) && fits(d.name, owner));
    let defs = defs.map(|d| (d.name.to_string(), d.args, if d.args.is_empty() { "()" } else { "(" }));
    let vars = vars(lines).into_iter().filter(|v| !dot && starts(v, &typed));
    (start, defs.chain(vars.map(|v| (v, "", ""))).collect())
}

fn room(y: f32, row: f32, h: f32, offers: usize) -> (bool, usize) {
    let fits = |n: usize| n as f32 * row + 10. + if offers > n { 18. } else { 0. };
    let (below, above) = (h - 28. - y - row, y);
    let down = fits(offers.min(SHOWN)) <= below || below >= above;
    let space = if down { below } else { above };
    (down, (1..=offers.min(SHOWN)).rev().find(|n| fits(*n) <= space).unwrap_or(1))
}

pub fn fill(line: &str, start: usize, col: usize, name: &str, tail: &str) -> (String, usize) {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    let (head, rest) = (String::from_iter(&chars[..start.min(col)]), String::from_iter(&chars[col..]));
    let (open, tail) = match rest.starts_with('(') {
        true => (tail.len().min(1), ""),
        false => (tail.len(), tail),
    };
    (format!("{head}{name}{tail}{rest}"), start.min(col) + name.chars().count() + open)
}

impl Origami {
    pub fn suggest(&mut self, k: &str) {
        if let Some(at) = self.cursor.filter(|_| k == "backspace" || k.chars().count() == 1) {
            self.offers = offers(&self.lines, at, self.script.parent().unwrap_or(Path::new("")));
            self.pick = 0;
        }
    }

    pub fn choose(&mut self, k: &str, plain: bool) -> bool {
        let (start, list) = mem::take(&mut self.offers);
        let Some((r, c)) = self.cursor.filter(|(r, _)| !list.is_empty() && *r < self.lines.len()) else {
            return false;
        };
        match k {
            _ if !plain => return false,
            "up" | "down" => {
                self.pick = (self.pick + if k == "up" { list.len() - 1 } else { 1 }) % list.len();
                self.offers = (start, list);
            }
            "enter" | "tab" => {
                let (name, _, tail) = &list[self.pick];
                self.draft(|this| {
                    let (line, col) = fill(&this.lines[r], start, c, name, tail);
                    (this.lines[r], this.cursor, this.anchor) = (line, Some((r, col)), None);
                });
                self.suggest(if name.ends_with('/') { "/" } else { "" });
            }
            "escape" => {}
            _ => return false,
        }
        true
    }

    pub fn skim(&mut self, rows: f32) {
        self.slide -= rows;
        let last = self.offers.1.len().saturating_sub(1) as f32;
        self.pick = (self.pick as f32 + self.slide.trunc()).clamp(0., last) as usize;
        self.slide = self.slide.fract();
    }

    pub fn popup(&self, window: &Window) -> Option<Div> {
        let (start, list) = &self.offers;
        let (r, _) = self.cursor.filter(|_| !list.is_empty())?;
        let y = self.line_y(r)?;
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        let (glyph, row) = (self.glyph(), self.row());
        let wide = list.iter().map(|(n, a, _)| n.chars().count() + a.chars().count()).max()? as f32;
        let (down, shown) = room(y, row, h, list.len());
        let (wide, first) = ((wide + 2.) * glyph + 22., (self.pick + 1).saturating_sub(shown));
        let rows = list.iter().enumerate().skip(first).take(shown).map(|(i, (name, args, _))| {
            div()
                .h(px(row))
                .px(px(6.))
                .flex()
                .items_center()
                .gap(px(2. * glyph))
                .rounded(px(3.))
                .whitespace_nowrap()
                .when(i == self.pick, |d| d.bg(rgba(BLUE << 8 | 0x40)))
                .child(name.clone())
                .child(div().text_color(rgb(DIM)).child(*args))
        });
        Some(
            div()
                .absolute()
                .left(px((w - self.side + self.inset() + *start as f32 * glyph - 9.).min(w - wide - 4.)))
                .map(|d| match down {
                    true => d.top(px(y + row)),
                    false => d.bottom(px(h - y)),
                })
                .w(px(wide))
                .p_1()
                .flex()
                .flex_col()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(6.))
                .text_size(px(self.font))
                .children(rows)
                .children((list.len() > shown).then(|| {
                    div()
                        .px(px(6.))
                        .text_size(px(11.))
                        .text_color(rgb(DIM))
                        .child(format!("{} of {}", self.pick + 1, list.len()))
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> Vec<String> {
        let lines: Vec<String> = src.lines().map(String::from).collect();
        let col = lines.last().unwrap().chars().count();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        offers(&lines, (lines.len() - 1, col), &dir).1.into_iter().map(|o| o.0).collect()
    }

    // Autocomplete should offer only what makes sense where you are typing: methods that suit the kind
    // of clip, clip names, properties that can be followed, and nothing inside comments, strings or
    // numbers.
    #[test]
    fn names() {
        assert_eq!(at("clip.te"), ["temperature"]);
        assert_eq!(at("  .sh"), ["shift", "shown", "shadows", "sharpen", "shadowTint"]);
        assert_eq!(at("video(\"a\")\n  .sh"), ["shift", "shadows", "sharpen", "shadowTint"]);
        assert_eq!(at("const t = caption(\"a\")\nt.for(1).e"), ["each", "exposure"]);
        assert_eq!(at("let m = sound(\"a\")\nm.o"), [] as [&str; 0]);
        assert_eq!(at("x.opacity(k"), ["keys"]);
        assert_eq!(at("x.opacity(m"), ["mul"]);
        assert_eq!(at("x.d"), ["duotone", "displace"]);
        assert_eq!((at("stack(a).me"), at("pa")), (vec!["merge".to_string()], vec!["path".to_string()]));
        assert!(at("rect(4, 4).me").is_empty() && at("let m = sound(\"a\")\nm.l") == ["loop"]);
        let title = "const title = text(\"a\")\n";
        assert_eq!(
            at(&format!("{title}x.pos(title.")),
            ["pos", "pos.x", "pos.y", "scale", "rotate", "opacity"]
        );
        assert_eq!(at(&format!("{title}x.pos(0.5, add(title.pos.")), ["x", "y"]);
        assert_eq!(at(&format!("{title}x.opacity(mul(title.o")), ["opacity"]);
        assert_eq!(at(&format!("{title}stack(title.op")), ["opacity"]);
        assert_eq!(at(&format!("{title}x.pos(0.5).op")), ["opacity"]);
        assert_eq!(
            at("const clip = video()\nconst cut = c"),
            ["caption", "captions", "color", "cat", "clip", "cut"]
        );
        assert_eq!(
            (at("re"), at("li"), at("x.li")),
            (vec!["rect".to_string()], vec!["line".to_string()], vec![])
        );
        assert_eq!(at("text(\"a\", { align: \""), ["left", "center", "right"]);
        assert_eq!(at("text(\"a\", { align: \"ri"), ["right"]);
        let on = |src: &str| {
            at(src)
                .iter()
                .any(|n| ["trim", "speed", "volume", "mute", "shown", "each"].contains(&n.as_str()))
        };
        assert!(!on("rect(4, 4).") && !on("const l = line(0, 0, 1, 1)\nl.for(2).") && at("rect(4, 4).").contains(&"rotate".to_string()));
        assert_eq!(at("x.").len(), DEFS.iter().filter(|d| d.method).count());
        assert!(!at("let m = sound(\"a\")\nm.trim(0, 1).").contains(&"temperature".to_string()));
        assert!(at("let v = video(\"a\")\nv\n  .trim(0, 1)\n  .").contains(&"temperature".to_string()));
        assert!(!at("text(\"a\").").contains(&"trim".to_string()));
        for none in [" ", "stack(", "1.", "1.5", "// vi", "text(\"vi", "clip.zz", "...", "let abc\nabc"] {
            assert!(at(none).is_empty(), "{none}");
        }
    }

    // Inside a file path, autocomplete should offer the matching files and folders, and after font:
    // the fonts installed on this Mac by name.
    #[test]
    fn paths() {
        assert_eq!(at("video(\"ma"), ["main.rs"]);
        assert_eq!(at("x.look(\"../s"), ["src/"]);
        assert_eq!(at("x.look(\"../src/"), [] as [&str; 0]);
        assert!(at("sound('../").contains(&"Cargo.toml".to_string()));
        assert_eq!(at("text(\"a\", { font: \"Menl"), ["Menlo"]);
        assert!(at("text(\"a\", { font: \"").len() > 20 && at("text(\"a\", { font: \"../").contains(&"src/".to_string()));
    }

    // The suggestion list should open below the line being typed when it fits, above it otherwise,
    // and show fewer rows rather than reach past the top of the window or the status bar, never
    // covering the line itself.
    #[test]
    fn list_stays_in_the_window() {
        for (y, row, h, offers) in [
            (120., 22., 330., 39),
            (164., 22., 330., 39),
            (246., 34., 600., 39),
            (98., 22., 330., 3),
        ] {
            let (down, n) = room(y, row, h, offers);
            let tall = n as f32 * row + 10. + if offers > n { 18. } else { 0. };
            let (from, to) = if down { (y + row, y + row + tall) } else { (y - tall, y) };
            assert!(from >= 0. && to <= h - 28. && (to <= y || from >= y + row), "{y} {down} {n}");
        }
        assert_eq!(room(98., 22., 900., 39), (true, 8));
        assert_eq!(room(700., 22., 900., 39), (false, 8));
    }

    // Accepting a suggestion should replace the part already typed and leave the cursor in the right
    // place.
    #[test]
    fn accept() {
        assert_eq!(fill("  clip.te", 7, 9, "temperature", "("), ("  clip.temperature(".into(), 19));
        assert_eq!(fill("a.mu(1)", 2, 4, "mute", "()"), ("a.mute(1)".into(), 7));
        assert_eq!(fill("a.mu", 2, 4, "mute", "()"), ("a.mute()".into(), 8));
        assert_eq!(fill("cat(cl, b)", 4, 6, "clip", ""), ("cat(clip, b)".into(), 8));
        assert_eq!(
            fill("video(\"clips/in\")", 13, 15, "intro.mp4", ""),
            ("video(\"clips/intro.mp4\")".into(), 22)
        );
    }

    // Typing a name in a script of 100,000 variables should offer the matching ones, in order, in
    // moments.
    #[test]
    fn many_names_offer_quickly() {
        let start = std::time::Instant::now();
        let src: String = (0..100_000).map(|i| format!("const v{i} = 1\n")).collect();
        let want: Vec<String> = (0..10).map(|d| format!("v1234{d}")).collect();
        assert_eq!(at(&format!("{src}v1234")), want);
        assert!(start.elapsed().as_secs() < 2);
    }
}
