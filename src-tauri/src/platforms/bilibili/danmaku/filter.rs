//! Filters over the list of floating comments.
//!
//! Three cuts, all optional: by text (loose word or regular expression), by type
//! (scrolling, top, bottom) and by density.
//!
//! The density cut is what saves the video. On a popular premiere more than two
//! hundred comments a second go by and the screen turns into alphabet soup.
//! Within each second the comments are ordered by `weight` — the score Bilibili
//! itself uses in its own smart filter — and only the first N stay. With equal
//! weights the ordering is stable, so whoever arrived first survives and the
//! result does not change between runs.

use anyhow::anyhow;
use regex::Regex;
use serde::Deserialize;

use super::proto::DanmakuElem;

pub const KIND_SCROLL: &str = "scroll";
pub const KIND_TOP: &str = "top";
pub const KIND_BOTTOM: &str = "bottom";
pub const KIND_OTHER: &str = "other";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DanmakuFilter {
    /// Snippet the comment must contain, case-insensitive.
    pub keyword: String,
    /// Regular expression; when filled in, it replaces the word search.
    pub regex: String,
    /// Inverts the search: what matches is discarded instead of kept.
    pub invert: bool,
    /// Accepted types (`scroll`, `top`, `bottom`, `other`). Empty = all.
    pub kinds: Vec<String>,
    /// Ceiling of comments per second; 0 = no ceiling.
    pub max_per_second: u32,
}

/// Bilibili's `mode`: 1-3 scroll right to left, 4 stays stuck at the bottom, 5
/// at the top. The rest (6 reverse, 7 positioned, 8 script) is rare and never
/// even gets drawn in the ASS, but still counts in the XML and JSON.
pub fn kind_of(mode: i32) -> &'static str {
    match mode {
        1..=3 => KIND_SCROLL,
        4 => KIND_BOTTOM,
        5 => KIND_TOP,
        _ => KIND_OTHER,
    }
}

pub fn apply(elems: &[DanmakuElem], f: &DanmakuFilter) -> anyhow::Result<Vec<DanmakuElem>> {
    let pattern = f.regex.trim();
    let re = if pattern.is_empty() {
        None
    } else {
        Some(Regex::new(pattern).map_err(|e| anyhow!("invalid regular expression: {}", e))?)
    };
    let needle = f.keyword.trim().to_lowercase();
    let kinds: Vec<&str> = f
        .kinds
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .collect();

    let mut kept: Vec<DanmakuElem> = Vec::with_capacity(elems.len());
    for e in elems {
        if !kinds.is_empty() && !kinds.contains(&kind_of(e.mode)) {
            continue;
        }
        let matched = match re.as_ref() {
            Some(r) => r.is_match(&e.content),
            None => {
                if needle.is_empty() {
                    // With no search term there is nothing to invert: everyone passes.
                    kept.push(e.clone());
                    continue;
                }
                e.content.to_lowercase().contains(&needle)
            }
        };
        if matched != f.invert {
            kept.push(e.clone());
        }
    }
    Ok(cap_density(kept, f.max_per_second))
}

fn cap_density(mut elems: Vec<DanmakuElem>, max_per_second: u32) -> Vec<DanmakuElem> {
    if max_per_second == 0 || elems.is_empty() {
        return elems;
    }
    elems.sort_by_key(|e| e.progress_ms);
    let mut out: Vec<DanmakuElem> = Vec::with_capacity(elems.len());
    let mut bucket: Vec<DanmakuElem> = Vec::new();
    let mut current = elems[0].progress_ms.div_euclid(1000);
    for e in elems {
        let sec = e.progress_ms.div_euclid(1000);
        if sec != current {
            flush_bucket(&mut bucket, max_per_second, &mut out);
            current = sec;
        }
        bucket.push(e);
    }
    flush_bucket(&mut bucket, max_per_second, &mut out);
    out
}

