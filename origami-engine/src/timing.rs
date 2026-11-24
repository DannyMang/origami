use crate::place::live;
use origami_lang::Clip;

pub fn index(c: &Clip, t: f64, fps: f64, (n, d, tb): (f64, f64, f64), dur: f64) -> i64 {
    let (rate, per) = (n / d, tb * d / n);
    let (step, at, first) = (fps / rate / c.speed, (t - c.start) * fps, (c.src_in * rate - 1e-6).ceil());
    let tail = if dur > 0. { (dur * rate + 1e-3).round() - first } else { f64::MAX };
    let last = ((c.end - c.start) * fps / step - 1e-6).ceil().min(tail) - 1.;
    let i = match tb > 0. && per.fract() == 0. {
        true => {
            let tick = |i: f64| ((i * per / c.speed + c.start / (1. / tb) + 1e-6) as i64 * fps as i64 + tb as i64 / 2) / tb as i64;
            let (k, mut i) = ((t * fps).round() as i64, (at / step - 3.).floor().max(0.));
            while tick(i + 1.) <= k {
                i += 1.;
            }
            while i > 0. && tick(i - 1.) == tick(i) {
                i -= 1.;
            }
            i
        }
        false => {
            let i = ((at - 0.5) / step - 1e-6).ceil().max(0.);
            if i * step < at + 0.5 - 1e-6 { i } else { i - 1. }
        }
    };
    (first + i.min(last).max(0.)) as i64
}

pub fn seam(c: &Clip, t: f64, fps: f64, (n, d, tb): (f64, f64, f64), dur: f64, seq: bool) -> Option<(f64, f64, f64, f64)> {
    let at = |c: &Clip, t: f64| index(c, t, fps, (n, d, tb), dur);
    let shown = |p: &[&Clip], k: i64| p.iter().rev().find(|c| live(c, k as f64 / fps)).map(|c| at(c, k as f64 / fps));
    let (a, v, i, first) = (c.src_in, c.speed, at(c, t), |s: f64| (s * n / d - 1e-6).ceil() as i64);
    let (b, cut) = (a + (c.end - c.start) * v, |f: i64| a + (f - first(a)) as f64 * d / n);
    let span = ((c.start + (cut(i) - 4. * d / n - a) / v) * fps).floor() as i64..(c.end * fps).ceil() as i64 + 2;
    let was: Vec<_> = span.clone().map(|k| shown(&[c], k)).collect();
    let read = |v: f64| serde_json::from_str(&v.to_string()).unwrap_or(v);
    let edge = if tb > 0. { 0.6 / tb } else { 1e-4 };
    let reads = |from: f64, to: f64, j: i64| (j - first(from)) as f64 * d / n + edge <= to - from;
    let tail = was.iter().rev().find_map(|w| *w).filter(|j| reads(a, b, *j));
    let tiers: [&[i64]; 7] = [&[i, i + 1], &[i + 2], &[i - 1], &[i + 3], &[i - 2], &[i + 4], &[i - 3]];
    let order = tiers
        .into_iter()
        .flat_map(|near| (3..7).flat_map(move |p| [true, false].map(|whole| (near, p, whole))));
    for (near, p, whole) in order {
        let k = 10f64.powi(p);
        let reach = if p == 3 { (d / n * k) as i64 + 3 } else { (edge * k) as i64 + 10 };
        let grid = |v: f64| (v * k).round() / k;
        let flat = |v: f64| (grid(v) - v).abs() < 1e-9;
        for &f in near {
            let mid = (cut(f) * k).round() as i64;
            let fit = |mut s: i64| {
                while first(s as f64 / k) > f {
                    s -= 1;
                }
                while first(s as f64 / k) < f {
                    s += 1;
                }
                s
            };
            let mut tries: Vec<_> = (mid - reach..=mid + reach).map(|m| (m, fit(m))).collect();
            tries.sort_by_key(|(m, s)| ((m - s).abs(), (m - mid).abs()));
            for (m, s) in tries.into_iter().map(|(m, s)| (m as f64 / k, s as f64 / k)) {
                let by = if seq { (m - a) / v } else { grid((m - a) / v) };
                let end = s + (b - a) - by * v;
                let end = if flat(end) {
                    grid(end)
                } else if (end - b).abs() < 1e-9 {
                    b
                } else {
                    continue;
                };
                let x = if seq || !flat(c.start + by) {
                    c.start + by
                } else {
                    grid(c.start + by)
                };
                let stop = read(x + (end - s) / v);
                let cuts = first(s - 5e-6) == f && tail.is_none_or(|j| reads(s, end, j));
                if m - a < 0.04 || end - s < 0.04 || (stop == c.end) != whole || !cuts {
                    continue;
                }
                let one = Clip {
                    end: read(c.start + (m - a) / v),
                    ..c.clone()
                };
                let two = Clip {
                    start: read(x),
                    end: stop,
                    src_in: s,
                    ..c.clone()
                };
                let k = ((one.end.min(two.start) - 1e-6) * fps).ceil() as i64 - 1;
                let last = was
                    .get((k - span.start) as usize)
                    .copied()
                    .flatten()
                    .filter(|_| live(&one, k as f64 / fps));
                if last.is_none_or(|j| reads(a, m, j)) && span.clone().zip(&was).all(|(k, w)| shown(&[&one, &two], k) == *w) {
                    return Some((m, s, by, end));
                }
            }
        }
    }
    None
}

