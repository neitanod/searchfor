//! --within and the reshaping of a file's items after filtering (spec §8, grill #4).
use crate::model::{ContextLine, Described, Item, Trail};

/// "Install › Linux", "Install > Linux" and "Install/Linux" → ["install", "linux"].
pub fn parse_within(s: &str) -> Vec<String> {
    s.split(['›', '>', '/'])
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect()
}

/// True when the Segment names contain every wanted name, in order, each as a
/// case-insensitive substring; gaps are allowed ("install › bash" matches
/// Install › Linux › bash).
pub fn within(trail: &Trail, wanted: &[String]) -> bool {
    let mut it = trail.iter();
    wanted
        .iter()
        .all(|w| it.any(|s| s.name.to_lowercase().contains(w.as_str())))
}

/// `retain` for an object file's rows (M4 plan, D8): a row keeps only its kept Matches and
/// their Trails, and a row with none kept is dropped (code review of the M4 plan r1, Codex #7).
/// Rows are strings, not lines, so none becomes a context line. With `max`, the first `max`
/// rows that stay: rows share their lines, so -m counts rows (Codex #8).
pub fn retain_rows(d: &mut Described, keep: &[Vec<bool>], max: Option<u64>) {
    let items = std::mem::take(&mut d.file.items);
    let trails = std::mem::take(&mut d.trails);
    let max = max.map_or(usize::MAX, |m| usize::try_from(m).unwrap_or(usize::MAX));
    for ((item, ts), ks) in items.into_iter().zip(trails).zip(keep) {
        if d.file.items.len() == max {
            break;
        }
        let Item::Match(mut m) = item else { continue };
        if !ks.iter().any(|&b| b) {
            continue;
        }
        debug_assert_eq!(ks.len(), m.submatches.len());
        m.submatches = m
            .submatches
            .into_iter()
            .zip(ks)
            .filter_map(|(s, &k)| k.then_some(s))
            .collect();
        let ts = ts
            .into_iter()
            .zip(ks)
            .filter_map(|(t, &k)| k.then_some(t))
            .collect();
        d.file.items.push(Item::Match(m));
        d.trails.push(ts);
    }
}

/// Applies per-submatch decisions. `keep[i][k]` is for submatch `k` of Match item `i`
/// (one entry for an inverted Match). A Match keeps only its kept submatches and their Trails;
/// a Match with none kept becomes a Context line when it lies within `before`/`after` lines of
/// a kept Match, and is dropped otherwise. Context lines stay only near a kept Match.
/// `stats.binary_cut` is moved so it still counts the items found before the binary data.
pub fn retain(d: &mut Described, keep: &[Vec<bool>], before: usize, after: usize) {
    debug_assert_eq!(keep.len(), d.trails.len());
    // Lines of the kept Matches, in file order (items are sorted by line).
    let kept_lines: Vec<u64> = d
        .file
        .matches()
        .zip(keep)
        .filter(|(_, k)| k.iter().any(|&b| b))
        .map(|(m, _)| m.line_number)
        .collect();
    // True when `line` lies within `before` lines above or `after` lines below a kept Match.
    let near = |line: u64| {
        let first = kept_lines.partition_point(|&k| k.saturating_add(after as u64) < line);
        kept_lines
            .get(first)
            .is_some_and(|&k| k <= line.saturating_add(before as u64))
    };

    let items = std::mem::take(&mut d.file.items);
    let mut trails = std::mem::take(&mut d.trails).into_iter();
    // Sections already set (--in-headings with --expand) follow their Trails.
    let mut sections = std::mem::take(&mut d.sections).into_iter();
    let mut new_sections = Vec::new();
    let cut = d.file.stats.binary_cut;
    let mut new_cut = cut.map(|_| 0);
    let mut new_items = Vec::with_capacity(items.len());
    let mut new_trails = Vec::with_capacity(kept_lines.len());
    let mut i = 0;
    for (n, item) in items.into_iter().enumerate() {
        let kept = match item {
            Item::Match(mut m) => {
                let ks = &keep[i];
                let ts = trails.next().unwrap_or_default();
                let ss = sections.next();
                i += 1;
                if ks.iter().any(|&b| b) {
                    if let Some(ss) = ss {
                        new_sections.push(if m.submatches.is_empty() {
                            ss
                        } else {
                            ss.into_iter()
                                .zip(ks)
                                .filter_map(|(s, &k)| k.then_some(s))
                                .collect()
                        });
                    }
                    if m.submatches.is_empty() {
                        new_trails.push(ts);
                    } else {
                        debug_assert_eq!(ks.len(), m.submatches.len());
                        m.submatches = m
                            .submatches
                            .into_iter()
                            .zip(ks)
                            .filter_map(|(s, &k)| k.then_some(s))
                            .collect();
                        new_trails.push(
                            ts.into_iter()
                                .zip(ks)
                                .filter_map(|(t, &k)| k.then_some(t))
                                .collect(),
                        );
                    }
                    Some(Item::Match(m))
                } else if near(m.line_number) {
                    Some(Item::Context(ContextLine {
                        submatches: vec![],
                        ..m
                    }))
                } else {
                    None
                }
            }
            Item::Context(c) => near(c.line_number).then_some(Item::Context(c)),
        };
        if let Some(item) = kept {
            if let (Some(old), Some(new)) = (cut, new_cut.as_mut())
                && n < old
            {
                *new += 1;
            }
            new_items.push(item);
        }
    }
    d.file.items = new_items;
    d.trails = new_trails;
    d.sections = new_sections;
    d.file.stats.binary_cut = new_cut;
}