fn flush_bucket(bucket: &mut Vec<DanmakuElem>, max: u32, out: &mut Vec<DanmakuElem>) {
    if bucket.len() > max as usize {
        bucket.sort_by_key(|e| std::cmp::Reverse(e.weight));
        bucket.truncate(max as usize);
        bucket.sort_by_key(|e| e.progress_ms);
    }
    out.append(bucket);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elem(progress_ms: i32, mode: i32, content: &str) -> DanmakuElem {
        DanmakuElem {
            progress_ms,
            mode,
            content: content.to_string(),
            ..DanmakuElem::default()
        }
    }

    fn weighted(progress_ms: i32, weight: i32, content: &str) -> DanmakuElem {
        DanmakuElem {
            progress_ms,
            mode: 1,
            weight,
            content: content.to_string(),
            ..DanmakuElem::default()
        }
    }

    fn sample() -> Vec<DanmakuElem> {
        vec![
            elem(0, 1, "primeiro rolante"),
            elem(500, 5, "fixo no topo"),
            elem(1200, 4, "fixo no rodapé"),
            elem(1800, 1, "OUTRO Rolante"),
            elem(2500, 7, "posicionado"),
        ]
    }

    #[test]
    fn empty_filter_keeps_everything() {
        let kept = apply(&sample(), &DanmakuFilter::default()).unwrap();
        assert_eq!(kept.len(), 5);
    }

    #[test]
    fn keyword_ignores_case() {
        let f = DanmakuFilter {
            keyword: "rolante".into(),
            ..DanmakuFilter::default()
        };
        let kept = apply(&sample(), &f).unwrap();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[1].content, "OUTRO Rolante");
    }

    #[test]
    fn invert_drops_what_matches() {
        let f = DanmakuFilter {
            keyword: "rolante".into(),
            invert: true,
            ..DanmakuFilter::default()
        };
        let kept = apply(&sample(), &f).unwrap();
        assert_eq!(kept.len(), 3);
        assert!(!kept
            .iter()
            .any(|e| e.content.to_lowercase().contains("rolante")));
    }

    #[test]
    fn regex_beats_keyword_and_bad_pattern_fails_loud() {
        let f = DanmakuFilter {
            keyword: "nada a ver".into(),
            regex: r"^fixo".into(),
            ..DanmakuFilter::default()
        };
        let kept = apply(&sample(), &f).unwrap();
        assert_eq!(kept.len(), 2);

        let bad = DanmakuFilter {
            regex: "[".into(),
            ..DanmakuFilter::default()
        };
        assert!(apply(&sample(), &bad).is_err());
    }

    #[test]
    fn kinds_map_to_the_bilibili_modes() {
        assert_eq!(kind_of(1), KIND_SCROLL);
        assert_eq!(kind_of(3), KIND_SCROLL);
        assert_eq!(kind_of(4), KIND_BOTTOM);
        assert_eq!(kind_of(5), KIND_TOP);
        assert_eq!(kind_of(7), KIND_OTHER);

        let f = DanmakuFilter {
            kinds: vec![KIND_TOP.into(), KIND_BOTTOM.into()],
            ..DanmakuFilter::default()
        };
        let kept = apply(&sample(), &f).unwrap();
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|e| e.mode == 4 || e.mode == 5));
    }

    #[test]
    fn density_cap_keeps_the_heaviest_of_each_second() {
        let elems = vec![
            weighted(100, 1, "a"),
            weighted(200, 9, "b"),
            weighted(300, 5, "c"),
            weighted(1100, 1, "d"),
            weighted(1200, 2, "e"),
            weighted(2400, 0, "f"),
        ];
        let f = DanmakuFilter {
            max_per_second: 2,
            ..DanmakuFilter::default()
        };
        let kept = apply(&elems, &f).unwrap();
        let texts: Vec<&str> = kept.iter().map(|e| e.content.as_str()).collect();
        // Second 0: weights 9 and 5 remain, back in time order.
        // Seconds 1 and 2 already fit the ceiling.
        assert_eq!(texts, vec!["b", "c", "d", "e", "f"]);
    }

    #[test]
    fn density_cap_is_stable_when_weights_tie() {
        let elems: Vec<DanmakuElem> = (0..10)
            .map(|i| weighted(i * 50, 0, &format!("n{}", i)))
            .collect();
        let f = DanmakuFilter {
            max_per_second: 3,
            ..DanmakuFilter::default()
        };
        let kept = apply(&elems, &f).unwrap();
        let texts: Vec<&str> = kept.iter().map(|e| e.content.as_str()).collect();
        assert_eq!(texts, vec!["n0", "n1", "n2"]);
    }

    #[test]
    fn density_cap_zero_means_no_cap() {
        let elems: Vec<DanmakuElem> = (0..50).map(|i| weighted(i, 0, "x")).collect();
        let kept = apply(&elems, &DanmakuFilter::default()).unwrap();
        assert_eq!(kept.len(), 50);
    }
}
