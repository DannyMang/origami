use crate::{
    Origami,
    edit::links,
    fold::depth,
    knob::close,
    pulse,
    tune::blank,
    view::{BG, BORDER, DIM, GREEN, RED, TEXT, wave},
};
use gpui::{Context, Div, deferred, div, prelude::*, px, relative, rgb, rgba};
use origami_engine::{Mic, inputs};
use std::{fs, path::Path};

pub struct Take {
    n: usize,
    from: f64,
    start: Option<f64>,
    mic: Option<Mic>,
}

impl Take {
    pub fn about(&self) -> String {
        format!(
            "recording voice/take-{}.wav \u{b7} space or click stop to finish \u{b7} esc discards",
            self.n
        )
    }
}

fn slot(dir: &Path, lines: &[String]) -> usize {
    let used = |n: &usize| {
        let name = format!("const voice_{n} ");
        dir.join(format!("take-{n}.wav")).exists() || lines.iter().any(|l| l.starts_with(&name))
    };
    (1..).find(|n| !used(n)).unwrap_or(1)
}

fn top(l: &String) -> bool {
    !l.starts_with([' ', '\t', ')', ']', '}', '.'])
}

fn bare(l: &str) -> usize {
    let (mut quote, mut skip) = (None, false);
    for (i, c) in l.char_indices() {
        match (quote, c) {
            _ if std::mem::take(&mut skip) => {}
            (Some(_), '\\') => skip = true,
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (_, '"' | '\'' | '`') => quote = Some(c),
            (_, '/') if l[i..].starts_with("//") => return l[..i].trim_end().len(),
            _ => {}
        }
    }
    l.trim_end().len()
}

pub fn placed(lines: &mut Vec<String>, from: usize, item: &str, groups: &[&str]) -> usize {
    let code = |l: &String| !l.trim().is_empty() && !l.trim_start().starts_with("//");
    let odd = |l: &String| l.matches('`').count() % 2 == 1;
    let free: Vec<bool> = lines
        .iter()
        .scan(true, |free, l| Some(std::mem::replace(free, *free != odd(l))))
        .collect();
    let Some(head) = (from..lines.len()).rfind(|i| free[*i] && top(&lines[*i]) && code(&lines[*i])) else {
        lines.push(item.into());
        return lines.len() - 1;
    };
    let body = lines[head..].join("\n");
    let group = groups.iter().find(|k| body.starts_with(**k));
    let chained = |end: &usize| {
        let rest: String = body[end + 1..].lines().map(|l| &l[..bare(l)]).collect();
        links(&rest)
            .iter()
            .map(|l| l.trim())
            .any(|l| l.starts_with('.') && !l.starts_with(".at(") && !l.starts_with(".shift("))
    };
    let Some(end) = group.and_then(|k| close(body.as_bytes(), k.len() - 1)).filter(|e| !chained(e)) else {
        let last = lines.iter().rposition(code).unwrap_or(head);
        lines[head].insert_str(0, "stack(");
        let (body, note) = lines[last].split_at(bare(&lines[last]));
        lines[last] = format!("{}, {item}){note}", body.trim_end_matches(';'));
        return last;
    };
    let (row, col) = (
        head + body[..end].matches('\n').count(),
        end - body[..end].rfind('\n').map_or(0, |i| i + 1),
    );
    let open = |l: &str| l.trim_end().ends_with([',', '(']);
    if !lines[row][..col].trim().is_empty() {
        let item = if open(&lines[row][..col]) {
            item.into()
        } else {
            format!(", {item}")
        };
        lines[row].insert_str(col, &item);
        return row;
    }
    if let Some(prev) = lines[..row].iter_mut().rfind(|l| code(l)).filter(|l| !open(&l[..bare(l)])) {
        prev.insert(bare(prev), ',');
    }
    lines.insert(row, format!("{}{item},", " ".repeat(col + 2)));
    row
}

