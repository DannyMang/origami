use crate::Origami;

impl Origami {
    pub fn hits(&self) -> Vec<(usize, usize)> {
        let q: Vec<char> = self.finding.as_deref().unwrap_or_default().to_lowercase().chars().collect();
        let rows = self.lines.iter().enumerate().filter(|_| !q.is_empty());
        rows.flat_map(|(r, l)| {
            let line: Vec<char> = l.to_lowercase().chars().collect();
            let at = (0..line.len().saturating_sub(q.len() - 1)).filter(|c| line[*c..].starts_with(&q));
            at.map(|c| (r, c)).collect::<Vec<_>>()
        })
        .collect()
    }

    pub fn hunt(&mut self, step: Option<bool>) {
        let (hits, q) = (self.hits(), self.finding.clone().unwrap_or_default());
        if hits.is_empty() {
            return (self.cursor, self.anchor) = (None, None);
        }
        let from = self.anchor.or(self.cursor).unwrap_or((0, 0));
        self.hit = match step {
            None => hits.iter().position(|h| *h >= from).unwrap_or(0),
            Some(true) => (self.hit + 1) % hits.len(),
            Some(false) => (self.hit + hits.len() - 1) % hits.len(),
        };
        let (r, c) = hits[self.hit.min(hits.len() - 1)];
        (self.anchor, self.cursor) = (Some((r, c)), Some((r, c + q.chars().count())));
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
        self.hunt(match k {
            "enter" | "down" => Some(!shift),
            "up" => Some(false),
            _ => None,
        });
    }
}
