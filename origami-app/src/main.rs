mod begin;
mod clip;
mod complete;
mod crash;
mod edit;
mod fold;
mod full;
mod graph;
mod keys;
mod knob;
mod lint;
mod menu;
mod motion;
mod mouse;
mod peek;
mod pen;
mod pulse;
mod quill;
mod release;
mod saves;
mod scene;
mod search;
mod shelf;
mod ship;
mod stage;
mod tune;
mod view;
mod voice;
mod web;
mod words;

use gpui::{
    App, AppContext, Application, Bounds, Context, FocusHandle, Pixels, Point, RenderImage, TitlebarOptions, Window, WindowBounds,
    WindowOptions, px, size,
};
use image::{Frame, RgbaImage};
use mouse::Grab;
use origami_engine::{Player, Preview, footprint, halt, near, peaks, thumb, warm};
use origami_lang::{Clip, Kind, Timeline};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env,
    ffi::c_void,
    fs, mem,
    path::{Path, PathBuf},
    process::Command,
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
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
    scans: RefCell<(u64, Rc<view::Scans>)>,
    linted: RefCell<Vec<(String, (usize, usize), Vec<lint::Issue>)>>,
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
    picking: Option<Point<Pixels>>,
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
    web: bool,
    roam: (f32, f32),
    open: Vec<u32>,
    tie: Option<(u32, &'static str)>,
    cord: Option<motion::Cord>,
    lift: f32,
    climb: (f32, f32),
    key: Option<(u32, &'static str, usize)>,
    slip: Option<motion::Slip>,
    stage: Bounds<Pixels>,
    dense: f32,
    finding: Option<String>,
    away: bool,
    hit: usize,
    modes: [bool; 3],
    peek: Option<peek::Peek>,
    scout: Arc<Mutex<Preview>>,
    eyeing: bool,
    seeker: Option<regex::Regex>,
    fresh: bool,
    past: edit::History,
    future: edit::History,
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
    sift: (String, f32),
    pin: Point<Pixels>,
    copied: String,
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
    shown: Option<(i64, usize, (usize, usize))>,
    rough: bool,
    rest: Option<Instant>,
    busy: bool,
    reading: bool,
    boot: Option<(Instant, bool)>,
    clash: bool,
    sealed: bool,
    player: Option<(Instant, f64, Option<Player>)>,
    cued: bool,
    planning: bool,
    assets: HashMap<String, Option<Asset>>,
    aged: HashMap<String, Option<Asset>>,
    stamps: HashMap<String, Option<(u64, SystemTime)>>,
    jobs: Vec<(String, String, Option<f64>)>,
    loading: usize,
    export: Option<Arc<Mutex<(String, bool)>>>,
    parting: bool,
    ship: Option<ship::Ship>,
    begin: Option<begin::Begin>,
    shelf: Option<Arc<shelf::Shelf>>,
    saves: Option<saves::Saves>,
    shelving: bool,
    take: Option<voice::Take>,
    pen: bool,
    trace: Option<Vec<(f64, f64)>>,
    glow: Option<(Instant, usize, usize)>,
    ghost: Option<Vec<(f64, f64)>>,
    words: Option<words::Words>,
    mic: Option<String>,
    mics: Vec<String>,
    stir: Instant,
    latest: Option<release::Update>,
    focus: FocusHandle,
}

static SAVED: Mutex<(u64, String)> = Mutex::new((u64::MAX, String::new()));

type Early = (PathBuf, Option<SystemTime>, mpsc::Receiver<anyhow::Result<Timeline>>);
static EARLY: Mutex<Option<Early>> = Mutex::new(None);

fn clock(script: &Path) -> String {
    let mtime = fs::metadata(script).and_then(|m| m.modified()).ok();
    let secs = mtime.and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let saved = SAVED.lock().unwrap().clone();
    if saved.0 == secs / 60 {
        return saved.1;
    }
    let text = crash::reply(Command::new("/bin/date").args(["-r", &secs.to_string(), "+%H:%M"]));
    *SAVED.lock().unwrap() = (secs / 60, text.clone());
    text
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
        self.sel = self.sel.filter(|_| self.motion || self.web);
        match self.scrubbing {
            _ if self.player.is_none() => {}
            true => self.player = Some((Instant::now(), self.t, None)),
            false => self.play(),
        }
    }

    fn play(&mut self) {
        if self.t >= self.end() {
            self.t = 0.;
        }
        (self.player, self.cued) = (Some((Instant::now(), self.t, None)), true);
    }

    fn cue(&mut self, cx: &mut Context<Self>) {
        let Some(&(_, t, _)) = self.player.as_ref().filter(|_| self.cued && !self.planning && !self.scrubbing) else {
            return;
        };
        let (tl, job, rate) = (self.tl.clone(), self.tl.clone(), self.rate);
        self.planning = true;
        cx.spawn(async move |this, cx| {
            let plan = cx.background_executor().spawn(async move { Player::plan(&job, t, rate) }).await;
            this.update(cx, |this, cx| {
                this.planning = false;
                let same = this.player.as_ref().is_some_and(|p| p.1 == t) && Arc::ptr_eq(&this.tl, &tl) && this.rate == rate;
                if this.cued && !this.scrubbing && same {
                    let player = plan.and_then(Player::new).map_err(|e| this.note = e.to_string()).ok();
                    (this.player, this.cued) = (Some((Instant::now(), t, player)), false);
                    cx.notify();
                }
            })
        })
        .detach();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let read = origami_lang::load(&self.script);
        self.sealed = read.is_err() && fs::metadata(&self.script).is_ok();
        let gone = read.is_err() && !self.fresh;
        let disk: Vec<String> = read.as_deref().unwrap_or_default().lines().map(String::from).collect();
        let differs = disk != self.lines;
        if self.dirty || gone {
            (self.clash, self.dirty) = (differs, differs);
            if let (true, Ok(text), Some(shelf)) = (differs, read, &self.shelf) {
                shelf.put(text, Some("changed on disk".into()));
            }
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
        let early = EARLY.lock().unwrap().take_if(|e| e.0 == script && e.1 == self.mtime);
        match early.as_ref().and_then(|e| e.2.try_recv().ok()) {
            Some(tl) => self.took(tl, cx),
            None => self.reading = true,
        }
        let job = script.clone();
        let eval = self.reading.then(|| {
            cx.background_executor().spawn(async move {
                let got = early.and_then(|e| e.2.recv().ok());
                got.unwrap_or_else(|| origami_lang::eval(&job, true, footprint))
            })
        });
        cx.spawn(async move |this, cx| {
            if let Some(eval) = eval {
                let tl = eval.await;
                this.update(cx, |this, cx| {
                    if this.script == script {
                        this.took(tl, cx);
                    }
                })?;
            }
            let saved = saved.await;
            this.update(cx, |this, cx| {
                if this.saved != saved {
                    this.saved = saved;
                    cx.notify();
                }
            })
        })
        .detach();
    }

    fn took(&mut self, tl: anyhow::Result<Timeline>, cx: &mut Context<Self>) {
        self.reading = false;
        match tl {
            Ok(tl) => {
                (self.tl, self.rev) = (Arc::new(tl), self.rev + 1);
                self.boot = self.boot.filter(|_| self.tl.duration > 0.);
                self.menu = if self.menu == "clip" { "" } else { self.menu };
                self.err.clear();
                self.bad = 0;
                if let Some(gone) = origami_lang::missing() {
                    self.note = gone;
                }
                let sel = self.sel;
                self.seek(self.t);
                self.sel = sel;
                self.redraft(cx);
                self.refresh(cx);
            }
            Err(e) => {
                self.boot = None;
                let text = e.to_string();
                let text = text.lines().next().unwrap_or_default();
                self.bad = ["at :", "line "]
                    .iter()
                    .find_map(|k| {
                        let rest = &text[text.find(k)? + k.len()..];
                        rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len())].parse().ok()
                    })
                    .unwrap_or(0);
                self.err = text.split(" (").next().unwrap_or_default().trim_start_matches("Error: ").into();
                self.unfold((self.bad as usize).saturating_sub(1));
            }
        }
        self.watch(cx);
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) -> bool {
        let (wide, still) = (
            f32::from(self.sheet().size.width) * self.dense,
            self.player.is_none() && !self.scrubbing,
        );
        let (w, h) = Preview::size(&self.tl, wide as f64, still);
        let want = ((self.t * self.fps()).round() as i64, self.rev, (w, h));
        if self.scrubbing && !self.busy && self.shown == Some(want) && self.rest.is_some_and(|r| r.elapsed() > Duration::from_millis(150)) {
            self.rest = None;
            let (preview, tl, t, size) = (
                self.preview.clone(),
                self.tl.clone(),
                want.0 as f64 / self.fps(),
                Preview::size(&self.tl, wide as f64, true),
            );
            cx.background_executor()
                .spawn(async move { preview.lock().unwrap().prime(&tl, t, size) })
                .detach();
        }
        if self.busy || self.shown == Some(want) && !(self.rough && still) || self.tl.duration <= 0. || wide <= 0. {
            return false;
        }
        self.busy = true;
        let (scrub, hop) = (self.tl.proxy && self.scrubbing, self.tl.proxy && still && self.shown != Some(want));
        let (preview, tl, t) = (self.preview.clone(), self.tl.clone(), want.0 as f64 / self.fps());
        let draw = cx.background_executor().spawn(async move {
            let mut preview = preview.lock().unwrap();
            preview.fast = scrub || hop && !preview.ready(&tl, t, (w, h));
            (preview.frame(&tl, t, (w, h)), preview.rough)
        });
        cx.spawn(async move |this, cx| {
            let (px, rough) = draw.await;
            this.update(cx, |this, cx| {
                if let Some(old) = this.image.replace(bitmap(px, w, h)) {
                    cx.drop_image(old, None);
                }
                this.boot = this.boot.filter(|_| want.1 != this.rev);
                if this.shown.is_none_or(|s| s.1 != want.1) {
                    let tl = this.tl.clone();
                    cx.background_executor().spawn(async move { warm(&tl, 0.) }).detach();
                }
                (this.busy, this.shown, this.rough, this.rest) = (false, Some(want), rough, Some(Instant::now()));
                this.refresh(cx);
                cx.notify();
            })
        })
        .detach();
        true
    }

    fn asset(&mut self, key: String, path: &str, t: Option<f64>) -> Option<&Asset> {
        if !self.assets.contains_key(&key) {
            let kept = self.aged.remove(&key);
            if kept.is_none() {
                self.jobs.push((key.clone(), path.into(), t));
            }
            self.assets.insert(key.clone(), kept.flatten());
        }
        self.assets[&key].as_ref()
    }

    fn watch(&mut self, cx: &mut Context<Self>) {
        let (tl, mut seen) = (self.tl.clone(), HashSet::new());
        let media = tl
            .clips
            .iter()
            .filter(|c| matches!(c.kind, Kind::Video | Kind::Sound | Kind::Image))
            .map(|c| &c.src);
        let looks = tl
            .clips
            .iter()
            .flat_map(|c| &c.grade)
            .flat_map(|g| g.input_lut.iter().chain(g.look.iter().map(|l| &l.0)));
        for src in media.chain(looks).filter(|s| seen.insert(*s)) {
            let now = fs::metadata(src).and_then(|m| Ok((m.len(), m.modified()?))).ok();
            if self.stamps.insert(src.clone(), now).is_some_and(|was| was != now) {
                for assets in [&mut self.assets, &mut self.aged] {
                    assets.retain(|k, _| !k.starts_with(src));
                }
                self.rev += 1;
                self.mtime = self.mtime.filter(|_| self.dirty);
            }
        }
        let mtime = fs::metadata(&self.script).and_then(|m| m.modified()).ok();
        if mtime != self.mtime && !self.reading {
            self.mtime = mtime;
            self.load(cx);
        }
    }

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let arrived = menu::ARRIVED.lock().unwrap().take();
        if let Some(path) = arrived {
            self.switch(path, window, cx);
        }
        self.watch(cx);
        self.stow(cx);
        crash::mind(
            &mut crash::UNSAVED.lock().unwrap(),
            self.dirty.then(|| (self.script.clone(), self.text())),
        );
        if self.dwell() || self.lit().is_some() {
            cx.notify();
        }
        self.gaze(cx);
        if self.push.is_some() {
            self.again = true;
            cx.notify();
        }
        let wait = self.boot.is_some_and(|b| !b.1 && b.0.elapsed() > Duration::from_millis(100));
        self.boot = self.boot.map(|b| (b.0, b.1 || wait));
        if wait || self.take.is_some() || self.full && (2.0..2.1).contains(&self.stir.elapsed().as_secs_f32()) {
            cx.notify();
        }
        let was = (self.t, self.player.is_some());
        if let Some((since, t0, player)) = self.player.as_ref().filter(|_| !self.scrubbing && !self.cued) {
            let t = player.as_ref().map_or(t0 + since.elapsed().as_secs_f64() * self.rate, Player::time);
            let mark = |m| self.marks.get(&m).copied();
            let (a, b) = mark('a').zip(mark('b')).filter(|_| self.looping).unwrap_or((0., self.tl.duration));
            if t < b && !player.as_ref().is_some_and(Player::done) {
                self.t = (t * self.fps()).round() / self.fps();
                self.sync(t);
            } else if self.looping {
                self.t = a;
                self.play();
            } else {
                self.player = None;
                self.t = self.end();
            }
        }
        self.cue(cx);
        if self.assets.len() > 500 {
            for (key, asset) in mem::replace(&mut self.aged, mem::take(&mut self.assets)) {
                match asset {
                    Some(Asset::Thumb(image)) => cx.drop_image(image, Some(window)),
                    Some(wave) => drop(self.assets.insert(key, Some(wave))),
                    None => {}
                }
            }
        }
        if let Some(job) = self.export.clone() {
            let (note, done) = mem::take(&mut *job.lock().unwrap());
            if !note.is_empty() {
                self.note = note;
                cx.notify();
            }
            self.export = self.export.take().filter(|_| !done);
        }
        if !self.refresh(cx) && was != (self.t, self.player.is_some()) {
            cx.notify();
        }
        while self.boot.is_none()
            && self.loading < 4
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

