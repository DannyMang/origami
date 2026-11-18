use crate::{
    Origami,
    fold::depth,
    motion::split,
    view::{DIM, MUTED, TEXT},
};
use gpui::{Context, Div, div, prelude::*, px, rgb};

pub fn blank(text: &str) -> String {
    let (s, mut out, mut i) = (text.as_bytes(), text.as_bytes().to_vec(), 0);
    while i < s.len() {
        let (a, b, next) = match (s[i], s.get(i + 1)) {
            (b'/', Some(b'/')) => (i, text[i..].find('\n').map_or(s.len(), |n| i + n), 0),
            (b'/', Some(b'*')) => (i, text[i + 2..].find("*/").map_or(s.len(), |n| i + n + 4), 0),
            (q @ (b'"' | b'\'' | b'`'), _) => {
                let mut j = i + 1;
                while j < s.len() && s[j] != q {
                    j += 1 + (s[j] == b'\\') as usize;
                }
                (i + 1, j.min(s.len()), 1)
            }
            _ => (i, i, 1),
        };
        out[a..b].iter_mut().filter(|c| **c != b'\n').for_each(|c| *c = b' ');
        i = b + next;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn shut(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    code[open..].char_indices().find_map(|(i, c)| {
        depth += "([{".contains(c) as i32 - ")]}".contains(c) as i32;
        (depth == 0).then_some(open + i)
    })
}

fn trim(code: &str, (a, b): (usize, usize)) -> (usize, usize) {
    let s = &code[a..b];
    let from = a + s.len() - s.trim_start().len();
    (from, from + s.trim().len())
}

pub fn tuned(text: &str, to: &str) -> Option<String> {
    let (code, pick) = (blank(text), Some(format!("preview: \"{to}\"")).filter(|_| to != "auto"));
    let word = |c: char| c.is_alphanumeric() || "_$.".contains(c);
    let mut calls = vec![];
    for (i, _) in code.match_indices("settings") {
        let open = trim(&code, (i + 8, code.len())).0;
        if code[open..].starts_with('(') && !code[..i].ends_with(word) && depth(&code[..i]) == 0 {
            calls.push((i, open, shut(&code, open)?));
        }
    }
    let Some(last) = calls.last() else {
        let head = if text.starts_with("#!") {
            text.find('\n').map_or(text.len(), |n| n + 1)
        } else {
            0
        };
        return Some(pick.map_or(text.into(), |p| format!("{}settings({{ {p} }})\n{}", &text[..head], &text[head..])));
    };
    let parse = |&(_, open, close): &(usize, usize, usize)| {
        let (lb, rb) = trim(&code, (open + 1, close));
        let pieces = |from: &mut usize, s: &str| {
            *from += s.len() + 1;
            Some((*from - s.len() - 1, *from - 1))
        };
        let whole = code[lb..].starts_with('{') && shut(&code, lb) == Some(rb - 1);
        whole.then(|| {
            (
                lb,
                rb,
                split(&code[lb + 1..rb - 1]).into_iter().scan(lb + 1, pieces).collect::<Vec<_>>(),
            )
        })
    };
    let key = |(a, b): (usize, usize)| {
        let colon = code[a..b].find(':').map(|n| a + n);
        let (ka, kb) = trim(&code, (a, colon.unwrap_or(b)));
        (text[ka..kb].trim_matches(['[', ']', '"', '\'', '`']) == "preview", colon)
    };
    let keyed = |c: &&(usize, usize, usize)| parse(c).is_some_and(|p| p.2.into_iter().any(|e| key(e).0));
    let call = calls.iter().rev().find(keyed).unwrap_or(last);
    let ((i, _, close), (lb, rb, entries)) = (*call, parse(call)?);
    let mut out = text.to_string();
    match (entries.iter().position(|e| key(*e).0), pick) {
        (Some(k), Some(_)) => {
            let (va, vb) = trim(&code, (key(entries[k]).1? + 1, entries[k].1));
            out.replace_range(va..vb, &format!("\"{to}\""));
        }
        (Some(k), None) => {
            let ((s, e), (a, b)) = (entries[k], trim(&code, entries[k]));
            let rest = |r: &str| r.trim_matches(|c: char| c.is_whitespace() || c == ',').is_empty();
            if rest(&text[lb + 1..a]) && rest(&text[b..rb - 1]) {
                let from = text[..i].rfind('\n').map_or(0, |n| n + 1);
                return Some(match text[from..i].trim().is_empty() {
                    true => format!(
                        "{}{}",
                        &text[..from],
                        text[close + 1..].strip_prefix('\n').unwrap_or(&text[close + 1..])
                    ),
                    false => format!("{}{{}}{}", &text[..lb], &text[rb..]),
                });
            }
            let (a, b, gap) = match (k + 1 < entries.len(), k) {
                (true, _) => (a, e + 1 + text[e + 1..].len() - text[e + 1..].trim_start_matches(' ').len(), ""),
                (false, 0) => (a, b, ""),
                (false, _) => (s - 1, b, Some(&text[s..a]).filter(|g| !g.trim().is_empty()).unwrap_or("")),
            };
            out.replace_range(a..b, gap);
            let (from, end) = (
                out[..a].rfind('\n').map_or(0, |n| n + 1),
                out[a..].find('\n').map_or(out.len(), |n| a + n + 1),
            );
            if out[from..end].trim().is_empty() {
                out.replace_range(from..end, "");
            }
        }
        (None, Some(p)) => match entries.iter().map(|e| trim(&code, *e)).rfind(|e| e.0 < e.1) {
            Some((_, b)) => out.insert_str(b, &format!(", {p}")),
            None => out.insert_str(lb + 1, &format!(" {p} ")),
        },
        (None, None) => {}
    }
    Some(out)
}

impl Origami {
    pub fn tune(&mut self, to: &str) {
        match tuned(&self.lines.join("\n"), to) {
            Some(text) => self.change(|this| this.lines = text.split('\n').map(String::from).collect()),
            None => self.note = "settings( ) cannot be rewritten safely, edit the script to pick the preview quality".into(),
        }
    }

    pub fn quality(&self, cx: &mut Context<Self>) -> Div {
        let pick = |to: &'static str| {
            self.tool(to, cx, move |t, _, _| t.tune(to))
                .text_color(rgb(if to == self.tl.preview { TEXT } else { DIM }))
        };
        div()
            .pl(px(8.))
            .flex()
            .items_center()
            .text_color(rgb(MUTED))
            .child("preview")
            .child(div().flex_1().min_w(px(8.)))
            .children(["auto", "full", "half", "quarter"].map(pick))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, point};
    use origami_engine::footprint;

    // While the playhead is dragged along the ruler the preview comes from the camera's .LRF sidecar and
    // the full picture replaces it once the button is let go, unless proxy: false in settings turns the
    // sidecar off.
    #[gpui::test]
    fn proxies(cx: &mut gpui::TestAppContext) {
        let dir = std::env::temp_dir().join("origami-proxy-test");
        std::fs::create_dir_all(&dir).unwrap();
        for (file, ink) in [("v.mp4", "red"), ("v.LRF", "blue")] {
            let src = format!("color={ink}:s=320x180:r=30000/1001:d=4");
            let made = origami_lang::status(
                origami_lang::ffmpeg()
                    .args(["-v", "error", "-y", "-f", "lavfi", "-i", &src, "-f", "mp4"])
                    .arg(dir.join(file)),
            );
            assert!(made.unwrap().success());
        }
        let (view, cx) = cx.add_window_view(|_, cx| crate::fresh(cx.focus_handle(), vec![]));
        let ink = |cx: &mut gpui::VisualTestContext, src: &str| {
            std::fs::write(dir.join("edit.js"), src).unwrap();
            let tl = origami_lang::eval(&dir.join("edit.js"), false, footprint).unwrap();
            view.update(cx, |o, cx| {
                (o.script, o.lines, o.tl, o.rev) = (dir.join("edit.js"), vec![src.into()], std::sync::Arc::new(tl), o.rev + 1);
                cx.notify();
            });
            cx.run_until_parked();
        };
        let seen = |cx: &mut gpui::VisualTestContext| {
            view.update(cx, |o, cx| o.refresh(cx));
            cx.run_until_parked();
            view.update(cx, |o, _| {
                let px = o.image.as_ref().unwrap().as_bytes(0).unwrap();
                (if px[0] > px[2] { "blue" } else { "red" }, o.rough)
            })
        };
        ink(cx, "stack(video(\"v.mp4\"))");
        let (x, y) = view.update_in(cx, |o, window, _| {
            let h = f32::from(window.viewport_size().height);
            (o.lead() + 100., h - 28. - o.dock + 11.)
        });
        assert_eq!(seen(cx), ("red", false));
        cx.simulate_mouse_move(point(px(x), px(y)), None, Modifiers::default());
        cx.simulate_mouse_down(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
        assert!(view.update(cx, |o, _| o.scrubbing && o.t > 0.));
        assert_eq!(seen(cx), ("blue", true));
        cx.simulate_mouse_move(point(px(x + 40.), px(y)), Some(MouseButton::Left), Modifiers::default());
        assert_eq!(seen(cx), ("blue", true));
        cx.simulate_mouse_up(point(px(x + 40.), px(y)), MouseButton::Left, Modifiers::default());
        assert_eq!(seen(cx), ("red", false));
        ink(cx, "settings({ proxy: false })\nstack(video(\"v.mp4\"))");
        cx.simulate_mouse_down(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
        assert_eq!(seen(cx), ("red", false));
        cx.simulate_mouse_up(point(px(x), px(y)), MouseButton::Left, Modifiers::default());
    }

    // Picking a preview quality adds or updates the settings line at the top of the script, keeping the
    // other settings, takes the entry out again for auto, and the rewritten script loads with the chosen
    // quality.
    #[test]
    fn rewrites_settings() {
        let tuned = |text: &str, to: &str| tuned(text, to).unwrap();
        let stack = "stack(color(\"#203040\").for(4))";
        assert_eq!(tuned(stack, "half"), format!("settings({{ preview: \"half\" }})\n{stack}"));
        assert_eq!(tuned(stack, "auto"), stack);
        let plain = format!("settings({{ fps: 30, size: [1920, 1080] }})\n\n{stack}");
        let full = tuned(&plain, "full");
        assert_eq!(
            full,
            format!("settings({{ fps: 30, size: [1920, 1080], preview: \"full\" }})\n\n{stack}")
        );
        assert_eq!(tuned(&full, "quarter"), full.replace("full", "quarter"));
        assert_eq!(tuned(&full, "auto"), plain);
        assert_eq!(tuned(&tuned(stack, "half"), "auto"), stack);
        let wide = format!("// cut\nsettings({{\n  'preview': 'half',\n  fps: 24,\n}})\n{stack}");
        assert_eq!(
            tuned(&wide, "quarter"),
            format!("// cut\nsettings({{\n  'preview': \"quarter\",\n  fps: 24,\n}})\n{stack}")
        );
        assert_eq!(tuned(&wide, "auto"), format!("// cut\nsettings({{\n  fps: 24,\n}})\n{stack}"));
        let path = std::env::temp_dir().join("origami-tune-eval.js");
        std::fs::write(&path, tuned(&plain, "half")).unwrap();
        let tl = origami_lang::eval(&path, false, footprint).unwrap();
        assert_eq!((tl.preview.as_str(), tl.proxy, tl.size), ("half", true, (1920, 1080)));
    }

    // The quality menu should edit the settings call the script really runs, keep comments and the
    // rest of the call as written, and leave a script it cannot safely rewrite untouched instead of
    // crashing: an unfinished settings(, or settings given something other than { ... }.
    #[test]
    fn finds_the_real_settings() {
        let stack = "stack(color(\"#203040\").for(4))";
        let path = std::env::temp_dir().join("origami-tune-real.js");
        std::fs::write(&path, stack).unwrap();
        for (src, half) in [
            (
                "// call settings( later",
                "settings({ preview: \"half\" })\n// call settings( later",
            ),
            (
                "stack(text(\"see settings(\").for(2))",
                "settings({ preview: \"half\" })\nstack(text(\"see settings(\").for(2))",
            ),
            ("#!/usr/bin/env origami", "#!/usr/bin/env origami\nsettings({ preview: \"half\" })"),
            (
                "settings({\n  fps: 30, // don't change\n})",
                "settings({\n  fps: 30, preview: \"half\", // don't change\n})",
            ),
            (
                "settings({ fps: 30 // frame rate\n})",
                "settings({ fps: 30, preview: \"half\" // frame rate\n})",
            ),
            (
                "settings({ fps: 30 /* (was 60 */ })",
                "settings({ fps: 30, preview: \"half\" /* (was 60 */ })",
            ),
            (
                "settings({ fps: 30, note: \"it\\\"s (fine\" })",
                "settings({ fps: 30, note: \"it\\\"s (fine\", preview: \"half\" })",
            ),
            (
                "// settings({ fps: 60 })\nsettings({ fps: 30 })",
                "// settings({ fps: 60 })\nsettings({ fps: 30, preview: \"half\" })",
            ),
            ("settings ({ preview: \"full\" })", "settings ({ preview: \"half\" })"),
            (
                "settings({ fps: 30 })\nsettings({ preview: \"full\" })",
                "settings({ fps: 30 })\nsettings({ preview: \"half\" })",
            ),
            (
                "settings({ [\"preview\"]: `full`, fps: 30 })",
                "settings({ [\"preview\"]: \"half\", fps: 30 })",
            ),
            ("settings({ preview: \"full\" // q\n})", "settings({ preview: \"half\" // q\n})"),
            ("settings({})", "settings({ preview: \"half\" })"),
        ] {
            let src = format!("{src}\n{stack}");
            assert_eq!(tuned(&src, "half"), Some(format!("{half}\n{stack}")), "{src}");
            for to in ["full", "half", "quarter", "auto"] {
                let out = tuned(&src, to).unwrap();
                let tl = origami_lang::peek(&path, &out, footprint).unwrap_or_else(|e| panic!("{e}: {out}"));
                assert_eq!(tl.preview, to, "{out}");
            }
        }
        let spread = "settings({\n  fps: 30,\n  preview: \"full\"\n})";
        assert_eq!(tuned(spread, "auto").unwrap(), "settings({\n  fps: 30\n})");
        let inside = "const setup = () => settings({ preview: \"full\" })";
        assert_eq!(tuned(inside, "auto").unwrap(), "const setup = () => settings({})");
        for bad in [
            "settings(",
            "settings({ fps: 30,",
            "settings(mine)",
            "settings()",
            "settings({ preview })",
        ] {
            assert_eq!(tuned(&format!("{bad}\n{stack}"), "half"), None, "{bad}");
        }
    }

    // The preview row in the "more" menu of the timeline bar rewrites and saves the script with one click
    // per choice, each click one step of undo, and leaves a settings call it cannot rewrite alone with a
    // note.
    #[gpui::test]
    fn picks(cx: &mut gpui::TestAppContext) {
        let path = std::env::temp_dir().join("origami-tune-test.js");
        let src = "settings({ fps: 24 })\nstack(color(\"#203040\").for(4))";
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines) = (path.clone(), src.lines().map(String::from).collect());
            app
        });
        let (edge, mid) = view.update_in(cx, |o, window, _| {
            window.focus(&o.focus);
            let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
            (w - o.side - 12. - 16. - 21. * 7.8, h - 28. - o.dock - 17.)
        });
        cx.run_until_parked();
        let pick = |cx: &mut gpui::VisualTestContext, back: f32| {
            for at in [point(px(edge - 4.), px(mid)), point(px(edge - 5. - back), px(mid - 31.))] {
                cx.simulate_mouse_move(at, None, Modifiers::default());
                cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
                cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
            }
            view.update(cx, |o, _| (o.lines[0].clone(), o.past.len(), o.menu))
        };
        assert_eq!(pick(cx, 94.), ("settings({ fps: 24, preview: \"half\" })".into(), 1, ""));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("{}\n", src.replace("24 }", "24, preview: \"half\" }"))
        );
        assert_eq!(pick(cx, 35.), ("settings({ fps: 24, preview: \"quarter\" })".into(), 2, ""));
        assert_eq!(pick(cx, 141.), ("settings({ fps: 24, preview: \"full\" })".into(), 3, ""));
        assert_eq!(pick(cx, 188.), ("settings({ fps: 24 })".into(), 4, ""));
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(
            view.update(cx, |o, _| (o.lines[0].clone(), o.past.len())),
            ("settings({ fps: 24, preview: \"full\" })".into(), 3)
        );
        view.update(cx, |o, _| o.lines[0] = "settings(".into());
        assert_eq!(pick(cx, 94.), ("settings(".into(), 3, ""));
        assert!(view.update(cx, |o, _| o.note.starts_with("settings( ) cannot be rewritten safely")));
    }
}
