/// Computes a quality score (0..=99) for a hook thread based on its recent text.
/// High scores (85..=99) indicate clean Japanese dialogue; medium scores (55..=75) indicate
/// short speaker names or UI labels; low scores (<40) indicate memory garbage.
/// (Score 100 is reserved for user-saved hooks.)
pub fn score_thread_text(text: &str) -> u8 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut total = 0usize;
    let mut hiragana_chars = 0usize;
    let mut katakana_chars = 0usize;
    let mut cjk_chars = 0usize;
    let mut jp_punct = 0usize;
    let mut ascii_readable = 0usize;
    let mut garbage_chars = 0usize;

    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            continue;
        }
        total += 1;
        let u = ch as u32;
        if (0x3040..=0x309F).contains(&u) {
            hiragana_chars += 1;
        } else if (0x30A0..=0x30FF).contains(&u) || (0xFF66..=0xFF9F).contains(&u) {
            katakana_chars += 1;
        } else if (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u) {
            cjk_chars += 1;
        } else if (0x3000..=0x303F).contains(&u) || (0xFF01..=0xFF60).contains(&u) {
            jp_punct += 1;
        } else if ch.is_ascii_graphic() {
            ascii_readable += 1;
        } else if ch.is_control()
            || u == 0xFFFD
            || (0xE000..=0xF8FF).contains(&u)
            || (0xD800..=0xDFFF).contains(&u)
        {
            garbage_chars += 1;
        }
    }

    if total == 0 {
        return 0;
    }

    if garbage_chars > 0 && garbage_chars * 4 >= total {
        return 5;
    }

    let jp_chars = hiragana_chars + katakana_chars + cjk_chars + jp_punct;
    let jp_ratio = (jp_chars * 100) / total;
    let ascii_ratio = (ascii_readable * 100) / total;

    let mut score: usize = if jp_ratio >= 60 {
        65 + (jp_ratio * 28 / 100)
    } else if jp_ratio > 0 {
        35 + jp_ratio / 2
    } else if ascii_ratio >= 80 {
        42
    } else {
        12
    };

    // Real Japanese sentences have Hiragana (okurigana/particles) and dialogue length (6..=200)
    if (6..=200).contains(&total) && hiragana_chars >= 2 && garbage_chars == 0 {
        score += 5;
        if jp_punct >= 1 {
            score += 3;
        }
    } else if total <= 4 {
        // Short speaker names (e.g. "小鳥", "瑚太郎") or single words shouldn't outrank full dialogue
        score = score.min(72);
    }

    // Penalize progressive buffer dumps (e.g. multiple opening brackets "「お「おは")
    let open_quote_count = trimmed.chars().filter(|&c| c == '「' || c == '『').count();
    let close_quote_count = trimmed.chars().filter(|&c| c == '」' || c == '』').count();
    if open_quote_count >= 3 && open_quote_count > close_quote_count + 1 {
        score = score.saturating_sub(25);
    }

    // Penalize ultra-long backlog dumps (>300 chars in single chunk) or any garbage chars
    if total > 300 {
        score = score.saturating_sub(30);
    }
    if garbage_chars > 0 {
        score = score.saturating_sub(20);
    }

    score.min(99) as u8
}

/// Returns true only when `text` and `rolling_score` strongly match genuine VN dialogue
/// (avoiding false positives on speaker-name threads, system strings, or memory dumps).
pub fn is_high_quality_dialogue_candidate(text: &str, rolling_score: u8) -> bool {
    if rolling_score < 85 {
        return false;
    }
    let trimmed = text.trim();
    let mut total = 0usize;
    let mut hiragana = 0usize;
    let mut ascii_symbols = 0usize;

    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            continue;
        }
        total += 1;
        let u = ch as u32;
        if ch.is_control()
            || u == 0xFFFD
            || (0xE000..=0xF8FF).contains(&u)
            || (0xD800..=0xDFFF).contains(&u)
        {
            return false;
        }
        if (0x3040..=0x309F).contains(&u) {
            hiragana += 1;
        } else if ch.is_ascii_punctuation() || ch.is_ascii_alphanumeric() {
            ascii_symbols += 1;
        }
    }

    // Must be normal dialogue length (6..=200 chars), contain >= 2 Hiragana chars, and < 20% ASCII
    (6..=200).contains(&total) && hiragana >= 2 && (ascii_symbols * 5 < total)
}