fn fresh(focus: FocusHandle, recent: Vec<PathBuf>) -> Origami {
    Origami {
        script: PathBuf::new(),
        lines: vec![],
        mtime: None,
        tl: Arc::default(),
        rev: 0,
        scans: RefCell::default(),
        linted: RefCell::default(),
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
        picking: None,
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
        web: false,
        roam: (0., 0.),
        open: vec![],
        tie: None,
        cord: None,
        lift: 0.,
        climb: (0., 0.),
        key: None,
        slip: None,
        stage: Bounds::default(),
        dense: 1.,
        finding: None,
        away: false,
        hit: 0,
        modes: [false; 3],
        peek: None,
        scout: Arc::default(),
        eyeing: false,
        seeker: None,
        fresh: false,
        past: Default::default(),
        future: Default::default(),
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
        sift: (String::new(), 0.),
        pin: Point::default(),
        copied: String::new(),
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
        rough: false,
        rest: None,
        busy: false,
        reading: false,
        boot: None,
        clash: false,
        sealed: false,
        player: None,
        cued: false,
        planning: false,
        assets: HashMap::new(),
        aged: HashMap::new(),
        stamps: HashMap::new(),
        jobs: vec![],
        loading: 0,
        export: None,
        parting: false,
        ship: None,
        begin: None,
        shelf: None,
        saves: None,
        shelving: false,
        take: None,
        pen: false,
        trace: None,
        glow: None,
        ghost: None,
        words: None,
        mic: None,
        mics: vec![],
        stir: Instant::now(),
        latest: None,
        focus,
    }
}

