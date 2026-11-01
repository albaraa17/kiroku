use crate::store::config::{AppConfig, same_game_path};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use std::collections::{HashMap, HashSet};

static RUBY_RT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<rt[^>]*>.*?</rt>|<rp[^>]*>.*?</rp>").unwrap());
static HTML_TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"</?[a-zA-Z][^>\n]*>").unwrap());
static ENGINE_TAG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[(?:r|n|p|l|c|wait|ruby|/|np|cm|er)\]|\[\[[nr]\]\]|%0").unwrap());
static SPEAKER_BRACKET_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^【[^】]{1,24}】\s*").unwrap());
static MULTI_SPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t\u{3000}]{2,}").unwrap());
static MULTI_NEWLINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{2,}").unwrap());

static REGEX_CACHE: Lazy<Mutex<HashMap<String, Option<Regex>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn get_cached_regex(pattern: &str) -> Option<Regex> {
    let mut cache = REGEX_CACHE.lock();
    if let Some(cached) = cache.get(pattern) {
        return cached.clone();
    }
    if cache.len() > 512 {
        cache.clear();
    }
    let compiled = Regex::new(pattern).ok();
    cache.insert(pattern.to_string(), compiled.clone());
    compiled
}

/// Clean a raw sentence from LunaHost or Knife according to the active pipeline configuration.
pub fn clean_sentence(raw: &str, process_path: Option<&str>, config: &AppConfig) -> String {
    // Bound raw input to prevent memory/CPU DoS from runaway hook dumps
    let bounded: &str = if raw.len() > 16_384 {
        let mut end = 16_384;
        while !raw.is_char_boundary(end) {
            end -= 1;
        }
        &raw[..end]
    } else {
        raw
    };

    let mut text = bounded.replace("\r\n", "\n").replace('\r', "\n");

    // Strip unpaired UTF-16 surrogates or NULL chars
    text.retain(|c| c != '\0' && c != '\u{FFFD}');

    if text.trim().is_empty() {
        return String::new();
    }

    // 1. Strip HTML / Ruby / Engine control tags first
    if config.strip_html_ruby_tags {
        text = RUBY_RT_RE.replace_all(&text, "").into_owned();
        text = HTML_TAG_RE.replace_all(&text, "").into_owned();
        text = ENGINE_TAG_RE.replace_all(&text, "").into_owned();
    }

    // 2. Optional speaker bracket removal BEFORE repeat dedup so names like 【凛凛】 match intact
    if config.remove_speaker_brackets {
        text = SPEAKER_BRACKET_RE.replace_all(&text, "").into_owned();
    }

    // 3. Remove repeated characters (Textractor "Remove Repeated Characters" equivalent)
    if config.remove_repeated_chars {
        text = remove_repeated_characters(&text);
    }

    // 4. Remove repeated phrases (Textractor "Remove Repeated Phrases" equivalent)
    if config.remove_repeated_phrases {
        text = remove_repeated_phrases(&text);
    }

    // 5. Apply Regex Filters (matched substrings are removed)
    for rule in &config.regex_filters {
        if !rule.enabled || rule.pattern.is_empty() {
            continue;
        }
        if !matches_process(rule.process_path.as_deref(), process_path) {
            continue;
        }
        if let Some(re) = get_cached_regex(&rule.pattern) {
            text = re.replace_all(&text, "").into_owned();
        }
    }

    // 6. Apply Regex Replacements
    for rule in &config.regex_replacements {
        if !rule.enabled || rule.pattern.is_empty() {
            continue;
        }
        if !matches_process(rule.process_path.as_deref(), process_path) {
            continue;
        }
        let pat = if rule.ignore_case {
            format!("(?i){}", rule.pattern)
        } else {
            rule.pattern.clone()
        };
        if let Some(re) = get_cached_regex(&pat) {
            text = re
                .replace_all(&text, rule.replacement.as_str())
                .into_owned();
        }
    }

    // 7. Apply Literal String Replacements
    for rule in &config.string_replacements {
        if !rule.enabled || rule.original.is_empty() {
            continue;
        }
        text = text.replace(&rule.original, &rule.replacement);
    }

    // 8. Normalize whitespace
    if config.normalize_whitespace {
        text = MULTI_SPACE_RE.replace_all(&text, " ").into_owned();
        text = MULTI_NEWLINE_RE.replace_all(&text, "\n").into_owned();
        text = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
    }

    text.trim().to_string()
}

fn matches_process(rule_proc: Option<&str>, active_proc: Option<&str>) -> bool {
    let Some(rule_p) = rule_proc else {
        return true; // Global rule
    };
    let rule_p = rule_p.trim();
    if rule_p.is_empty() {
        return true;
    }
    let Some(active_p) = active_proc else {
        return false;
    };
    same_game_path(rule_p, active_p)
}

