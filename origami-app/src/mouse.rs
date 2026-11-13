use crate::{Origami, view::HEAD};
use gpui::{Context, Pixels, PromptLevel, ScrollWheelEvent, Window, px};
use origami_lang::{Clip, Kind};
use std::{env, fs, mem, path::PathBuf};

const STARTER: &str = "stack(\n  cat(\n    color(\"#282c33\").for(5),\n  ),\n  text(\"hello origami\"),\n)\n";

pub fn recent() -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap_or_default()).join(".origami/recent")
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

    pub fn per_px(&self, window: &Window) -> f64 {
        self.span() / (f32::from(window.viewport_size().width) - self.side - HEAD - 12.).max(1.) as f64
    }

    fn at(&self, x: Pixels, window: &Window) -> f64 {
        self.pan + (f32::from(x) - HEAD) as f64 * self.per_px(window)
    }

    pub fn toggle(&mut self) {
        match self.player.take() {
            Some(_) => {}
            None => self.play(),
        }
    }

    pub fn open(&mut self, path: PathBuf) {
        self.recent.retain(|p| *p != path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(6);
        let (list, all) = (recent(), self.recent.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>());
        let _ = fs::create_dir_all(list.parent().unwrap());
        let _ = fs::write(list, all.join("\n"));
        (self.script, self.mtime, self.t, self.dirty) = (path, None, 0., false);
        (self.past, self.future, self.cursor, self.top, self.left) = (vec![], vec![], None, 0., 0.);
        (self.folded, self.fresh) = (vec![], true);
    }

    pub fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dirty {
            return cx.quit();
        }
        let detail = Some("Save them before closing?");
        let answer = window.prompt(
            PromptLevel::Warning,
            "You have unsaved changes",
            detail,
            &["Save", "Discard", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let pick = answer.await.unwrap_or(2);
            let _ = this.update(cx, |this, cx| {
                if pick == 0 {
                    this.store();
                }
                if pick < 2 {
                    cx.quit();
                }
            });
        })
        .detach();
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
        self.zoom = (self.zoom * k).clamp(1., 64.);
        self.pan = (self.t - self.span() / 2.).min(self.tl.duration - self.span()).max(0.);
    }

    pub fn fullscreen(&mut self, window: &Window) {
        self.full = !self.full;
        if window.is_fullscreen() != self.full {
            window.toggle_fullscreen();
        }
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
        match grab {
            _ if dt == 0. => {}
            Grab::Move => self.rewrite(c.line, "shift", moved),
            Grab::Left if media => self.rewrite(c.line, "trim", |_| trim(a + d, b)),
            Grab::Right if media => self.rewrite(c.line, "trim", |_| trim(a, b + d)),
            Grab::Right => self.rewrite(c.line, "for", |_| last(len + dt)),
            Grab::Left => {
                self.rewrite(c.line, "for", |_| last(len - dt));
                self.rewrite(c.line, "shift", moved);
            }
        }
    }

    pub fn drag_to(&mut self, x: Pixels, window: &Window) {
        let past = f32::from(x) > f32::from(window.viewport_size().width) - self.side - 12.;
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
            true => self.side = (w - f32::from(x)).min(w - 420.).max(260.),
            false => self.dock = (h - f32::from(y) - 62.).min(h - 260.).max(70.),
        }
    }

    pub fn release(&mut self) {
        if self.scrubbing && self.player.is_some() {
            self.play();
        }
        (self.scrubbing, self.pull, self.picking, self.turning, self.held, self.push) = (false, None, false, None, None, None);
        let Some((_, c, _, dt)) = self.drag.take() else {
            return;
        };
        match dt == 0. {
            true => {
                self.sel = Some(c.z);
                self.reveal((c.line as usize).saturating_sub(1));
            }
            false => {
                self.past.push(mem::take(&mut self.base));
                self.future.clear();
            }
        }
    }

    pub fn scroll(&mut self, e: &ScrollWheelEvent, window: &Window) {
        if !self.mouse {
            return;
        }
        let (d, t, per) = (e.delta.pixel_delta(px(16.)), self.at(e.position.x, window), self.per_px(window));
        let k = (t - self.pan) / self.span();
        self.zoom = (self.zoom * (f64::from(d.y) / 200.).exp()).clamp(1., 64.);
        self.pan = t - k * self.span() - f64::from(d.x) * per;
        self.pan = self.pan.min(self.tl.duration - self.span()).max(0.);
    }

    pub fn create(&mut self, cx: &mut Context<Self>) {
        let path = cx.prompt_for_new_path(&env::current_dir().unwrap_or_default(), Some("edit.js"));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = path.await
                && fs::write(&path, STARTER).is_ok()
            {
                let _ = this.update(cx, |this, cx| {
                    this.open(path);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
