use crate::{
    Origami, crash,
    keys::stamp,
    pulse,
    view::{BAR, BG, BLUE, BORDER, DIM, INK, MUTED, PANEL, TEXT},
};
use gpui::{App, Context, Div, MouseButton, MouseDownEvent, PromptLevel, Window, div, prelude::*, px, rgb, rgba};
use origami_engine::{Codec, Opts, render, sized};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct Ship {
    path: PathBuf,
    opts: Opts,
    clip: bool,
}

const SIZES: [(u32, &str); 4] = [(0, "Timeline"), (2160, "4K"), (1080, "1080p"), (720, "720p")];
const CODECS: [(Codec, &str); 3] = [(Codec::H264, "H.264"), (Codec::Hevc, "HEVC"), (Codec::ProRes, "ProRes 422")];
const GRADES: [(u8, &str); 3] = [(80, "High"), (70, "Standard"), (50, "Small")];

fn bits(o: &Opts) -> f64 {
    match (o.codec, o.quality) {
        (Codec::ProRes, _) => 1.8,
        (Codec::H264, 80) => 0.48,
        (Codec::H264, 70) => 0.26,
        (Codec::Hevc, 80) => 0.33,
        (Codec::Hevc, 70) => 0.21,
        _ => 0.08,
    }
}

fn suffix(path: &Path, codec: Codec) -> PathBuf {
    path.with_extension(if codec == Codec::ProRes { "mov" } else { "mp4" })
}

fn named(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().to_string()
}

