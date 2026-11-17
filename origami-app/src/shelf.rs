use crate::{crash, edit::same, knob::close, menu::place};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    mem,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

const MERGE: u64 = 60;
const CAP: u64 = 50 << 20;
const SLACK: u64 = 1 << 16;
pub const DAY: i64 = 86400;

static ZONE: Mutex<(u64, i64)> = Mutex::new((u64::MAX, 0));

type Record = (u64, usize, usize, Vec<String>);
type Line = (u64, i64, usize, usize, String, String, u64, u64, u64);

#[derive(Clone)]
pub struct Entry {
    pub t: u64,
    pub z: i64,
    pub adds: usize,
    pub dels: usize,
    pub clip: String,
    pub what: String,
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

struct Log {
    newest: Option<(String, u64, u64)>,
    last: Option<Last>,
    size: (u64, u64),
}

pub struct Shelf {
    dir: PathBuf,
    name: String,
    pub queue: Mutex<Vec<(String, u64, Option<String>)>>,
    log: Mutex<Option<Log>>,
}
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn zone(t: u64) -> i64 {
    let mut kept = ZONE.lock().unwrap();
    if kept.0 != t / 3600 {
        let s = crash::reply(Command::new("/bin/date").args(["-r", &t.to_string(), "+%z"]));
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

pub fn diff<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(Option<usize>, Option<usize>)> {
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
        let _ = room(&self.dir);
        let _held = make(&self.dir.join("lock")).and_then(|f| f.lock().map(|_| f));
        loop {
            let batch = mem::take(&mut *self.queue.lock().unwrap());
            if batch.is_empty() {
                return;
            }
            for (text, t, mark) in batch {
                let kept = log.take().filter(|l| l.size == self.size());
                let log = log.insert(kept.unwrap_or_else(|| self.load(t)));
                log.size = match self.add(log, text, t, mark) {
                    Ok(end) if end <= CAP => self.size(),
                    _ => (u64::MAX, 0),
                };
            }
        }
    }

    fn size(&self) -> (u64, u64) {
        (len(&self.dir.join("log")), len(&self.dir.join("index")))
    }

    pub fn list(&self) -> Vec<Entry> {
        let index = self.dir.join("index");
        let all = slice(&index, 0, len(&index)).unwrap_or_default();
        all.split(|b| *b == b'\n').filter_map(entry).collect()
    }

    pub fn read(&self, e: &Entry) -> Option<Vec<String>> {
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
        Log {
            newest,
            last: None,
            size: self.size(),
        }
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
                make(&index)?.set_len(last.cut.1)?;
                make(&path)?.set_len(last.cut.0)?;
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

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::env;

    pub const NOW: u64 = 1794830400;

    pub fn shelf(name: &str) -> (Shelf, PathBuf) {
        let script = env::temp_dir().join(format!("origami-saves-{name}-test")).join("edit.js");
        let shelf = Shelf::new(&script);
        let _ = fs::remove_dir_all(&shelf.dir);
        (shelf, script)
    }

    pub fn save(shelf: &Shelf, lines: &[&str], t: u64, mark: Option<&str>) {
        let text = lines.iter().map(|l| format!("{l}\n")).collect();
        shelf.queue.lock().unwrap().push((text, t, mark.map(String::from)));
        shelf.drain();
    }

    fn texts(shelf: &Shelf) -> Vec<Option<Vec<String>>> {
        shelf.list().iter().map(|e| shelf.read(e)).collect()
    }

    pub fn owned(lines: &[&str]) -> Option<Vec<String>> {
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

    // Two windows saving the same script never fold over or cut off each other's versions: every save
    // from either reads back exactly, in order.
    #[test]
    fn shares_a_script() {
        let (one, script) = shelf("share");
        let two = Shelf::new(&script);
        save(&one, &["a", "b", "c"], NOW, None);
        save(&two, &["x", "b", "c"], NOW + 10, None);
        save(&one, &["a", "b", "C"], NOW + 20, None);
        save(&two, &["x", "b", "c", "d"], NOW + 30, None);
        let want = [
            owned(&["a", "b", "c"]),
            owned(&["x", "b", "c"]),
            owned(&["a", "b", "C"]),
            owned(&["x", "b", "c", "d"]),
        ];
        assert_eq!(texts(&one), want);
    }

    // A fold that cannot rewrite the history, as in a read-only folder, changes nothing: the save still
    // goes through, and every kept version reads back once the history can be written again.
    #[test]
    fn survives_a_failed_fold() {
        let (first, _) = shelf("fold");
        save(&first, &["a", "b", "c"], NOW, None);
        save(&first, &["a", "B", "c"], NOW + 100, None);
        let index = first.dir.join("index");
        let mode = |m| fs::set_permissions(&index, std::os::unix::fs::PermissionsExt::from_mode(m)).unwrap();
        mode(0o400);
        save(&first, &["a", "BB", "c"], NOW + 110, None);
        mode(0o600);
        save(&first, &["a", "BB", "c", "d"], NOW + 500, None);
        assert_eq!(
            texts(&first),
            [owned(&["a", "b", "c"]), owned(&["a", "B", "c"]), owned(&["a", "BB", "c", "d"])]
        );
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
}
