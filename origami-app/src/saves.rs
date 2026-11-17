use crate::{
    Mode, Origami, crash,
    edit::same,
    knob::close,
    menu::place,
    view::{BAR, BG, BLUE, BORDER, COMMENT, DIM, GREEN, GUTTER, INK, MUTED, PANEL, RED, TEXT, tokens},
};
use gpui::{Context, Div, HighlightStyle, MouseButton, ScrollWheelEvent, StyledText, deferred, div, prelude::*, px, rgb, rgba};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    mem,
    ops::Range,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const MERGE: u64 = 60;
const CAP: u64 = 50 << 20;
const SLACK: u64 = 1 << 16;
const DAY: i64 = 86400;
const SHOWN: usize = 12;
const FAINT: u32 = 0x6f7685;
const WEEK: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

static ZONE: Mutex<(u64, i64)> = Mutex::new((u64::MAX, 0));

type Record = (u64, usize, usize, Vec<String>);
type Line = (u64, i64, usize, usize, String, String, u64, u64, u64);

#[derive(Clone)]
struct Entry {
    t: u64,
    z: i64,
    adds: usize,
    dels: usize,
    clip: String,
    what: String,
    base: u64,
    at: u64,
    end: u64,
}

struct Last {
    t: u64,
    rows: (usize, usize),
    cut: (u64, u64),
    before: Option<(String, u64, u64)>,
}

#[derive(Default)]
struct Log {
    newest: Option<(String, u64, u64)>,
    last: Option<Last>,
}

pub struct Shelf {
    dir: PathBuf,
    name: String,
    queue: Mutex<Vec<(String, u64, Option<String>)>>,
    log: Mutex<Option<Log>>,
}

enum Row {
    Fold(usize),
    Same(usize),
    Gone(usize),
    Came(usize),
}

struct Diff {
    at: usize,
    lines: Vec<String>,
    now: Vec<String>,
    rows: Vec<Row>,
    hunks: Vec<usize>,
    hunk: usize,
    top: f32,
}

