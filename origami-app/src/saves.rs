use crate::{
    Mode, Origami,
    shelf::{DAY, Entry, Shelf, diff, now},
    view::{BAR, BG, BLUE, BORDER, COMMENT, DIM, GREEN, GUTTER, INK, MUTED, PANEL, RED, TEXT, tokens},
};
use gpui::{Context, Div, HighlightStyle, MouseButton, ScrollWheelEvent, StyledText, deferred, div, prelude::*, px, rgb, rgba};
use std::{ops::Range, sync::Arc, time::Duration};

const SHOWN: usize = 12;
const FAINT: u32 = 0x6f7685;
const WEEK: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

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
    sure: bool,
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
        let (open, mine) = (self.saves.is_some(), shelf.clone());
        self.shelving = true;
        cx.spawn(async move |this, cx| {
            let list = cx
                .background_executor()
                .spawn(async move {
                    shelf.drain(true, Duration::from_millis(100));
                    open.then(|| shelf.list().into_iter().rev().collect())
                })
                .await;
            this.update(cx, |this, cx| {
                this.shelving = false;
                if this.holds(&mine) {
                    this.reread(list);
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn holds(&self, shelf: &Arc<Shelf>) -> bool {
        self.shelf.as_ref().is_some_and(|s| Arc::ptr_eq(s, shelf))
    }

    fn reread(&mut self, list: Option<Vec<Entry>>) {
        let (Some(list), Some(s)) = (list, &mut self.saves) else {
            return;
        };
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
            zone: list.first().map_or(0, |e| e.z),
            list,
            top: 0.,
            diff: None,
            mark: None,
            sure: false,
        })
    }

    pub fn history(&mut self, pick: Option<usize>) {
        self.saves = match self.saves.take() {
            Some(_) => None,
            None => self.versions(pick),
        };
    }

    fn compare(&mut self, i: usize, cx: &mut Context<Self>) {
        let (Some(shelf), Some(e)) = (self.shelf.clone(), self.saves.as_ref().and_then(|s| s.list.get(i).cloned())) else {
            return;
        };
        let (now, mine) = (self.lines.clone(), shelf.clone());
        cx.spawn(async move |this, cx| {
            let diff = cx
                .background_executor()
                .spawn(async move { shelf.read(&e).map(|lines| Diff::new(i, lines, now)) })
                .await;
            this.update(cx, |this, cx| {
                if !this.holds(&mine) {
                    return;
                }
                let Some(s) = &mut this.saves else { return };
                match diff {
                    Some(d) => {
                        (s.pick, s.diff) = (Some(i), Some(d));
                        (this.cursor, this.anchor, this.hover, this.knobbed) = (None, None, None, None);
                    }
                    None => this.note = "this version could not be read".into(),
                }
                cx.notify();
            })
        })
        .detach();
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
        if !self.dirty {
            self.note = format!("restored {when} as a new save \u{b7} cmd+z undoes it");
        }
    }

    pub fn wipe(&mut self, cx: &mut Context<Self>) {
        let Some(shelf) = self.shelf.clone() else { return };
        let mine = shelf.clone();
        cx.spawn(async move |this, cx| {
            let done = cx.background_executor().spawn(async move { shelf.clear() }).await;
            this.update(cx, |this, cx| {
                if !this.holds(&mine) {
                    return;
                }
                this.note = match done {
                    true => "removed this script's saved versions".into(),
                    false => "saved versions could not be removed: try again".into(),
                };
                if let Some(s) = this.saves.as_mut().filter(|_| done) {
                    (s.list, s.pick, s.diff, s.top) = (vec![], None, None, 0.);
                }
                cx.notify();
            })
        })
        .detach();
    }

    fn spin(&mut self, by: f32) {
        let row = self.row();
        let Some(s) = &mut self.saves else { return };
        match &mut s.diff {
            Some(d) => d.top = (d.top - by / row).clamp(0., d.rows.len().saturating_sub(1) as f32),
            None => s.top = (s.top - by / 24.).clamp(0., s.list.len().saturating_sub(SHOWN) as f32),
        }
    }

    pub fn leaf(&mut self, k: &str, plain: bool, cx: &mut Context<Self>) -> bool {
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
            "enter" => self.compare(pick, cx),
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
                        this.compare(i, cx);
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
                        .children((i == 0 && !self.dirty).then_some("current")),
                );
            head.into_iter().chain([row])
        });
        let sure = s.sure;
        let ask = |label: &'static str, color: u32, wipe: bool| {
            div()
                .w(px(44.))
                .flex()
                .justify_end()
                .cursor_pointer()
                .text_color(rgb(color))
                .hover(|s| s.text_color(rgb(TEXT)))
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if let Some(s) = &mut this.saves {
                            s.sure = !sure;
                        }
                        if wipe {
                            this.wipe(cx);
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
        };
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
            .child(match s.sure {
                true => format!("Remove all {} versions? This cannot be undone.", s.list.len()),
                false => format!("{} versions \u{b7} kept on this Mac, not in the project", s.list.len()),
            })
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(match s.sure {
                        true => ask("cancel", MUTED, false),
                        false => div().child("esc closes"),
                    })
                    .child(match s.sure {
                        true => ask("remove", RED, true),
                        false => ask("clear", MUTED, false),
                    }),
            );
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
            marks.extend(tokens(&code).map(|(r, h)| (r.start + skip..r.end + skip, h)));
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
                "Restoring saves {when} as a new version. Nothing is lost, and cmd+z undoes it."
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
    use crate::shelf::tests::{NOW, owned, save, shelf};
    use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point};
    use std::{env, fs, path::PathBuf};

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
            sure: false,
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

    fn open<'a>(cx: &'a mut TestAppContext, name: &str, versions: &[&[&str]]) -> (Entity<Origami>, &'a mut VisualTestContext, PathBuf) {
        let dir = env::temp_dir().join(format!("origami-saves-{name}-test"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = fs::canonicalize(&dir).unwrap().join("edit.js");
        let last = versions.last().unwrap();
        fs::write(&path, last.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        let _ = fs::remove_dir_all(crate::crash::kept(&path, "history"));
        let shelf = Shelf::new(&path);
        for (n, lines) in versions.iter().enumerate() {
            save(&shelf, lines, now() - 100 * (versions.len() - n) as u64, None);
        }
        cx.update(crate::menu::install);
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
        view.update(cx, |o, cx| {
            o.history(Some(0));
            o.compare(2, cx);
        });
        cx.run_until_parked();
        let when = view.update(cx, |o, cx| {
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
            assert_eq!(o.note, format!("restored {when} as a new save \u{b7} cmd+z undoes it"));
        });
        cx.simulate_keystrokes("cmd-z");
        assert_eq!(view.update(cx, |o, _| o.lines.clone()), shown);
        assert_eq!(fs::read_to_string(&path).unwrap(), "A\nmine\nC\n");
    }

    // A script changed by another program while the code on screen is unsaved keeps the other program's
    // text as a version marked changed on disk, so keeping mine over it and saving again soon after
    // still leaves a way back to it.
    #[gpui::test]
    fn keeps_what_changed_on_disk(cx: &mut TestAppContext) {
        let (view, cx, path) = open(cx, "disk", &[&["a", "b"]]);
        view.update(cx, |o, _| (o.lines[1], o.dirty) = ("mine".into(), true));
        fs::write(&path, "a\ntheirs\n").unwrap();
        view.update(cx, |o, cx| o.watch(cx));
        cx.run_until_parked();
        view.update(cx, |o, cx| {
            assert!(o.clash);
            (o.clash, o.lines[1]) = (false, "mine 2".into());
            o.store();
            o.stow(cx);
        });
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nmine 2\n");
        let shelf = view.update(cx, |o, _| o.shelf.clone().unwrap());
        let kept: Vec<_> = shelf.list().iter().map(|e| (e.what.clone(), shelf.read(e))).collect();
        let theirs = ("changed on disk".to_string(), owned(&["a", "theirs"]));
        assert!(
            kept.contains(&theirs) && kept.last().unwrap().1 == owned(&["a", "mine 2"]),
            "{kept:?}"
        );
    }

    // A version or a version list still loading when another script opens never lands in that
    // script's history list.
    #[gpui::test]
    fn keeps_versions_with_their_script(cx: &mut TestAppContext) {
        let (view, cx, path) = open(cx, "mine", &[&["a"], &["b"], &["c"]]);
        let other = path.with_file_name("other.js");
        fs::write(&other, "y\n").unwrap();
        let _ = fs::remove_dir_all(crate::crash::kept(&other, "history"));
        let theirs = Shelf::new(&other);
        save(&theirs, &["x"], now() - 1000, None);
        save(&theirs, &["y"], now() - 500, None);
        view.update(cx, |o, cx| {
            o.history(Some(0));
            o.compare(1, cx);
            o.shelve("d\n".into());
            o.stow(cx);
            o.open(other.clone(), cx);
            o.history(None);
        });
        cx.run_until_parked();
        let shown = view.update(cx, |o, _| o.saves.as_ref().map(|s| (s.list.len(), s.pick, s.diff.is_none())));
        assert_eq!(shown, Some((2, None, true)));
    }

    // The clear button asks first, cancel keeps every version, and remove or :history clear deletes
    // them all from the script's history.
    #[gpui::test]
    fn clears_by_command_and_click(cx: &mut TestAppContext) {
        let (view, cx, _) = open(cx, "clear", &[&["a"], &["b"], &["c"]]);
        let w = cx.update(|window, _| f32::from(window.viewport_size().width));
        let kept = |cx: &mut VisualTestContext, at: Option<f32>| {
            if let Some(x) = at {
                cx.simulate_click(point(px(x), px(179.)), Modifiers::default());
            }
            cx.run_until_parked();
            view.update(cx, |o, _| {
                (o.shelf.as_ref().unwrap().list().len(), o.saves.as_ref().map(|s| s.list.len()))
            })
        };
        cx.simulate_keystrokes("cmd-y");
        let asked = [kept(cx, Some(w - 30.)), kept(cx, Some(w - 86.)), kept(cx, Some(w - 30.))];
        assert_eq!(asked, [(3, Some(3)); 3]);
        assert_eq!(kept(cx, Some(w - 30.)), (0, Some(0)));
        view.update(cx, |o, cx| {
            o.shelve("d\n".into());
            o.stow(cx);
        });
        assert_eq!(kept(cx, None).0, 1);
        cx.simulate_keystrokes(": h i s t o r y space c l e a r enter");
        assert_eq!(kept(cx, None).0, 0);
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
        cx.simulate_keystrokes("cmd-y j enter");
        state(cx);
        cx.simulate_keystrokes("r");
        let typed = fs::read_to_string(&path).unwrap();
        assert_eq!((typed.as_str(), state(cx)), ("A\nb\nc\nd\ne\nf\ng\nh\ni\nJ\n", Some((None, None))));
        cx.simulate_keystrokes("cmd-z cmd-y");
        click(cx, dial);
        row(cx, 1);
        assert_eq!(click(cx, restore), Some((None, None)));
        assert_eq!(fs::read_to_string(&path).unwrap(), typed);
    }
}
