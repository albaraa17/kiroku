/// Computes a quality score (0..=100) for a hook thread based on its recent text.
/// High scores indicate clean Japanese / readable dialogue; low scores indicate memory garbage.
pub fn score_thread_text(text: &str) -> u8 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut total = 0usize;
    let mut jp_chars = 0usize;
    let mut ascii_readable = 0usize;
    let mut garbage_chars = 0usize;

    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            continue;
        }
        total += 1;
        let u = ch as u32;
        let is_hiragana = (0x3040..=0x309F).contains(&u);
        let is_katakana = (0x30A0..=0x30FF).contains(&u);
        let is_cjk = (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u);
        let is_jp_punct = (0x3000..=0x303F).contains(&u) || (0xFF01..=0xFF60).contains(&u);

        if is_hiragana || is_katakana || is_cjk || is_jp_punct {
            jp_chars += 1;
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

    if garbage_chars * 4 >= total {
        return 5;
    }

    let jp_ratio = (jp_chars * 100) / total;
    let ascii_ratio = (ascii_readable * 100) / total;

    let mut score = if jp_ratio >= 50 {
        65 + (jp_ratio * 35 / 100)
    } else if jp_ratio > 0 {
        40 + jp_ratio / 2
    } else if ascii_ratio >= 80 {
        45
    } else {
        15
    };

    // Penalize ultra-long memory dumps (>500 chars in single chunk) or single-char spam
    if total > 600 {
        score = score.saturating_sub(30);
    } else if total == 1 && jp_chars == 0 {
        score = score.saturating_sub(25);
    }

    score.min(100) as u8
}