fn voiced(lines: &mut Vec<String>, n: usize, at: f64) -> usize {
    let media = |l: &String| {
        ["const ", "let ", "var "].iter().any(|k| l.starts_with(k)) && ["video(", "sound(", "image("].iter().any(|k| l.contains(k))
    };
    let rows: Vec<i32> = blank(&lines.join("\n")).split('\n').map(depth).collect();
    let after = |i: usize| {
        let mut open = 0;
        let end = (i..lines.len()).find(|j| {
            open += rows[*j];
            open <= 0 && lines.get(j + 1).is_none_or(top)
        });
        end.map_or(lines.len(), |j| j + 1)
    };
    let def = lines.iter().rposition(media).map_or(0, after);
    lines.insert(def, format!("const voice_{n} = sound(\"voice/take-{n}.wav\")"));
    let at = (at * 100.).round() / 100.;
    let item = if at > 0. {
        format!("voice_{n}.at({at})")
    } else {
        format!("voice_{n}")
    };
    placed(lines, def + 1, &item, &["stack(", "cat("])
}

impl Origami {
    pub fn record(&mut self) {
        if self.take.is_some() {
            return self.finish(true);
        }
        let dir = self.script.parent().unwrap().join("voice");
        if fs::symlink_metadata(&dir).is_ok_and(|m| m.is_symlink()) {
            return self.note = "cannot record: voice is a link to another folder".into();
        }
        let n = slot(&dir, &self.lines);
        let path = dir.join(format!("take-{n}.wav"));
        let _ = fs::create_dir_all(&dir);
        match Mic::new(&path, self.mic.as_deref()) {
            Ok(mic) => {
                self.play();
                self.take = Some(Take {
                    n,
                    from: self.t,
                    start: None,
                    mic: Some(mic),
                });
            }
            Err(e) => {
                let _ = fs::remove_file(path);
                let _ = fs::remove_dir(dir);
                self.note = format!("cannot record: {e}");
            }
        }
    }

    pub fn sync(&mut self, t: f64) {
        if let Some(take) = self.take.as_mut().filter(|k| k.start.is_none() && t > k.from + 0.25)
            && let Some(mic) = &take.mic
        {
            take.start = Some((t - mic.secs() * self.rate).max(0.));
        }
    }

    pub fn finish(&mut self, keep: bool) {
        let Some(take) = self.take.take() else { return };
        self.player = None;
        let dir = self.script.parent().unwrap().join("voice");
        let silent = take.mic.as_ref().is_some_and(|m| m.peaks().iter().all(|p| *p == 0.));
        drop(take.mic);
        if silent || !keep {
            let _ = fs::remove_file(dir.join(format!("take-{}.wav", take.n)));
            let _ = fs::remove_dir(dir);
            return self.note = match keep {
                true => "heard only silence, nothing added: allow the microphone in System Settings, Privacy & Security".into(),
                false => "discarded the take".into(),
            };
        }
        let mut row = 0;
        self.change(|this| row = voiced(&mut this.lines, take.n, take.start.unwrap_or(take.from)));
        self.sel = Some(self.tl.clips.len() as u32);
        self.reveal(row);
        pulse::beat("voiceover recorded");
        self.note = format!("added voice_{0} \u{b7} voice/take-{0}.wav \u{b7} cmd+Z removes it", take.n);
    }

