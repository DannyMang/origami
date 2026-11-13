mod complete;
mod edit;
mod fold;
mod graph;
mod keys;
mod knob;
mod lint;
mod motion;
mod mouse;
mod peek;
mod scene;
mod search;
mod view;

use gpui::{
    App, AppContext, Application, Bounds, Context, FocusHandle, Pixels, RenderImage, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};
use image::{Frame, RgbaImage};
use mouse::Grab;
use origami_engine::{Player, Preview, peaks, thumb};
use origami_lang::{Clip, Timeline};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, PartialEq)]
enum Mode {
    Normal,
    Visual(f64),
    Command(String),
    Search(String),
}

enum Asset {
    Thumb(Arc<RenderImage>),
    Wave(Vec<f32>),
}

struct Origami {
    script: PathBuf,
    lines: Vec<String>,
    mtime: Option<SystemTime>,
    tl: Arc<Timeline>,
    rev: usize,
    err: String,
    bad: u32,
    note: String,
    t: f64,
    mode: Mode,
    mouse: bool,
    scrubbing: bool,
    drag: Option<(Grab, Clip, f64, f64)>,
    base: Vec<String>,
    sel: Option<u32>,
    held: Option<f64>,
    push: Option<Pixels>,
    again: bool,
    zoom: f64,
    pan: f64,
    cursor: Option<(usize, usize)>,
    anchor: Option<(usize, usize)>,
    picking: bool,
    offers: (usize, Vec<complete::Offer>),
    pick: usize,
    slide: f32,
    left: f32,
    folded: Vec<String>,
    hover: Option<(usize, usize)>,
    since: Instant,
    tipped: bool,
    knobbed: Option<(usize, usize)>,
    kept: Option<(usize, usize)>,
    turning: Option<knob::Turn>,
    motion: bool,
    graph: bool,
    lift: f32,
    key: Option<(u32, &'static str, usize)>,
    slip: Option<motion::Slip>,
    disc: Arc<RenderImage>,
    strips: [Arc<RenderImage>; 3],
    finding: Option<String>,
    hit: usize,
    modes: [bool; 3],
    peek: Option<peek::Peek>,
    scout: Arc<Mutex<Preview>>,
    eyeing: bool,
    seeker: Option<regex::Regex>,
    fresh: bool,
    past: Vec<Vec<String>>,
    future: Vec<Vec<String>>,
    full: bool,
    dirty: bool,
    saved: String,
    top: f32,
    fit: usize,
    recent: Vec<PathBuf>,
    rate: f64,
    side: f32,
    dock: f32,
    font: f32,
    pull: Option<bool>,
    menu: &'static str,
    shut: &'static str,
    count: String,
    pending: Option<char>,
    marks: HashMap<char, f64>,
    layer: usize,
    query: String,
    last: String,
    looping: bool,
    help: bool,
    preview: Arc<Mutex<Preview>>,
    image: Option<Arc<RenderImage>>,
    shown: Option<(i64, usize)>,
    busy: bool,
    reading: bool,
    clash: bool,
    player: Option<(Instant, f64, Option<Player>)>,
    assets: HashMap<String, Option<Asset>>,
    jobs: Vec<(String, String, Option<f64>)>,
    loading: usize,
    export: Option<Arc<Mutex<(String, bool)>>>,
    focus: FocusHandle,
}

fn clock(script: &Path) -> String {
    let mtime = fs::metadata(script).and_then(|m| m.modified()).ok();
    let secs = mtime.and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let out = Command::new("date").args(["-r", &secs.to_string(), "+%H:%M"]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn bitmap(px: Vec<u8>, w: usize, h: usize) -> Arc<RenderImage> {
    Arc::new(RenderImage::new(vec![Frame::new(
        RgbaImage::from_raw(w as u32, h as u32, px).unwrap(),
    )]))
}

impl Origami {
    fn fps(&self) -> f64 {
        self.tl.fps.max(1) as f64
    }

    fn end(&self) -> f64 {
        (self.tl.duration - 1. / self.fps()).max(0.)
    }

    fn under(&self) -> Vec<&Clip> {
        self.tl.clips.iter().filter(|c| c.start <= self.t && self.t < c.end).collect()
    }

    fn opened(&self) -> Option<&Clip> {
        self.tl.clips.iter().find(|c| Some(c.z) == self.sel)
    }

    fn focused(&self) -> Option<&Clip> {
        let under = self.under();
        self.opened().or(under.get(self.layer.min(under.len().saturating_sub(1))).copied())
    }

    fn seek(&mut self, t: f64) {
        self.t = (t.max(0.).min(self.end()) * self.fps()).round() / self.fps();
        self.sel = self.sel.filter(|_| self.motion);
        match self.scrubbing {
            _ if self.player.is_none() => {}
            true => self.player = Some((Instant::now(), self.t, None)),
            false => self.play(),
        }
    }

    fn play(&mut self) {
        self.player = None;
        if self.t >= self.end() {
            self.t = 0.;
        }
        let player = Player::new(&self.tl, self.t, self.rate).map_err(|e| self.note = e.to_string()).ok();
        self.player = Some((Instant::now(), self.t, player));
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let disk: Vec<String> = fs::read_to_string(&self.script)
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect();
        let differs = disk != self.lines;
        if self.dirty {
            (self.clash, self.dirty) = (differs, differs);
        } else if differs {
            let old = std::mem::replace(&mut self.lines, disk);
            if !self.fresh {
                self.past.push(old);
            }
        }
        if std::mem::take(&mut self.fresh) {
            self.fold_all(true);
        }
        let (script, path) = (self.script.clone(), self.script.clone());
        let saved = cx.background_executor().spawn(async move { clock(&path) });
        self.reading = true;
        cx.spawn(async move |this, cx| {
            let tl = cx
                .background_executor()
                .spawn(async move { origami_lang::eval(&script, true) })
                .await;
            this.update(cx, |this, cx| {
                this.reading = false;
                match tl {
                    Ok(tl) => {
                        (this.tl, this.rev) = (Arc::new(tl), this.rev + 1);
                        this.err.clear();
                        this.bad = 0;
                        this.seek(this.t);
                        this.refresh(cx);
                    }
                    Err(e) => {
                        let text = e.to_string();
                        let text = text.lines().next().unwrap_or_default();
                        this.bad = ["at :", "line "]
                            .iter()
                            .find_map(|k| {
                                let rest = &text[text.find(k)? + k.len()..];
                                rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len())].parse().ok()
                            })
                            .unwrap_or(0);
                        this.err = text.split(" (").next().unwrap_or_default().trim_start_matches("Error: ").into();
                        this.unfold((this.bad as usize).saturating_sub(1));
                    }
                }
                cx.notify();
            })?;
            let saved = saved.await;
            this.update(cx, |this, cx| {
                this.saved = saved;
                cx.notify();
            })
        })
        .detach();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) -> bool {
        let want = ((self.t * self.fps()).round() as i64, self.rev);
        if self.busy || self.shown == Some(want) || self.tl.duration <= 0. {
            return false;
        }
        self.busy = true;
        let (preview, tl, t) = (self.preview.clone(), self.tl.clone(), want.0 as f64 / self.fps());
        cx.spawn(async move |this, cx| {
            let (w, h) = Preview::size(&tl);
            let px = cx
                .background_executor()
                .spawn(async move { preview.lock().unwrap().frame(&tl, t, (w, h)) })
                .await;
            this.update(cx, |this, cx| {
                if let Some(old) = this.image.replace(bitmap(px, w, h)) {
                    cx.drop_image(old, None);
                }
                (this.busy, this.shown) = (false, Some(want));
                this.refresh(cx);
                cx.notify();
            })
        })
        .detach();
        true
    }

    fn asset(&mut self, key: String, path: &str, t: Option<f64>) -> Option<&Asset> {
        if !self.assets.contains_key(&key) {
            self.jobs.push((key.clone(), path.into(), t));
        }
        self.assets.entry(key).or_default().as_ref()
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let mtime = fs::metadata(&self.script).and_then(|m| m.modified()).ok();
        if mtime != self.mtime && !self.reading {
            self.mtime = mtime;
            self.load(cx);
        }
        if self.dwell() {
            cx.notify();
        }
        self.gaze(cx);
        if self.push.is_some() {
            self.again = true;
            cx.notify();
        }
        let was = (self.t, self.player.is_some());
        if let Some((since, t0, player)) = self.player.as_ref().filter(|_| !self.scrubbing) {
            let t = player.as_ref().map_or(t0 + since.elapsed().as_secs_f64() * self.rate, Player::time);
            let mark = |m| self.marks.get(&m).copied();
            let (a, b) = mark('a').zip(mark('b')).filter(|_| self.looping).unwrap_or((0., self.tl.duration));
            if t < b && !player.as_ref().is_some_and(Player::done) {
                self.t = (t * self.fps()).round() / self.fps();
            } else if self.looping {
                self.t = a;
                self.play();
            } else {
                self.player = None;
                self.t = self.end();
            }
        }
        if let Some(job) = self.export.clone() {
            let (note, done) = job.lock().unwrap().clone();
            if self.note != note || done {
                cx.notify();
            }
            self.note = note;
            self.export = self.export.take().filter(|_| !done);
        }
        if !self.refresh(cx) && was != (self.t, self.player.is_some()) {
            cx.notify();
        }
        while self.loading < 4
            && let Some(i) = self.jobs.iter().rposition(|j| j.2.is_some()).or(self.jobs.len().checked_sub(1))
        {
            let (key, path, t) = self.jobs.remove(i);
            self.loading += 1;
            cx.spawn(async move |this, cx| {
                let load = async move {
                    match t {
                        Some(t) => thumb(&path, t, 176, 100).map(|px| Asset::Thumb(bitmap(px, 176, 100))),
                        None => Some(Asset::Wave(peaks(&path))),
                    }
                };
                let asset = cx.background_executor().spawn(load).await;
                this.update(cx, |this, cx| {
                    this.assets.insert(key, asset);
                    this.loading -= 1;
                    cx.notify();
                })
            })
            .detach();
        }
    }
}

fn main() {
    let Ok(script) = env::args().nth(1).map(fs::canonicalize).transpose() else {
        return eprintln!("usage: origami-app [script.js]");
    };
    let script = script.unwrap_or_default();
    thread::spawn(Player::warm);
    Application::new().run(move |cx: &mut App| {
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("origami".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1512.), px(900.)), cx))),
            window_min_size: Some(size(px(680.), px(330.))),
            ..Default::default()
        };
        cx.on_window_closed(|cx| cx.quit()).detach();
        cx.open_window(options, |window, cx| {
            cx.new(|cx: &mut Context<Origami>| {
                let focus = cx.focus_handle();
                window.focus(&focus);
                let weak = cx.weak_entity();
                window.on_window_should_close(cx, move |window, cx| {
                    let ask = |this: &mut Origami, cx: &mut Context<Origami>| {
                        if this.dirty {
                            this.leave(window, cx);
                        }
                        !this.dirty
                    };
                    weak.update(cx, ask).unwrap_or(true)
                });
                let recent = fs::read_to_string(mouse::recent())
                    .unwrap_or_default()
                    .lines()
                    .map(PathBuf::from)
                    .collect();
                cx.spawn(async move |this, cx| {
                    while this.update(cx, |this, cx| this.tick(cx)).is_ok() {
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                })
                .detach();
                let mut app = Origami {
                    script: PathBuf::new(),
                    lines: vec![],
                    mtime: None,
                    tl: Arc::default(),
                    rev: 0,
                    err: String::new(),
                    bad: 0,
                    note: String::new(),
                    t: 0.,
                    mode: Mode::Normal,
                    mouse: true,
                    scrubbing: false,
                    drag: None,
                    base: vec![],
                    sel: None,
                    held: None,
                    push: None,
                    again: false,
                    zoom: 1.,
                    pan: 0.,
                    cursor: None,
                    anchor: None,
                    picking: false,
                    offers: (0, vec![]),
                    pick: 0,
                    slide: 0.,
                    left: 0.,
                    folded: vec![],
                    hover: None,
                    since: Instant::now(),
                    tipped: false,
                    knobbed: None,
                    kept: None,
                    turning: None,
                    motion: false,
                    graph: false,
                    lift: 0.,
                    key: None,
                    slip: None,
                    disc: bitmap(knob::disc(), 300, 300),
                    strips: [0, 1, 2].map(|kind| bitmap(knob::strip(kind), 150, 150)),
                    finding: None,
                    hit: 0,
                    modes: [false; 3],
                    peek: None,
                    scout: Arc::default(),
                    eyeing: false,
                    seeker: None,
                    fresh: false,
                    past: vec![],
                    future: vec![],
                    full: false,
                    dirty: false,
                    saved: String::new(),
                    top: 0.,
                    fit: 24,
                    recent,
                    rate: 1.,
                    side: 520.,
                    dock: 214.,
                    font: 13.,
                    pull: None,
                    menu: "",
                    shut: "",
                    count: String::new(),
                    pending: None,
                    marks: HashMap::new(),
                    layer: 0,
                    query: String::new(),
                    last: String::new(),
                    looping: false,
                    help: false,
                    preview: Arc::default(),
                    image: None,
                    shown: None,
                    busy: false,
                    reading: false,
                    clash: false,
                    player: None,
                    assets: HashMap::new(),
                    jobs: vec![],
                    loading: 0,
                    export: None,
                    focus,
                };
                if !script.as_os_str().is_empty() {
                    app.open(script);
                }
                app
            })
        })
        .unwrap();
        cx.activate(true);
    });
}
