use crate::tool::output;
use std::{
    env,
    ffi::c_void,
    io::{self, Write},
    os::unix::ffi::OsStrExt,
    path::Path,
    process::{self, Command},
    ptr,
};

const FLAG: &str = "--origami-picture";
const KINDS: [&str; 7] = ["heic", "heif", "avif", "jxl", "webp", "jpg", "jpeg"];
const UTF8: u32 = 0x0800_0100;
const MEDIUM: i32 = 4;
const BGRA: u32 = 2 | 2 << 12;

type Ref = *const c_void;

#[repr(C)]
struct Rect(f64, f64, f64, f64);

#[repr(C)]
struct Turn(f64, f64, f64, f64, f64, f64);

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFURLCreateFromFileSystemRepresentation(alloc: Ref, path: *const u8, len: isize, dir: u8) -> Ref;
    fn CFDictionaryGetValue(dict: Ref, key: Ref) -> Ref;
    fn CFNumberGetValue(num: Ref, kind: isize, out: *mut c_void) -> u8;
    fn CFStringGetCString(text: Ref, out: *mut u8, len: isize, encoding: u32) -> u8;
    fn CFRelease(obj: Ref);
}

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    static kCGImagePropertyOrientation: Ref;
    fn CGImageSourceCreateWithURL(url: Ref, opts: Ref) -> Ref;
    fn CGImageSourceGetType(src: Ref) -> Ref;
    fn CGImageSourceCreateImageAtIndex(src: Ref, i: usize, opts: Ref) -> Ref;
    fn CGImageSourceCopyPropertiesAtIndex(src: Ref, i: usize, opts: Ref) -> Ref;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    static kCGColorSpaceSRGB: Ref;
    fn CGColorSpaceCreateWithName(name: Ref) -> Ref;
    fn CGImageGetWidth(img: Ref) -> usize;
    fn CGImageGetHeight(img: Ref) -> usize;
    fn CGImageGetDataProvider(img: Ref) -> Ref;
    fn CGDataProviderCopyData(data: Ref) -> Ref;
    fn CGBitmapContextCreate(data: *mut c_void, w: usize, h: usize, bits: usize, row: usize, space: Ref, info: u32) -> Ref;
    fn CGContextSetInterpolationQuality(cx: Ref, quality: i32);
    fn CGContextConcatCTM(cx: Ref, turn: Turn);
    fn CGContextDrawImage(cx: Ref, rect: Rect, img: Ref);
}

struct Held(Ref);

