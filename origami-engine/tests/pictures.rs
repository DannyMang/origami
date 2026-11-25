use origami_lang::{ffmpeg, output, status};
use std::{
    env, fs,
    path::Path,
    process::{Command, Stdio},
};

fn origami(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_origami"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(if out.status.success() { &out.stdout } else { &out.stderr }).into_owned()
}

fn made(dir: &Path, tool: &mut Command, args: &[&str]) {
    assert!(
        status(tool.args(args).current_dir(dir).stdout(Stdio::null())).unwrap().success(),
        "{args:?}"
    );
}

fn grey(dir: &Path, file: &str, (w, h): (usize, usize)) -> Vec<u8> {
    let args = [
        "-v",
        "error",
        "-i",
        file,
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "gray",
        "-",
    ];
    let px = output(ffmpeg().args(args).current_dir(dir)).unwrap().stdout;
    assert_eq!(px.len(), w * h, "{file}");
    px
}

fn turned(jpeg: &[u8]) -> Vec<u8> {
    let exif = b"\xff\xe1\x00\x22Exif\0\0MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0\0\0\0\0";
    [&jpeg[..2], exif, &jpeg[2..]].concat()
}

// A HEIC picture, the tiled kind phones write, and a JPEG turned on its side by its EXIF tag are each read
// at their upright size, drawn the right way up in the preview and exported with the same picture, and a
// dry run names a layer file that is there to be read, while an upright JPEG is read as before.
#[test]
fn pictures_macos_reads_show_and_export() {
    let dir = env::temp_dir().join("origami-engine-pictures");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let lavfi = |src: &str, file: &str| {
        made(
            &dir,
            &mut ffmpeg(),
            &["-v", "error", "-f", "lavfi", "-i", src, "-frames:v", "1", file],
        )
    };
    let sips = |from: &str, to: &str| {
        made(
            &dir,
            &mut Command::new("/usr/bin/sips"),
            &["-s", "format", "heic", from, "--out", to],
        )
    };
    lavfi("color=black:s=64x36,drawbox=w=32:h=36:color=white:t=fill", "half.png");
    lavfi("color=black:s=64x36,drawbox=w=32:h=36:color=white:t=fill", "half.jpg");
    lavfi("testsrc2=s=4032x3024", "big.png");
    sips("half.png", "half.heic");
    sips("big.png", "big.heic");
    fs::write(dir.join("turned.jpg"), turned(&fs::read(dir.join("half.jpg")).unwrap())).unwrap();
    let script = |src: &str| fs::write(dir.join("edit.js"), src).unwrap();
    let size = |file: &str| {
        script(&format!("image('{file}').for(1)"));
        let tl: serde_json::Value = serde_json::from_str(&origami(&dir, &["ir", "edit.js"])).unwrap();
        (tl["clips"][0]["w"].as_u64().unwrap(), tl["clips"][0]["h"].as_u64().unwrap())
    };
    assert_eq!(
        ["half.heic", "big.heic", "turned.jpg", "half.jpg"].map(size),
        [(64, 36), (4032, 3024), (36, 64), (64, 36)]
    );
    for (file, (w, h), white, black) in [
        ("half.heic", (64, 36), 18 * 64 + 8, 18 * 64 + 56),
        ("turned.jpg", (36, 64), 8 * 36 + 18, 56 * 36 + 18),
    ] {
        script(&format!("settings({{ size: [{w}, {h}] }})\nimage('{file}').for(1)"));
        assert_eq!(origami(&dir, &["frame", "edit.js", "0.5", "shown.png"]), "", "{file}");
        assert_eq!(origami(&dir, &["render", "edit.js", "out.mp4"]), "", "{file}");
        for px in [grey(&dir, "shown.png", (w, h)), grey(&dir, "out.mp4", (w, h))] {
            assert!(px[white] > 200 && px[black] < 60, "{file}: {} {}", px[white], px[black]);
        }
        let line = origami(&dir, &["render", "edit.js", "out.mp4", "--dry-run"]);
        let layer = line.split('\'').find(|a| a.ends_with("image-0.bgra")).unwrap();
        assert_eq!(fs::metadata(layer).unwrap().len(), (w * h * 4) as u64);
        fs::remove_dir_all(Path::new(layer).parent().unwrap()).unwrap();
    }
    script("settings({ size: [64, 48] })\nimage('big.heic').for(1).scale(64 / 4032)");
    assert_eq!(origami(&dir, &["frame", "edit.js", "0.5", "shown.png"]), "");
    let px = grey(&dir, "shown.png", (64, 48));
    assert!(px.iter().max().unwrap() - px.iter().min().unwrap() > 100);
    script("image('half.jpg').for(1)");
    assert!(!origami(&dir, &["render", "edit.js", "out.mp4", "--dry-run"]).contains(".bgra"));
}

// Once origami.lock knows an upright JPEG, drawing and exporting it start only the command and its script
// helper, never the picture reader, and when the file gains a turn tag the next read of the script marks it
// for the reader, which is then started to draw it.
#[test]
fn upright_jpegs_start_no_reader() {
    let dir = env::temp_dir().join("origami-engine-upright");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let src = "color=black:s=64x36,drawbox=w=32:h=36:color=white:t=fill";
    made(
        &dir,
        &mut ffmpeg(),
        &["-v", "error", "-f", "lavfi", "-i", src, "-frames:v", "1", "a.jpg"],
    );
    fs::write(dir.join("edit.js"), "settings({ size: [64, 64] })\nimage('a.jpg').for(1)").unwrap();
    let marked = || {
        let tl: serde_json::Value = serde_json::from_str(&origami(&dir, &["ir", "edit.js"])).unwrap();
        (tl["clips"][0]["native"].as_bool(), tl["clips"][0]["w"].as_u64())
    };
    let starts = |args: &[&str]| {
        let log = dir.join("dyld.txt");
        let _ = fs::remove_file(&log);
        let run = Command::new(env!("CARGO_BIN_EXE_origami"))
            .args(args)
            .current_dir(&dir)
            .env("DYLD_PRINT_LIBRARIES", "1")
            .env("DYLD_PRINT_TO_FILE", &log)
            .output();
        assert!(run.unwrap().status.success(), "{args:?}");
        let loads = fs::read_to_string(&log).unwrap();
        loads.lines().filter(|l| l.ends_with(env!("CARGO_BIN_EXE_origami"))).count()
    };
    assert_eq!(marked(), (None, Some(64)));
    assert_eq!(
        [&["frame", "edit.js", "0.5", "shown.png"][..], &["render", "edit.js", "out.mp4"]].map(starts),
        [2, 2]
    );
    fs::write(dir.join("a.jpg"), turned(&fs::read(dir.join("a.jpg")).unwrap())).unwrap();
    assert_eq!(marked(), (Some(true), Some(36)));
    assert_eq!(
        [&["frame", "edit.js", "0.5", "shown.png"][..], &["render", "edit.js", "out.mp4"]].map(starts),
        [3, 3]
    );
}
