use crate::Origami;

pub struct Region {
    pub start: usize,
    pub end: usize,
    pub note: String,
    pub run: bool,
}

pub fn depth(l: &str) -> i32 {
    l.chars().fold(0, |d, c| d + "([{".contains(c) as i32 - ")]}".contains(c) as i32)
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
        for (i, _) in lines.iter().enumerate().filter(|(_, l)| want(l) && depth(l) > 0) {
            let mut d = 0;
            let close = lines[i..].iter().position(|l| {
                d += depth(l);
                d <= 0
            });
            if let Some(n) = close.filter(|n| *n > 0) {
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
        self.spans(|l| self.folded.contains(l))
    }

    pub fn hidden(&self) -> Vec<bool> {
        let mut hidden = vec![false; self.lines.len()];
        for r in self.shut() {
            hidden[r.start + 1..=r.end].fill(true);
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
}
