use crate::{Mode, Origami, knob::close};
use gpui::{ClipboardItem, Context, KeyDownEvent, Window};
use origami_engine::{command, render};
use origami_lang::Clip;
use std::{
    env, fs, mem,
    process::Command,
    sync::{Arc, Mutex},
};

pub fn stamp(t: f64) -> String {
    format!("{}:{:05.2}", (t / 60.) as u32, t % 60.)
}

fn secs(s: &str) -> Option<f64> {
    s.split(':').try_fold(0., |a, x| x.parse::<f64>().ok().map(|x| a * 60. + x))
}

pub fn link(lines: &[String], line: u32, op: &str) -> Option<usize> {
    let (key, last) = (format!(".{op}("), (line as usize).checked_sub(1).filter(|i| *i < lines.len())?);
    let linked = |i: &usize| lines[*i..=last].iter().skip(1).all(|l| l.trim_start().starts_with('.'));
    (0..=last).rev().take_while(linked).find(|i| lines[*i].contains(&key))
}

pub fn rewrite(lines: &mut [String], line: u32, op: &str, arg: impl Fn(&str) -> String) {
    let key = format!(".{op}(");
    let Some(last) = (line as usize).checked_sub(1).filter(|i| *i < lines.len()) else {
        return;
    };
    let s = &mut lines[link(lines, line, op).unwrap_or(last)];
    if let Some(a) = s.find(&key).map(|a| a + key.len()) {
        let b = close(s.as_bytes(), a - 1).unwrap_or(a);
        let new = arg(&s[a..b]);
        s.replace_range(a..b, &new);
    } else if let Some(a) = s.rfind(')') {
        s.insert_str(a + 1, &format!("{key}{})", arg("")));
    }
}

impl Origami {
    pub fn code(&self, c: &Clip, a: f64, b: f64) -> String {
        let src = |t: f64| stamp(c.src_in + (t.clamp(c.start, c.end) - c.start) * c.speed);
        format!("{}.trim(\"{}\", \"{}\")", c.label, src(a), src(b))
    }