/// -m after a filter, as ripgrep 14.1.1 counts it (see `search::MaxCount`): the first `max`
/// Match lines, then the `after` lines after the last of them, where a Match stays a Match
/// without extending the window (code review r2, Fable #5). Later Matches are dropped, or become
/// context lines within `before`/`after` lines of a kept one (as `retain`).
pub fn limit(d: &mut Described, max: u64, before: usize, after: usize) {
    let lines: Vec<u64> = d.file.matches().map(|m| m.line_number).collect();
    let Some(&last) = usize::try_from(max)
        .ok()
        .and_then(|m| m.checked_sub(1))
        .and_then(|i| lines.get(i))
    else {
        return;
    };
    let end = last.saturating_add(after as u64);
    let keep: Vec<Vec<bool>> = d
        .file
        .matches()
        .enumerate()
        .map(|(i, m)| vec![(i as u64) < max || m.line_number <= end; m.submatches.len().max(1)])
        .collect();
    retain(d, &keep, before, after);
    // Nothing after the window: the search stopped there.
    let cut = d
        .file
        .items
        .iter()
        .position(|i| match i {
            Item::Match(l) | Item::Context(l) => l.line_number > end,
        })
        .unwrap_or(d.file.items.len());
    d.file.items.truncate(cut);
    let matches = d.file.matches().count();
    d.trails.truncate(matches);
    d.sections.truncate(matches);
    if let Some(c) = d.file.stats.binary_cut.as_mut() {
        *c = (*c).min(cut);
    }
}

