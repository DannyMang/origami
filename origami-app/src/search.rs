use crate::Origami;

impl Origami {
    pub fn hits(&self) -> Vec<(usize, usize, usize)> {
        let Some(seek) = self.seeker.as_ref().filter(|_| self.finding.is_some()) else {
            return vec![];
        };
        let rows = self.lines.iter().enumerate();
        rows.flat_map(|(r, l)| {
            let (found, mut at) = (seek.find_iter(l).filter(|m| !m.is_empty()), (0, 0));
            found.map(move |m| {
                at = (m.start(), at.1 + l[at.0..m.start()].chars().count());
                (r, at.1, m.as_str().chars().count())
            })
        })
        .collect()
    }

    pub fn aim(&mut self) {
        let q = self.finding.as_deref().unwrap_or_default();
        let plain = if self.modes[2] { q.to_string() } else { regex::escape(q) };
        let word = if self.modes[1] { format!(r"\b(?:{plain})\b") } else { plain };
        let seek = regex::RegexBuilder::new(&word).case_insensitive(!self.modes[0]).build();
        self.seeker = seek.ok().filter(|_| !q.is_empty());
    }

    pub fn flip(&mut self, mode: usize) {
        self.modes[mode] ^= true;
        self.aim();
        self.hunt(None);
    }

    pub fn hunt(&mut self, step: Option<bool>) {
        let (hits, q) = (self.hits(), self.finding.clone().unwrap_or_default());
        if hits.is_empty() {
            return (self.cursor, self.anchor) = (None, None);
        }
        let from = self.anchor.or(self.cursor).unwrap_or((0, 0));
        self.hit = match step {
            None => hits.iter().position(|h| (h.0, h.1) >= from).unwrap_or(0),
            Some(true) => (self.hit + 1) % hits.len(),
            Some(false) => (self.hit + hits.len() - 1) % hits.len(),
        };
        let (r, c, n) = hits[self.hit.min(hits.len() - 1)];
        (self.anchor, self.cursor) = (Some((r, c)), Some((r, c + n)));
        self.reveal(r);
        if let Some(t) = self.tl.clips.iter().find(|c| c.label == q).map(|c| c.start) {
            self.seek(t);
        }
    }

    pub fn search(&mut self, k: &str, shift: bool) {
        let Some(mut q) = self.finding.take() else { return };
        match k {
            "escape" => return,
            "enter" | "down" => self.finding = Some(q),
            "up" => self.finding = Some(q),
            "backspace" => {
                q.pop();
                self.finding = Some(q);
            }
            k => {
                q.push_str(if k == "space" {
                    " "
                } else if k.chars().count() == 1 {
                    k
                } else {
                    ""
                });
                self.finding = Some(q);
            }
        }
        self.aim();
        self.hunt(match k {
            "enter" | "down" => Some(!shift),
            "up" => Some(false),
            _ => None,
        });
    }
}

#[cfg(test)]
mod tests {
    // Searching a line of 400,000 letters should find every match at its column in moments.
    #[gpui::test]
    fn long_lines_search_quickly(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, cx| crate::fresh(cx.focus_handle(), vec![]));
        view.update(cx, |o, _| {
            (o.lines, o.finding) = (vec!["\u{e9}a".repeat(200_000)], Some("a".into()));
            o.aim();
            let start = std::time::Instant::now();
            let hits = o.hits();
            assert_eq!((hits.len(), hits[1], hits[199_999]), (200_000, (0, 3, 1), (0, 399_999, 1)));
            assert!(start.elapsed().as_secs() < 2);
        });
    }
}
