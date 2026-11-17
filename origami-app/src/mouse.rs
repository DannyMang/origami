use crate::{
    Origami,
    clip::set,
    crash,
    menu::menus,
    scene::{FOOT, LEAD, sink},
    shelf::Shelf,
    view::{HEAD, RULER},
};
use gpui::{Context, Pixels, ScrollWheelEvent, Window, px};
use origami_lang::{Clip, Kind};
use std::{
    fs, mem,
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn recent() -> PathBuf {
    carried(&crash::home())
}

fn carried(home: &Path) -> PathBuf {
    let (list, old) = (
        home.join("Library/Application Support/Origami/recent"),
        home.join(".origami/recent"),
    );
    if !list.exists() && old.exists() {
        let _ = fs::create_dir_all(list.parent().unwrap());
        let _ = fs::copy(old, &list);
    }
    list
}

fn deep(zoom: f64, duration: f64, fps: f64) -> f64 {
    zoom.clamp(1., (duration * fps / 8.).clamp(1., 64.))
}

fn within(pan: f64, duration: f64, span: f64) -> f64 {
    pan.min(duration - span).max(0.)
}

fn pane(want: f32, room: f32, least: f32) -> f32 {
    want.min(room).max(least)
}

#[derive(Clone, PartialEq)]
pub enum Grab {
    Move,
    Left,
    Right,
}

impl Origami {
    pub fn span(&self) -> f64 {
        self.held.unwrap_or(self.tl.duration).max(1e-3) / self.zoom
    }

    pub fn lead(&self) -> f32 {
        if self.motion { LEAD } else { HEAD }
    }

    pub fn depth(&self) -> f64 {
        deep(f64::MAX, self.tl.duration, self.fps()).ln().max(1e-9)
    }

    pub fn per_px(&self, window: &Window) -> f64 {
        self.span() / (f32::from(window.viewport_size().width) - self.side - self.lead() - 12.).max(1.) as f64
    }

    pub fn at(&self, x: Pixels, window: &Window) -> f64 {
        self.pan + (f32::from(x) - self.lead()) as f64 * self.per_px(window)
    }

    pub fn toggle(&mut self) {
        match self.player.take() {
            Some(_) => {}
            None => self.play(),
        }
    }

    pub fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(6);
        let (list, all) = (recent(), self.recent.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>());
        let _ = fs::create_dir_all(list.parent().unwrap());
        let _ = fs::write(list, all.join("\n"));
        cx.set_menus(menus(&self.recent));
        (self.script, self.mtime, self.t, self.dirty, self.clash) = (path, None, 0., false, false);
        (self.past, self.future, self.cursor, self.top, self.left) = (Default::default(), Default::default(), None, 0., 0.);
        (self.folded, self.fresh, self.saves) = (vec![], true, None);
        if self.shelf.as_ref().is_none_or(|s| !s.holds(&self.script)) {
            let old = self.shelf.replace(Arc::new(Shelf::new(&self.script)));
            cx.background_executor()
                .spawn(async move {
                    if let Some(s) = old {
                        s.drain()
                    }
                })
                .detach();
        }
        let stamp = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
        let spare = crash::spare(&self.script);
        if fs::symlink_metadata(&spare).is_ok_and(|m| m.is_file())
            && stamp(&spare) > stamp(&self.script)
            && let Ok(text) = origami_lang::load(&spare)
        {
            let _ = fs::remove_file(&spare);
            if origami_lang::load(&self.script).unwrap_or_default().lines().ne(text.lines()) {
                (self.lines, self.dirty) = (text.lines().map(String::from).collect(), true);
                self.note = "recovered unsaved changes from last time: keep mine saves them, use disk drops them".into();
            }
        }
    }

    pub fn reset(&mut self) {
        (self.side, self.dock, self.font) = (520., 214., 13.);
        (self.zoom, self.pan) = (1., 0.);
    }

    pub fn step(&mut self, frames: f64) {
        self.seek(self.t + frames / self.fps());
    }

    pub fn pace(&mut self) {
        self.rate = if self.rate == 2. { 0.5 } else { self.rate * 2. };
        if self.player.is_some() {
            self.play();
        }
    }

    pub fn magnify(&mut self, k: f64) {
        self.zoom = deep(self.zoom * k, self.tl.duration, self.fps());
        self.pan = within(self.t - self.span() / 2., self.tl.duration, self.span());
    }

    pub fn scrub(&mut self, x: Pixels, window: &Window) {
        if self.mouse {
            self.scrubbing = true;
            self.seek(self.at(x, window));
        }
    }

    pub fn grab(&mut self, c: &Clip, x: Pixels, window: &Window) {
        if !self.mouse {
            return;
        }
        let (t, edge) = (self.at(x, window), 8. * self.per_px(window));
        let grab = match () {
            _ if t - c.start < edge => Grab::Left,
            _ if c.end - t < edge => Grab::Right,
            _ => Grab::Move,
        };
        (self.drag, self.base, self.held) = (Some((grab, c.clone(), t, 0.)), self.lines.clone(), Some(self.tl.duration));
    }

    fn reshape(&mut self, grab: &Grab, c: &Clip, dt: f64) {
        let (a, b, d, len) = (c.src_in, c.src_in + (c.end - c.start) * c.speed, dt * c.speed, c.end - c.start);
        let top = if c.total > 0. { c.total } else { f64::MAX };
        let trim = |a: f64, b: f64| {
            let a = a.clamp(0., top - 0.04);
            format!("{:.3}, {:.3}", a, b.clamp(a + 0.04, top))
        };
        let last = |n: f64| format!("{:.2}", n.max(0.04));
        let moved = |old: &str| format!("{:.4}", old.trim().parse::<f64>().unwrap_or(0.) + dt);
        let media = matches!(c.kind, Kind::Video | Kind::Sound);
        let verb = if *grab == Grab::Move { "move" } else { "trim" };
        let Some((c, ..)) = self.used(c.clone(), verb).filter(|_| dt != 0.) else {
            return;
        };
        let lines = &mut self.lines;
        match grab {
            Grab::Move => set(lines, c.line, "shift", moved),
            Grab::Left if media => set(lines, c.line, "trim", |_| trim(a + d, b)),
            Grab::Right if media => set(lines, c.line, "trim", |_| trim(a, b + d)),
            Grab::Right => set(lines, c.line, "for", |_| last(len + dt)),
            Grab::Left => {
                set(lines, c.line, "for", |_| last(len - dt));
                set(lines, c.line, "shift", moved);
            }
        }
    }

    pub fn drag_to(&mut self, x: Pixels, window: &Window) {
        let edge = f32::from(window.viewport_size().width) - self.side - 12.;
        let (past, x) = (f32::from(x) > edge, px(f32::from(x).min(edge + 12.)));
        self.push = Some(x).filter(|_| past && self.drag.is_some());
        if self.push.is_some() {
            self.held = Some(self.tl.duration);
        }
        let (t, f, snap) = (self.at(x, window), self.fps(), 8. * self.per_px(window));
        if self.scrubbing {
            self.seek(t);
        }
        let Some((grab, c, t0, was)) = self.drag.take() else {
            return;
        };
        let mut dt = if (t - t0).abs() < snap / 2. {
            0.
        } else {
            ((t - t0) * f).round() / f
        };
        if grab == Grab::Move && dt != 0. {
            let edges = self.tl.clips.iter().filter(|o| o.z != c.z).flat_map(|o| [o.start, o.end]);
            let near = edges
                .chain([0., self.t])
                .flat_map(|e| [e - c.start, e - c.end])
                .min_by(|a, b| (a - dt).abs().total_cmp(&(b - dt).abs()));
            dt = near.filter(|n| (n - dt).abs() < snap).unwrap_or(dt);
        }
        if dt != was {
            self.lines = self.base.clone();
            self.reshape(&grab, &c, dt);
            self.store();
        }
        self.drag = Some((grab, c, t0, dt));
    }

    pub fn resize(&mut self, wide: bool, x: Pixels, y: Pixels, window: &Window) {
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        match wide {
            true => self.side = pane(w - f32::from(x), w - 420., 260.),
            false => self.dock = pane(h - f32::from(y) - 62., h - 260., 70.),
        }
    }

    pub fn settle(&mut self, window: &Window) {
        let (w, h) = (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height));
        (self.side, self.dock) = (pane(self.side, w - 420., 260.), pane(self.dock, h - 260., 70.));
        self.fit = ((h - self.head() - 28.) / self.row()).max(1.) as usize;
        self.lift = sink(self.lift, self.tall(), self.dock - RULER - FOOT);
        self.roll(0.);
        self.sway(0.);
        if self.held.is_none() {
            self.zoom = deep(self.zoom, self.tl.duration, self.fps());
            self.pan = within(self.pan, self.tl.duration, self.span());
        }
    }

    pub fn release(&mut self) {
        if self.scrubbing && self.player.is_some() {
            self.play();
        }
        if self.turning.is_some() && self.past.last() == Some(&self.lines) {
            self.past.pop();
        }
        (self.scrubbing, self.pull, self.picking, self.turning, self.held, self.push) = (false, None, false, None, None, None);
        if self.slip.take().is_some() && self.lines != self.base {
            self.past.push(mem::take(&mut self.base));
            self.future.clear();
        }
        self.eyeing = false;
        match self.cord.take() {
            Some(c) if c.still() => {
                let shut = self.shut == "follow" && self.tie == Some(c.to);
                (self.menu, self.tie) = (if shut { "" } else { "follow" }, Some(c.to));
            }
            Some(c) => drop(c.over.map(|from| self.bind(c.to, from))),
            None => {}
        }
        let Some((_, c, _, dt)) = self.drag.take() else {
            return;
        };
        match dt == 0. {
            true => {
                self.sel = Some(c.z);
                self.reveal((c.line as usize).saturating_sub(1));
            }
            false if self.lines != self.base => {
                self.past.push(mem::take(&mut self.base));
                self.future.clear();
            }
            false => {}
        }
    }

    pub fn scroll(&mut self, e: &ScrollWheelEvent, window: &Window) {
        let (d, t, per) = (e.delta.pixel_delta(px(16.)), self.at(e.position.x, window), self.per_px(window));
        if self.web {
            return self.roam = (self.roam.0 - f32::from(d.x), self.roam.1 - f32::from(d.y));
        }
        if self.motion && t < self.pan {
            return self.lift -= f32::from(d.y);
        }
        let k = (t - self.pan) / self.span();
        self.zoom = deep(self.zoom * (f64::from(d.y) / 200.).exp(), self.tl.duration, self.fps());
        self.pan = within(t - k * self.span() - f64::from(d.x) * per, self.tl.duration, self.span());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // The recent list should live in Application Support, start from the old ~/.origami list the first
    // time, leave that old file alone, and never replace a new list with the old one.
    #[test]
    fn recent_list_moves() {
        let home = env::temp_dir().join("origami-recent-test");
        let _ = fs::remove_dir_all(&home);
        let (list, old) = (
            home.join("Library/Application Support/Origami/recent"),
            home.join(".origami/recent"),
        );
        assert_eq!(carried(&home), list);
        assert!(!list.exists());
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, "/a.js\n/b.js").unwrap();
        assert_eq!(fs::read_to_string(carried(&home)).unwrap(), "/a.js\n/b.js");
        fs::write(&list, "/c.js").unwrap();
        assert_eq!(fs::read_to_string(carried(&home)).unwrap(), "/c.js");
        assert_eq!(fs::read_to_string(old).unwrap(), "/a.js\n/b.js");
    }

    // Timeline zoom, scroll position and panel sizes should stay inside sensible limits.
    #[test]
    fn limits() {
        assert_eq!((deep(0.2, 60., 30.), deep(500., 60., 30.)), (1., 64.));
        assert_eq!((deep(500., 2., 30.), deep(500., 0., 30.)), (7.5, 1.));
        assert_eq!(
            (within(-3., 60., 10.), within(99., 60., 10.), within(20., 60., 10.)),
            (0., 50., 20.)
        );
        assert_eq!((within(5., 0., 0.001), within(40., 30., 30.)), (0., 0.));
        assert_eq!(
            (pane(900., 380., 260.), pane(100., 380., 260.), pane(900., 100., 260.)),
            (380., 260., 260.)
        );
    }

    // Scrolling the wheel over the timeline should zoom in, and scrolling back should never zoom out
    // further than showing the whole timeline.
    #[gpui::test]
    fn zooms(cx: &mut gpui::TestAppContext) {
        let path = env::temp_dir().join("origami-zoom-test.js");
        fs::write(&path, "stack(color(\"#203040\").for(40))\n").unwrap();
        let tl = origami_lang::eval(&path, false, origami_engine::footprint).unwrap();
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (
                path.clone(),
                vec!["stack(color(\"#203040\").for(40))".into()],
                std::sync::Arc::new(tl),
            );
            app
        });
        cx.run_until_parked();
        let h = view.update_in(cx, |_, window, _| f32::from(window.viewport_size().height));
        let wheel = |cx: &mut gpui::VisualTestContext, y: f32| {
            cx.simulate_event(ScrollWheelEvent {
                position: gpui::point(px(300.), px(h - 100.)),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(y))),
                ..Default::default()
            });
            view.update(cx, |o, _| o.zoom)
        };
        let spot = gpui::point(px(300.), px(h - 40.));
        cx.simulate_event(gpui::MouseDownEvent {
            button: gpui::MouseButton::Left,
            position: spot,
            modifiers: Default::default(),
            click_count: 2,
            first_mouse: false,
        });
        let (t, want) = view.update_in(cx, |o, window, _| (o.t, o.at(spot.x, window)));
        assert!(t > 0. && (t - want).abs() < 0.04);
        assert!(wheel(cx, 200.) > 2.);
        view.update(cx, |o, _| o.mouse = false);
        assert_eq!(wheel(cx, -400.), 1.);
    }

    fn window<'a>(cx: &'a mut gpui::TestAppContext, name: &str, src: &str) -> (gpui::Entity<Origami>, &'a mut gpui::VisualTestContext) {
        let path = env::temp_dir().join(format!("origami-{name}-test.js"));
        fs::write(&path, src).unwrap();
        let tl = origami_lang::eval(&path, false, origami_engine::footprint).unwrap();
        cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (path, src.lines().map(String::from).collect(), std::sync::Arc::new(tl));
            app
        })
    }

    // Clicking a clip that the timeline still shows after its lines were deleted from the script,
    // or picking its layer in the Motion tab, should select it without crashing.
    #[gpui::test]
    fn stale_clips_still_click(cx: &mut gpui::TestAppContext) {
        let src = "stack(\n  color(\"#203040\").for(4),\n  text(\"a\").for(4),\n  text(\"b\").for(4),\n)";
        let (view, cx) = window(cx, "stale", src);
        cx.run_until_parked();
        view.update(cx, |o, _| o.lines = vec!["stack(".into()]);
        let (x, top, h) = view.update_in(cx, |o, window, _| {
            let h = f32::from(window.viewport_size().height);
            (o.lead() + (2. / o.per_px(window)) as f32, h - 28. - o.dock + RULER, h)
        });
        let mut picked = None;
        for y in (0..((h - 30. - top) / 6.) as usize).map(|i| top + 6. * i as f32) {
            let at = gpui::point(px(x), px(y));
            cx.simulate_mouse_down(at, gpui::MouseButton::Left, Default::default());
            cx.simulate_mouse_up(at, gpui::MouseButton::Left, Default::default());
            picked = picked.or(view.update(cx, |o, _| o.sel));
        }
        assert!(picked.is_some());
        view.update(cx, |o, _| {
            o.lines.clear();
            (o.motion, o.sel) = (true, None);
            o.pick(2);
            assert_eq!((o.sel, o.top), (Some(2), 0.));
        });
    }

    // Letting go of the mouse outside the window, or a button that comes up where the window does not
    // see it, should end a clip drag, a ruler scrub or any other drag right there: later moves with no
    // button held change nothing, and a drag that changed the script leaves one undo step.
    #[gpui::test]
    fn drags_end_outside_the_window(cx: &mut gpui::TestAppContext) {
        use gpui::{Modifiers, MouseButton::Left, point};
        let src = "stack(\n  color(\"#203040\").for(4),\n  text(\"hi\").at(2).for(3),\n)";
        let (view, cx) = window(cx, "outside", src);
        cx.run_until_parked();
        let (x, lane, ruler, h) = view.update_in(cx, |o, window, _| {
            let (h, x) = (f32::from(window.viewport_size().height), o.lead() + (3.5 / o.per_px(window)) as f32);
            (x, h - 28. - o.dock + RULER + 28., h - 28. - o.dock + 11., h)
        });
        let state = |cx: &mut gpui::VisualTestContext| {
            view.update(cx, |o, _| (o.lines[2].clone(), o.drag.is_some() || o.scrubbing, o.past.len(), o.t))
        };
        let m = Modifiers::default();
        cx.simulate_mouse_move(point(px(x), px(lane)), None, m);
        cx.simulate_mouse_down(point(px(x), px(lane)), Left, m);
        cx.simulate_mouse_move(point(px(x + 40.), px(lane)), Some(Left), m);
        cx.simulate_mouse_up(point(px(x + 60.), px(h + 40.)), Left, m);
        let moved = state(cx);
        assert!(moved.0.contains(".shift(") && !moved.1 && moved.2 == 1, "{moved:?}");
        for dx in [100., 160.] {
            cx.simulate_mouse_move(point(px(x + dx), px(lane - 200.)), None, m);
            assert_eq!(state(cx), moved);
        }
        assert_eq!(
            fs::read_to_string(view.update(cx, |o, _| o.script.clone())).unwrap().lines().nth(2),
            Some(moved.0.as_str())
        );
        cx.simulate_mouse_down(point(px(x), px(ruler)), Left, m);
        let t = state(cx).3;
        assert!(state(cx).1 && t > 0.);
        cx.simulate_mouse_up(point(px(x), px(h + 40.)), Left, m);
        cx.simulate_mouse_move(point(px(x + 150.), px(ruler)), None, m);
        assert_eq!(state(cx), (moved.0.clone(), false, 1, t));
        cx.simulate_mouse_down(point(px(x), px(ruler)), Left, m);
        cx.simulate_mouse_move(point(px(x + 150.), px(ruler)), None, m);
        assert_eq!(state(cx), (moved.0.clone(), false, 1, t));
        view.update(cx, |o, _| {
            o.seize(point(px(x), px(lane)), [0.; 3], crate::motion::Pull::Zoom(1.));
            (o.pull, o.eyeing, o.picking, o.turning) = (Some(true), true, true, Some(crate::knob::Turn::Wheel));
        });
        cx.simulate_mouse_up(point(px(-30.), px(-30.)), Left, m);
        let gone = view.update(cx, |o, _| {
            o.pull.is_none() && !o.eyeing && !o.picking && o.turning.is_none() && o.slip.is_none()
        });
        assert!(gone);
    }

    // Dragging a clip that shares its line with another clip, or whose call is spread over several
    // lines, should leave the script alone with a note, and should not add an undo step. A clip on
    // its own line still moves. Pressing a knob without turning it adds no undo step either.
    #[gpui::test]
    fn drags_refuse_unsafe_rewrites(cx: &mut gpui::TestAppContext) {
        let shared = "stack(text(\"a\").for(2), text(\"b\").for(2).at(1))";
        let (view, cx) = window(cx, "shared", shared);
        let drag = |cx: &mut gpui::VisualTestContext, grab: Grab, z: usize, dt: f64| {
            view.update(cx, |o, _| {
                let c = o.tl.clips[z].clone();
                o.base = o.lines.clone();
                o.reshape(&grab, &c, dt);
                o.drag = Some((grab, c, 0., dt));
                o.release();
                (o.lines.join("\n"), o.past.len(), o.note.clone())
            })
        };
        let says = "line 1 holds more than b, edit the script to";
        for grab in [Grab::Move, Grab::Left, Grab::Right] {
            let got = drag(cx, grab, 1, 0.5);
            assert_eq!((got.0.as_str(), got.1), (shared, 0));
            assert!(got.2.starts_with(says), "{}", got.2);
        }
        view.update(cx, |o, _| {
            (o.sel, o.note) = (Some(1), String::new());
            o.shift(0.5);
            assert_eq!((o.lines.join("\n").as_str(), o.past.len()), (shared, 0));
            assert!(o.note.starts_with(says), "{}", o.note);
        });
        let spread = "stack(\n  text(\"a\").for(\n    2\n  ),\n  text(\"b\").for(2),\n)";
        let (view, cx) = window(cx, "spread", spread);
        let got = view.update(cx, |o, _| {
            let c = o.tl.clips[0].clone();
            o.base = o.lines.clone();
            o.reshape(&Grab::Right, &c, 0.5);
            o.drag = Some((Grab::Right, c, 0., 0.5));
            o.release();
            (o.lines.join("\n"), o.past.len(), o.note.clone())
        });
        assert_eq!((got.0.as_str(), got.1), (spread, 0));
        assert_eq!(got.2, "line 2 holds more than a, edit the script to trim it");
        view.update(cx, |o, _| {
            let c = o.tl.clips[1].clone();
            o.base = o.lines.clone();
            o.reshape(&Grab::Move, &c, 0.5);
            assert_eq!(o.lines[4], "  text(\"b\").for(2).shift(0.5000),");
            (o.past, o.turning) = (Default::default(), Some(crate::knob::Turn::Wheel));
            o.past.push(o.lines.clone());
            o.release();
            assert_eq!(o.past.len(), 0);
        });
    }

    // A named clip used inside a stack on another line should still trim and move from the
    // timeline, while the clip menu, which copies whole lines, still leaves its definition alone.
    #[gpui::test]
    fn named_clips_still_drag(cx: &mut gpui::TestAppContext) {
        let (view, cx) = window(cx, "named", "const a = text(\"a\").for(2)\nstack(text(\"b\").for(4), a)");
        view.update(cx, |o, _| {
            let c = o.tl.clips.iter().find(|c| c.label == "a").unwrap().clone();
            o.reshape(&Grab::Right, &c, 0.5);
            o.reshape(&Grab::Move, &c, 0.5);
            assert_eq!(o.lines[0], "const a = text(\"a\").for(2.50).shift(0.5000)");
            assert!(o.used(c, "duplicate").is_none());
        });
    }
}