/// --expand: per submatch, the Section of the last Segment of its Trail, or None for an
/// empty Trail. Aligned with `d.trails`.
pub fn sections(d: &Described) -> Vec<Vec<Option<(u64, u64)>>> {
    d.trails
        .iter()
        .map(|ts| {
            ts.iter()
                .map(|t| t.last().map(|s| (s.line, s.end_line)))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileMatches, FileStats, Input, LineMatch, Segment, Submatch};
    use crate::processors::Processor;
    use crate::processors::markdown::Markdown;
    use std::path::{Path, PathBuf};

    fn seg(kind: &str, name: &str, line: u64, end_line: u64) -> Segment {
        Segment {
            kind: kind.into(),
            name: name.into(),
            line,
            end_line,
            level: None,
            signature: None,
        }
    }

    fn trail(names: &[&str]) -> Trail {
        names.iter().map(|n| seg("heading", n, 1, 1)).collect()
    }

    fn wanted(s: &str) -> Vec<String> {
        parse_within(s)
    }

    /// A Match on `line` with one submatch per `(start, end)`.
    fn m(line: u64, subs: &[(usize, usize)]) -> Item {
        Item::Match(LineMatch {
            submatches: subs
                .iter()
                .map(|&(start, end)| Submatch { start, end })
                .collect(),
            ..LineMatch::owned(line, line * 100, format!("line {line}\n").as_bytes())
        })
    }

    fn c(line: u64) -> Item {
        Item::Context(ContextLine::owned(
            line,
            line * 100,
            format!("line {line}\n").as_bytes(),
        ))
    }

    /// Items with one Trail per submatch (one for a Match without submatches), each named
    /// after its line and submatch index.
    fn described(items: Vec<Item>) -> Described {
        let trails = items
            .iter()
            .filter_map(|i| match i {
                Item::Match(m) => Some(
                    (0..m.submatches.len().max(1))
                        .map(|k| vec![seg("heading", &format!("L{}#{k}", m.line_number), 1, 1)])
                        .collect(),
                ),
                Item::Context(_) => None,
            })
            .collect();
        Described {
            file: FileMatches {
                path: PathBuf::from("f.md"),
                explicit: true,
                items,
                stats: FileStats::default(),
            },
            format: Some("markdown".into()),
            trails,
            sections: vec![],
            offsets: false,
            replacements: Vec::new(),
        }
    }

    /// Each item as "m5" (Match on line 5) or "c6" (Context on line 6).
    fn shape(d: &Described) -> Vec<String> {
        d.file
            .items
            .iter()
            .map(|i| match i {
                Item::Match(m) => format!("m{}", m.line_number),
                Item::Context(c) => format!("c{}", c.line_number),
            })
            .collect()
    }

    fn trail_names(d: &Described) -> Vec<Vec<String>> {
        d.trails
            .iter()
            .map(|ts| ts.iter().map(|t| t[0].name.clone()).collect())
            .collect()
    }

    #[test]
    fn parse_within_splits_on_the_three_separators() {
        let expected = vec!["install".to_string(), "linux".to_string()];
        assert_eq!(parse_within("Install › Linux"), expected);
        assert_eq!(parse_within("Install > Linux"), expected);
        assert_eq!(parse_within("Install/Linux"), expected);
        assert_eq!(parse_within("  Install  ››  Linux /  "), expected);
        assert_eq!(
            parse_within("Install › Linux > bash/zsh"),
            vec!["install", "linux", "bash", "zsh"]
        );
        assert!(parse_within(" › / ").is_empty());
    }

    #[test]
    fn within_matches_in_order_with_gaps_and_ignores_case() {
        let t = trail(&["Deploy guide", "Install", "Linux", "bash"]);
        assert!(within(&t, &wanted("install › bash")));
        assert!(within(&t, &wanted("INST > lin")));
        assert!(within(&t, &wanted("guide/Install/Linux/BASH")));
        assert!(!within(&t, &wanted("bash › install")));
        assert!(!within(&t, &wanted("install › windows")));
        // One Segment satisfies one wanted name only.
        assert!(!within(&trail(&["Install"]), &wanted("install › install")));
        assert!(within(&t, &[]));
        assert!(!within(&Vec::new(), &wanted("install")));
    }

    #[test]
    fn within_compares_unicode_case_insensitively() {
        assert!(within(&trail(&["ÁRBOL Grande"]), &wanted("árbol")));
    }

    #[test]
    fn dropped_match_inside_after_becomes_context() {
        let mut d = described(vec![m(5, &[(0, 1)]), m(6, &[(0, 1)])]);
        retain(&mut d, &[vec![true], vec![false]], 0, 1);
        assert_eq!(shape(&d), vec!["m5", "c6"]);
        assert_eq!(trail_names(&d), vec![vec!["L5#0"]]);
        let Item::Context(ctx) = &d.file.items[1] else {
            unreachable!()
        };
        assert_eq!(ctx, &ContextLine::owned(6, 600, b"line 6\n"));
    }

    #[test]
    fn dropped_match_outside_after_is_dropped() {
        let mut d = described(vec![m(5, &[(0, 1)]), m(6, &[(0, 1)])]);
        retain(&mut d, &[vec![true], vec![false]], 0, 0);
        assert_eq!(shape(&d), vec!["m5"]);
        assert_eq!(trail_names(&d), vec![vec!["L5#0"]]);
    }

    #[test]
    fn dropped_match_inside_before_becomes_context() {
        let mut d = described(vec![m(5, &[(0, 1)]), m(6, &[(0, 1)])]);
        retain(&mut d, &[vec![false], vec![true]], 1, 0);
        assert_eq!(shape(&d), vec!["c5", "m6"]);
        assert_eq!(trail_names(&d), vec![vec!["L6#0"]]);
        let mut d = described(vec![m(5, &[(0, 1)]), m(6, &[(0, 1)])]);
        retain(&mut d, &[vec![false], vec![true]], 0, 1);
        assert_eq!(shape(&d), vec!["m6"]);
    }

    #[test]
    fn context_of_a_dropped_match_goes_with_it() {
        let mut d = described(vec![
            c(1),
            m(2, &[(0, 1)]),
            c(3),
            c(9),
            m(10, &[(0, 1)]),
            c(11),
            c(19),
            m(20, &[(0, 1)]),
            c(21),
        ]);
        retain(&mut d, &[vec![true], vec![false], vec![true]], 1, 1);
        assert_eq!(shape(&d), vec!["c1", "m2", "c3", "c19", "m20", "c21"]);
        assert_eq!(trail_names(&d), vec![vec!["L2#0"], vec!["L20#0"]]);
    }

    #[test]
    fn nothing_kept_leaves_nothing() {
        let mut d = described(vec![c(1), m(2, &[(0, 1)]), c(3)]);
        retain(&mut d, &[vec![false]], 1, 1);
        assert!(d.file.items.is_empty());
        assert!(d.trails.is_empty());
    }

    #[test]
    fn a_match_keeps_only_its_kept_submatches() {
        let mut d = described(vec![m(4, &[(2, 5), (9, 12)])]);
        retain(&mut d, &[vec![false, true]], 0, 0);
        let Item::Match(lm) = &d.file.items[0] else {
            unreachable!()
        };
        assert_eq!(lm.submatches, vec![Submatch { start: 9, end: 12 }]);
        assert_eq!(trail_names(&d), vec![vec!["L4#1"]]);
    }

    #[test]
    fn a_kept_inverted_match_stays_whole() {
        let mut d = described(vec![m(3, &[]), m(4, &[]), m(7, &[])]);
        retain(&mut d, &[vec![true], vec![false], vec![false]], 0, 1);
        assert_eq!(shape(&d), vec!["m3", "c4"]);
        assert_eq!(trail_names(&d), vec![vec!["L3#0"]]);
        let Item::Match(lm) = &d.file.items[0] else {
            unreachable!()
        };
        assert!(lm.submatches.is_empty());
    }

    #[test]
    fn context_submatches_of_kept_neighbours_survive() {
        // With -v, context lines are the matching lines and carry their submatches.
        let mut d = described(vec![m(1, &[]), c(2)]);
        if let Item::Context(ctx) = &mut d.file.items[1] {
            ctx.submatches = vec![Submatch { start: 0, end: 4 }];
        }
        let before = d.file.items[1].clone();
        retain(&mut d, &[vec![true]], 0, 1);
        assert_eq!(d.file.items[1], before);
    }

    #[test]
    fn limit_keeps_the_first_lines() {
        let mut d = described(vec![
            m(1, &[(0, 1)]),
            m(2, &[(0, 1), (2, 3)]),
            m(3, &[]),
            m(9, &[]),
        ]);
        limit(&mut d, 2, 0, 1);
        // Line 3 is in the after context of line 2: a Match, as rg 14.1.1 prints it.
        assert_eq!(shape(&d), vec!["m1", "m2", "m3"]);
        assert_eq!(
            trail_names(&d),
            vec![vec!["L1#0"], vec!["L2#0", "L2#1"], vec!["L3#0"]]
        );
    }

    #[test]
    fn a_match_in_the_window_does_not_extend_it() {
        let mut d = described(vec![m(1, &[]), m(2, &[]), m(3, &[]), m(4, &[]), c(5)]);
        limit(&mut d, 1, 0, 2);
        assert_eq!(shape(&d), vec!["m1", "m2", "m3"]);
        let mut d = described(vec![m(1, &[]), c(2), m(4, &[])]);
        limit(&mut d, 1, 0, 1);
        assert_eq!(shape(&d), vec!["m1", "c2"]);
    }

    #[test]
    fn binary_cut_counts_the_surviving_items() {
        // Cut after the 4th item (lines 1..=10): two of those four are dropped.
        let mut d = described(vec![
            m(1, &[(0, 1)]),
            c(9),
            m(10, &[(0, 1)]),
            m(30, &[(0, 1)]),
            m(31, &[(0, 1)]),
        ]);
        d.file.stats.binary_cut = Some(4);
        retain(
            &mut d,
            &[vec![false], vec![true], vec![true], vec![true]],
            0,
            0,
        );
        assert_eq!(shape(&d), vec!["m10", "m30", "m31"]);
        assert_eq!(d.file.stats.binary_cut, Some(2));
    }

    #[test]
    fn sections_follow_the_last_segment_or_none() {
        let mut d = described(vec![m(1, &[(0, 1), (2, 3)]), m(2, &[])]);
        d.trails = vec![
            vec![
                vec![seg("heading", "A", 1, 40), seg("column", "Price", 21, 24)],
                vec![],
            ],
            vec![vec![seg("heading", "A", 1, 40)]],
        ];
        assert_eq!(
            sections(&d),
            vec![vec![Some((21, 24)), None], vec![Some((1, 40))]]
        );
    }

    /// Codex r2 #4: `-e '\| Pro' -e needle-10` on line 23 of the fixture gives two submatches
    /// whose Trails end in different Segments, so they have different Sections.
    #[test]
    fn sections_per_submatch_on_the_fixture() {
        let guide = include_str!("../tests/fixtures/md/guide.md");
        let line_start = guide.match_indices('\n').nth(21).unwrap().0 + 1;
        let line = guide[line_start..].split_inclusive('\n').next().unwrap();
        assert_eq!(line, "| Pro  | needle-10 |\n");
        let needle = line.find("needle-10").unwrap();
        let lm = LineMatch {
            line_number: 23,
            absolute_offset: line_start as u64,
            len: line.len(),
            owned: None,
            submatches: vec![
                Submatch { start: 0, end: 5 },
                Submatch {
                    start: needle,
                    end: needle + 9,
                },
            ],
        };
        let input = Input {
            bytes: guide.as_bytes(),
            path: Path::new("guide.md"),
            format: "markdown",
            disk: None,
        };
        let trails = Markdown
            .trails(&input, &toml::Table::new(), &lm.anchors())
            .unwrap();
        let last: Vec<String> = trails
            .iter()
            .map(|t| {
                let s = t.last().unwrap();
                format!("{}:{}", s.kind, s.name)
            })
            .collect();
        assert_eq!(last, vec!["heading:Plans", "column:Price"]);
        let mut d = described(vec![Item::Match(lm)]);
        d.trails = vec![trails];
        assert_eq!(sections(&d), vec![vec![Some((19, 34)), Some((21, 24))]]);
    }

    #[test]
    fn retain_rows_keeps_rows_with_a_kept_match_and_counts_m_by_row() {
        // Object rows (M4 plan, D8): strings that share line 1, never Context lines.
        let rows = || {
            described(vec![
                m(1, &[(0, 1), (2, 3)]),
                m(1, &[(4, 5)]),
                m(1, &[(6, 7), (8, 9)]),
                m(1, &[(10, 11)]),
            ])
        };
        let keep = vec![vec![false, true], vec![false], vec![true, true], vec![true]];
        let mut d = rows();
        retain_rows(&mut d, &keep, None);
        assert_eq!(shape(&d), vec!["m1", "m1", "m1"]);
        assert_eq!(
            trail_names(&d),
            vec![vec!["L1#1"], vec!["L1#0", "L1#1"], vec!["L1#0"]]
        );
        let Item::Match(first) = &d.file.items[0] else {
            panic!("a row is a Match")
        };
        assert_eq!(first.submatches, vec![Submatch { start: 2, end: 3 }]);
        let mut d = rows();
        retain_rows(&mut d, &keep, Some(2));
        assert_eq!(shape(&d), vec!["m1", "m1"]);
        assert_eq!(trail_names(&d), vec![vec!["L1#1"], vec!["L1#0", "L1#1"]]);
    }
}
