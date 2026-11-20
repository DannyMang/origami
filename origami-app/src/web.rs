use crate::{
    Origami,
    motion::{Form, Link, name, ties},
    scene::title,
    view::{BLUE, BORDER, DIM, MUTED, PANEL, TEXT, tint},
};
use gpui::{App, Bounds, Context, Div, MouseButton, PathBuilder, Pixels, Window, canvas, div, fill, point, prelude::*, px, rgb, size};
use origami_lang::Val;
use std::collections::{HashMap, HashSet};

const WIDE: f32 = 196.;
const HEAD: f32 = 24.;
const ROW: f32 = 22.;
const GAP: (f32, f32) = (112., 22.);
const PAD: f32 = 28.;

fn firsts(names: &[Option<String>]) -> HashMap<&str, usize> {
    names
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(i, n)| Some((n.as_deref()?, i)))
        .collect()
}

pub fn depths(names: &[Option<String>], ties: &[Vec<(&'static str, Link)>]) -> Vec<usize> {
    let (find, n) = (firsts(names), names.len());
    let from = |i: usize| {
        ties[i]
            .iter()
            .filter_map(|(_, l)| find.get(l.clip.as_str()).copied())
            .filter(move |j| *j != i)
    };
    let (mut deep, mut busy) = (vec![None; n], vec![false; n]);
    for root in 0..n {
        let mut stack = vec![root];
        while let Some(&i) = stack.last().filter(|i| deep[**i].is_none()) {
            busy[i] = true;
            match from(i).find(|j| deep[*j].is_none() && !busy[*j]) {
                Some(j) => stack.push(j),
                None => {
                    deep[i] = Some(from(i).map(|j| deep[j].map_or(n, |d: usize| d + 1)).max().unwrap_or(0).min(n));
                    (busy[i], _) = (false, stack.pop());
                }
            }
        }
    }
    deep.into_iter().flatten().collect()
}

pub fn seats(deep: &[usize], homes: &[u32], tall: &[f32]) -> Vec<(f32, f32)> {
    let (mut next, mut out) = (vec![PAD; deep.len() + 1], vec![(0., 0.); deep.len()]);
    let mut order: Vec<usize> = (0..deep.len()).collect();
    order.sort_by_key(|i| homes[*i]);
    for i in order {
        out[i] = (PAD + deep[i] as f32 * (WIDE + GAP.0), next[deep[i]]);
        next[deep[i]] += tall[i] + GAP.1;
    }
    out
}

impl Origami {
    pub fn visit(&mut self, z: u32) {
        if self.sel != Some(z) {
            self.pick(z);
        }
        self.cursor = self.opened().map(|c| ((c.home as usize).saturating_sub(1), 0));
    }

    pub fn web(&mut self, room: (f32, f32), cx: &mut Context<Self>) -> Div {
        let (tl, (text, at)) = (self.tl.clone(), self.sites());
        let names = tl.clips.iter().zip(&at).map(|(c, f)| Some(name(&text, (*f)?, c.kind)?.to_string()));
        let names: Vec<Option<String>> = names.collect();
        let tied = tl.clips.iter().zip(&at).map(|(c, f)| f.map_or(vec![], |f| ties(&text, f, c.kind)));
        let tied: Vec<Vec<(&'static str, Link)>> = tied.collect();
        let find = firsts(&names);
        let source = |l: &Link| find.get(l.clip.as_str()).copied();
        let fed: HashSet<(usize, &str)> = tied.iter().flatten().filter_map(|(_, l)| Some((source(l)?, l.attr))).collect();
        let listed = tl.clips.iter().enumerate().map(|(i, c)| {
            let live = |n: &str| c.vals().any(|v| v.0 == n && matches!(v.1, Val::Frames(..)));
            let shown = |r: &crate::motion::Row| {
                let text = match &r.form {
                    Form::Keyed(_) => "\u{25c6} keys".into(),
                    Form::Linked(l) => format!("\u{2190} {}.{}", l.clip, title(l.attr)),
                    Form::Code if live(r.name) => "code".into(),
                    _ if fed.contains(&(i, r.name)) => format!("{:.2}", r.now),
                    _ => return None,
                };
                Some((r.name, text, matches!(r.form, Form::Keyed(_) | Form::Linked(_))))
            };
            self.rows_at(c, &text, at[i]).iter().filter_map(shown).collect::<Vec<_>>()
        });
        let listed: Vec<Vec<(&'static str, String, bool)>> = listed.collect();
        let tall: Vec<f32> = listed.iter().map(|l| HEAD + l.len().max(1) as f32 * ROW).collect();
        let homes: Vec<u32> = tl.clips.iter().map(|c| c.home).collect();
        let spots = seats(&depths(&names, &tied), &homes, &tall);
        let far = spots
            .iter()
            .zip(&tall)
            .fold((0f32, 0f32), |m, (p, t)| (m.0.max(p.0 + WIDE + PAD), m.1.max(p.1 + t + PAD)));
        self.roam = (
            self.roam.0.clamp(0., (far.0 - room.0).max(0.)),
            self.roam.1.clamp(0., (far.1 - room.1).max(0.)),
        );
        let (dx, dy) = self.roam;
        let port = |i: usize, attr: &str, right: bool| {
            let row = listed[i].iter().position(|r| r.0 == attr)? as f32;
            Some((
                spots[i].0 + if right { WIDE } else { 0. } - dx,
                spots[i].1 + HEAD + (row + 0.5) * ROW - dy,
            ))
        };
        let wires = tied.iter().enumerate().flat_map(|(i, t)| {
            let (port, source) = (&port, &source);
            t.iter()
                .filter_map(move |(name, l)| Some((port(source(l)?, l.attr, true)?, port(i, name, false)?, l.wire)))
        });
        let wires: Vec<_> = wires.collect();
        let badges = wires.iter().filter_map(|&(a, b, wire)| {
            let (mul, n) = wire?;
            let text = match (mul, n < 0.) {
                (true, _) => format!("\u{d7} {n}"),
                (_, true) => format!("\u{2212} {}", -n),
                _ => format!("+ {n}"),
            };
            Some(
                div()
                    .absolute()
                    .left(px((a.0 + b.0) / 2. - 26.))
                    .top(px((a.1 + b.1) / 2. - 8.))
                    .w(px(52.))
                    .h(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BLUE))
                    .text_size(px(10.))
                    .text_color(rgb(BLUE))
                    .child(text),
            )
        });
        let badges: Vec<Div> = badges.collect();
        let draw = move |b: Bounds<Pixels>, _, window: &mut Window, _: &mut App| {
            let at = |(x, y): (f32, f32)| b.origin + point(px(x), px(y));
            let (cols, rows) = ((f32::from(b.size.width) / 20.) as i32, (f32::from(b.size.height) / 20.) as i32);
            for (i, j) in (0..=cols).flat_map(|i| (0..=rows).map(move |j| (i, j))) {
                let dot = at((i as f32 * 20. + 10. - dx % 20., j as f32 * 20. + 10. - dy % 20.));
                window.paint_quad(fill(Bounds::new(dot, size(px(1.), px(1.))), rgb(BORDER)));
            }
            for &(a, b, _) in &wires {
                let (mut line, bow) = (PathBuilder::stroke(px(1.5)), ((b.0 - a.0).abs() / 2.).max(32.));
                line.move_to(at(a));
                line.cubic_bezier_to(at(b), at((a.0 + bow, a.1)), at((b.0 - bow, b.1)));
                if let Ok(line) = line.build() {
                    window.paint_path(line, rgb(BLUE));
                }
            }
        };
        let nodes = tl.clips.iter().enumerate().map(|(i, c)| {
            let (z, on) = (c.z, self.sel == Some(c.z));
            let pin = |right: bool, lit: bool| {
                div()
                    .absolute()
                    .top(px(ROW / 2. - 3.5))
                    .size(px(7.))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(if lit { BLUE } else { DIM }))
                    .bg(rgb(if lit { BLUE } else { PANEL }))
                    .map(|d| if right { d.right(px(-4.)) } else { d.left(px(-4.)) })
            };
            let rows = listed[i].iter().map(|(name, text, lit)| {
                div()
                    .relative()
                    .h(px(ROW))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(title(name).to_string())
                    .child(div().text_color(rgb(if *lit { BLUE } else { MUTED })).child(text.clone()))
                    .child(pin(false, tied[i].iter().any(|t| t.0 == *name)))
                    .child(pin(true, fed.contains(&(i, name))))
            });
            let rows: Vec<Div> = rows.collect();
            let none = div()
                .h(px(ROW))
                .px(px(12.))
                .flex()
                .items_center()
                .text_color(rgb(DIM))
                .child("no connections");
            let head = div()
                .h(px(HEAD))
                .px(px(8.))
                .flex()
                .items_center()
                .gap_2()
                .border_b_1()
                .border_color(rgb(BORDER))
                .overflow_hidden()
                .child(div().size(px(10.)).flex_none().rounded(px(2.)).bg(rgb(tint(c.kind))))
                .child(div().text_color(rgb(TEXT)).child(c.label.clone()))
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(DIM))
                        .child(format!("{:?} \u{b7} line {}", c.kind, c.home).to_lowercase()),
                );
            div()
                .absolute()
                .left(px(spots[i].0 - dx))
                .top(px(spots[i].1 - dy))
                .w(px(WIDE))
                .rounded(px(5.))
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(if on { BLUE } else { BORDER }))
                .text_color(rgb(MUTED))
                .whitespace_nowrap()
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, Self::tap(cx, move |this, _, _| this.visit(z)))
                .child(head)
                .children(rows.is_empty().then_some(none))
                .children(rows)
        });
        let nodes: Vec<Div> = nodes.collect();
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .text_size(px(11.))
            .child(canvas(|_, _, _| (), draw).absolute().size_full())
            .children(nodes)
            .children(badges)
            .child(
                div()
                    .absolute()
                    .top(px(6.))
                    .right(px(12.))
                    .text_color(rgb(DIM))
                    .child("read-only \u{b7} drawn from the script"),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use origami_lang::Kind;

    // In the Graph tab, a clip that follows another should be drawn one column to its right, so
    // connections read left to right, clips in a loop of follows should go to the last column and a
    // chain of 2,000 clips should be laid out at once.
    #[test]
    fn lays_out_by_depth() {
        let src = "const shot = color(\"#203040\").for(6)
const title = text(\"t\").pos(0.5, keys([0, 0.5], [1, 0.4]))
const tag = text(\"g\").pos(title.pos.x, add(title.pos.y, -0.06))
const dot = rect(8, 8).pos(tag.pos).opacity(mul(title.opacity, 0.5))
stack(shot, title, text(\"loose\").scale(title.scale), tag, dot)";
        let clips = [(0, Kind::Color), (1, Kind::Text), (4, Kind::Text), (2, Kind::Text), (3, Kind::Rect)];
        let from = |row: usize| src.lines().take(row).map(|l| l.len() + 1).sum::<usize>();
        let names: Vec<Option<String>> = clips
            .iter()
            .map(|c| crate::motion::name(src, from(c.0), c.1).map(String::from))
            .collect();
        let tied: Vec<_> = clips.iter().map(|c| ties(src, from(c.0), c.1)).collect();
        assert_eq!(names[..2], [Some("shot".to_string()), Some("title".to_string())]);
        assert_eq!((&names[2], tied[3].len(), tied[4].len()), (&None, 2, 3));
        let deep = depths(&names, &tied);
        assert_eq!(deep, [0, 0, 1, 1, 2]);
        let homes: Vec<u32> = clips.iter().map(|c| c.0 as u32 + 1).collect();
        let at = seats(&deep, &homes, &[46., 68., 46., 68., 90.]);
        let col = |n: f32| PAD + n * (WIDE + GAP.0);
        assert_eq!(at[..2], [(col(0.), PAD), (col(0.), PAD + 46. + GAP.1)]);
        assert_eq!(at[2..], [(col(1.), PAD + 68. + GAP.1), (col(1.), PAD), (col(2.), PAD)]);
        let own = vec![vec![(
            "x",
            Link {
                clip: "a".into(),
                attr: "x",
                wire: None,
            },
        )]];
        assert_eq!(depths(&[Some("a".into())], &own), [0]);
        let tie = |c: &str| {
            (
                "x",
                Link {
                    clip: c.into(),
                    attr: "x",
                    wire: None,
                },
            )
        };
        let names: Vec<Option<String>> = (0..2000).map(|i| Some(format!("c{i}"))).collect();
        let ring = [vec![tie("c1")], vec![tie("c0")], vec![tie("c0"), tie("c9")], vec![]];
        assert_eq!(depths(&names[..4], &ring), [4, 4, 4, 0]);
        let chain: Vec<_> = (0..2000).map(|i| vec![tie(&format!("c{}", i + 1))]).collect();
        let start = std::time::Instant::now();
        assert_eq!(depths(&names, &chain), (0..2000).rev().collect::<Vec<_>>());
        assert!(start.elapsed().as_secs() < 1);
    }

    // The Graph tab, the follow menu and going to a followed clip's source should take time that
    // grows with the clips, not with clips times script, on 20,000 clips that each follow the one
    // above, including clips written to follow another's whole position.
    #[gpui::test]
    fn many_clips_graph_quickly(cx: &mut gpui::TestAppContext) {
        let path = std::env::temp_dir().join("origami-graph-test.js");
        std::fs::write(&path, "text('t')").unwrap();
        let mut tl = origami_lang::eval(&path, false, origami_engine::footprint).unwrap();
        let (view, cx) = cx.add_window_view(|_, cx| crate::fresh(cx.focus_handle(), vec![]));
        cx.update(|window, cx| {
            view.update(cx, |o, cx| {
                o.lines = (0..20000)
                    .map(|i| format!("const c{} = text('t').pos(c{i}.pos).opacity(c{i}.opacity)", i + 1))
                    .collect();
                let each = |home| origami_lang::Clip {
                    home,
                    z: home,
                    ..tl.clips[0].clone()
                };
                tl.clips = (1..=20000).map(each).collect();
                (o.tl, o.tie, o.motion, o.menu) = (std::sync::Arc::new(tl), Some((20000, "opacity")), true, "follow");
                let start = std::time::Instant::now();
                o.web((800., 600.), cx);
                let graph = start.elapsed().as_secs_f32();
                assert!(o.wires(window, cx).is_some());
                let menu = start.elapsed().as_secs_f32() - graph;
                o.trace((20000, "opacity"));
                let source = start.elapsed().as_secs_f32() - graph - menu;
                assert_eq!(o.sel, Some(19999));
                let most = if cfg!(debug_assertions) { 10. } else { 1. };
                assert!([graph, menu, source].iter().all(|&t| t < most), "{graph} {menu} {source}");
            })
        });
    }
}
