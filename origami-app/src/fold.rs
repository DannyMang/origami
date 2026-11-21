use crate::Origami;
use std::collections::HashSet;

pub struct Region {
    pub start: usize,
    pub end: usize,
    pub note: String,
    pub run: bool,
}

pub fn depth(l: &str) -> i32 {
    l.bytes().fold(0, |d, c| {
        d + matches!(c, b'(' | b'[' | b'{') as i32 - matches!(c, b')' | b']' | b'}') as i32
    })
}

fn kind(l: &str) -> Option<&'static str> {
    let t = l.trim_start();
    if t.starts_with("//") {
        return Some("comment");
    }
    let def = ["let ", "const ", "var "].iter().any(|k| t.starts_with(k)) && depth(l) == 0;
    def.then(|| {
        ["video", "sound", "image"]
            .into_iter()
            .find(|k| t.contains(&format!("{k}(")))
            .unwrap_or("value")
    })
}

pub fn floor(hidden: &[bool], rows: usize) -> usize {
    let seen = hidden.iter().enumerate().rev().filter(|h| !h.1);
    seen.map(|h| h.0).nth(rows.saturating_sub(1)).unwrap_or(0)
}

impl Origami {
    fn spans(&self, want: impl Fn(&String) -> bool) -> Vec<Region> {
        let (lines, mut out) = (&self.lines, vec![]);
        for (i, l) in lines.iter().enumerate().filter(|(_, l)| want(l)) {
            let k = kind(l).filter(|k| i == 0 || kind(&lines[i - 1]) != Some(*k));
            let n = lines[i..].iter().take_while(|l| k.is_some() && kind(l) == k).count();
            if n >= 3 {
                let note = format!("\u{2026} {} more {}s", n - 1, k.unwrap_or_default());
                out.push(Region {
                    start: i,
                    end: i + n - 1,
                    note,
                    run: true,
                });
            }
        }
        let (mut ends, mut open, mut d) = (vec![None; lines.len()], vec![], 0);
        for (j, l) in lines.iter().enumerate() {
            let before = d;
            d += depth(l);
            while let Some(&(i, _)) = open.last().filter(|o: &&(usize, i32)| o.1 >= d) {
                (ends[i], _) = (Some(j), open.pop());
            }
            if d > before {
                open.push((j, before));
            }
        }
        for (i, _) in lines.iter().enumerate().filter(|(_, l)| want(l) && depth(l) > 0) {
            if let Some(n) = ends[i].map(|e| e - i) {
                let note = format!("\u{2026} {} lines {}", n - 1, lines[i + n].trim());
                out.push(Region {
                    start: i,
                    end: i + n,
                    note,
                    run: false,
                });
            }
        }
        out
    }

    pub fn regions(&self) -> Vec<Region> {
        self.spans(|_| true)
    }

    fn shut(&self) -> Vec<Region> {
        let folded: HashSet<&String> = self.folded.iter().collect();
        self.spans(|l| folded.contains(l))
    }

    pub fn hidden(&self) -> Vec<bool> {
        let (mut hidden, mut shut, mut reach) = (vec![false; self.lines.len()], self.shut(), 0);
        shut.sort_by_key(|r| r.start);
        for r in shut {
            hidden[(r.start + 1).max(reach).min(r.end + 1)..=r.end].fill(true);
            reach = reach.max(r.end + 1);
        }
        hidden
    }

    pub fn fold(&mut self, row: usize, shut: Option<bool>) {
        let regions = self.regions();
        let Some(r) = regions.iter().filter(|r| r.start <= row && row <= r.end).max_by_key(|r| r.start) else {
            return;
        };
        let line = self.lines[r.start].clone();
        let was = self.folded.contains(&line);
        self.folded.retain(|l| *l != line);
        if shut.unwrap_or(!was) {
            self.folded.push(line);
            self.cursor = self.cursor.map(|c| if c.0 > r.start && c.0 <= r.end { (r.start, 0) } else { c });
            self.anchor = None;
        }
    }

    pub fn unfold(&mut self, row: usize) {
        for r in self.shut().iter().filter(|r| r.start < row && row <= r.end) {
            self.folded.retain(|l| *l != self.lines[r.start]);
        }
    }

    pub fn fold_all(&mut self, long: bool) {
        let pick = self.regions().into_iter().filter(|r| !long || r.run && r.end - r.start >= 7);
        self.folded = pick.map(|r| self.lines[r.start].clone()).collect();
    }

    pub fn roll(&mut self, rows: f32) {
        let hidden = self.hidden();
        let last = floor(&hidden, self.fit);
        let mut top = (self.top - rows).clamp(0., last as f32);
        while hidden.get(top as usize) == Some(&true) && top > 0. && (top as usize) < last {
            top += if rows < 0. { 1. } else { -1. };
        }
        self.top = top;
    }
}

#[cfg(test)]
mod tests {
    // The furthest the script can scroll should count only visible lines, so folded (hidden) lines
    // never let you scroll past the end.
    #[test]
    fn floor() {
        let folded = [false, false, true, true, false, false];
        assert_eq!(super::floor(&[false; 50], 36), 14);
        assert_eq!(super::floor(&[false; 10], 36), 0);
        assert_eq!(super::floor(&folded, 3), 1);
        assert_eq!(super::floor(&folded, 2), 4);
        assert_eq!(super::floor(&[], 36), 0);
    }

    // Working out folds should take moments on 100,000 lines that each open a bracket, and folding
    // every brace of a deep nest should hide everything inside the outermost one.
    #[gpui::test]
    fn huge_scripts_fold_quickly(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| crate::fresh(cx.focus_handle(), vec![]));
        view.update(cx, |o, _| {
            let start = std::time::Instant::now();
            o.lines = vec!["f(".to_string(); 100_000];
            assert!(o.regions().is_empty());
            o.lines = ["{", "}"].iter().flat_map(|b| vec![b.to_string(); 50_000]).collect();
            o.fold_all(false);
            let hidden = o.hidden();
            assert_eq!(
                (o.folded.len(), hidden.iter().filter(|h| **h).count(), hidden[0]),
                (50_000, 99_999, false)
            );
            assert!(start.elapsed().as_secs() < 2);
        });
    }
}