pub struct Saves {
    list: Vec<Entry>,
    pick: Option<usize>,
    top: f32,
    zone: i64,
    diff: Option<Diff>,
    mark: Option<String>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn zone(t: u64) -> i64 {
    let mut kept = ZONE.lock().unwrap();
    if kept.0 != t / 3600 {
        let out = origami_lang::output(Command::new("/bin/date").args(["-r", &t.to_string(), "+%z"]));
        let s = out
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let n: i64 = s.get(1..5).and_then(|n| n.parse().ok()).unwrap_or(0);
        *kept = (t / 3600, (n / 100 * 3600 + n % 100 * 60) * if s.starts_with('-') { -1 } else { 1 });
    }
    kept.1
}

fn len(path: &Path) -> u64 {
    fs::symlink_metadata(path).map_or(0, |m| m.len())
}

fn make(path: &Path) -> io::Result<File> {
    if fs::symlink_metadata(path).is_ok_and(|m| !m.is_file()) {
        fs::remove_file(path)?;
    }
    OpenOptions::new().create(true).append(true).mode(0o600).open(path)
}

fn slice(path: &Path, from: u64, to: u64) -> Option<Vec<u8>> {
    let size = fs::symlink_metadata(path).ok().filter(|m| m.is_file())?.len();
    if from > to || to > size || to - from > 1 << 26 {
        return None;
    }
    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = vec![0; (to - from) as usize];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn room(dir: &Path) -> io::Result<()> {
    for d in [dir.parent().unwrap_or(dir), dir] {
        if fs::symlink_metadata(d).is_ok_and(|m| !m.is_dir()) {
            fs::remove_file(d)?;
        }
        DirBuilder::new().recursive(true).mode(0o700).create(d)?;
    }
    Ok(())
}

fn line(e: &Entry) -> String {
    let row = (e.t, e.z, e.adds, e.dels, &e.clip, &e.what, e.base, e.at, e.end);
    format!("\n{}", serde_json::to_string(&row).unwrap_or_default())
}

fn entry(line: &[u8]) -> Option<Entry> {
    let (t, z, adds, dels, clip, what, base, at, end): Line = serde_json::from_slice(line).ok()?;
    Some(Entry {
        t,
        z,
        adds,
        dels,
        clip,
        what,
        base,
        at,
        end,
    })
}

fn thin(list: &[Entry], now: u64) -> Vec<bool> {
    let mut newer = (i64::MAX, i64::MAX);
    let mut keep: Vec<bool> = list
        .iter()
        .rev()
        .map(|e| {
            let local = e.t as i64 + e.z;
            let (hour, day) = (local.div_euclid(3600), local.div_euclid(DAY));
            let keep = match now.saturating_sub(e.t) as i64 / DAY {
                0..7 => true,
                7..30 => hour != newer.0,
                30..365 => day != newer.1,
                _ => false,
            };
            newer = (hour, day);
            keep
        })
        .collect();
    keep.reverse();
    keep
}

fn word(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn label(line: &str) -> Option<&str> {
    let mut words = line.split('=').next()?.split_whitespace();
    match (words.next(), words.next()) {
        (Some("const" | "let" | "var"), Some(name)) => Some(name).filter(|n| word(n)),
        _ => line.trim_start().split_once('.').map(|s| s.0).filter(|n| word(n)),
    }
}

fn tweak(o: &str, n: &str) -> Option<(String, String)> {
    let same = o.bytes().zip(n.bytes()).take_while(|(a, b)| a == b).count();
    let open = o.as_bytes()[..same].iter().rposition(|c| *c == b'(')?;
    let call = &o[o[..open].rfind('.')? + 1..open];
    let (a, b) = (close(o.as_bytes(), open)?, close(n.as_bytes(), open)?);
    let (was, now) = (&o[open + 1..a], &n[open + 1..b]);
    let num = |s: &str| s.split(',').all(|x| x.trim().parse::<f64>().is_ok());
    (word(call) && o[a..] == n[b..] && num(was) && num(now)).then_some(())?;
    Some((label(o)?.into(), format!("{call} {} \u{2192} {}", was.trim(), now.trim())))
}

fn sum(dels: &[&str], adds: &[&str]) -> (String, String) {
    if let ([o], [n]) = (dels, adds)
        && let Some(s) = tweak(o, n)
    {
        return s;
    }
    let first = adds.first().or(dels.first()).map_or("", |l| l.trim());
    (String::new(), first.chars().take(160).collect())
}

fn diff<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(Option<usize>, Option<usize>)> {
    let (head, tail) = same(a, b);
    let (x, y) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let (n, m) = (x.len() as isize, y.len() as isize);
    let at = |k: isize| (k + n + m) as usize;
    let (mut v, mut trace, mut kept) = (vec![0isize; 2 * (n + m) as usize + 2], vec![], 0);
    'walk: for d in 0..=n + m {
        let (lo, hi) = ((n + m - d - 1).max(0) as usize, at(d + 1));
        kept += hi - lo + 1;
        if kept > 1 << 22 {
            trace.clear();
            break;
        }
        trace.push((lo, v[lo..=hi].to_vec()));
        for k in (-d..=d).step_by(2) {
            let mut i = if k == -d || k != d && v[at(k - 1)] < v[at(k + 1)] {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut j = i - k;
            while i < n && j < m && x[i as usize] == y[j as usize] {
                (i, j) = (i + 1, j + 1);
            }
            v[at(k)] = i;
            if i >= n && j >= m {
                break 'walk;
            }
        }
    }
    let mut mid = vec![];
    let (mut i, mut j) = (n, m);
    for (d, (lo, v)) in trace.iter().enumerate().rev() {
        let get = |k: isize| v[at(k) - lo];
        let (d, k) = (d as isize, i - j);
        let p = if k == -d || k != d && get(k - 1) < get(k + 1) {
            k + 1
        } else {
            k - 1
        };
        let (pi, pj) = (get(p), get(p) - p);
        while i > pi && j > pj {
            (i, j) = (i - 1, j - 1);
            mid.push((Some(i), Some(j)));
        }
        if d > 0 && i == pi {
            j -= 1;
            mid.push((None, Some(j)));
        } else if d > 0 {
            i -= 1;
            mid.push((Some(i), None));
        }
    }
    if trace.is_empty() {
        mid = (0..m)
            .rev()
            .map(|j| (None, Some(j)))
            .chain((0..n).rev().map(|i| (Some(i), None)))
            .collect();
    }
    let shift = |o: Option<isize>| o.map(|o| o as usize + head);
    (0..head)
        .map(|i| (Some(i), Some(i)))
        .chain(mid.into_iter().rev().map(|(i, j)| (shift(i), shift(j))))
        .chain((0..tail).map(|t| (Some(a.len() - tail + t), Some(b.len() - tail + t))))
        .collect()
}

fn changed(row: &Row) -> bool {
    matches!(row, Row::Gone(_) | Row::Came(_))
}

fn layout(old: &[String], now: &[String]) -> (Vec<Row>, Vec<usize>) {
    fn fold(rows: &mut Vec<Row>, run: &mut Vec<usize>, lead: bool, trail: bool) {
        let (a, b) = (3 * lead as usize, 3 * trail as usize);
        let same = |j: &usize| Row::Same(*j);
        match run.len() > a + b + 1 {
            true => {
                rows.extend(run[..a].iter().map(same));
                rows.push(Row::Fold(run.len() - a - b));
                rows.extend(run[run.len() - b..].iter().map(same));
            }
            false => rows.extend(run.iter().map(same)),
        }
        run.clear();
    }
    let (mut rows, mut run, mut seen) = (vec![], vec![], false);
    for op in diff(old, now) {
        let row = match op {
            (Some(_), Some(j)) => {
                run.push(j);
                continue;
            }
            (Some(i), None) => Row::Gone(i),
            (None, Some(j)) => Row::Came(j),
            (None, None) => continue,
        };
        fold(&mut rows, &mut run, seen, true);
        seen = true;
        rows.push(row);
    }
    fold(&mut rows, &mut run, seen, false);
    let hunks = (0..rows.len())
        .filter(|&n| changed(&rows[n]) && (n == 0 || !changed(&rows[n - 1])))
        .collect();
    (rows, hunks)
}

fn hhmm(t: u64, z: i64) -> String {
    let s = (t as i64 + z).rem_euclid(DAY);
    format!("{:02}:{:02}", s / 3600, s / 60 % 60)
}

fn date(day: i64) -> String {
    let doe = (day + 719468).rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let weekday = WEEK[(day + 4).rem_euclid(7) as usize];
    format!("{weekday} {} {}", doy - (153 * mp + 2) / 5 + 1, MONTHS[((mp + 2) % 12) as usize])
}

fn heading(day: i64, today: i64) -> String {
    match today - day {
        0 => "Today".into(),
        1 => format!("Yesterday \u{b7} {}", date(day)),
        _ => date(day),
    }
}

impl Entry {
    fn day(&self) -> i64 {
        (self.t as i64 + self.z).div_euclid(DAY)
    }

    fn ago(&self, now: u64) -> String {
        match now.saturating_sub(self.t) {
            0..60 => "just now".into(),
            a @ 60..3600 => format!("{} min ago", a / 60),
            _ => hhmm(self.t, self.z),
        }
    }

    fn moment(&self, today: i64) -> String {
        let (time, day) = (hhmm(self.t, self.z), self.day());
        match today - day {
            0 => time,
            1..7 => format!("{} {time}", WEEK[(day + 4).rem_euclid(7) as usize]),
            _ => format!("{} {time}", date(day)),
        }
    }

    fn since(&self, today: i64) -> String {
        let day = match today - self.day() {
            0 => "today".into(),
            1 => "yesterday".into(),
            _ => date(self.day()),
        };
        format!("{} {day}", hhmm(self.t, self.z))
    }
}

impl Shelf {
    pub fn new(script: &Path) -> Self {
        Shelf {
            dir: crash::kept(script, "history"),
            name: place(script).0,
            queue: Mutex::default(),
            log: Mutex::default(),
        }
    }

    pub fn holds(&self, script: &Path) -> bool {
        self.dir == crash::kept(script, "history")
    }

    pub fn put(&self, text: String, mark: Option<String>) {
        self.queue.lock().unwrap().push((text, now(), mark));
    }

    pub fn drain(&self) {
        let mut log = self.log.lock().unwrap();
        loop {
            let batch = mem::take(&mut *self.queue.lock().unwrap());
            if batch.is_empty() {
                return;
            }
            for (text, t, mark) in batch {
                let log = log.get_or_insert_with(|| self.load(t));
                if self.add(log, text, t, mark).is_ok_and(|end| end > CAP) {
                    *log = self.load(t);
                }
            }
        }
    }

    fn list(&self) -> Vec<Entry> {
        let index = self.dir.join("index");
        let all = slice(&index, 0, len(&index)).unwrap_or_default();
        all.split(|b| *b == b'\n').filter_map(entry).collect()
    }

    fn read(&self, e: &Entry) -> Option<Vec<String>> {
        let (buf, mut pos, mut recs) = (slice(&self.dir.join("log"), e.base, e.end)?, e.base, vec![]);
        for piece in buf.split(|b| *b == b'\n') {
            if let Ok(r) = serde_json::from_slice::<Record>(piece) {
                recs.push((pos, r));
            }
            pos += piece.len() as u64 + 1;
        }
        let (mut chain, mut at) = (vec![], e.at);
        loop {
            let (_, r) = &recs[recs.binary_search_by_key(&at, |r| r.0).ok()?];
            chain.push(r);
            if r.0 == 0 {
                break;
            }
            at = at.checked_sub(r.0)?;
        }
        let mut lines: Vec<String> = vec![];
        for (_, head, tail, mid) in chain.into_iter().rev() {
            let to = lines.len().checked_sub(*tail).filter(|to| to >= head)?;
            lines.splice(*head..to, mid.iter().cloned()).for_each(drop);
        }
        Some(lines)
    }

    fn load(&self, now: u64) -> Log {
        let _ = room(&self.dir);
        let (all, size) = (self.list(), len(&self.dir.join("log")));
        let cut = match size > CAP {
            true => all
                .iter()
                .map(|e| e.base)
                .filter(|b| size.saturating_sub(*b) <= CAP)
                .min()
                .unwrap_or(size),
            false => 0,
        };
        let list: Vec<Entry> = all
            .iter()
            .zip(thin(&all, now))
            .filter(|(e, keep)| *keep && e.base >= cut)
            .map(|(e, _)| Entry {
                base: e.base - cut,
                at: e.at.saturating_sub(cut),
                end: e.end.saturating_sub(cut),
                ..e.clone()
            })
            .collect();
        if cut > 0 || list.len() < all.len() {
            let _ = self.rewrite(&list, cut);
        }
        let newest = list.last().and_then(|e| {
            let text = self.read(e)?.iter().flat_map(|l| [l.as_str(), "\n"]).collect();
            Some((text, e.base, e.at))
        });
        Log { newest, last: None }
    }

    fn rewrite(&self, list: &[Entry], cut: u64) -> io::Result<()> {
        let (log, index) = (self.dir.join("log"), self.dir.join("index"));
        if cut > 0 {
            let fresh = self.dir.join("log.new");
            let _ = fs::remove_file(&fresh);
            make(&fresh)?.write_all(&slice(&log, cut, len(&log)).ok_or(io::ErrorKind::InvalidData)?)?;
            fs::rename(&fresh, &log)?;
        }
        let fresh = self.dir.join("index.new");
        let _ = fs::remove_file(&fresh);
        make(&fresh)?.write_all(list.iter().map(line).collect::<String>().as_bytes())?;
        fs::rename(&fresh, &index)
    }

    fn add(&self, log: &mut Log, text: String, t: u64, mark: Option<String>) -> io::Result<u64> {
        let (path, index) = (self.dir.join("log"), self.dir.join("index"));
        if log.newest.as_ref().is_some_and(|n| n.0 == text) {
            return Ok(len(&path));
        }
        let new: Vec<&str> = text.lines().collect();
        if let Some(last) = log.last.take() {
            let old: Vec<&str> = log.newest.as_ref().map_or(vec![], |n| n.0.lines().collect());
            let (head, tail) = same(&old, &new);
            if mark.is_none() && t < last.t + MERGE && head >= last.rows.0 && old.len() - tail <= last.rows.1 {
                make(&path)?.set_len(last.cut.0)?;
                make(&index)?.set_len(last.cut.1)?;
                log.newest = last.before;
            }
        }
        if log.newest.as_ref().is_some_and(|n| n.0 == text) {
            return Ok(len(&path));
        }
        let old: Vec<&str> = log.newest.as_ref().map_or(vec![], |n| n.0.lines().collect());
        let (head, tail) = same(&old, &new);
        let (gone, came) = (&old[head..old.len() - tail], &new[head..new.len() - tail]);
        let ops = diff(gone, came);
        let dels: Vec<&str> = ops.iter().filter_map(|op| Some(gone[op.0?]).filter(|_| op.1.is_none())).collect();
        let adds: Vec<&str> = ops.iter().filter_map(|op| Some(came[op.1?]).filter(|_| op.0.is_none())).collect();
        let (clip, what) = match (&mark, &log.newest) {
            (Some(m), _) => (String::new(), m.clone()),
            (_, None) if len(&index) == 0 => (String::new(), format!("first save of {}", self.name)),
            _ => sum(&dels, &adds),
        };
        let mut file = make(&path)?;
        let at = file.metadata()?.len() + 1;
        let limit = 2 * text.len() as u64 + SLACK;
        let (base, record) = match &log.newest {
            Some((_, base, was)) if base <= was && *was < at && at - base <= limit => (*base, (at - was, head, tail, came)),
            _ => (at, (0, 0, 0, &new[..])),
        };
        let body = format!("\n{}", serde_json::to_string(&record)?);
        file.write_all(body.as_bytes())?;
        let end = at - 1 + body.len() as u64;
        let (adds, dels, rows) = (adds.len(), dels.len(), (head, new.len() - tail));
        let row = line(&Entry {
            t,
            z: zone(t),
            adds,
            dels,
            clip,
            what,
            base,
            at,
            end,
        });
        let mut list = make(&index)?;
        let top = list.metadata()?.len();
        list.write_all(row.as_bytes())?;
        log.last = mark.is_none().then(|| Last {
            t,
            rows,
            cut: (at - 1, top),
            before: log.newest.take(),
        });
        log.newest = Some((text, base, at));
        Ok(end)
    }
}

impl Saves {
    fn shown(&self, now: u64) -> Vec<(Option<String>, usize)> {
        let (today, mut day) = ((now as i64 + self.zone).div_euclid(DAY), None);
        (self.top as usize..self.list.len())
            .take(SHOWN)
            .map(|i| {
                let d = self.list[i].day();
                let head = (day != Some(d)).then(|| heading(d, today));
                day = Some(d);
                (head, i)
            })
            .collect()
    }

    fn today(&self) -> i64 {
        (now() as i64 + self.zone).div_euclid(DAY)
    }
}

impl Diff {
    fn new(at: usize, lines: Vec<String>, now: Vec<String>) -> Self {
        let (rows, hunks) = layout(&lines, &now);
        Diff {
            at,
            lines,
            now,
            rows,
            hunks,
            hunk: 0,
            top: 0.,
        }
    }

    fn step(&mut self, on: bool) {
        let last = self.hunks.len().saturating_sub(1);
        self.hunk = if on {
            (self.hunk + 1).min(last)
        } else {
            self.hunk.saturating_sub(1)
        };
        self.top = self.hunks.get(self.hunk).map_or(0, |h| h.saturating_sub(4)) as f32;
    }
}

fn ink(range: Range<usize>, color: u32) -> (Range<usize>, HighlightStyle) {
    (range, rgb(color).into())
}

fn painted(text: String, marks: Vec<(Range<usize>, HighlightStyle)>) -> StyledText {
    StyledText::new(text).with_highlights(marks.into_iter().filter(|m| !m.0.is_empty()))
}

fn tally(adds: usize, dels: usize) -> StyledText {
    let (a, d) = (
        if adds > 0 { format!("+{adds}") } else { String::new() },
        if dels > 0 { format!("\u{2212}{dels}") } else { String::new() },
    );
    let text = format!("{a}{}{d}", if a.is_empty() || d.is_empty() { "" } else { " " });
    let marks = vec![ink(0..a.len(), GREEN), ink(text.len() - d.len()..text.len(), RED)];
    painted(text, marks)
}

impl Origami {
    pub fn shelve(&mut self, text: String) {
        let mark = self.saves.as_mut().and_then(|s| s.mark.take());
        if let Some(shelf) = &self.shelf {
            shelf.put(text, mark);
        }
    }

    pub fn stow(&mut self, cx: &mut Context<Self>) {
        let Some(shelf) = self.shelf.clone().filter(|s| !self.shelving && !s.queue.lock().unwrap().is_empty()) else {
            return;
        };
        self.shelving = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().spawn(async move { shelf.drain() }).await;
            this.update(cx, |this, cx| {
                this.shelving = false;
                this.reread();
                cx.notify();
            })
        })
        .detach();
    }

    fn reread(&mut self) {
        let (Some(shelf), Some(s)) = (&self.shelf, &mut self.saves) else {
            return;
        };
        let mut list = shelf.list();
        list.reverse();
        let (more, last) = (list.len().saturating_sub(s.list.len()), list.len().saturating_sub(1));
        s.pick = s.pick.map(|p| (p + more).min(last));
        if let Some(d) = &mut s.diff {
            d.at = (d.at + more).min(last);
        }
        s.list = list;
    }

    pub fn versions(&mut self, pick: Option<usize>) -> Option<Saves> {
        let mut list = self.shelf.as_ref()?.list();
        list.reverse();
        (self.cursor, self.anchor, self.hover) = (None, None, None);
        Some(Saves {
            pick: pick.filter(|_| list.len() > 1),
            list,
            top: 0.,
            zone: zone(now()),
            diff: None,
            mark: None,
        })
    }

    pub fn history(&mut self, pick: Option<usize>) {
        self.saves = match self.saves.take() {
            Some(_) => None,
            None => self.versions(pick),
        };
    }

    fn compare(&mut self, i: usize) {
        let (Some(shelf), Some(s)) = (&self.shelf, &mut self.saves) else {
            return;
        };
        let Some(lines) = s.list.get(i).and_then(|e| shelf.read(e)) else {
            return self.note = "this version could not be read".into();
        };
        (s.pick, s.diff) = (Some(i), Some(Diff::new(i, lines, self.lines.clone())));
        (self.cursor, self.anchor, self.hover, self.knobbed) = (None, None, None, None);
    }

    fn back(&mut self) {
        if let Some(s) = &mut self.saves {
            (s.diff, self.pending) = (None, None);
        }
    }

    fn hop(&mut self, on: bool) {
        if let Some(d) = self.saves.as_mut().and_then(|s| s.diff.as_mut()) {
            d.step(on);
        }
    }

    fn restore(&mut self) {
        let Some((lines, when)) = self.saves.as_ref().and_then(|s| {
            let d = s.diff.as_ref()?;
            Some((d.lines.clone(), s.list.get(d.at)?.moment(s.today())))
        }) else {
            return;
        };
        if self.clash {
            return self.note = "the file changed on disk: keep mine or use disk, above the code".into();
        }
        if lines == self.lines {
            return self.note = "this version is already the code on screen".into();
        }
        self.shelve(self.text());
        if let Some(s) = &mut self.saves {
            (s.mark, s.diff, s.pick) = (Some(format!("\u{21ba} restored {when}")), None, None);
        }
        self.change(|this| this.lines = lines);
        self.note = format!("restored {when} as a new save \u{b7} cmd+Z undoes it");
    }

    fn spin(&mut self, by: f32) {
        let row = self.row();
        let Some(s) = &mut self.saves else { return };
        match &mut s.diff {
            Some(d) => d.top = (d.top - by / row).clamp(0., d.rows.len().saturating_sub(1) as f32),
            None => s.top = (s.top - by / 24.).clamp(0., s.list.len().saturating_sub(SHOWN) as f32),
        }
    }

    pub fn leaf(&mut self, k: &str, plain: bool) -> bool {
        let typing = matches!(self.mode, Mode::Command(_) | Mode::Search(_));
        let Some(s) = self.saves.as_mut().filter(|_| plain && !typing) else {
            return false;
        };
        if let Some(d) = &mut s.diff {
            match (self.pending.take(), k) {
                (Some(p @ (']' | '[')), "c") => d.step(p == ']'),
                (Some(']' | '['), _) => {}
                (_, "]" | "[") => self.pending = k.chars().next(),
                (_, "r") => self.restore(),
                (_, "escape") => s.diff = None,
                _ => return false,
            }
            return true;
        }
        let (last, pick) = (s.list.len().saturating_sub(1), s.pick.unwrap_or(0));
        match k {
            "escape" => self.saves = None,
            _ if last == 0 => return false,
            "j" | "down" => s.pick = Some(s.pick.map_or(0, |p| (p + 1).min(last))),
            "k" | "up" => s.pick = Some(pick.saturating_sub(1)),
            "enter" => self.compare(pick),
            _ => return false,
        }
        if let Some(s) = &mut self.saves {
            let p = s.pick.unwrap_or(0) as f32;
            s.top = s.top.clamp(p - SHOWN as f32 + 1., p).max(0.);
        }
        true
    }

    pub fn comparing(&self) -> bool {
        self.saves.as_ref().is_some_and(|s| s.diff.is_some())
    }

    pub fn ledge(&self) -> f32 {
        match &self.saves {
            Some(s) if s.diff.is_some() => 0.,
            Some(s) if s.list.len() < 2 => 63.,
            Some(s) => s.shown(now()).iter().enumerate().fold(35., |h, (n, (head, _))| {
                h + 24. + head.as_ref().map_or(0., |_| if n == 0 { 22. } else { 30. })
            }),
            None => 0.,
        }
    }

    pub fn lens(&self) -> (String, &'static str) {
        match &self.saves {
            Some(Saves { diff: Some(d), list, .. }) => (
                format!("-- DIFF {} --", list.get(d.at).map_or(String::new(), |e| hhmm(e.t, e.z))),
                "]c [c next/prev change \u{b7} r restores \u{b7} esc back",
            ),
            Some(s) if s.pick.is_some() => ("-- HISTORY --".into(), "j k move \u{b7} enter shows changes \u{b7} esc closes"),
            None if matches!(&self.mode, Mode::Command(c) if c == "history") => (String::new(), "enter opens saved versions"),
            _ => (String::new(), ""),
        }
    }

    pub fn dial(&self, cx: &mut Context<Self>) -> Div {
        let open = self.saves.is_some();
        let hint = div()
            .absolute()
            .top(px(32.))
            .right(px(-5.))
            .h(px(24.))
            .px(px(8.))
            .items_center()
            .gap(px(10.))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(6.))
            .text_size(px(12.))
            .text_color(rgb(TEXT))
            .whitespace_nowrap()
            .hidden()
            .group_hover("dial", |s| s.flex())
            .child("history")
            .child(div().text_color(rgb(DIM)).child("\u{2318}Y"));
        div()
            .group("dial")
            .relative()
            .size(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .text_size(px(14.))
            .text_color(rgb(if open { TEXT } else { MUTED }))
            .map(|d| match open {
                true => d.bg(rgba(BLUE << 8 | 0x40)),
                false => d.hover(|s| s.bg(rgb(BAR)).text_color(rgb(TEXT))).child(deferred(hint)),
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.history(None);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child("\u{21ba}")
    }

    pub fn ledger(&self, rows: usize, cx: &mut Context<Self>) -> Option<Div> {
        let s = self.saves.as_ref()?;
        if let Some(d) = &s.diff {
            return Some(self.contrast(s, d, rows, cx));
        }
        let scroll = cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
            this.spin(f32::from(e.delta.pixel_delta(px(24.)).y));
            cx.notify();
        });
        let panel = div()
            .flex_none()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(BORDER))
            .text_size(px(12.))
            .line_height(px(16.))
            .on_scroll_wheel(scroll);
        if s.list.len() < 2 {
            return Some(
                panel
                    .gap(px(4.))
                    .pt(px(14.))
                    .pb(px(12.))
                    .px(px(12.))
                    .child(div().h(px(16.)).child("No earlier versions yet."))
                    .child(div().h(px(16.)).text_color(rgb(DIM)).child("Every save is kept here.")),
            );
        }
        let now = now();
        let lines = s.shown(now).into_iter().enumerate().flat_map(|(n, (head, i))| {
            let e = &s.list[i];
            let keyed = s.pick == Some(i);
            let what = match e.clip.is_empty() {
                true => painted(e.what.clone(), vec![]),
                false => painted(format!("{} \u{b7} {}", e.clip, e.what), vec![ink(0..e.clip.len(), TEXT)]),
            };
            let head = head.map(|h| {
                div()
                    .h(px(if n == 0 { 22. } else { 30. }))
                    .flex_none()
                    .flex()
                    .items_end()
                    .pb(px(3.))
                    .px(px(8.))
                    .text_size(px(11.))
                    .line_height(px(14.))
                    .text_color(rgb(DIM))
                    .child(h)
            });
            let row = div()
                .h(px(24.))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(8.))
                .rounded(px(3.))
                .cursor_pointer()
                .whitespace_nowrap()
                .text_color(rgb(if keyed { TEXT } else { MUTED }))
                .map(|d| match keyed {
                    true => d.bg(rgba(BLUE << 8 | 0x40)),
                    false => d.hover(|s| s.bg(rgb(BAR))),
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.compare(i);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(div().w(px(84.)).flex_none().text_color(rgb(TEXT)).child(e.ago(now)))
                .child(div().flex_1().min_w_0().truncate().child(what))
                .child(div().w(px(48.)).flex_none().flex().justify_end().child(tally(e.adds, e.dels)))
                .child(
                    div()
                        .w(px(52.))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .text_size(px(11.))
                        .text_color(rgb(BLUE))
                        .children((i == 0).then_some("current")),
                );
            head.into_iter().chain([row])
        });
        let foot = div()
            .h(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.))
            .border_t_1()
            .border_color(rgb(BORDER))
            .text_size(px(11.))
            .text_color(rgb(FAINT))
            .child(format!("{} versions \u{b7} kept on this Mac, not in the project", s.list.len()))
            .child("esc closes");
        Some(panel.p(px(4.)).children(lines.collect::<Vec<_>>()).child(foot))
    }