impl Drop for Held {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

fn held(obj: Ref) -> Option<Held> {
    (!obj.is_null()).then(|| Held(obj))
}

fn read(path: &Path, size: Option<(usize, usize)>) -> Option<(Vec<u8>, usize, usize)> {
    unsafe {
        let name = path.as_os_str().as_bytes();
        let url = held(CFURLCreateFromFileSystemRepresentation(
            ptr::null(),
            name.as_ptr(),
            name.len() as isize,
            0,
        ))?;
        let src = held(CGImageSourceCreateWithURL(url.0, ptr::null()))?;
        let img = held(CGImageSourceCreateImageAtIndex(src.0, 0, ptr::null()))?;
        let (mut turn, mut kind) = (1i32, [0u8; 32]);
        if let Some(props) = held(CGImageSourceCopyPropertiesAtIndex(src.0, 0, ptr::null())) {
            let n = CFDictionaryGetValue(props.0, kCGImagePropertyOrientation);
            if !n.is_null() {
                CFNumberGetValue(n, 3, (&raw mut turn).cast());
            }
        }
        let named = CGImageSourceGetType(src.0);
        let jpeg = !named.is_null() && CFStringGetCString(named, kind.as_mut_ptr(), 32, UTF8) != 0 && kind.starts_with(b"public.jpeg\0");
        let (iw, ih) = (CGImageGetWidth(img.0), CGImageGetHeight(img.0));
        if jpeg && !(2..=8).contains(&turn) || iw.min(ih) == 0 || iw.saturating_mul(ih) > 1 << 28 {
            return None;
        }
        let decodes = || held(CGDataProviderCopyData(CGImageGetDataProvider(img.0))).is_some();
        let (nw, nh) = if (5..=8).contains(&turn) { (ih, iw) } else { (iw, ih) };
        let Some((w, h)) = size else {
            return decodes().then(|| (vec![], nw, nh));
        };
        let mut px = vec![0u8; w * h * 4];
        let space = held(CGColorSpaceCreateWithName(kCGColorSpaceSRGB))?;
        let cx = held(CGBitmapContextCreate(px.as_mut_ptr().cast(), w, h, 8, w * 4, space.0, BGRA))?;
        CGContextSetInterpolationQuality(cx.0, MEDIUM);
        let (a, b, c, d, x, y) = match turn {
            2 => (-1., 0., 0., 1., 1., 0.),
            3 => (-1., 0., 0., -1., 1., 1.),
            4 => (1., 0., 0., -1., 0., 1.),
            5 => (0., -1., -1., 0., 1., 1.),
            6 => (0., -1., 1., 0., 0., 1.),
            7 => (0., 1., 1., 0., 0., 0.),
            8 => (0., 1., -1., 0., 1., 0.),
            _ => (1., 0., 0., 1., 0., 0.),
        };
        let (fw, fh) = (w as f64, h as f64);
        CGContextConcatCTM(cx.0, Turn(a * fw, b * fh, c * fw, d * fh, x * fw, y * fh));
        CGContextDrawImage(cx.0, Rect(0., 0., 1., 1.), img.0);
        drop(cx);
        if px.iter().all(|v| *v == 0) && !decodes() {
            return None;
        }
        for p in px.chunks_exact_mut(4) {
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                for v in &mut p[..3] {
                    *v = ((*v as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Some((px, w, h))
    }
}

pub(crate) fn serve() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.first().is_none_or(|a| a != FLAG) {
        return;
    }
    let side = |i: usize| args.get(i)?.to_str()?.parse().ok().filter(|n| (1..=16384).contains(n));
    let size = side(2).zip(side(3));
    let made = args.get(1).filter(|_| size.is_some() || args.len() == 2);
    let sent = made.and_then(|p| read(Path::new(p), size)).and_then(|(px, w, h)| {
        let mut out = io::stdout().lock();
        let wrote = match size {
            Some(_) => out.write_all(&px),
            None => write!(out, "{w} {h}"),
        };
        wrote.and_then(|_| out.flush()).ok()
    });
    process::exit(sent.is_none() as i32)
}

fn ask(path: &Path, size: &[String]) -> Option<Vec<u8>> {
    let kind = path.extension()?.to_str()?.to_ascii_lowercase();
    let me = env::current_exe().ok().filter(|_| KINDS.contains(&kind.as_str()))?;
    let out = output(Command::new(me).arg(FLAG).arg(path).args(size)).ok()?;
    out.status.success().then_some(out.stdout)
}

pub fn picture(path: &str, w: usize, h: usize) -> Option<Vec<u8>> {
    ask(path.as_ref(), &[w.to_string(), h.to_string()]).filter(|px| px.len() == w * h * 4)
}

pub(crate) fn size(path: &Path) -> Option<(u32, u32)> {
    let said = String::from_utf8(ask(path, &[])?).ok()?;
    let (w, h) = said.split_once(' ')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ffmpeg, status, tests::scratch};
    use std::fs;

    // A JPEG with an EXIF turn is read at its upright size with its corners where the tag puts them, an
    // upright JPEG is left for ffmpeg, a half see-through picture keeps its own colour and alpha, a picture
    // is scaled to the size asked for, and a file cut short or that is no picture gives nothing.
    #[test]
    fn pictures_are_read_upright() {
        let dir = scratch("picture");
        let make = |src: &str, file: &str| {
            let args = ["-v", "error", "-f", "lavfi", "-i", src, "-frames:v", "1", file];
            assert!(status(ffmpeg().args(args).current_dir(&dir)).unwrap().success());
        };
        make("color=black:s=64x36,drawbox=w=32:h=18:color=white:t=fill", "corner.jpg");
        make("color=red@0.5:s=8x8,format=rgba", "clear.png");
        let jpeg = fs::read(dir.join("corner.jpg")).unwrap();
        let turned = |turn: u8| {
            let exif = [
                b"\xff\xe1\0\x22Exif\0\0MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0",
                &[turn][..],
                &[0; 6],
            ]
            .concat();
            let file = dir.join(format!("turn{turn}.jpg"));
            fs::write(&file, [&jpeg[..2], &exif, &jpeg[2..]].concat()).unwrap();
            file
        };
        assert!(read(&dir.join("corner.jpg"), None).is_none() && read(&turned(1), Some((64, 36))).is_none());
        let lit = (2..=8).map(|turn| {
            let (_, w, h) = read(&turned(turn), None).unwrap();
            let (px, ..) = read(&turned(turn), Some((w, h))).unwrap();
            let at = |x: usize, y: usize| px[(y * w + x) * 4] > 128;
            (
                (w, h),
                [
                    at(w / 4, h / 4),
                    at(w * 3 / 4, h / 4),
                    at(w * 3 / 4, h * 3 / 4),
                    at(w / 4, h * 3 / 4),
                ],
            )
        });
        let corner = |n: usize| std::array::from_fn(|i| i == n);
        let (wide, tall) = ((64, 36), (36, 64));
        let want = [(wide, 1), (wide, 2), (wide, 3), (tall, 0), (tall, 1), (tall, 2), (tall, 3)].map(|(size, n)| (size, corner(n)));
        assert_eq!(lit.collect::<Vec<_>>(), want);
        let clear = dir.join("clear.png");
        let (px, w, h) = read(&clear, Some((8, 8))).unwrap();
        assert!(
            (w, h) == (8, 8) && px[..3] == [0, 0, 255] && px[3].abs_diff(128) < 2,
            "{:?}",
            &px[..4]
        );
        assert_eq!(read(&clear, None).map(|r| (r.1, r.2)), Some((8, 8)));
        assert_eq!(read(&turned(6), Some((18, 32))).map(|r| r.0.len()), Some(18 * 32 * 4));
        let sips = Command::new("/usr/bin/sips")
            .args(["-s", "format", "heic", "corner.jpg", "--out", "cut.heic"])
            .current_dir(&dir)
            .output();
        assert!(sips.unwrap().status.success() && read(&dir.join("cut.heic"), None).map(|r| (r.1, r.2)) == Some(wide));
        let whole = fs::read(dir.join("cut.heic")).unwrap();
        fs::write(dir.join("cut.heic"), &whole[..whole.len() / 2]).unwrap();
        fs::write(dir.join("junk.jpg"), "not a picture").unwrap();
        for file in ["cut.heic", "junk.jpg", "none.jpg"] {
            assert!(
                read(&dir.join(file), None).is_none() && read(&dir.join(file), Some((8, 8))).is_none(),
                "{file}"
            );
        }
    }
}
