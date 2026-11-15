use crate::{
    Origami,
    knob::close,
    motion::split,
    view::{DIM, MUTED, TEXT},
};
use gpui::{Context, Div, div, prelude::*, px, rgb};

pub fn tuned(text: &str, to: &str) -> String {
    let pick = Some(format!("preview: \"{to}\"")).filter(|_| to != "auto");
    let Some(open) = text.find("settings(").map(|i| i + 8) else {
        return pick.map_or(text.into(), |p| format!("settings({{ {p} }})\n{text}"));
    };
    let end = close(text.as_bytes(), open).unwrap_or(open);
    let Some(body) = text[open + 1..end].trim().strip_prefix('{').and_then(|b| b.strip_suffix('}')) else {
        return text.into();
    };
    let named = |e: &&str| e.split(':').next().is_some_and(|k| k.trim_matches(['"', '\'', ' ']) == "preview");
    let kept = split(body).into_iter().map(str::trim).filter(|e| !e.is_empty() && !named(e));
    let all: Vec<String> = kept.map(String::from).chain(pick).collect();
    let rest = &text[end + 1..];
    match all.is_empty() {
        true => format!("{}{}", &text[..open - 8], rest.strip_prefix('\n').unwrap_or(rest)),
        false => format!("{}{{ {} }}){rest}", &text[..open + 1], all.join(", ")),
    }
}

impl Origami {
    pub fn tune(&mut self, to: &str) {
        self.change(|this| this.lines = tuned(&this.lines.join("\n"), to).split('\n').map(String::from).collect());
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

    // While the playhead is dragged along the ruler the preview should come from the camera's .LRF
    // sidecar, and the full picture should replace it once the button is let go. With proxy: false
    // in settings the sidecar is never used.
    #[gpui::test]
    fn proxies(cx: &mut gpui::TestAppContext) {
        let dir = std::env::temp_dir().join("origami-proxy-test");
        std::fs::create_dir_all(&dir).unwrap();
        for (file, ink) in [("v.mp4", "red"), ("v.LRF", "blue")] {
            let src = format!("color={ink}:s=320x180:r=30000/1001:d=4");
            let made = std::process::Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-f", "lavfi", "-i", &src, "-f", "mp4"])
                .arg(dir.join(file))
                .status();
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

    // Picking a preview quality should add or update the settings line at the top of the script,
    // keep the other settings, and take the entry out again when auto is picked. A rewritten script
    // should still load with the chosen quality.
    #[test]
    fn rewrites_settings() {
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
            format!("// cut\nsettings({{ fps: 24, preview: \"quarter\" }})\n{stack}")
        );
        let named = format!("settings(mine)\n{stack}");
        assert_eq!(tuned(&named, "half"), named);
        let path = std::env::temp_dir().join("origami-tune-eval.js");
        std::fs::write(&path, tuned(&plain, "half")).unwrap();
        let tl = origami_lang::eval(&path, false, footprint).unwrap();
        assert_eq!((tl.preview.as_str(), tl.proxy, tl.size), ("half", true, (1920, 1080)));
    }

    // The preview row in the "more" menu of the timeline bar should rewrite the script and save it
    // with one click per choice, and each click should be one step of undo.
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
    }
}