pub fn stamp(c: &Clip, t: f64, fps: f64, tb: f64, stamps: &[(f64, bool)]) -> i64 {
    let at = |s: f64| stamps.partition_point(|p| p.0 < s.round());
    let first = at(c.src_in * tb).min(stamps.len() - 1);
    let end = at(stamps[first].0 + (c.end - c.start) * c.speed * tb).max(first + 1);
    let tick = |p: &(f64, bool)| {
        (((p.0 - stamps[first].0) / c.speed + c.start / (1. / tb) + 1e-6) as i64 * fps as i64 + tb as i64 / 2) / tb as i64
    };
    let (k, kept) = ((t * fps).round() as i64, &stamps[first..end]);
    let (before, by) = (kept.partition_point(|p| tick(p) < k), kept.partition_point(|p| tick(p) <= k));
    (first + before.min(by.max(1) - 1)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::clip;

    // Each timeline frame shows the source frame a plain export shows, for 29.97 fps footage on a 30 fps
    // timeline, faster, slower and trimmed clips, last frames held to the clip's end, and frames landing
    // half-way between two timeline frames whatever the file's time base.
    #[test]
    fn frames_match_export() {
        let held = |(start, end, src_in, speed): (f64, f64, f64, f64), fps: f64, rate: (f64, f64, f64), dur: f64, frames: &[i64]| {
            let c = Clip {
                start,
                end,
                src_in,
                speed,
                ..clip("video", "")
            };
            frames
                .iter()
                .map(|n| index(&c, *n as f64 / fps, fps, rate, dur))
                .collect::<Vec<_>>()
        };
        let see = |span, fps, rate, frames: &[i64]| held(span, fps, rate, 0., frames);
        let (film, thirty, day) = ((30000., 1001., 30000.), (30., 1., 15360.), (24., 1., 12288.));
        assert_eq!(
            see((0., 40., 0., 1.), 30., film, &[0, 1, 499, 500, 501, 1199]),
            [0, 1, 499, 499, 500, 1198]
        );
        assert_eq!(see((2., 3.666, 4.334, 1.), 30., film, &[60, 61, 109]), [130, 131, 179]);
        assert_eq!(see((3.666, 4.166, 1.5, 2.), 30., film, &[110, 111, 124]), [45, 47, 73]);
        assert_eq!(
            see((4.166, 6.166, 3., 0.5), 30., film, &[125, 126, 127, 183, 184]),
            [90, 90, 91, 119, 119]
        );
        assert_eq!(
            see((9.426, 13.526, 7.77, 0.3), 30., film, &[400, 402, 403, 405]),
            [268, 268, 269, 269]
        );
        assert_eq!(
            see((13.526, 15.7193, 30.01, 1.5), 30., film, &[406, 407, 408, 409]),
            [900, 902, 903, 905]
        );
        assert_eq!(see((0., 3., 1., 1.), 24., film, &[0, 1, 2, 3, 70, 71]), [30, 31, 32, 34, 117, 119]);
        assert_eq!(see((0., 2., 0.5, 1.), 30., thirty, &[0, 1, 59]), [15, 16, 74]);
        assert_eq!(held((0., 1.99, 4.01, 1.), 30., thirty, 6., &[0, 58, 59]), [121, 179, 179]);
        assert_eq!(see((0., 0.99, 1.01, 1.), 30., thirty, &[0, 28, 29]), [31, 59, 60]);
        assert_eq!(
            held((0.4, 0.606, 5.8, 1.), 30., (24000., 1001., 24000.), 6.006, &[12, 15, 17]),
            [140, 142, 143]
        );
        assert_eq!(held((0., 1., 5.98, 1.), 24., day, 6., &[0, 5]), [144, 144]);
        assert_eq!(see((0.05, 3.05, 0., 1.), 60., day, &[3, 5, 60, 65]), [0, 1, 23, 25]);
        assert_eq!(see((0., 2., 0., 2.), 30., thirty, &[0, 1, 2, 30]), [0, 1, 3, 59]);
        assert_eq!(see((0.05, 3.05, 0., 1.), 60., (24., 1., 0.), &[5, 60]), [0, 22]);
    }

    // Splitting a clip in a cat or a stack leaves every timeline frame showing the source frame it showed
    // before, at common rates, time bases, speeds and trims, with times of at most six decimals, a first
    // piece that still reads its last frame, and only a few cuts near the clip's ends refused.
    #[test]
    fn seams_keep_every_frame() {
        let rates = [
            (30000., 1001., 30000.),
            (30., 1., 15360.),
            (25., 1., 12800.),
            (24., 1., 12288.),
            (60., 1., 15360.),
        ];
        let read = |v: f64| serde_json::from_str::<f64>(&v.to_string()).unwrap();
        let (mut changed, mut longer, mut refused, mut early) = (vec![], vec![], vec![], vec![]);
        for (rate, fps) in rates.into_iter().flat_map(|rate| [24., 25., 30., 60.].map(|fps| (rate, fps))) {
            let shown = |clips: &[Clip], t: f64| clips.iter().rev().find(|c| live(c, t)).map(|c| (c.z, index(c, t, fps, rate, 0.)));
            let (mut bad, mut more, mut no, mut held) = (0, 0, 0, 0);
            for (seq, (speed, src_in, start, len)) in [true, false].into_iter().flat_map(|seq| {
                [
                    (0.5, 0., 0., 3.),
                    (0.5, 0.37, 0.2, 2.),
                    (1., 0., 0., 3.),
                    (1., 0.51, 0.7, 2.9),
                    (1.5, 0., 1.234, 2.),
                    (1.5, 0.2, 0., 2.5),
                    (2., 0.1, 0., 3.),
                    (2., 0., 0.5, 2.),
                    (0.5, 0.1, 0.35, 2.5),
                    (0.5, 0.021, 0.2, 2.5),
                    (1., 0.021, 0.05, 2.5),
                    (1., 0.333, 0.35, 2.5),
                    (2., 0.063, 0.15, 2.5),
                ]
                .map(|case| (seq, case))
            }) {
                let b = src_in + len * speed;
                let c = Clip {
                    start,
                    end: read(start + (b - src_in) / speed),
                    src_in,
                    speed,
                    ..clip("video", "")
                };
                let next = Clip {
                    start: c.end,
                    end: c.end + 1.,
                    z: 1,
                    ..clip("video", "")
                };
                for k in (start * fps).ceil() as usize + 1..(c.end * fps) as usize {
                    let Some((m, s, by, end)) = seam(&c, k as f64 / fps, fps, rate, 0., seq) else {
                        no += 1;
                        continue;
                    };
                    let x = if seq {
                        start + by
                    } else {
                        format!("{:.9}", start + by).parse().unwrap()
                    };
                    let one = Clip {
                        end: read(start + (m - src_in) / speed),
                        ..c.clone()
                    };
                    let two = Clip {
                        start: read(x),
                        end: read(x + (end - s) / speed),
                        src_in: s,
                        ..c.clone()
                    };
                    let after = Clip {
                        start: if seq { two.end } else { c.end },
                        end: next.end,
                        ..next.clone()
                    };
                    let (was, now) = ([next.clone(), c.clone()], [after, one, two]);
                    let same = |j: usize| shown(&now, j as f64 / fps) == shown(&was, j as f64 / fps);
                    assert!(same(k), "{rate:?} {fps} {speed} {k}");
                    bad += (0..(next.end * fps) as usize + 2).filter(|j| !same(*j)).count();
                    let alone = |j: &usize| live(&now[1], *j as f64 / fps) && !live(&now[2], *j as f64 / fps);
                    let lead = (src_in * rate.0 / rate.1 - 1e-6).ceil() as i64;
                    let cut = |j: usize| {
                        (index(&now[1], j as f64 / fps, fps, rate, 0.) - lead) as f64 * rate.1 / rate.0 + 0.6 / rate.2 > m - src_in
                    };
                    held += (0..(next.end * fps) as usize).filter(alone).filter(|j| cut(*j)).count();
                    let written = [m, s, end, if seq { 0. } else { by }];
                    more += written.iter().any(|v| ((v * 1e3).round() / 1e3 - v).abs() > 1e-9) as usize;
                    assert!(written.iter().all(|v| ((v * 1e6).round() / 1e6 - v).abs() < 1e-9), "{written:?}");
                }
            }
            changed.push(bad);
            longer.push(more);
            refused.push(no);
            early.push(held);
        }
        assert_eq!((changed, early), (vec![0; 20], vec![0; 20]));
        assert_eq!(longer, [33, 70, 0, 23, 0, 0, 0, 0, 226, 0, 0, 247, 0, 100, 227, 40, 0, 0, 0, 0]);
        assert_eq!(refused.iter().sum::<usize>(), 76);
    }

    // Variable-frame-rate footage should show the frame a plain export shows: the clip starts on the
    // first frame at or after the trim point, each later frame shows from the timeline frame its time
    // rounds to, and of two frames rounding to the same timeline frame the first one is shown there.
    #[test]
    fn stamps_match_export() {
        let shown = |stamps: &[(f64, bool)], (start, end, src_in, speed): (f64, f64, f64, f64), fps: f64, frames: &[i64]| {
            let c = Clip {
                start,
                end,
                src_in,
                speed,
                ..clip("video", "")
            };
            frames
                .iter()
                .map(|k| stamp(&c, *k as f64 / fps, fps, 15360., stamps))
                .collect::<Vec<_>>()
        };
        let uneven: Vec<_> = (0..180).map(|n| ((n + n / 2) as f64 * 512., n % 24 == 0)).collect();
        let sixty: Vec<_> = (0..480).map(|n| ((n + n / 3 * 2) as f64 * 256., n % 30 == 0)).collect();
        assert_eq!(
            shown(&uneven, (0., 2.7, 0.5, 1.), 30., &[0, 1, 2, 3, 4, 5, 6, 7]),
            [10, 11, 11, 12, 13, 13, 14, 15]
        );
        assert_eq!(shown(&uneven, (0., 1.334, 1., 1.5), 24., &[0, 1, 2, 3, 31]), [20, 21, 22, 24, 59]);
        assert_eq!(shown(&uneven, (0., 0.466667, 8.5, 1.), 30., &[0, 1, 2, 13]), [170, 171, 171, 179]);
        assert_eq!(
            shown(&sixty, (0., 3., 0., 1.), 30., &[0, 1, 2, 3, 4, 5, 6, 7]),
            [0, 1, 2, 3, 5, 6, 7, 8]
        );
        assert_eq!(shown(&sixty, (0., 1.98, 1.01, 0.5), 30., &[0, 1, 2, 3, 4]), [37, 38, 38, 38, 39]);
    }
}
