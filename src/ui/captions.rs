//! Helpers for the captions menu (translation languages, saved tracks).

use crate::*;

/// Languages offered for auto-translated captions.
pub(crate) const TRANSLATE_TO: &[(&str, &str)] = &[
    ("en", "English"), ("es", "Spanish"), ("fr", "French"), ("de", "German"), ("pt", "Portuguese"),
    ("it", "Italian"), ("ru", "Russian"), ("ja", "Japanese"), ("ko", "Korean"), ("zh-Hans", "Chinese"),
    ("ar", "Arabic"), ("hi", "Hindi"), ("sw", "Swahili"),
];

/// Saved caption tracks cannot be translated (the translation needs YouTube).
pub(crate) fn is_local_file(tracks: &[yt::CaptionTrack]) -> bool {
    tracks.iter().any(|t| t.url.starts_with("file:") || t.url.starts_with("podtr:"))
}

/// Longest line of a caption (Netflix timed text style guide: 42 characters per line).
pub(crate) const MAX_LINE: usize = 42;
/// Shortest time a caption stays on screen (5/6 of a second).
const MIN_SHOWN: f64 = 5.0 / 6.0;
/// Longest time a caption stays on screen (7 seconds).
const MAX_SHOWN: f64 = 7.0;

/// Words a line may start with (conjunctions and common prepositions).
const CONNECTORS: &[&str] = &["and", "but", "or", "so", "because", "that", "which", "when", "while", "if", "with", "for", "to", "in", "on", "at", "of", "from", "by", "as"];
/// Words that must stay on the line of the noun they belong to.
const ARTICLES: &[&str] = &["a", "an", "the"];

/// Breaks a caption into lines of at most 42 characters, in two lines where possible, with the
/// shorter line on top (a pyramid shape reads best), as the Netflix guide asks.
pub(crate) fn netflix_lines(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let joined = words.join(" ");
    let total = joined.chars().count();
    if total <= MAX_LINE {
        return vec![joined];
    }
    let len = |w: &[&str]| w.join(" ").chars().count();
    // two lines: the split that keeps both within the limit and the lines closest in length,
    // preferring the shorter one on top
    let mut best: Option<(i32, usize)> = None;
    for i in 1..words.len() {
        let (a, b) = (len(&words[..i]), len(&words[i..]));
        if a <= MAX_LINE && b <= MAX_LINE {
            // the Netflix guide breaks after punctuation and before conjunctions or prepositions, and
            // keeps an article with its noun: favour those places
            let mut score = a.abs_diff(b) as i32 + if a > b { 3 } else { 0 };
            let (last, next) = (words[i - 1], words[i].to_lowercase());
            if last.ends_with([',', ';', ':', '.', '?', '!', '—', '-']) {
                score -= 6;
            }
            if CONNECTORS.contains(&next.as_str()) {
                score -= 4;
            }
            if ARTICLES.contains(&last.to_lowercase().as_str()) {
                score += 8;
            }
            if best.is_none_or(|(sc, _)| score < sc) {
                best = Some((score, i));
            }
        }
    }
    if let Some((_, i)) = best {
        return vec![words[..i].join(" "), words[i..].join(" ")];
    }
    // too long for two lines: fill lines greedily (a single very long word keeps its own line)
    let mut lines: Vec<String> = vec![];
    let mut cur = String::new();
    for w in words {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > MAX_LINE {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// The caption to show at `pos`. A very short cue is held for at least 5/6 s (never into the
/// next one), so it can be read.
pub(crate) fn cue_at(cues: &[yt::Cue], pos: f64) -> Option<&yt::Cue> {
    let idx = cues.partition_point(|c| c.start <= pos);
    let cue = cues.get(idx.checked_sub(1)?)?;
    let mut end = cue.end.max(cue.start + MIN_SHOWN).min(cue.start + MAX_SHOWN);
    if let Some(n) = cues.get(idx) {
        end = end.min(n.start.max(cue.end));
    }
    (pos < end).then_some(cue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_captions_stay_on_one_line() {
        assert_eq!(netflix_lines("Hello  there"), vec!["Hello there"]);
    }

    #[test]
    fn long_captions_split_in_two_with_the_shorter_line_on_top() {
        let l = netflix_lines("Well, that was certainly not what anybody expected to happen tonight");
        assert_eq!(l.len(), 2);
        assert!(l.iter().all(|x| x.chars().count() <= MAX_LINE));
        assert!(l[0].chars().count() <= l[1].chars().count());
    }

    #[test]
    fn lines_break_after_punctuation_and_keep_an_article_with_its_noun() {
        let l = netflix_lines("When we finally arrived at the station, the whole place was quiet and empty");
        assert_eq!(l.len(), 2);
        assert!(l[0].ends_with("station,"), "{l:?}");
        let l = netflix_lines("She walked all the way across the old town to find the");
        assert!(l.len() == 1 || !l[0].to_lowercase().ends_with(" the"), "{l:?}");
    }

    #[test]
    fn very_long_captions_use_more_lines_and_keep_every_word() {
        let text = "word ".repeat(40);
        let l = netflix_lines(&text);
        assert!(l.len() >= 3 && l.iter().all(|x| x.chars().count() <= MAX_LINE));
        assert_eq!(l.join(" "), text.trim());
    }

    #[test]
    fn a_short_cue_is_held_long_enough_but_not_into_the_next() {
        let c = |s: f64, e: f64| yt::Cue { start: s, end: e, text: "x".into() };
        let cues = vec![c(1.0, 1.2), c(5.0, 6.0)];
        assert!(cue_at(&cues, 1.6).is_some());
        assert!(cue_at(&cues, 2.0).is_none());
        let long = vec![c(1.0, 30.0)];
        assert!(cue_at(&long, 8.5).is_none() && cue_at(&long, 6.0).is_some());
        let tight = vec![c(1.0, 1.2), c(1.4, 3.0)];
        assert_eq!(cue_at(&tight, 1.45).unwrap().start, 1.4);
    }
}