    pub fn key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = &e.keystroke;
        let save = key.modifiers.control && matches!(key.key.as_str(), "s" | "0" | "f");
        if key.modifiers.control && !save || self.script.as_os_str().is_empty() {
            return;
        }
        let k = match key.key.as_str() {
            k @ ("enter" | "escape" | "backspace" | "delete" | "space" | "tab") => k.to_string(),
            k => key.key_char.clone().unwrap_or(k.to_string()),
        };
        let plain = !(key.modifiers.platform || key.modifiers.shift || key.modifiers.alt || save);
        if self.choose(&k, plain) {
            return cx.notify();
        }
        if self.knobbed.take().is_some() && k == "escape" {
            return cx.notify();
        }
        if k == "escape" && self.key.take().is_some() {
            return cx.notify();
        }
        if self.finding.is_some() && !(key.modifiers.platform || save) {
            self.search(&k, key.modifiers.shift);
            return cx.notify();
        }
        match (key.modifiers.platform || save, key.key.as_str()) {
            (true, "s") => self.store(),
            (true, "f") => self.finding = self.finding.take().xor(Some(String::new())),
            (true, "0") => self.reset(),
            (true, "[" | "]") if key.modifiers.alt => {
                let row = self.cursor.map(|c| c.0).or(self.focused().map(|c| c.line as usize - 1));
                self.fold(row.unwrap_or(0), Some(key.key == "["));
            }
            (true, "q") => self.leave(window, cx),
            (true, "z") if key.modifiers.shift => self.redo(),
            (true, "z") => self.undo(),
            (true, "=" | "+") => self.scale(1.),
            (true, "-") => self.scale(-1.),
            (true, "v") if self.cursor.is_some() => self.paste(cx),
            (true, "a") if self.cursor.is_some() => self.all(),
            (true, "c" | "x") if self.cursor.is_some() => {
                cx.write_to_clipboard(ClipboardItem::new_string(self.picked()));
                if key.key == "x" {
                    self.draft(Self::cut);
                }
            }
            (true, arrow @ ("left" | "right" | "up" | "down")) if self.cursor.is_some() => {
                let to = ["home", "end", "top", "bottom"][["left", "right", "up", "down"].iter().position(|a| *a == arrow).unwrap_or(0)];
                self.edit(to, key.modifiers.shift);
            }
            (true, _) => return,
            _ if self.cursor.is_some() => {
                self.draft(|this| this.edit(&k, key.modifiers.shift));
                self.suggest(&k);
            }
            (_, "escape") if self.full => self.fullscreen(window),
            (_, "f") if self.mouse && self.mode == Mode::Normal => self.fullscreen(window),
            _ => return self.press(&k, cx),
        }
        cx.notify();
    }

    pub fn press(&mut self, k: &str, cx: &mut Context<Self>) {
        self.note.clear();
        match mem::replace(&mut self.mode, Mode::Normal) {
            mode @ (Mode::Command(_) | Mode::Search(_)) => {
                let cmd = matches!(mode, Mode::Command(_));
                let (Mode::Command(mut s) | Mode::Search(mut s)) = mode else {
                    return;
                };
                match k {
                    "enter" if cmd => {
                        self.last = s.clone();
                        self.run(&s, cx);
                    }
                    "enter" => {
                        self.query = s;
                        self.find(true);
                    }
                    "escape" => {}
                    "backspace" if s.is_empty() => {}
                    k => {
                        match k {
                            "backspace" => drop(s.pop()),
                            "space" => s.push(' '),
                            k => s.push_str(k),
                        }
                        self.mode = if cmd { Mode::Command(s) } else { Mode::Search(s) };
                    }
                }
            }
            mode => {
                self.mode = mode;
                let f = self.fps();
                match k {
                    _ if !self.mouse => self.normal(k, cx),
                    "space" => self.toggle(),
                    "backspace" | "delete" => self.delete(),
                    "left" => self.seek(self.t - 1. / f),
                    "right" => self.seek(self.t + 1. / f),
                    ":" => self.mode = Mode::Command(String::new()),
                    "escape" => (self.mouse, self.help) = (false, false),
                    _ => {}
                }
            }
        }
        cx.notify();
    }

    fn normal(&mut self, k: &str, cx: &mut Context<Self>) {
        let (f, t) = (self.fps(), self.t);
        if let Some(p) = self.pending.take() {
            let c = k.chars().next().unwrap_or(' ');
            match (p, k) {
                ('g', "g") => self.seek(0.),
                ('g', "d") => self.editor(),
                ('z', "a") => self.fold(self.focused().map_or(0, |c| c.line as usize - 1), None),
                ('z', "M") => self.fold_all(false),
                ('z', "R") => self.folded.clear(),
                ('m', _) => drop(self.marks.insert(c, t)),
                ('\'', _) => self.seek(self.marks.get(&c).copied().unwrap_or(t)),
                _ => {}
            }
            return;
        }
        if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() && (k != "0" || !self.count.is_empty()) {
            return self.count.push_str(k);
        }
        let n = mem::take(&mut self.count).parse::<f64>().unwrap_or(1.);
        match k {
            "h" => self.seek(t - n / f),
            "l" => self.seek(t + n / f),
            "H" => self.seek(t - n),
            "L" => self.seek(t + n),
            "w" | "b" => {
                let mut edges: Vec<f64> = self.tl.clips.iter().flat_map(|c| [c.start, c.end]).collect();
                edges.sort_by(f64::total_cmp);
                let hit = match k {
                    "w" => edges.iter().filter(|&&x| x > t + 0.5 / f).nth(n as usize - 1),
                    _ => edges.iter().rev().filter(|&&x| x < t - 0.5 / f).nth(n as usize - 1),
                };
                self.seek(hit.copied().unwrap_or(t));
            }
            "0" => self.seek(0.),
            "$" | "G" => self.seek(self.tl.duration),
            "g" | "m" | "'" | "z" => self.pending = k.chars().next(),
            "j" => self.layer = self.layer.saturating_sub(1),
            "k" => self.layer = (self.layer + 1).min(self.under().len().saturating_sub(1)),
            "<" | ">" => {
                if let Some(line) = self.focused().map(|c| c.line) {
                    self.shift(line, if k == "<" { -n / f } else { n / f });
                }
            }
            "space" => self.toggle(),
            "x" => self.delete(),
            "i" => (self.mouse, self.mode) = (true, Mode::Normal),
            "v" => self.mode = if self.mode == Mode::Normal { Mode::Visual(t) } else { Mode::Normal },
            "y" => {
                let Some(c) = self.focused() else { return };
                let (a, b) = if let Mode::Visual(a) = self.mode {
                    (a.min(t), a.max(t))
                } else {
                    (c.start, c.end)
                };
                let code = self.code(c, a, b);
                cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                (self.note, self.mode) = (format!("yanked {code}"), Mode::Normal);
            }
            ":" => self.mode = Mode::Command(String::new()),
            "/" => self.mode = Mode::Search(String::new()),
            "n" => self.find(true),
            "N" => self.find(false),
            "." => self.run(&self.last.clone(), cx),
            "escape" => (self.mode, self.help) = (Mode::Normal, false),
            _ => {}
        }
    }

    fn find(&mut self, forward: bool) {
        let q = &self.query;
        let mut hits: Vec<f64> = self
            .tl
            .clips
            .iter()
            .filter(|c| !q.is_empty() && c.label.contains(q))
            .map(|c| c.start)
            .collect();
        hits.sort_by(f64::total_cmp);
        let hit = match forward {
            true => hits.iter().find(|&&s| s > self.t + 1e-6).or(hits.first()),
            false => hits.iter().rev().find(|&&s| s < self.t - 1e-6).or(hits.last()),
        };
        self.seek(hit.copied().unwrap_or(self.t));
    }

    pub fn shift(&mut self, line: u32, dt: f64) {
        self.patch(line, "shift", |old| format!("{:.4}", old.trim().parse::<f64>().unwrap_or(0.) + dt));
    }

    pub fn rewrite(&mut self, line: u32, op: &str, arg: impl Fn(&str) -> String) {
        rewrite(&mut self.lines, line, op, arg);
    }

    pub fn patch(&mut self, line: u32, op: &str, arg: impl Fn(&str) -> String) {
        self.change(|this| this.rewrite(line, op, arg));
    }

    fn editor(&self) {
        let Some(c) = self.focused() else { return };
        let editor = env::var("EDITOR").unwrap_or("zed".into());
        let _ = Command::new(editor).arg(format!("{}:{}", self.script.display(), c.line)).spawn();
    }

    pub fn run(&mut self, cmd: &str, cx: &mut Context<Self>) {
        let (head, arg) = cmd.trim().split_once(' ').unwrap_or((cmd.trim(), ""));
        let dir = self.script.parent().unwrap().to_path_buf();
        match head {
            "q" if self.dirty => self.note = "unsaved changes: cmd+s saves, :q! discards".into(),
            "q" | "q!" => cx.quit(),
            "goto" => self.seek(secs(arg).unwrap_or(self.t)),
            "f" => self.seek(arg.parse::<f64>().map_or(self.t, |n| n / self.fps())),
            "e" => self.open(dir.join(arg)),
            "set" => self.looping = arg == "loop",
            "help" => self.help = !self.help,
            "ir" => {
                let _ = fs::create_dir_all(dir.join(".origami"));
                let _ = fs::write(
                    dir.join(".origami/ir.json"),
                    serde_json::to_string_pretty(&*self.tl).unwrap_or_default(),
                );
                self.note = "wrote .origami/ir.json".into();
            }
            "export" => {
                let name = arg.split(' ').find(|a| !a.is_empty() && !a.starts_with("--")).unwrap_or("out.mp4");
                let out = dir.join(name).to_string_lossy().to_string();
                if arg.contains("--dry-run") {
                    let quote = |a: String| format!("'{}'", a.replace('\'', "'\\''"));
                    let line = command(&self.tl, &out).into_iter().map(quote).collect::<Vec<_>>().join(" ");
                    let _ = fs::write(dir.join("export.sh"), format!("ffmpeg {line}\n"));
                    return self.note = "wrote export.sh".into();
                }
                let (tl, job) = (self.tl.clone(), Arc::new(Mutex::new(("exporting 0%".to_string(), false))));
                self.export = Some(job.clone());
                let work = async move {
                    let done = render(&tl, &out, |p| job.lock().unwrap().0 = format!("exporting {:.0}%", p * 100.));
                    *job.lock().unwrap() = (done.map_or_else(|e| e.to_string(), |_| format!("exported {out}")), true);
                };
                cx.background_executor().spawn(work).detach();
            }
            t => match secs(t) {
                Some(t) => self.seek(t),
                None => self.note = format!("unknown command: {cmd}"),
            },
        }
    }
}