    fn contrast(&self, s: &Saves, d: &Diff, rows: usize, cx: &mut Context<Self>) -> Div {
        let today = s.today();
        let (since, when) = s.list.get(d.at).map(|e| (e.since(today), e.moment(today))).unwrap_or_default();
        let span = format!("{since} \u{2192} now");
        let small = |d: Div| d.h(px(20.)).rounded(px(3.));
        let arrow = |d: Div| small(d).w(px(22.)).px_0().justify_center();
        let bar = div()
            .h(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(6.))
            .pr(px(8.))
            .bg(rgb(PANEL))
            .border_b_1()
            .border_color(rgb(BORDER))
            .text_size(px(12.))
            .whitespace_nowrap()
            .child(small(self.tool("\u{2190} back", cx, |t, _, _| t.back())).px(px(6.)))
            .child(div().w(px(1.)).h(px(14.)).bg(rgb(BORDER)))
            .child(
                div()
                    .pl(px(4.))
                    .child(painted(span.clone(), vec![ink(since.len()..span.len(), DIM)])),
            )
            .child(div().flex_1())
            .child(div().text_size(px(11.)).text_color(rgb(DIM)).child(format!(
                "change {}/{}",
                (d.hunk + 1).min(d.hunks.len()),
                d.hunks.len()
            )))
            .child(arrow(self.tool("\u{2191}", cx, |t, _, _| t.hop(false))))
            .child(arrow(self.tool("\u{2193}", cx, |t, _, _| t.hop(true))));
        let lit = d
            .hunks
            .get(d.hunk)
            .map_or(0..0, |&h| h..h + d.rows[h..].iter().take_while(|r| changed(r)).count());
        let shown = d.rows.iter().enumerate().skip(d.top as usize).take(rows).map(|(n, row)| {
            let text = |lines: &[String], i: usize| lines.get(i).cloned().unwrap_or_default();
            let (gutter, code, edge) = match row {
                Row::Fold(n) => (format!("     \u{22ef} {n} unchanged lines"), String::new(), None),
                Row::Same(j) => (format!("{:>3}  ", j + 1), text(&d.now, *j), None),
                Row::Gone(i) => ("   \u{2212} ".into(), text(&d.lines, *i), Some(RED)),
                Row::Came(j) => (format!("{:>3}+ ", j + 1), text(&d.now, *j), Some(GREEN)),
            };
            let skip = gutter.len();
            let mut marks = match row {
                Row::Fold(_) => vec![ink(0..skip, COMMENT)],
                Row::Same(_) => vec![ink(0..skip, GUTTER)],
                Row::Gone(_) => vec![ink(0..skip, RED)],
                Row::Came(_) => vec![
                    ink(0..skip - 2, if lit.contains(&n) { TEXT } else { DIM }),
                    ink(skip - 2..skip, GREEN),
                ],
            };
            marks.extend(tokens(&code).into_iter().map(|(r, h)| (r.start + skip..r.end + skip, h)));
            div()
                .relative()
                .h(px(self.row()))
                .flex_none()
                .flex()
                .items_center()
                .pl(px(12.))
                .whitespace_nowrap()
                .when(matches!(row, Row::Same(_)), |d| d.opacity(0.45))
                .when_some(edge, |d, c| {
                    d.bg(rgba(c << 8 | 0x26))
                        .child(div().absolute().left_0().top_0().bottom_0().w(px(2.)).bg(rgb(c)))
                })
                .child(painted(gutter + &code, marks))
        });
        let body = div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .py(px(8.))
            .bg(rgb(BG))
            .text_size(px(self.font))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                this.spin(f32::from(e.delta.pixel_delta(px(22.)).y));
                cx.notify();
            }))
            .children(shown.collect::<Vec<_>>());
        let foot = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(16.))
            .py(px(10.))
            .pl(px(12.))
            .pr(px(8.))
            .bg(rgb(PANEL))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(div().flex_1().min_w_0().text_size(px(11.)).text_color(rgb(DIM)).child(format!(
                "Restoring saves {when} as a new version. Nothing is lost, and cmd+Z undoes it."
            )))
            .child(
                div()
                    .h(px(26.))
                    .px(px(10.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .rounded(px(4.))
                    .bg(rgb(BLUE))
                    .text_color(rgb(INK))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.restore();
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child("Restore this version"),
            );
        div().flex_1().min_h_0().flex().flex_col().child(bar).child(body).child(foot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point};
    use std::env;

    const NOW: u64 = 1794830400;

    fn shelf(name: &str) -> (Shelf, PathBuf) {
        let script = env::temp_dir().join(format!("origami-saves-{name}-test")).join("edit.js");
        let shelf = Shelf::new(&script);
        let _ = fs::remove_dir_all(&shelf.dir);
        (shelf, script)
    }

    fn save(shelf: &Shelf, lines: &[&str], t: u64, mark: Option<&str>) {
        let text = lines.iter().map(|l| format!("{l}\n")).collect();
        shelf.queue.lock().unwrap().push((text, t, mark.map(String::from)));
        shelf.drain();
    }

    fn texts(shelf: &Shelf) -> Vec<Option<Vec<String>>> {
        shelf.list().iter().map(|e| shelf.read(e)).collect()
    }

    fn owned(lines: &[&str]) -> Option<Vec<String>> {
        Some(lines.iter().map(|l| l.to_string()).collect())
    }

    // A save keeps a version and a save with nothing new keeps none; saves within a minute of the last
    // one that touch the same lines fold into it and show the net change, other lines or a later save
    // start a new version, folding back to the earlier text drops it, and every version reads back
    // exactly after a restart.
    #[test]
    fn keeps_each_save() {
        let (first, script) = shelf("keep");
        let t = NOW;
        save(&first, &["a", "b", "c"], t, None);
        save(&first, &["a", "b", "c"], t + 5, None);
        save(&first, &["a", "B", "c"], t + 100, None);
        save(&first, &["a", "BB", "c"], t + 110, None);
        save(&first, &["a", "BBB", "c"], t + 160, None);
        save(&first, &["x", "BBB", "c"], t + 170, None);
        save(&first, &["y", "BBB", "c"], t + 300, None);
        save(&first, &["x", "BBB", "c"], t + 310, None);
        let shown = |s: &Shelf| s.list().iter().map(|e| (e.t, e.adds, e.dels, e.what.clone())).collect::<Vec<_>>();
        let want = vec![
            (t, 3, 0, "first save of edit.js".to_string()),
            (t + 160, 1, 1, "BBB".into()),
            (t + 170, 1, 1, "x".into()),
        ];
        assert_eq!(shown(&first), want);
        let again = Shelf::new(&script);
        assert_eq!(shown(&again), want);
        assert_eq!(
            texts(&again),
            [owned(&["a", "b", "c"]), owned(&["a", "BBB", "c"]), owned(&["x", "BBB", "c"])]
        );
        save(&again, &["x", "BBB", "c"], t + 400, None);
        save(&again, &["x", "BBB"], t + 410, None);
        assert_eq!(shown(&again).len(), 4);
        assert_eq!(texts(&again)[3], owned(&["x", "BBB"]));
    }

    // Versions read back newest first and grouped by day after a restart; one numeric call changed on a
    // named clip reads as the clip and setting, any other change as its first changed line, the first
    // save and a restore by name.
    #[test]
    fn lists_by_day() {
        let (first, script) = shelf("list");
        let (dome, camp) = ("const dome = video(\"d.mp4\").temperature(-4)", "  camp.trim(12, 18).fade(0.5, 1),");
        save(&first, &[dome, camp], NOW - 4 * 86400, None);
        save(&first, &[dome, "  camp.trim(12, 20).fade(0.5, 1),"], NOW - 86400, None);
        let cold = "const dome = video(\"d.mp4\").temperature(-10)";
        save(&first, &[cold, "  camp.trim(12, 20).fade(0.5, 1),"], NOW - 7200, None);
        save(
            &first,
            &[cold, "  camp.trim(12, 20).fade(1, 1),", "  text(\"half dome\").at(4),"],
            NOW - 120,
            None,
        );
        save(&first, &[dome, camp], NOW - 30, Some("\u{21ba} restored Thu 12:00"));
        let mut list = Shelf::new(&script).list();
        list.iter_mut().for_each(|e| e.z = 0);
        list.reverse();
        let saves = Saves {
            list,
            pick: None,
            top: 0.,
            zone: 0,
            diff: None,
            mark: None,
        };
        let rows: Vec<_> = saves
            .shown(NOW)
            .into_iter()
            .map(|(head, i)| {
                let e = &saves.list[i];
                (head, e.ago(NOW), e.clip.clone(), e.what.clone(), e.adds, e.dels)
            })
            .collect();
        let row = |head: Option<&str>, ago: &str, clip: &str, what: &str, adds, dels| {
            (
                head.map(String::from),
                ago.to_string(),
                clip.to_string(),
                what.to_string(),
                adds,
                dels,
            )
        };
        assert_eq!(
            rows,
            [
                row(Some("Today"), "just now", "", "\u{21ba} restored Thu 12:00", 2, 3),
                row(None, "2 min ago", "", "camp.trim(12, 20).fade(1, 1),", 2, 1),
                row(None, "10:00", "dome", "temperature -4 \u{2192} -10", 1, 1),
                row(
                    Some("Yesterday \u{b7} Sun 15 Nov"),
                    "12:00",
                    "camp",
                    "trim 12, 18 \u{2192} 12, 20",
                    1,
                    1
                ),
                row(Some("Thu 12 Nov"), "12:00", "", "first save of edit.js", 2, 0),
            ]
        );
        assert_eq!(
            (saves.list[4].moment(20773), saves.list[3].since(20773)),
            ("Thu 12:00".into(), "12:00 yesterday".into())
        );
    }

    // Against the code on screen, lines only in the old version show as removed, lines added since show
    // with their line number, and long unchanged runs fold away leaving three lines around each change.
    #[test]
    fn marks_changes() {
        let now: Vec<String> = (1..=17).map(|n| format!("line {n}")).collect();
        let mut old = now.clone();
        (old[7], old[9]) = ("old 8".into(), "old 10".into());
        old.remove(14);
        let (rows, hunks) = layout(&old, &now);
        let shown: Vec<String> = rows
            .iter()
            .map(|r| match r {
                Row::Fold(n) => format!("{n} folded"),
                Row::Same(j) => format!("{}", j + 1),
                Row::Gone(i) => format!("-{}", old[*i]),
                Row::Came(j) => format!("+{}", j + 1),
            })
            .collect();
        let want = "4 folded,5,6,7,-old 8,+8,9,-old 10,+10,11,12,13,14,+15,16,17";
        assert_eq!(shown.join(","), want);
        assert_eq!(hunks, [4, 7, 13]);
        let (rows, hunks) = layout(&now, &now);
        assert!(matches!(rows[..], [Row::Fold(17)]) && hunks.is_empty());
    }

    // Random pairs of scripts diff into the fewest removed and added lines with every line of both kept in
    // order, and two scripts too far apart to compare line by line show as one replaced block.
    #[test]
    fn diffs_are_shortest() {
        fn roll(seed: &mut u64, n: u64) -> u64 {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (*seed >> 33) % n
        }
        let seed = &mut 7;
        let ordered = |ops: &[(Option<usize>, Option<usize>)], a: usize, b: usize| {
            let (olds, news): (Vec<_>, Vec<_>) = (ops.iter().filter_map(|o| o.0).collect(), ops.iter().filter_map(|o| o.1).collect());
            olds == (0..a).collect::<Vec<_>>() && news == (0..b).collect::<Vec<_>>()
        };
        for _ in 0..3000 {
            let a: Vec<u64> = (0..roll(seed, 12)).map(|_| roll(seed, 3)).collect();
            let b: Vec<u64> = (0..roll(seed, 12)).map(|_| roll(seed, 3)).collect();
            let ops = diff(&a, &b);
            let mut lcs = vec![vec![0; b.len() + 1]; a.len() + 1];
            for i in (0..a.len()).rev() {
                for j in (0..b.len()).rev() {
                    lcs[i][j] = if a[i] == b[j] {
                        lcs[i + 1][j + 1] + 1
                    } else {
                        lcs[i + 1][j].max(lcs[i][j + 1])
                    };
                }
            }
            let kept: Vec<_> = ops.iter().filter_map(|o| o.0.zip(o.1)).collect();
            assert!(ordered(&ops, a.len(), b.len()) && kept.iter().all(|(i, j)| a[*i] == b[*j]));
            assert_eq!(kept.len(), lcs[0][0]);
        }
        let ops = diff(&[0; 3000], &[1; 3000]);
        assert!(ordered(&ops, 3000, 3000) && ops.len() == 6000);
    }

    // Everything from the last week is kept, then the last version of each hour for a month, then the
    // last of each day for a year, and nothing older; a restart thins the saved list the same way.
    #[test]
    fn thins_with_age() {
        let (h, d) = (3600, 86400);
        let ages = [
            500 * d,
            40 * d + 1200,
            40 * d + 600,
            39 * d,
            10 * d + 1800,
            10 * d + 1200,
            10 * d - h,
            2 * d,
            2 * d - 60,
            60,
        ];
        let list: Vec<Entry> = ages
            .iter()
            .map(|a| Entry {
                t: NOW - a,
                z: 0,
                adds: 0,
                dels: 0,
                clip: String::new(),
                what: String::new(),
                base: 0,
                at: 0,
                end: 0,
            })
            .collect();
        assert_eq!(thin(&list, NOW), [false, false, true, true, false, true, true, true, true, true]);
        let (first, script) = shelf("thin");
        for (n, a) in ages.iter().enumerate() {
            save(&first, &[&n.to_string()], NOW - a, None);
        }
        let again = Shelf::new(&script);
        again.load(NOW);
        assert_eq!(again.list().len(), 7);
        assert_eq!(texts(&again)[0], owned(&["2"]));
    }

    // A torn write at the end of the history and garbage in the middle are skipped: the readable
    // versions still read back, a version whose stored change is damaged is refused, and saves after a
    // restart keep working.
    #[test]
    fn skips_damage() {
        let (first, script) = shelf("torn");
        for (n, lines) in [["a", "b"], ["A", "b"], ["A", "B"]].iter().enumerate() {
            save(&first, lines, NOW + 100 * n as u64, None);
        }
        let (log, index) = (first.dir.join("log"), first.dir.join("index"));
        let mut bytes = fs::read(&log).unwrap();
        let at = first.list()[1].at as usize;
        bytes[at + 3] = b'#';
        bytes.extend(b"\n[9, 0, 0, [\"torn");
        fs::write(&log, bytes).unwrap();
        let rows = fs::read_to_string(&index).unwrap().replacen('\n', "\n{oops}\n", 2) + "\n[1, 0";
        fs::write(&index, rows).unwrap();
        let again = Shelf::new(&script);
        save(&again, &["A", "B", "c"], NOW + 1000, None);
        assert_eq!(again.list().len(), 4);
        assert_eq!(texts(&again), [owned(&["a", "b"]), None, None, owned(&["A", "B", "c"])]);
    }

    // A link or a pipe planted where the history lives is replaced, never followed or opened, and the
    // history carries on in a real folder of its own.
    #[test]
    fn refuses_links() {
        let (first, script) = shelf("links");
        let decoy = env::temp_dir().join("origami-saves-decoy-test");
        let _ = fs::remove_dir_all(&decoy);
        fs::create_dir_all(&decoy).unwrap();
        fs::write(decoy.join("log"), "precious").unwrap();
        fs::create_dir_all(first.dir.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&decoy, &first.dir).unwrap();
        save(&first, &["a"], NOW, None);
        assert!(fs::symlink_metadata(&first.dir).unwrap().is_dir());
        assert_eq!(fs::read_dir(&decoy).unwrap().count(), 1);
        assert_eq!(fs::read_to_string(decoy.join("log")).unwrap(), "precious");
        let again = Shelf::new(&script);
        fs::remove_file(again.dir.join("log")).unwrap();
        std::os::unix::fs::symlink(decoy.join("log"), again.dir.join("log")).unwrap();
        fs::remove_file(again.dir.join("index")).unwrap();
        let made = Command::new("mkfifo").arg(again.dir.join("index")).status();
        assert!(made.unwrap().success());
        assert!(again.list().is_empty());
        save(&again, &["b"], NOW + 100, None);
        assert_eq!(fs::read_to_string(decoy.join("log")).unwrap(), "precious");
        assert!(fs::symlink_metadata(again.dir.join("log")).unwrap().is_file());
        assert_eq!(texts(&again), [owned(&["b"])]);
    }

    fn open<'a>(cx: &'a mut TestAppContext, name: &str, versions: &[&[&str]]) -> (Entity<Origami>, &'a mut VisualTestContext, PathBuf) {
        let dir = env::temp_dir().join(format!("origami-saves-{name}-test"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = fs::canonicalize(&dir).unwrap().join("edit.js");
        let last = versions.last().unwrap();
        fs::write(&path, last.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let shelf = Shelf::new(&path);
        let _ = fs::remove_dir_all(&shelf.dir);
        for (n, lines) in versions.iter().enumerate() {
            save(&shelf, lines, now() - 100 * (versions.len() - n) as u64, None);
        }
        cx.update(|cx| crate::menu::install(cx, &[]));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut app = crate::fresh(cx.focus_handle(), vec![]);
            app.open(path.clone(), cx);
            window.focus(&app.focus);
            window.activate_window();
            app
        });
        cx.run_until_parked();
        view.update(cx, |o, cx| o.watch(cx));
        cx.run_until_parked();
        (view, cx, path)
    }

    // Restoring a version keeps the unsaved code on screen as a version first, writes the old version
    // into the script and saves it as a new version on top named after the one restored, keeps the list
    // open, and cmd+Z brings back the code that was on screen.
    #[gpui::test]
    fn restores_as_a_new_save(cx: &mut TestAppContext) {
        let (view, cx, path) = open(cx, "restore", &[&["a", "b", "c"], &["A", "b", "c"], &["A", "b", "C"]]);
        view.update(cx, |o, _| (o.lines[1], o.dirty) = ("mine".into(), true));
        let shown = ["A", "mine", "C"].map(String::from).to_vec();
        let when = view.update(cx, |o, cx| {
            o.history(Some(0));
            o.compare(2);
            o.restore();
            o.stow(cx);
            let s = o.saves.as_ref().unwrap();
            s.list[2].moment(s.today())
        });
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\nc\n");
        view.update(cx, |o, _| {
            let (shelf, s) = (o.shelf.clone().unwrap(), o.saves.as_ref().unwrap());
            assert_eq!(s.list.len(), 5);
            assert_eq!(
                (s.list[0].what.clone(), s.pick, s.diff.is_none()),
                (format!("\u{21ba} restored {when}"), None, true)
            );
            assert_eq!(
                (shelf.read(&s.list[0]), shelf.read(&s.list[1])),
                (owned(&["a", "b", "c"]), Some(shown.clone()))
            );
            assert_eq!(o.note, format!("restored {when} as a new save \u{b7} cmd+Z undoes it"));
        });
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), shown);
        assert_eq!(fs::read_to_string(&path).unwrap(), "A\nmine\nC\n");
    }

    // :history, cmd+Y, File > History, j k enter esc and ]c [c reach the same states as clicking the
    // history button, a version, the arrows and back, while a : command typed over the list stays a
    // command; r and Restore this version restore the same text.
    #[gpui::test]
    fn keys_match_clicks(cx: &mut TestAppContext) {
        let ten = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        let (mut two, mut three) = (ten, ten);
        (two[0], two[9], three[0], three[4]) = ("A", "J", "A", "E");
        let (view, cx, path) = open(cx, "keys", &[&ten, &two, &three]);
        let state = |cx: &mut VisualTestContext| {
            cx.run_until_parked();
            view.update(cx, |o, _| {
                o.saves
                    .as_ref()
                    .map(|s| (s.pick, s.diff.as_ref().map(|d| (d.at, d.hunk, d.hunks.len()))))
            })
        };
        let (w, h) = cx.update(|window, _| (f32::from(window.viewport_size().width), f32::from(window.viewport_size().height)));
        let click = |cx: &mut VisualTestContext, at: (f32, f32)| {
            cx.simulate_click(point(px(at.0), px(at.1)), Modifiers::default());
            state(cx)
        };
        let row = |cx: &mut VisualTestContext, i: usize| {
            let shown = view.update(cx, |o, _| o.saves.as_ref().unwrap().shown(now()));
            let y = shown.iter().take(i + 1).enumerate().fold(72., |y, (n, (head, _))| {
                y + 24. + head.as_ref().map_or(0., |_| if n == 0 { 22. } else { 30. })
            });
            click(cx, (w - 320., y - 12.))
        };
        let (dial, back, up, down, restore) = ((w - 19., 51.), (w - 486., 83.), (w - 49., 83.), (w - 19., 83.), (w - 90., h - 55.));
        let mut keys = vec![];
        for typed in [
            ":",
            "h i s t o r y enter",
            "j",
            "j",
            "k",
            "enter",
            "] c",
            "[ c",
            "escape",
            "escape",
            "cmd-y",
            ": j k escape",
        ] {
            cx.simulate_keystrokes(typed);
            keys.push(state(cx));
        }
        cx.dispatch_action(crate::menu::History);
        keys.push(state(cx));
        let listed = |pick| Some((Some(pick), None));
        let (diffed, flipped) = (Some((Some(1), Some((1, 0, 2)))), Some((Some(1), Some((1, 1, 2)))));
        let want = [
            None,
            listed(0),
            listed(1),
            listed(2),
            listed(1),
            diffed,
            flipped,
            diffed,
            listed(1),
            None,
            listed(0),
            listed(0),
            None,
        ];
        assert_eq!(keys, want);
        let mut clicks = vec![click(cx, dial)];
        clicks.push(row(cx, 1));
        for at in [down, up, back, dial] {
            clicks.push(click(cx, at));
        }
        assert_eq!(clicks, [Some((None, None)), diffed, flipped, diffed, listed(1), None]);
        cx.simulate_keystrokes("cmd-y j enter r");
        let typed = fs::read_to_string(&path).unwrap();
        assert_eq!((typed.as_str(), state(cx)), ("A\nb\nc\nd\ne\nf\ng\nh\ni\nJ\n", Some((None, None))));
        cx.simulate_keystrokes("cmd-z cmd-y");
        click(cx, dial);
        row(cx, 1);
        assert_eq!(click(cx, restore), Some((None, None)));
        assert_eq!(fs::read_to_string(&path).unwrap(), typed);
    }
}