fn free(path: &Path) -> PathBuf {
    let (stem, ext) = (
        path.file_stem().unwrap_or_default().to_string_lossy(),
        path.extension().unwrap_or_default().to_string_lossy(),
    );
    (2..)
        .map(|n| path.with_file_name(format!("{stem} {n}.{ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

fn home(path: &Path) -> String {
    let p = path.to_string_lossy().to_string();
    match env::var("HOME") {
        Ok(h) if p.starts_with(&h) => format!("~{}", &p[h.len()..]),
        _ => p,
    }
}

fn hit(cx: &Context<Origami>, f: impl Fn(&mut Ship) + 'static) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _, _, cx| {
        if let Some(s) = &mut this.ship {
            f(s);
        }
        cx.stop_propagation();
        cx.notify();
    })
}

fn chip(label: &str, sub: Option<String>, on: bool, live: bool) -> Div {
    div()
        .h(px(24.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(4.))
        .whitespace_nowrap()
        .text_color(rgb(if on {
            TEXT
        } else if live {
            MUTED
        } else {
            DIM
        }))
        .when(on, |d| d.bg(rgb(BAR)))
        .when(live && !on, |d| d.cursor_pointer().hover(|s| s.text_color(rgb(TEXT))))
        .child(label.to_string())
        .children(sub.map(|s| div().text_size(px(11.)).text_color(rgb(DIM)).child(s)))
}

fn group(chips: Vec<Div>) -> Div {
    div()
        .flex()
        .gap(px(2.))
        .p(px(2.))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded(px(6.))
        .children(chips)
}

fn hint(text: &str) -> Div {
    div().text_size(px(11.)).text_color(rgb(DIM)).child(text.to_string())
}

fn row(label: &str, body: Div) -> Div {
    div()
        .flex()
        .gap(px(16.))
        .child(
            div()
                .w(px(96.))
                .flex_none()
                .pt(px(6.))
                .text_color(rgb(MUTED))
                .child(label.to_string()),
        )
        .child(body.flex_1().min_w_0().flex().flex_col().items_start().gap(px(6.)))
}

fn button(label: &str, main: bool) -> Div {
    div()
        .h(px(30.))
        .px(px(16.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(6.))
        .cursor_pointer()
        .when(main, |d| d.bg(rgb(BLUE)).text_color(rgb(INK)))
        .when(!main, |d| d.border_1().border_color(rgb(BORDER)).hover(|s| s.bg(rgb(BAR))))
        .child(label.to_string())
}

impl Origami {
    fn kept(&self) -> PathBuf {
        crash::kept(&self.script, "export")
    }

    pub fn ask(&mut self) {
        if self.export.is_some() {
            return self.note = "an export is already running: wait for it to finish".into();
        }
        let dir = self.script.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = dir.file_name().map_or("out".into(), |n| n.to_string_lossy().to_string());
        let kept = origami_lang::load(&self.kept()).ok();
        let kept: Option<(PathBuf, u32, usize, u8, bool)> = kept.and_then(|s| serde_json::from_str(&s).ok());
        self.ship = Some(match kept {
            Some((path, height, codec, quality, audio)) if path.is_absolute() => Ship {
                path,
                opts: Opts {
                    height: if SIZES.iter().any(|s| s.0 == height) { height } else { 0 },
                    codec: CODECS.get(codec).map_or(Codec::H264, |c| c.0),
                    quality: if GRADES.iter().any(|g| g.0 == quality) { quality } else { 80 },
                    audio,
                    range: None,
                },
                clip: false,
            },
            _ => Ship {
                path: dir.join(format!("{name}.mp4")),
                opts: Opts {
                    quality: 80,
                    ..Opts::default()
                },
                clip: false,
            },
        });
    }

    pub fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.ship.as_ref().map(|s| s.path.clone()).filter(|p| p.exists()) else {
            return self.ship_out(None, cx);
        };
        let both = free(&path);
        let ask = format!("Replace it, or keep it and save this export as \u{201c}{}\u{201d}?", named(&both));
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("\u{201c}{}\u{201d} already exists", named(&path)),
            Some(&ask),
            &["Replace", "Keep Both", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let pick = answer.await;
            let _ = this.update(cx, |this, cx| {
                match pick {
                    Ok(0) => this.ship_out(None, cx),
                    Ok(1) => this.ship_out(Some(both), cx),
                    _ => return,
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn ship_out(&mut self, out: Option<PathBuf>, cx: &mut Context<Self>) {
        let Some(mut s) = self.ship.take() else { return };
        let codec = CODECS.iter().position(|c| c.0 == s.opts.codec).unwrap_or(0);
        let kept = (&s.path, s.opts.height, codec, s.opts.quality, s.opts.audio);
        let _ = self.kept().parent().map(fs::create_dir_all);
        let _ = origami_lang::save(&self.kept(), serde_json::to_string(&kept).unwrap_or_default());
        s.opts.range = self.opened().filter(|_| s.clip).map(|c| (c.start, c.end));
        self.send(out.unwrap_or(s.path).to_string_lossy().to_string(), s.opts, cx);
    }

    pub fn send(&mut self, out: String, opts: Opts, cx: &mut Context<Self>) {
        if self.export.is_some() {
            return self.note = "an export is already running: wait for it to finish".into();
        }
        let (tl, job) = (self.tl.clone(), Arc::new(Mutex::new(("exporting 0%".to_string(), false))));
        self.export = Some(job.clone());
        let work = async move {
            let mut last = String::new();
            let done = render(&tl, &out, &opts, |p| {
                let note = format!("exporting {:.0}%", p * 100.);
                if note != last {
                    job.lock().unwrap().0 = note.clone();
                    last = note;
                }
            });
            pulse::exported(&tl, &opts, done.is_ok());
            *job.lock().unwrap() = (done.map_or_else(|e| e.to_string(), |_| format!("exported {out}")), true);
        };
        cx.background_executor().spawn(work).detach();
    }

    fn locate(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &self.ship else { return };
        let dir = s.path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = named(&s.path);
        let path = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = path.await {
                let _ = this.update(cx, |this, cx| {
                    if let Some(s) = &mut this.ship {
                        s.path = suffix(&path, s.opts.codec);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub fn dialog(&self, cx: &mut Context<Self>) -> Option<Div> {
        let s = self.ship.as_ref()?;
        let (tl, o) = (&self.tl, s.opts);
        let (_, (w, h)) = sized(tl, &o);
        let clip = self.opened();
        let (from, to) = match (s.clip, clip) {
            (true, Some(c)) => (c.start, c.end),
            _ => (0., tl.duration),
        };
        let audio = if o.audio { 256e3 } else { 0. };
        let mb = (bits(&o) * (w * h) as f64 * tl.fps as f64 + audio) * (to - from) / 8e6;
        let name = self
            .script
            .parent()
            .and_then(|p| p.file_name())
            .unwrap_or_default()
            .to_string_lossy();
        let file = named(&s.path);
        let head = format!(
            "{name} \u{b7} {} \u{b7} timeline {}\u{d7}{} \u{b7} {} fps",
            stamp(tl.duration),
            tl.size.0,
            tl.size.1,
            tl.fps
        );
        let sizes = SIZES.iter().map(|&(height, label)| {
            let sub = (height == 0).then(|| format!("{}\u{d7}{}", tl.size.0, tl.size.1));
            chip(label, sub, o.height == height, true).on_mouse_down(MouseButton::Left, hit(cx, move |s| s.opts.height = height))
        });
        let codecs = CODECS.iter().map(|&(codec, label)| {
            chip(label, None, o.codec == codec, true).on_mouse_down(
                MouseButton::Left,
                hit(cx, move |s| {
                    s.opts.codec = codec;
                    s.path = suffix(&s.path, codec);
                }),
            )
        });
        let pro = o.codec == Codec::ProRes;
        let grades = GRADES.iter().map(|&(q, label)| {
            let c = chip(label, None, !pro && o.quality == q, !pro);
            c.when(!pro, |c| c.on_mouse_down(MouseButton::Left, hit(cx, move |s| s.opts.quality = q)))
        });
        let whole =
            chip("Whole timeline", None, !s.clip || clip.is_none(), true).on_mouse_down(MouseButton::Left, hit(cx, |s| s.clip = false));
        let picked = chip("Selected clip", None, s.clip && clip.is_some(), clip.is_some())
            .when(clip.is_some(), |c| c.on_mouse_down(MouseButton::Left, hit(cx, |s| s.clip = true)));
        let span = match (s.clip, clip) {
            (true, Some(c)) => format!("{} \u{b7} {}\u{2013}{}", c.label, stamp(c.start), stamp(c.end)),
            (false, Some(_)) => format!("0:00\u{2013}{}", stamp(tl.duration)),
            (_, None) => format!(
                "0:00\u{2013}{} \u{b7} select a clip on the timeline to export only that clip",
                stamp(tl.duration)
            ),
        };
        let switch = div()
            .w(px(30.))
            .h(px(18.))
            .p(px(2.))
            .flex()
            .rounded(px(9.))
            .cursor_pointer()
            .bg(rgb(if o.audio { BLUE } else { BAR }))
            .when(o.audio, |d| d.justify_end())
            .child(div().size(px(14.)).rounded(px(7.)).bg(rgb(if o.audio { INK } else { MUTED })))
            .on_mouse_down(MouseButton::Left, hit(cx, |s| s.opts.audio = !s.opts.audio));
        let saving = div()
            .flex()
            .gap(px(8.))
            .w_full()
            .child(
                div()
                    .flex_1()
                    .h(px(30.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .child(file)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.locate(cx))),
            )
            .child(button("Choose\u{2026}", false).on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.locate(cx))));
        let say = match o.codec {
            Codec::H264 => "Plays everywhere.",
            Codec::Hevc => "Smaller files; plays on Apple devices and most recent players.",
            Codec::ProRes => "Large files for editing further in another app, saved as .mov.",
        };
        let body = div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .px(px(24.))
            .py(px(20.))
            .child(row("Save as", div().child(saving).child(hint(&home(&s.path)))))
            .child(row(
                "Resolution",
                div().child(group(sizes.collect())).child(hint(
                    "4K draws text and name tags sharp at full size; video is scaled up from the clips.",
                )),
            ))
            .child(row("Format", div().child(group(codecs.collect())).child(hint(say))))
            .child(row(
                "Quality",
                div().child(group(grades.collect())).child(hint(match pro {
                    true => "ProRes has one quality.",
                    false => "High for the final cut, Small for a quick draft.",
                })),
            ))
            .child(row("Range", div().child(group(vec![whole, picked])).child(hint(&span))))
            .child(row(
                "Audio",
                div().child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(switch)
                        .child("Include audio")
                        .child(div().text_size(px(11.)).text_color(rgb(DIM)).child("AAC 256 kb/s")),
                ),
            ));
        let foot = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(24.))
            .py(px(14.))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(format!("\u{2248} {mb:.0} MB \u{b7} {w}\u{d7}{h} \u{b7} {}", stamp(to - from)))
                    .child(hint("Remembered for this project")),
            )
            .child(button("Cancel", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.ship = None;
                    cx.notify();
                }),
            ))
            .child(
                button("Export", true)
                    .child(div().text_size(px(11.)).child("\u{21b5}"))
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.go(window, cx))),
            );
        let panel = div()
            .w(px(600.))
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(10.))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .px(px(24.))
                    .pt(px(20.))
                    .pb(px(16.))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(div().text_size(px(15.)).font_weight(gpui::FontWeight::BOLD).child("Export"))
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(head)),
            )
            .child(body)
            .child(foot);
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(96.))
                .bg(rgba(0x13161ba6))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.ship = None;
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;

    pub static ONE: Mutex<()> = Mutex::new(());

    // The export dialog opens on the project's name at High quality, Enter exports what was picked (here
    // ProRes at twice the size, only the selected clip, as a .mov) and the next opening remembers the file
    // and format but not a size the dialog does not offer, Esc closes it without exporting, and exporting
    // onto a file that already exists asks first, Cancel leaving the dialog open and the file alone, Keep
    // Both saving the new export beside it with a number and Replace writing over it.
    #[gpui::test]
    fn exports_what_is_picked(cx: &mut gpui::TestAppContext) {
        let _one = ONE.lock();
        let dir = env::temp_dir().join("origami-ship-test").join("trip");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_file(crash::kept(&dir.join("edit.js"), "export"));
        fs::create_dir_all(&dir).unwrap();
        let src = "settings({ size: [64, 36] })\ncat(color('#ff0000').for(1), color('#00ff00').for(1))";
        fs::write(dir.join("edit.js"), src).unwrap();
        let tl = origami_lang::eval(&dir.join("edit.js"), false, origami_engine::footprint).unwrap();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.tl) = (dir.join("edit.js"), Arc::new(tl));
            window.focus(&app.focus);
            app
        });
        view.update(cx, |o, _| {
            o.ask();
            let s = o.ship.as_mut().unwrap();
            assert_eq!((s.path.clone(), s.opts.quality), (dir.join("trip.mp4"), 80));
            (s.opts.codec, s.path, s.opts.height, s.clip) = (Codec::ProRes, suffix(&s.path, Codec::ProRes), 72, true);
            o.sel = Some(1);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        let job = view.update(cx, |o, _| o.export.clone().unwrap());
        let (note, done) = job.lock().unwrap().clone();
        assert!(done && note.ends_with("trip.mov"), "{note}");
        let out = dir.join("trip.mov").to_string_lossy().to_string();
        let args = ["-v", "error", "-i", &out, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"];
        let px = origami_lang::output(origami_lang::ffmpeg().args(args)).unwrap().stdout;
        assert_eq!(px.len(), 128 * 72 * 3 * 30);
        assert!(px[(36 * 128 + 64) * 3 + 1] > 200 && px[(36 * 128 + 64) * 3] < 60);
        view.update(cx, |o, _| {
            o.export = None;
            o.ask();
            let s = o.ship.as_ref().unwrap();
            assert_eq!(
                (s.path.clone(), s.opts.codec, s.opts.height, s.clip),
                (dir.join("trip.mov"), Codec::ProRes, 0, false)
            );
        });
        cx.simulate_keystrokes("escape");
        assert!(view.update(cx, |o, _| o.ship.is_none() && o.export.is_none()));
        let before = fs::metadata(&out).unwrap().modified().unwrap();
        let again = |cx: &mut gpui::VisualTestContext, answer: &str| {
            view.update(cx, |o, _| (o.export, o.ship) = (None, None));
            view.update(cx, |o, _| o.ask());
            cx.simulate_keystrokes("enter");
            assert_eq!(
                cx.pending_prompt().map(|p| p.0),
                Some("\u{201c}trip.mov\u{201d} already exists".into())
            );
            cx.simulate_prompt_answer(answer);
            cx.run_until_parked();
            view.update(cx, |o, _| (o.ship.is_some(), o.export.is_some()))
        };
        assert_eq!(again(cx, "Cancel"), (true, false));
        assert_eq!(fs::metadata(&out).unwrap().modified().unwrap(), before);
        assert_eq!(again(cx, "Keep Both"), (false, true));
        assert!(dir.join("trip 2.mov").exists() && fs::metadata(&out).unwrap().modified().unwrap() == before);
        assert_eq!(again(cx, "Replace"), (false, true));
        assert!(fs::metadata(&out).unwrap().modified().unwrap() > before);
    }

    // A remembered destination shipped inside a downloaded project, such as a web address in its
    // .origami folder, is ignored and the dialog offers the file next to the script, and a remembered
    // size or quality the dialog does not offer opens on the default one instead.
    #[gpui::test]
    fn ignores_a_remembered_web_address(cx: &mut gpui::TestAppContext) {
        let dir = env::temp_dir().join("origami-ship-test").join("far");
        fs::create_dir_all(dir.join(".origami")).unwrap();
        fs::write(dir.join(".origami/export.json"), r#"["http://127.0.0.1:9/a.ts",0,0,80,true]"#).unwrap();
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.script = dir.join("edit.js");
            app
        });
        let kept = view.update(cx, |o, _| o.kept());
        fs::create_dir_all(kept.parent().unwrap()).unwrap();
        let far = dir.join("x.mp4");
        let shown = |cx: &mut gpui::VisualTestContext, row: String| {
            fs::write(&kept, row).unwrap();
            view.update(cx, |o, _| {
                o.ask();
                o.ship
                    .as_ref()
                    .map(|s| (s.path.clone(), s.opts.height, s.opts.codec, s.opts.quality))
            })
        };
        let want = |path: &Path, height, codec, quality| Some((path.to_path_buf(), height, codec, quality));
        let rows = [
            (
                r#"["http://127.0.0.1:9/a.ts",720,0,50,true]"#.to_string(),
                want(&dir.join("far.mp4"), 0, Codec::H264, 80),
            ),
            (format!("[{far:?},100000,2,7,true]"), want(&far, 0, Codec::ProRes, 80)),
            (format!("[{far:?},720,2,50,true]"), want(&far, 720, Codec::ProRes, 50)),
        ];
        for (row, want) in rows {
            assert_eq!(shown(cx, row.clone()), want, "{row}");
        }
        fs::remove_file(&kept).unwrap();
    }
}