/// Collapses uniform N-tuple character repetition (e.g. "ここんんににちちはは" -> "こんにちは")
/// while preserving intentional single-character lines like "ーーーー" or "っっっっ".
pub fn remove_repeated_characters(input: &str) -> String {
    let mut out_lines = Vec::new();
    for line in input.split('\n') {
        let chars: Vec<char> = line.chars().collect();
        if chars.len() < 4 {
            out_lines.push(line.to_string());
            continue;
        }

        // Check uniform N-tuple repetition (N = 4, 3, 2) only if there are >= 2 distinct characters
        let distinct_count: usize = chars.iter().copied().collect::<HashSet<char>>().len();
        let mut collapsed: Option<String> = None;
        if distinct_count >= 2 {
            for n in [4usize, 3, 2] {
                if chars.len() >= n * 2 && chars.len() % n == 0 {
                    let is_uniform = chars
                        .chunks_exact(n)
                        .all(|chunk| chunk.iter().all(|&c| c == chunk[0]));
                    if is_uniform {
                        collapsed = Some(chars.chunks_exact(n).map(|c| c[0]).collect());
                        break;
                    }
                }
            }
        }

        let base: Vec<char> = match collapsed {
            Some(s) => s.chars().collect(),
            None => chars,
        };

        let processed = collapse_mostly_doubled(&base);
        out_lines.push(processed);
    }
    out_lines.join("\n")
}

fn collapse_mostly_doubled(chars: &[char]) -> String {
    if chars.len() < 8 {
        return chars.iter().collect();
    }
    let distinct_count: usize = chars.iter().copied().collect::<HashSet<char>>().len();
    if distinct_count < 3 {
        return chars.iter().collect();
    }

    let mut pair_matches = 0usize;
    let mut total_pairs = 0usize;
    let mut i = 0;
    while i + 1 < chars.len() {
        if !chars[i].is_whitespace() {
            total_pairs += 1;
            if chars[i] == chars[i + 1] {
                pair_matches += 1;
                i += 2;
                continue;
            }
        }
        i += 1;
    }

    if total_pairs >= 4 && pair_matches * 100 / total_pairs >= 80 {
        let mut res = String::with_capacity(chars.len() / 2);
        let mut idx = 0;
        while idx < chars.len() {
            res.push(chars[idx]);
            if idx + 1 < chars.len() && chars[idx] == chars[idx + 1] {
                idx += 2;
            } else {
                idx += 1;
            }
        }
        return res;
    }

    chars.iter().collect()
}

/// Removes immediate repeated phrases (e.g. "こんにちはこんにちは" -> "こんにちは").
/// Bounded to prevent quadratic/cubic CPU stalls on long memory dumps.
pub fn remove_repeated_phrases(input: &str) -> String {
    let mut out_lines = Vec::new();
    for line in input.split('\n') {
        out_lines.push(dedup_line_phrases(line));
    }
    out_lines.join("\n")
}

fn dedup_line_phrases(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    if n < 6 || n > 256 {
        return line.to_string();
    }

    // 1. Check if the entire line is a phrase repeated K times (K = 2..=6)
    for k in [2usize, 3, 4, 5, 6] {
        if n % k == 0 {
            let chunk_len = n / k;
            if chunk_len >= 3 {
                let first = &chars[0..chunk_len];
                if chars.chunks_exact(chunk_len).all(|c| c == first) {
                    return first.iter().collect();
                }
            }
        }
    }

    // 2. Bounded sliding window removal of adjacent repeated substrings of length >= 4
    let mut result = chars;
    let mut passes = 0;
    let mut changed = true;
    while changed && passes < 8 {
        passes += 1;
        changed = false;
        let len = result.len();
        if len < 8 {
            break;
        }
        'outer: for pat_len in (4..=len / 2).rev() {
            for start in 0..=(len - pat_len * 2) {
                if result[start..start + pat_len] == result[start + pat_len..start + pat_len * 2] {
                    result.drain(start + pat_len..start + pat_len * 2);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }

    result.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_repeated_characters() {
        assert_eq!(
            remove_repeated_characters("ここんんににちちはは"),
            "こんにちは"
        );
        assert_eq!(
            remove_repeated_characters("ああいいううええおお"),
            "あいうえお"
        );
        assert_eq!(
            remove_repeated_characters("普通のテキストです。"),
            "普通のテキストです。"
        );
        // Must preserve single-character elongation / stutter lines!
        assert_eq!(remove_repeated_characters("ーーーー"), "ーーーー");
        assert_eq!(remove_repeated_characters("っっっっ"), "っっっっ");
    }

    #[test]
    fn test_remove_repeated_phrases() {
        assert_eq!(
            remove_repeated_phrases("おはようございますおはようございます"),
            "おはようございます"
        );
        assert_eq!(
            remove_repeated_phrases("「先輩、おはようございます」「先輩、おはようございます」"),
            "「先輩、おはようございます」"
        );
    }

    #[test]
    fn test_clean_sentence_pipeline() {
        let mut cfg = AppConfig::default();
        cfg.remove_speaker_brackets = true;
        let raw = "【凛凛】<ruby>月姫<rt>つきひめ</rt></ruby>の世界へようこそ[r]ようこそようこそ";
        let cleaned = clean_sentence(raw, None, &cfg);
        assert_eq!(cleaned, "月姫の世界へようこそ");
    }
}