unsafe extern "C" {
    fn MTLCreateSystemDefaultDevice() -> *mut c_void;
}

fn main() {
    crash::guard();
    origami_lang::serve(footprint);
    thread::spawn(|| unsafe {
        MTLCreateSystemDefaultDevice();
    });
    let Ok(script) = env::args_os().nth(1).map(fs::canonicalize).transpose() else {
        return eprintln!("usage: origami-app [script.js]");
    };
    if let Some((path, Err(e))) = script.as_ref().map(|s| (s, origami_lang::load(s))) {
        return eprintln!("{} was not opened: {e}", path.display());
    }
    let script = script.unwrap_or_default();
    if !script.as_os_str().is_empty() {
        let ((tell, heard), job) = (mpsc::channel(), script.clone());
        *EARLY.lock().unwrap() = Some((script.clone(), fs::metadata(&script).and_then(|m| m.modified()).ok(), heard));
        thread::spawn(move || {
            let tl = origami_lang::eval(&job, true, footprint);
            let first = tl.as_ref().ok().cloned();
            drop(tell.send(tl));
            first.inspect(|tl| near(tl, 0.));
        });
    }
    thread::spawn(Player::warm);
    let app = Application::new();
    app.on_open_urls(|urls| {
        if let Some(path) = urls.iter().filter_map(|u| menu::local(u)).next_back() {
            *menu::ARRIVED.lock().unwrap() = Some(path);
        }
    });
    app.run(move |cx: &mut App| {
        let _ = cx.text_system().add_fonts(view::faces());
        let recent: Vec<PathBuf> = origami_lang::load(&mouse::recent())
            .unwrap_or_default()
            .lines()
            .map(PathBuf::from)
            .collect();
        menu::install(cx);
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
        cx.on_app_quit(|_| {
            halt();
            async {}
        })
        .detach();
        cx.open_window(options, |window, cx| {
            cx.new(|cx: &mut Context<Origami>| {
                let focus = cx.focus_handle();
                window.focus(&focus);
                cx.on_app_quit(Origami::keep).detach();
                let weak = cx.weak_entity();
                window.on_window_should_close(cx, move |window, cx| {
                    let ask = |this: &mut Origami, cx: &mut Context<Origami>| {
                        let busy = this.dirty || this.export.is_some();
                        if busy {
                            this.leave(window, cx);
                        }
                        !busy
                    };
                    weak.update(cx, ask).unwrap_or(true)
                });
                cx.spawn_in(window, async move |this, cx| {
                    while this.update_in(cx, |this, window, cx| this.tick(window, cx)).is_ok() {
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                })
                .detach();
                cx.spawn_in(window, async move |this, cx| {
                    let latest = cx.background_executor().spawn(async {
                        pulse::launch();
                        release::latest()
                    });
                    let latest = latest.await;
                    this.update_in(cx, |this, window, cx| this.offer(latest, false, window, cx))
                })
                .detach();
                let mut app = fresh(focus, recent);
                match script.as_os_str().is_empty() {
                    true => app.hoist(cx),
                    false => app.open(script, cx),
                }
                app
            })
        })
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(test)]
pub mod tests {
    use crate::Origami;
    use gpui::{Entity, TestAppContext, VisualTestContext};
    use origami_lang::{ffmpeg, status};
    use std::{
        env, fs,
        path::{Path, PathBuf},
        sync::Arc,
    };

    pub fn scratch(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("origami-{name}-test"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn film(out: &Path, src: &str, more: &[&str]) {
        let made = status(ffmpeg().args(["-v", "error", "-y", "-f", "lavfi", "-i", src]).args(more).arg(out));
        assert!(made.unwrap().success());
    }

    pub(crate) fn window<'a>(cx: &'a mut TestAppContext, name: &str, src: &str) -> (Entity<Origami>, &'a mut VisualTestContext) {
        let path = env::temp_dir().join(format!("origami-{name}-test.js"));
        fs::write(&path, src).unwrap();
        let tl = origami_lang::eval(&path, false, origami_engine::footprint).unwrap();
        cx.add_window_view(|_, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            (app.script, app.lines, app.tl) = (path, src.lines().map(String::from).collect(), Arc::new(tl));
            app
        })
    }
}