    pub fn recorder(&self, cx: &mut Context<Self>) -> Div {
        let on = self.take.is_some();
        let dot = div().size(px(8.)).rounded(px(if on { 1. } else { 4. })).bg(rgb(RED));
        let ring = div()
            .size(px(18.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(rgb(if on { RED } else { DIM }))
            .when(on, |d| d.bg(rgba(RED << 8 | 0x40)))
            .child(dot);
        self.tool(ring, cx, |t, _, _| t.record())
    }

    pub fn mic(&self, cx: &mut Context<Self>) -> Div {
        let heard = self.take.as_ref().and_then(|t| t.mic.as_ref());
        let level = heard.and_then(|m| m.peaks().last().copied()).unwrap_or(0.);
        let ink = rgb(if self.take.is_some() { RED } else { DIM });
        let icon = div()
            .flex()
            .flex_col()
            .items_center()
            .child(div().w(px(5.)).h(px(8.)).rounded_full().bg(ink))
            .child(
                div()
                    .mt(px(-4.))
                    .w(px(9.))
                    .h(px(6.))
                    .rounded_b(px(5.))
                    .border_1()
                    .border_t_0()
                    .border_color(ink),
            )
            .child(div().w(px(1.)).h(px(2.)).bg(ink))
            .child(div().w(px(5.)).h(px(1.)).bg(ink));
        let bar = |i: usize| {
            let lit = level > [0.005, 0.06, 0.25][i];
            div().w(px(2.)).h(px(4. + 3. * i as f32)).bg(rgb(if lit { GREEN } else { BORDER }))
        };
        let meter = div().flex().items_end().gap(px(1.)).children((0..3).map(bar));
        let pick = |(i, name): (usize, &String)| {
            let (name, on) = (name.clone(), self.mic.as_ref().map_or(i == 0, |m| m == name));
            self.tool(name.clone(), cx, move |t, _, _| t.mic = Some(name.clone()))
                .when(on, |d| d.text_color(rgb(TEXT)))
        };
        let mut items: Vec<Div> = self.mics.iter().enumerate().map(pick).collect();
        if items.is_empty() {
            items.push(div().px(px(8.)).text_color(rgb(DIM)).child("no microphone found"));
        }
        let open = |t: &mut Self| (t.mics, t.menu) = (inputs(), if t.shut == "mic" { "" } else { "mic" });
        self.tool(
            div().flex().items_center().gap(px(4.)).child(icon).child(meter),
            cx,
            move |t, _, _| open(t),
        )
        .relative()
        .children((self.menu == "mic").then(|| deferred(Self::list(items).left_0())))
    }

    pub fn growing(&self) -> Option<Div> {
        let take = self.take.as_ref()?;
        let (d, heard) = (self.span(), take.mic.as_ref());
        let (peaks, secs) = (heard.map(Mic::peaks).unwrap_or_default(), heard.map_or(0., Mic::secs));
        let top = peaks.iter().fold(0.05, |m: f32, &p| m.max(p));
        let bars = peaks
            .chunks((peaks.len() / 160).max(1))
            .map(|c| c.iter().fold(0., |m: f32, &p| m.max(p)) / top)
            .collect();
        let label = div()
            .absolute()
            .top(px(3.))
            .left(px(4.))
            .px_1()
            .rounded(px(3.))
            .bg(rgba(BG << 8 | 0xd9))
            .text_size(px(11.))
            .whitespace_nowrap()
            .child(format!("voice  take {} \u{b7} {}:{:04.1}", take.n, (secs / 60.) as u32, secs % 60.));
        Some(
            div()
                .absolute()
                .top(px(3.))
                .bottom(px(3.))
                .left(relative(((take.from - self.pan) / d) as f32))
                .w(relative(((self.t - take.from).max(0.) / d) as f32))
                .min_w(px(2.))
                .rounded(px(3.))
                .border_1()
                .border_color(rgb(RED))
                .bg(rgba(RED << 8 | 0x40))
                .overflow_hidden()
                .child(wave(bars, RED))
                .child(label),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, point};

    fn after(src: &str, at: f64) -> String {
        let mut lines: Vec<String> = src.lines().map(String::from).collect();
        let row = voiced(&mut lines, 1, at);
        assert!(lines[row].contains("voice_1"), "{row} {lines:?}");
        lines.join("\n").replace("const voice_1 = sound(\"voice/take-1.wav\")", "DEF")
    }

    // A recorded voice take should be added to the script as a named sound and placed at the time
    // recording started, whatever shape the end of the script has and whatever its comments hold, and
    // outside a group whose own calls (mute, volume, speed, fade) would change how it plays.
    #[test]
    fn places() {
        assert_eq!(after("stack(shot, title)", 0.), "DEF\nstack(shot, title, voice_1)");
        assert_eq!(after("stack()", 1.), "DEF\nstack(voice_1.at(1))");
        assert_eq!(
            after("const body = cat(\n  a,\n)\n\nbody\n// end", 2.004),
            "DEF\nconst body = cat(\n  a,\n)\n\nstack(body, voice_1.at(2))\n// end"
        );
        assert_eq!(
            after("let m = video(\"m.mp4\")\n  .trim(0, 5)\n\ncat(m, m)\n  .volume(0.5);", 0.333),
            "let m = video(\"m.mp4\")\n  .trim(0, 5)\nDEF\n\nstack(cat(m, m)\n  .volume(0.5), voice_1.at(0.33))"
        );
        assert_eq!(
            after("cat(\n  a,\n  b,\n).mute() // quiet", 1.5),
            "DEF\nstack(cat(\n  a,\n  b,\n).mute(), voice_1.at(1.5)) // quiet"
        );
        assert_eq!(
            after("stack(\n  a,\n)\n  .fade(1)\n  .speed(2)", 1.),
            "DEF\nstack(stack(\n  a,\n)\n  .fade(1)\n  .speed(2), voice_1.at(1))"
        );
        assert_eq!(
            after("stack(\n  a,\n).shift(1)", 1.),
            "DEF\nstack(\n  a,\n  voice_1.at(1),\n).shift(1)"
        );
        assert_eq!(
            after("const v = video(\n\"v.mp4\"\n)\nstack(\n  v,\n)", 1.),
            "const v = video(\n\"v.mp4\"\n)\nDEF\nstack(\n  v,\n  voice_1.at(1),\n)"
        );
        assert_eq!(
            after("stack(\n  cat(a, b),\n  text(\"it's\")\n  // note\n)", 4.27),
            "DEF\nstack(\n  cat(a, b),\n  text(\"it's\"),\n  // note\n  voice_1.at(4.27),\n)"
        );
        assert_eq!(after("stack(a,\n  b)", 1.5), "DEF\nstack(a,\n  b, voice_1.at(1.5))");
        assert_eq!(
            after("video(\"a.mp4\").trim(0, 5);", 1.),
            "DEF\nstack(video(\"a.mp4\").trim(0, 5), voice_1.at(1))"
        );
        assert_eq!(after("", 1.), "DEF\nvoice_1.at(1)");
        for (src, want) in [
            ("v.trim(1, 5) // the clip", "DEF\nstack(v.trim(1, 5), voice_1.at(1)) // the clip"),
            ("v.trim(1, 5); // the clip", "DEF\nstack(v.trim(1, 5), voice_1.at(1)) // the clip"),
            ("text('it\\'s // here')", "DEF\nstack(text('it\\'s // here'), voice_1.at(1))"),
            (
                "stack(\n  a.trim(0, 2), // first\n  b.trim(0, 2) // second\n) // end",
                "DEF\nstack(\n  a.trim(0, 2), // first\n  b.trim(0, 2), // second\n  voice_1.at(1),\n) // end",
            ),
            (
                "const out = stack(a)\nout // result",
                "DEF\nconst out = stack(a)\nstack(out, voice_1.at(1)) // result",
            ),
            (
                "stack(\n  v.trim(0, 2), // don't\n  cat(\n    v.trim(2, 4), /* won't ) */\n    v.trim(4, 5),\n  ),\n)",
                "DEF\nstack(\n  v.trim(0, 2), // don't\n  cat(\n    v.trim(2, 4), /* won't ) */\n    v.trim(4, 5),\n  ),\n  voice_1.at(1),\n)",
            ),
            (
                "stack(\n  a,\n  text(`two\nlines`),\n)",
                "DEF\nstack(\n  a,\n  text(`two\nlines`),\n  voice_1.at(1),\n)",
            ),
        ] {
            assert_eq!(after(src, 1.), want);
        }
        let mut lines = vec!["cat(a, b); // x".to_string()];
        placed(&mut lines, 0, "c.at(2)", &["stack("]);
        assert_eq!(lines, ["stack(cat(a, b), c.at(2)) // x"]);
    }

    // Stopping a recording should add the take to the script and save it, undo should remove it, and
    // cancelling should delete the unused audio file without touching earlier takes.
    #[gpui::test]
    fn takes(cx: &mut gpui::TestAppContext) {
        let dir = std::env::temp_dir().join("origami-voice-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("voice")).unwrap();
        fs::write(dir.join("voice/take-1.wav"), "old").unwrap();
        let script = "const a = sound(\"a.mp3\")\n  .volume(0.5)\n\nstack(\n  color(\"#203040\").for(4),\n  a\n)";
        let lines: Vec<String> = script.lines().map(String::from).collect();
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines) = (dir.join("edit.js"), lines.clone());
            app
        });
        let start = |cx: &mut gpui::VisualTestContext| {
            view.update_in(cx, |o, window, cx| {
                let n = slot(&dir.join("voice"), &o.lines);
                fs::write(dir.join(format!("voice/take-{n}.wav")), "new").unwrap();
                o.take = Some(Take {
                    n,
                    from: 1.5,
                    start: None,
                    mic: None,
                });
                window.focus(&o.focus);
                cx.notify();
                let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
                point(px((w - o.side) / 2. + 48.), px(h - 28. - o.dock - 17.))
            })
        };
        let files = || {
            let mut names: Vec<_> = fs::read_dir(dir.join("voice")).unwrap().map(|e| e.unwrap().file_name()).collect();
            names.sort();
            names
        };
        let stop = start(cx);
        cx.run_until_parked();
        cx.simulate_mouse_move(stop, None, Modifiers::default());
        cx.simulate_mouse_down(stop, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(stop, MouseButton::Left, Modifiers::default());
        let (got, past, note, on) = view.update(cx, |o, _| (o.lines.join("\n"), o.past.len(), o.note.clone(), o.take.is_some()));
        let want = "const a = sound(\"a.mp3\")\n  .volume(0.5)\nconst voice_2 = sound(\"voice/take-2.wav\")\n\nstack(\n  color(\"#203040\").for(4),\n  a,\n  voice_2.at(1.5),\n)";
        assert_eq!((got.as_str(), past, on), (want, 1, false));
        assert_eq!(note, "added voice_2 \u{b7} voice/take-2.wav \u{b7} cmd+Z removes it");
        assert_eq!(fs::read_to_string(dir.join("edit.js")).unwrap(), format!("{want}\n"));
        assert_eq!(fs::read_to_string(dir.join("voice/take-1.wav")).unwrap(), "old");
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), lines);
        start(cx);
        assert_eq!(files(), ["take-1.wav", "take-2.wav", "take-3.wav"]);
        cx.simulate_keystrokes("escape");
        assert_eq!(
            view.update(cx, |o, _| (o.lines.clone(), o.past.len(), o.take.is_some())),
            (lines.clone(), 0, false)
        );
        assert_eq!(files(), ["take-1.wav", "take-2.wav"]);
        fs::remove_dir_all(dir.join("voice")).unwrap();
        fs::create_dir(dir.join("voice")).unwrap();
        start(cx);
        cx.simulate_keystrokes("space");
        let got = view.update(cx, |o, _| o.lines.join("\n"));
        assert!(
            got.contains("const voice_1 = sound(\"voice/take-1.wav\")") && got.contains("  voice_1.at(1.5),"),
            "{got}"
        );
        cx.simulate_keystrokes("cmd-z");
        start(cx);
        cx.simulate_keystrokes("escape");
        assert_eq!(files(), ["take-1.wav"]);
        fs::remove_file(dir.join("voice/take-1.wav")).unwrap();
        start(cx);
        cx.simulate_keystrokes("escape");
        assert!(!dir.join("voice").exists());
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), lines);
    }
}
