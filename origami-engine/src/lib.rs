mod audio;
mod export;
mod fx;
mod grade;
mod lut;
mod memo;
mod mic;
mod pixels;
mod place;
mod preview;
mod shape;
mod text;
mod timing;

pub use audio::Player;
pub use export::{Codec, Opts, guard, halt, render, shell, sized};
pub use mic::{Mic, inputs};
pub use place::{footprint, place, seat};
pub use preview::{Preview, near, peaks, thumb, warm};
pub use text::tone;

#[cfg(test)]
mod tests {
    use origami_lang::{Clip, Timeline, ffmpeg, status};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::RwLock,
    };

    pub static TIDY: RwLock<()> = RwLock::new(());

    pub fn clip(kind: &str, more: &str) -> Clip {
        let base = r##""src":"","w":40,"h":20,"start":0,"end":1,"src_in":0,"speed":1,"z":0,"scale":1,"pos":[null,null],
            "gain":1,"fade":[0,0],"label":"","line":1,"video":true,"audio":false,"size":6,"color":"#ff000080","opacity":1,"rotate":0,"shown":1"##;
        serde_json::from_str(&format!("{{\"kind\":\"{kind}\",{base}{more}}}")).unwrap()
    }

    pub fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("origami-engine-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn timeline(fps: u32, size: (u32, u32), duration: f64, clips: Vec<Clip>) -> Timeline {
        Timeline {
            fps,
            size,
            duration,
            clips,
            ..Default::default()
        }
    }

    pub fn script(dir: &Path, src: &str) -> Timeline {
        fs::write(dir.join("edit.js"), src).unwrap();
        origami_lang::eval(&dir.join("edit.js"), false, crate::footprint).unwrap()
    }

    pub fn film(out: &Path, src: &str, more: &[&str]) {
        let made = status(ffmpeg().args(["-v", "error", "-y", "-f", "lavfi", "-i", src]).args(more).arg(out));
        assert!(made.unwrap().success());
    }
}
