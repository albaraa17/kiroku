//! Backlog-flood detector.
//!
//! Suppresses backlog redraws (multi-line dumps, line-by-line scroll replays,
//! focus redraws) while guaranteeing that **novel text is always emitted**.
//!
//! Pipeline per candidate line:
//! 1. Normalise (fullwidth ASCII, ruby, control codes, `」「` hard boundary,
//!    `【name】` speaker tags) into a whitespace/bracket-free matching key with
//!    hard-segment offsets.
//! 2. Gate 0 – redraw of the last emitted / currently held line -> drop.
//! 3. Analyse against the rolling history: overlapping substring hits,
//!    nested-hit removal, union coverage, longest index-contiguous chain,
//!    per-segment residual with speaker-label exclusion in dump context.
//! 4. Gate 1 – novelty veto: residual >= `novel_min_weight` (or a trailing
//!    novel run after a quote) -> always emit. Nothing can override this.
//! 5. Gate 2 – learned secondary backlog thread (never the primary thread).
//! 6. Repeat scoring + state machine (`Live` / `Pending` / `Backlog`).
//!    Repeats that are not suppressed are held for `hold_window` and then
//!    emitted in original order (flush-on-next-event or on `on_tick`).
//!
//! The module is synchronous and clock-free: every event carries its own
//! `Instant`, and the caller must drive [`BacklogDetector::on_tick`]
//! (e.g. every 50 ms) so held lines are released.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::time::{Duration, Instant};

// ───────────────────────────── public types ─────────────────────────────

/// Opaque hook identity (hash of hook address / context / subcontext).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ThreadKey(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub text: String,
    pub thread: ThreadKey,
    /// Capture time – ideally taken on the hook side, not after pipe batching.
    pub at: Instant,
}

impl Candidate {
    pub fn new(text: impl Into<String>, thread: ThreadKey, at: Instant) -> Self {
        Self { text: text.into(), thread, at }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Nothing left after normalisation.
    Empty,
    /// Same as the last emitted / held line (focus, menu-close, backlog-close redraw).
    Redraw,
    /// Emitted by a learned secondary backlog thread.
    BacklogThread,
    /// Several contiguous history lines joined by hard boundaries.
    BacklogDump,
    /// Line-by-line backlog scrolling (reverse steps / inside a backlog session).
    BacklogScroll,
    /// Forward contiguous replay of recent history (load + skip re-read).
    BacklogRun,
    /// Intermediate line suppressed during a rapid-fire skip wave (3+ fast lines).
    SkipBurst,
    /// Earlier turns suppressed from a single-event concatenated multi-turn skip dump.
    SkipDump,
}

impl Reason {
    pub fn is_skip(self) -> bool {
        matches!(self, Reason::SkipBurst | Reason::SkipDump)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadRole {
    Unknown,
    Primary,
    Backlog,
}

/// Result of feeding one event. `emit` is in display order.
#[derive(Debug, Default)]
pub struct Outcome {
    pub emit: Vec<Candidate>,
    pub suppressed: Vec<(Candidate, Reason)>,
    /// The incoming line is being held (released by a later event or `on_tick`).
    pub held: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub history_cap: usize,
    /// History lines at or above this weight are matched as substrings;
    /// lighter lines only match whole hard segments.
    pub pattern_min_weight: f32,
    /// Gate 1: residual weight that always counts as new story text.
    pub novel_min_weight: f32,
    /// Gate 1b: novel run trailing a matched quote in the same segment.
    pub suffix_novel_min_weight: f32,
    /// Max weight of an unmatched speaker label inside a dump.
    pub label_max_weight: f32,
    /// Min weight of a candidate for reverse containment (C ⊂ H).
    pub contained_min_weight: f32,
    /// covered / (covered + residual) needed to call a line a repeat.
    pub repeat_cover_ratio: f32,
    pub redraw_window: Duration,
    pub burst_window: Duration,
    pub hold_window: Duration,
    pub isolation_gap: Duration,
    pub input_window: Duration,
    pub continuity_window: Duration,
    pub backlog_timeout: Duration,
    pub page_jump: u64,
    pub recent_lines: u64,
    pub suppress_score: i32,
    pub thread_window: usize,
    pub thread_min_events: usize,
    pub thread_match_ratio: f32,
    /// Policy: suppress forward replays of recent history (load save + skip).
    pub suppress_rereads: bool,
    pub suppressed_ring_cap: usize,
    /// Policy: suppress backlog dumps, scroll replays, and redraws.
    pub backlog_filter_enabled: bool,
    /// Policy: suppress rapid-fire skip waves and multi-turn skip dumps, emitting only the landing line.
    pub skip_filter_enabled: bool,
    /// Settle / burst window for skip waves (default 120ms, configurable via Settings slider).
    pub skip_settle_window: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            history_cap: 128,
            pattern_min_weight: 8.0,
            novel_min_weight: 8.0,
            suffix_novel_min_weight: 4.0,
            label_max_weight: 6.0,
            contained_min_weight: 12.0,
            repeat_cover_ratio: 0.6,
            redraw_window: Duration::from_secs(3),
            burst_window: Duration::from_millis(150),
            hold_window: Duration::from_millis(150),
            isolation_gap: Duration::from_secs(1),
            input_window: Duration::from_secs(1),
            continuity_window: Duration::from_secs(3),
            backlog_timeout: Duration::from_secs(2),
            page_jump: 8,
            recent_lines: 10,
            suppress_score: 4,
            thread_window: 20,
            thread_min_events: 10,
            thread_match_ratio: 0.9,
            suppress_rereads: true,
            suppressed_ring_cap: 32,
            backlog_filter_enabled: true,
            skip_filter_enabled: false,
            skip_settle_window: Duration::from_millis(120),
        }
    }
}

// ───────────────────────────── normalisation ─────────────────────────────

/// Information weight: kanji 2, kana 1, other alphanumerics 0.25, rest 0.
fn char_weight(c: char) -> f32 {
    match c {
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆' => 2.0,
        '\u{3041}'..='\u{3096}' | '\u{30A1}'..='\u{30FA}' | 'ー' | '\u{FF66}'..='\u{FF9D}' => 1.0,
        c if c.is_alphanumeric() => 0.25,
        _ => 0.0,
    }
}

fn weight_of(s: &str) -> f32 {
    s.chars().map(char_weight).sum()
}

fn map_char(c: char) -> Option<char> {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0),
        '\u{3000}' => Some(' '),
        '\r' | '\u{2028}' | '\u{2029}' => Some('\n'),
        '\u{200B}'..='\u{200D}' | '\u{FEFF}' => None,
        '\n' | '\t' => Some(c),
        c if c.is_control() => Some('\n'),
        c => Some(c),
    }
}

#[derive(Debug)]
struct Normalized {
    /// Matching key: no whitespace, no 「」『』, no speaker tags.
    key: String,
    /// Byte offset in `key` where each (non-empty) hard segment starts.
    seg_starts: Vec<usize>,
    seg_quoted: Vec<bool>,
    names: Vec<String>,
    weight: f32,
}

impl Normalized {
    fn seg_count(&self) -> usize {
        self.seg_starts.len()
    }
    fn seg_range(&self, i: usize) -> Range<usize> {
        let end = self.seg_starts.get(i + 1).copied().unwrap_or(self.key.len());
        self.seg_starts[i]..end
    }
    fn seg_of(&self, byte: usize) -> usize {
        self.seg_starts.partition_point(|&s| s <= byte).saturating_sub(1)
    }
}

fn normalize(raw: &str) -> Normalized {
    let has_ruby = raw.contains('《');
    let mut buf = String::with_capacity(raw.len());
    let mut in_ruby = false;
    let mut last_sig: Option<char> = None;
    let mut prev_is_close_quote = false;
    for ch in raw.chars() {
        let Some(ch) = map_char(ch) else { continue };
        if has_ruby {
            if ch == '《' {
                in_ruby = true;
                continue;
            }
            if in_ruby {
                if ch == '》' {
                    in_ruby = false;
                }
                continue;
            }
            if ch == '|' {
                continue;
            }
        }
        // Segment break after closing quote when followed by next non-whitespace character,
        // or between adjacent quotes 「a」「b」 -> two hard segments.
        if prev_is_close_quote && !ch.is_whitespace() && ch != '\n' {
            buf.push('\n');
            prev_is_close_quote = false;
        } else if matches!(ch, '「' | '『') && matches!(last_sig, Some('」' | '』')) {
            buf.push('\n');
        } else if matches!(ch, '「' | '『') {
            // Segment break between narration and dialogue:
            // e.g. "分からなかった。絵理子" -> split after "。" so "絵理子「..." starts as a new segment.
            if let Some(last_punc_pos) = buf.rfind(['。', '！', '？', '」', '』']) {
                let punc_char_len = buf[last_punc_pos..].chars().next().map_or(0, |c| c.len_utf8());
                let after_punc = &buf[last_punc_pos + punc_char_len..];
                if !after_punc.contains('\n') {
                    let chars_after: Vec<char> = after_punc.chars().filter(|c| !c.is_whitespace()).collect();
                    if chars_after.is_empty()
                        || ((1..=12).contains(&chars_after.len())
                            && !chars_after.iter().any(|&c| matches!(c, '、' | '!' | '?' | '.' | ',')))
                    {
                        let split_idx = last_punc_pos + punc_char_len;
                        buf.insert(split_idx, '\n');
                    }
                }
            }
        }
        if matches!(ch, '」' | '』') {
            prev_is_close_quote = true;
        } else if !ch.is_whitespace() {
            prev_is_close_quote = false;
        }

        if ch == '\n' {
            last_sig = None;
        } else if !ch.is_whitespace() {
            last_sig = Some(ch);
        }
        buf.push(ch);
    }

    let mut n = Normalized { key: String::new(), seg_starts: vec![], seg_quoted: vec![], names: vec![], weight: 0.0 };
    for line in buf.split('\n') {
        let mut line = line.trim();
        if let Some(rest) = line.strip_prefix('【') {
            if let Some(end) = rest.find('】') {
                let name: String = rest[..end].chars().filter(|c| !c.is_whitespace()).collect();
                if !name.is_empty() {
                    n.names.push(name);
                }
                line = rest[end + '】'.len_utf8()..].trim_start();
            }
        } else if let Some(q_idx) = line.find(['「', '『']) {
            // Unbracketed speaker name directly preceding quote (e.g. 柊一郎「...」 or 妙花「...」)
            if q_idx > 0 {
                let prefix = &line[..q_idx];
                let prefix_chars: Vec<char> = prefix.chars().filter(|c| !c.is_whitespace()).collect();
                if (1..=12).contains(&prefix_chars.len())
                    && !prefix_chars.iter().any(|&c| matches!(c, '。' | '、' | '！' | '？' | '!' | '?' | '.' | ','))
                {
                    let name: String = prefix_chars.into_iter().collect();
                    n.names.push(name);
                    line = &line[q_idx..];
                }
            }
        }
        let quoted = line.contains(['「', '『']);
        let start = n.key.len();
        n.key.extend(line.chars().filter(|&c| !c.is_whitespace() && !matches!(c, '「' | '」' | '『' | '』')));
        if n.key.len() > start {
            n.seg_starts.push(start);
            n.seg_quoted.push(quoted);
        }
    }
    n.weight = weight_of(&n.key);
    n
}

fn starts_with_speaker_before_quote(s: &str) -> bool {
    let line = s.split('\n').next().unwrap_or("");
    if let Some(rest) = line.strip_prefix('【') {
        if let Some(end) = rest.find('】') {
            let name_chars = rest[..end].chars().filter(|c| !c.is_whitespace()).count();
            return (1..=16).contains(&name_chars);
        }
    }
    if let Some(q_idx) = line.find(['「', '『']) {
        if q_idx > 0 {
            let prefix = &line[..q_idx];
            let prefix_chars: Vec<char> = prefix.chars().filter(|c| !c.is_whitespace()).collect();
            if (1..=12).contains(&prefix_chars.len())
                && !prefix_chars.iter().any(|&c| {
                    matches!(
                        c,
                        '。' | '、' | '！' | '？' | '!' | '?' | '.' | ',' | '「' | '」' | '『' | '』' | '…' | '―' | '—'
                    )
                })
            {
                return true;
            }
        }
    }
    false
}

/// Structural-only splitter for single-event concatenated skip dumps (never splits by character length).
/// Returns `Some((suppressed_prefix_turns, final_landing_turn))` when a single hook emission contains:
/// - 3+ distinct turns where at least 2 are dialogue turns (`「...」` / `『...』`), OR
/// - 3+ speaker-tagged turns (`【Name】` / `Name「`).
pub fn split_multiturn_skip_dump(raw: &str) -> Option<(String, String)> {
    let clean_lf = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut buf = String::with_capacity(clean_lf.len() + 16);
    let mut quote_depth = 0usize;
    let mut turn_has_speaker = starts_with_speaker_before_quote(clean_lf.trim_start());

    for (idx, ch) in clean_lf.char_indices() {
        buf.push(ch);
        if ch == '\n' {
            quote_depth = 0;
            let rest = clean_lf[idx + ch.len_utf8()..].trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
            turn_has_speaker = starts_with_speaker_before_quote(rest);
            continue;
        }
        if matches!(ch, '「' | '『') {
            quote_depth += 1;
            continue;
        }
        if matches!(ch, '」' | '』') {
            quote_depth = quote_depth.saturating_sub(1);
            if quote_depth == 0 {
                let rest = &clean_lf[idx + ch.len_utf8()..];
                let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
                if !trimmed.is_empty() && !trimmed.starts_with('\n') {
                    let next_is_quote_or_bracket = trimmed.starts_with(['「', '『', '【']);
                    let next_is_speaker = starts_with_speaker_before_quote(trimmed);
                    if next_is_quote_or_bracket || next_is_speaker || turn_has_speaker {
                        buf.push('\n');
                        turn_has_speaker = next_is_speaker || trimmed.starts_with('【');
                    }
                }
            }
            continue;
        }
        if quote_depth == 0 && matches!(ch, '。' | '！' | '？') {
            let rest = &clean_lf[idx + ch.len_utf8()..];
            let trimmed = rest.trim_start_matches(|c: char| c.is_whitespace() && c != '\n');
            if !trimmed.is_empty() && !trimmed.starts_with('\n') {
                let next_is_speaker = starts_with_speaker_before_quote(trimmed);
                if trimmed.starts_with(['「', '『', '【']) || next_is_speaker {
                    buf.push('\n');
                    turn_has_speaker = next_is_speaker || trimmed.starts_with('【');
                }
            }
        }
    }

    let mut turns: Vec<&str> = Vec::new();
    let mut quoted_turns = 0usize;
    let mut speaker_turns = 0usize;

    for line in buf.split('\n') {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.contains(['「', '『']) && t.contains(['」', '』']) {
            quoted_turns += 1;
        }
        if starts_with_speaker_before_quote(t) {
            speaker_turns += 1;
        }
        turns.push(t);
    }

    let is_multiturn_dump = (turns.len() >= 3 && quoted_turns >= 2) || speaker_turns >= 3;
    if !is_multiturn_dump {
        return None;
    }

    let last_turn = turns.pop()?.to_string();
    let prefix_dump = turns.join("\n");
    Some((prefix_dump, last_turn))
}

// ───────────────────────────── history ─────────────────────────────

#[derive(Debug)]
struct Entry {
    seq: u64,
    key: String,
    weight: f32,
}

#[derive(Debug)]
struct History {
    ring: VecDeque<Entry>,
    cap: usize,
    next_seq: u64,
}

impl History {
    fn new(cap: usize) -> Self {
        Self { ring: VecDeque::with_capacity(cap), cap, next_seq: 0 }
    }
    fn push(&mut self, key: String, weight: f32) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        if self.ring.len() == self.cap {
            self.ring.pop_front();
        }
        self.ring.push_back(Entry { seq, key, weight });
        seq
    }
    fn tail(&self) -> Option<u64> {
        self.ring.back().map(|e| e.seq)
    }
}

// ───────────────────────────── analysis ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hit {
    start: usize,
    end: usize,
    seq: u64,
}

#[derive(Debug, Default)]
struct Analysis {
    covered_w: f32,
    residual_w: f32,
    suffix_novel: bool,
    exact: Vec<u64>,
    contained_in: Vec<u64>,
    chain: Vec<Hit>,
    chain_hard: bool,
    seg_fully_covered: Vec<bool>,
    labels: Vec<String>,
}

impl Analysis {
    fn is_dump(&self) -> bool {
        (self.chain.len() >= 2 && self.chain_hard) || self.chain.len() >= 3
    }
    fn is_repeat(&self, cfg: &Config) -> bool {
        !self.exact.is_empty()
            || !self.contained_in.is_empty()
            || (self.covered_w > 0.0 && self.covered_w / (self.covered_w + self.residual_w) >= cfg.repeat_cover_ratio)
    }
    fn anchors(&self) -> Vec<u64> {
        if !self.exact.is_empty() {
            self.exact.clone()
        } else if let (Some(f), Some(l)) = (self.chain.first(), self.chain.last()) {
            vec![f.seq, l.seq]
        } else {
            self.contained_in.clone()
        }
    }
}

fn analyze(n: &Normalized, hist: &History, names: &HashSet<String>, cfg: &Config) -> Analysis {
    let mut a = Analysis::default();
    let mut hits: Vec<Hit> = Vec::new();

    // Overlapping search: every occurrence of every entry (no automaton shadowing).
    for e in &hist.ring {
        if e.key == n.key {
            a.exact.push(e.seq);
        }
        if e.weight >= cfg.pattern_min_weight {
            for (pos, _) in n.key.match_indices(e.key.as_str()) {
                hits.push(Hit { start: pos, end: pos + e.key.len(), seq: e.seq });
            }
            if n.weight >= cfg.contained_min_weight && e.key.len() > n.key.len() && e.key.contains(n.key.as_str()) {
                a.contained_in.push(e.seq);
            }
        } else {
            for i in 0..n.seg_count() {
                let r = n.seg_range(i);
                if &n.key[r.clone()] == e.key.as_str() {
                    hits.push(Hit { start: r.start, end: r.end, seq: e.seq });
                }
            }
        }
    }

    // Drop hits strictly nested inside a longer hit (keep same-range duplicates).
    let all = hits.clone();
    hits.retain(|h| !all.iter().any(|g| g.start <= h.start && h.end <= g.end && (g.end - g.start) > (h.end - h.start)));
    hits.sort_by_key(|h| (h.start, h.end, h.seq));

    let mut covered = vec![false; n.key.len()];
    for h in &hits {
        covered[h.start..h.end].fill(true);
    }

    let (chain, hard) = best_chain(&hits, n, cfg);
    a.chain = chain;
    a.chain_hard = hard;

    // Per-segment residual.
    let cnt = n.seg_count();
    let mut seg_runs: Vec<Vec<Range<usize>>> = Vec::with_capacity(cnt);
    let mut any_cov = vec![false; cnt];
    for (i, cov) in any_cov.iter_mut().enumerate() {
        let r = n.seg_range(i);
        let mut runs = Vec::new();
        let mut open: Option<usize> = None;
        for (off, ch) in n.key[r.clone()].char_indices() {
            let b = r.start + off;
            if covered[b] {
                *cov = true;
                a.covered_w += char_weight(ch);
                if let Some(s) = open.take() {
                    runs.push(s..b);
                }
            } else if open.is_none() {
                open = Some(b);
            }
        }
        if let Some(s) = open {
            runs.push(s..r.end);
        }
        seg_runs.push(runs);
    }

    let dump_ctx = a.is_dump();
    a.seg_fully_covered = vec![false; cnt];
    for i in 0..cnt {
        let r = n.seg_range(i);
        let runs = &seg_runs[i];
        let mut seg_res = 0.0;
        for run in runs {
            let text = &n.key[run.clone()];
            let w = weight_of(text);
            if w <= 0.0 {
                continue;
            }
            let core = text.trim_matches(|c: char| char_weight(c) == 0.0);
            if names.contains(core) || n.names.iter().any(|x| x == core) {
                continue;
            }
            let at_start = run.start == r.start;
            let at_end = run.end == r.end;
            let whole = at_start && at_end;
            let neighbour = (i > 0 && any_cov[i - 1]) || (i + 1 < cnt && any_cov[i + 1]);
            let label = dump_ctx
                && w <= cfg.label_max_weight
                && ((whole && !n.seg_quoted[i] && neighbour) || (at_start && !whole && runs.len() == 1));
            if label {
                a.labels.push(core.to_string());
                continue;
            }
            if at_end && !at_start && any_cov[i] && w >= cfg.suffix_novel_min_weight {
                a.suffix_novel = true;
            }
            seg_res += w;
        }
        a.residual_w += seg_res;
        a.seg_fully_covered[i] = any_cov[i] && seg_res == 0.0;
    }
    a
}

/// Longest chain of hits whose history seqs step by exactly ±1 (one direction),
/// left to right, separated only by light gaps (labels / punctuation).
fn best_chain(hits: &[Hit], n: &Normalized, cfg: &Config) -> (Vec<Hit>, bool) {
    let Some(&first) = hits.first() else { return (vec![], false) };
    let mut best = vec![first];
    for dir in [1i64, -1] {
        let k = hits.len();
        let mut len = vec![1usize; k];
        let mut prev = vec![usize::MAX; k];
        for j in 0..k {
            for i in 0..j {
                let (a, b) = (hits[i], hits[j]);
                if a.end <= b.start
                    && b.seq as i64 - a.seq as i64 == dir
                    && len[i] + 1 > len[j]
                    && weight_of(&n.key[a.end..b.start]) <= cfg.label_max_weight
                {
                    len[j] = len[i] + 1;
                    prev[j] = i;
                }
            }
        }
        let (mut j, &l) = len.iter().enumerate().max_by_key(|&(_, l)| *l).unwrap();
        if l > best.len() {
            let mut chain = vec![hits[j]];
            while prev[j] != usize::MAX {
                j = prev[j];
                chain.push(hits[j]);
            }
            chain.reverse();
            best = chain;
        }
    }
    let hard = best.windows(2).all(|w| n.seg_of(w[0].end - 1) < n.seg_of(w[1].start));
    (best, hard)
}

// ───────────────────────────── detector ─────────────────────────────

#[derive(Debug)]
enum Mode {
    Live,
    Pending { cand: Candidate, key: String, weight: f32, anchor: u64, deadline: Instant },
    Backlog { cursor: u64, last: Instant, entered_by_reverse: bool },
}

#[derive(Debug)]
struct Emitted {
    key: String,
    seq: u64,
    at: Instant,
    novel: bool,
}

#[derive(Debug)]
struct SkipTail {
    cand: Candidate,
    key: String,
    deadline: Instant,
}

#[derive(Debug, Default)]
struct ThreadStats {
    novel: u32,
    recent: VecDeque<bool>,
}

enum Class {
    Novel,
    Repeat,
    Other,
}

#[derive(Debug)]
pub struct BacklogDetector {
    cfg: Config,
    history: History,
    mode: Mode,
    names: HashSet<String>,
    threads: HashMap<ThreadKey, ThreadStats>,
    primary_thread: Option<ThreadKey>,
    last_emitted: Option<Emitted>,
    last_event_at: Option<Instant>,
    last_novel_at: Option<Instant>,
    skip_burst_count: usize,
    skip_tail: Option<SkipTail>,
    last_repeat: Option<(u64, Instant)>,
    last_input_at: Option<Instant>,
    suppressed_ring: VecDeque<Candidate>,
}

impl Default for BacklogDetector {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl BacklogDetector {
    pub fn new(cfg: Config) -> Self {
        Self {
            history: History::new(cfg.history_cap),
            cfg,
            mode: Mode::Live,
            names: HashSet::new(),
            threads: HashMap::new(),
            primary_thread: None,
            last_emitted: None,
            last_event_at: None,
            last_novel_at: None,
            skip_burst_count: 0,
            skip_tail: None,
            last_repeat: None,
            last_input_at: None,
            suppressed_ring: VecDeque::new(),
        }
    }

    /// Updates the backlog and fast-skip flood guard configuration on the fly.
    pub fn set_filter_config(&mut self, backlog_enabled: bool, skip_enabled: bool, settle_window: Duration) {
        self.cfg.backlog_filter_enabled = backlog_enabled;
        self.cfg.skip_filter_enabled = skip_enabled;
        self.cfg.skip_settle_window = settle_window;
    }

    /// Updates the fast-skip flood guard configuration on the fly.
    pub fn set_skip_config(&mut self, enabled: bool, settle_window: Duration) {
        self.cfg.skip_filter_enabled = enabled;
        self.cfg.skip_settle_window = settle_window;
    }

    /// The user-selected dialogue thread; never auto-classified as backlog.
    pub fn set_primary_thread(&mut self, t: Option<ThreadKey>) {
        self.primary_thread = t;
    }

    /// Wheel-up / PageUp / backlog key while the game window is focused.
    pub fn on_input(&mut self, at: Instant) {
        self.last_input_at = Some(at);
    }

    /// Cold-start seeding (e.g. history persisted per game).
    pub fn seed_history<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>) {
        for l in lines {
            let n = normalize(l);
            if !n.key.is_empty() {
                self.history.push(n.key, n.weight);
            }
        }
    }

    /// Recovery hatch: take back the most recently suppressed line.
    pub fn take_last_suppressed(&mut self) -> Option<Candidate> {
        self.suppressed_ring.pop_back()
    }

    pub fn thread_role(&self, t: ThreadKey) -> ThreadRole {
        if self.primary_thread == Some(t) {
            return ThreadRole::Primary;
        }
        match self.threads.get(&t) {
            None => ThreadRole::Unknown,
            Some(st) if st.novel > 0 => ThreadRole::Primary,
            Some(st) if st.recent.len() >= self.cfg.thread_min_events => {
                let hits = st.recent.iter().filter(|&&r| r).count() as f32;
                if hits / st.recent.len() as f32 >= self.cfg.thread_match_ratio {
                    ThreadRole::Backlog
                } else {
                    ThreadRole::Unknown
                }
            }
            Some(_) => ThreadRole::Unknown,
        }
    }

    /// Releases expired held lines and ends idle backlog sessions.
    pub fn on_tick(&mut self, now: Instant) -> Outcome {
        let mut out = Outcome::default();
        let (expire_pending, end_backlog) = match &self.mode {
            Mode::Pending { deadline, .. } => (now >= *deadline, false),
            Mode::Backlog { last, .. } => (false, now.saturating_duration_since(*last) >= self.cfg.backlog_timeout),
            Mode::Live => (false, false),
        };
        if expire_pending {
            self.flush_pending(&mut out);
        }
        if end_backlog {
            self.mode = Mode::Live;
        }
        if self.skip_tail.as_ref().is_some_and(|t| now >= t.deadline) {
            if let Some(tail) = self.skip_tail.take() {
                self.skip_burst_count = 0;
                if let Some(seq) = self.history.tail() {
                    self.last_emitted = Some(Emitted {
                        key: tail.key,
                        seq,
                        at: tail.cand.at,
                        novel: true,
                    });
                }
                out.emit.push(tail.cand);
            }
        }
        out
    }

    pub fn on_line(&mut self, c: Candidate) -> Outcome {
        let now = c.at;
        let mut out = self.on_tick(now);
        let prev_event = self.last_event_at.replace(now);
        let n = normalize(&c.text);
        if n.key.is_empty() {
            out.suppressed.push((c, Reason::Empty));
            return out;
        }
        self.names.extend(n.names.iter().cloned());

        // Gate 0: redraw.
        if let Some(seq) = self.redraw_of(&n.key, now) {
            self.last_repeat = Some((seq, now));
            if let Mode::Backlog { last, .. } = &mut self.mode {
                *last = now;
            }
            self.suppress(&mut out, c, Reason::Redraw);
            return out;
        }

        let a = analyze(&n, &self.history, &self.names, &self.cfg);

        // When backlog filter is disabled, treat all non-redraw lines as live emissions
        // (still subject to skip_filter_enabled if active).
        if !self.cfg.backlog_filter_enabled {
            self.flush_pending(&mut out);
            self.mode = Mode::Live;
            if self.cfg.skip_filter_enabled {
                self.handle_novel_with_skip_guard(c, &n, &a, now, &mut out);
            } else {
                self.accept(&n, &a, now);
                out.emit.push(c);
            }
            return out;
        }

        // Gate 1: novelty veto.
        if a.residual_w >= self.cfg.novel_min_weight || a.suffix_novel {
            self.note_thread(c.thread, Class::Novel);
            self.flush_pending(&mut out);
            self.mode = Mode::Live;
            if self.cfg.skip_filter_enabled {
                self.handle_novel_with_skip_guard(c, &n, &a, now, &mut out);
            } else {
                self.accept(&n, &a, now);
                out.emit.push(c);
            }
            return out;
        }

        let repeat = a.is_repeat(&self.cfg);
        self.note_thread(c.thread, if repeat { Class::Repeat } else { Class::Other });

        // Gate 2: learned secondary backlog thread.
        if self.thread_role(c.thread) == ThreadRole::Backlog {
            self.suppress(&mut out, c, Reason::BacklogThread);
            return out;
        }

        // Low-information but not a repeat (e.g. a new 「え？」): emit.
        if !repeat {
            self.flush_pending(&mut out);
            self.mode = Mode::Live;
            if self.cfg.skip_filter_enabled {
                self.handle_novel_with_skip_guard(c, &n, &a, now, &mut out);
            } else {
                self.accept(&n, &a, now);
                out.emit.push(c);
            }
            return out;
        }

        self.score_repeat(c, &n, &a, prev_event, &mut out);
        out
    }

    fn handle_novel_with_skip_guard(
        &mut self,
        c: Candidate,
        n: &Normalized,
        a: &Analysis,
        now: Instant,
        out: &mut Outcome,
    ) {
        let settle = self.cfg.skip_settle_window;

        // 1. Single-event concatenated multi-turn skip dump (3+ dialogue turns in one flush)
        if let Some((prefix_dump, last_turn)) = split_multiturn_skip_dump(&c.text) {
            self.accept(n, a, now);
            self.last_novel_at = Some(now);
            self.skip_burst_count = self.skip_burst_count.saturating_add(2).max(3);

            if let Some(prev_tail) = self.skip_tail.take() {
                self.suppress(out, prev_tail.cand, Reason::SkipBurst);
            }

            self.suppress(
                out,
                Candidate::new(prefix_dump, c.thread, now),
                Reason::SkipDump,
            );

            let last_norm = normalize(&last_turn);
            self.skip_tail = Some(SkipTail {
                cand: Candidate::new(last_turn, c.thread, now),
                key: last_norm.key,
                deadline: now + settle,
            });
            out.held = true;
            return;
        }

        // 2. Single-turn novel line
        self.accept(n, a, now);
        let prev_novel_at = self.last_novel_at.replace(now);

        // Case A: A skip_tail is still waiting (line 3+ of a rapid skip wave, or line after SkipDump)
        if let Some(prev_tail) = self.skip_tail.take() {
            self.skip_burst_count += 1;
            self.suppress(out, prev_tail.cand, Reason::SkipBurst);
            self.skip_tail = Some(SkipTail {
                cand: c,
                key: n.key.clone(),
                deadline: now + settle,
            });
            out.held = true;
            return;
        }

        // Case B: Line 2 arriving within settle window of Line 1 -> hold in skip_tail
        if prev_novel_at.is_some_and(|prev| now.saturating_duration_since(prev) < settle) {
            self.skip_burst_count = 2;
            self.skip_tail = Some(SkipTail {
                cand: c,
                key: n.key.clone(),
                deadline: now + settle,
            });
            out.held = true;
            return;
        }

        // Case C: Normal reading pace (Line 1 after quiet gap >= settle) -> emit at 0ms
        self.skip_burst_count = 1;
        out.emit.push(c);
    }

    fn score_repeat(&mut self, c: Candidate, n: &Normalized, a: &Analysis, prev_event: Option<Instant>, out: &mut Outcome) {
        let cfg = self.cfg.clone();
        let now = c.at;
        let tail = self.history.tail();
        let (reference, has_cursor) = match &self.mode {
            Mode::Backlog { cursor, .. } => (Some(*cursor), true),
            Mode::Pending { anchor, .. } => (Some(*anchor), true),
            Mode::Live => match self.last_repeat {
                Some((s, at)) if now.saturating_duration_since(at) <= cfg.continuity_window => (Some(s), true),
                _ => (tail, false),
            },
        };
        let anchors = a.anchors();
        let anchor = match reference {
            Some(r) => anchors.iter().copied().min_by_key(|&s| (s as i64 - r as i64).abs()),
            None => anchors.iter().copied().max(),
        }
        .or(tail)
        .unwrap_or(0);
        let d = reference.map_or(0, |r| anchor as i64 - r as i64);
        let page = cfg.page_jump as i64;
        let dump = a.is_dump();
        let in_backlog = matches!(self.mode, Mode::Backlog { .. });
        let entered_rev = matches!(self.mode, Mode::Backlog { entered_by_reverse: true, .. });
        let forward_ok = cfg.suppress_rereads || entered_rev;

        // Inside a backlog session: anything on the current page is suppressed.
        if in_backlog && (dump || (d.abs() <= page && (d <= 0 || forward_ok))) {
            self.mode = Mode::Backlog { cursor: anchor, last: now, entered_by_reverse: entered_rev };
            self.last_repeat = Some((anchor, now));
            if dump {
                self.names.extend(a.labels.iter().cloned());
            }
            self.suppress(out, c, if dump { Reason::BacklogDump } else { Reason::BacklogScroll });
            return;
        }

        let input = self.last_input_at.is_some_and(|t| now.saturating_duration_since(t) <= cfg.input_window);
        let reverse = d < 0 && -d <= page;
        let mut s = 0i32;
        if dump {
            s += 4;
        }
        if has_cursor && d != 0 && d.abs() <= page && (d < 0 || forward_ok) {
            s += 3;
        }
        if self.last_repeat.is_some_and(|(_, t)| now.saturating_duration_since(t) < cfg.burst_window) {
            s += 2;
        }
        if reverse {
            s += 2;
        }
        if input {
            s += 3;
        }
        if tail.is_some_and(|t| anchor as i64 >= t as i64 - cfg.recent_lines as i64) {
            s += 1;
        }
        let last_novel = self.last_emitted.as_ref().is_some_and(|e| e.novel);
        let quiet = prev_event.is_none_or(|p| now.saturating_duration_since(p) > cfg.isolation_gap);
        if !dump && last_novel && quiet && !input {
            s -= 3; // isolated: catchphrase / hesitation pattern
        }
        if a.chain.len() >= 2 && !a.chain_hard {
            s -= 2; // soft joints: hesitation pattern
        }

        self.last_repeat = Some((anchor, now));

        if s >= cfg.suppress_score {
            let reason = if dump {
                Reason::BacklogDump
            } else if reverse {
                Reason::BacklogScroll
            } else {
                Reason::BacklogRun
            };
            if let Mode::Pending { cand, .. } = std::mem::replace(&mut self.mode, Mode::Live) {
                self.suppress(out, cand, reason);
            }
            if dump {
                self.names.extend(a.labels.iter().cloned());
            }
            self.mode = Mode::Backlog { cursor: anchor, last: now, entered_by_reverse: reverse || dump || input || entered_rev };
            self.suppress(out, c, reason);
            return;
        }

        // Not proven: hold briefly; any non-corroborating event flushes in order.
        self.flush_pending(out);
        self.mode = Mode::Pending { cand: c, key: n.key.clone(), weight: n.weight, anchor, deadline: now + cfg.hold_window };
        out.held = true;
    }

    fn redraw_of(&self, key: &str, now: Instant) -> Option<u64> {
        if let Mode::Pending { key: k, anchor, .. } = &self.mode {
            if k == key {
                return Some(*anchor);
            }
        }
        if let Some(tail) = &self.skip_tail {
            if tail.key == key {
                return self.history.tail().or(Some(0));
            }
        }
        let e = self.last_emitted.as_ref()?;
        let recent = now.saturating_duration_since(e.at) < self.cfg.redraw_window;
        (e.key == key && (recent || matches!(self.mode, Mode::Backlog { .. }))).then_some(e.seq)
    }

    fn flush_pending(&mut self, out: &mut Outcome) {
        if !matches!(self.mode, Mode::Pending { .. }) {
            return;
        }
        if let Mode::Pending { cand, key, weight, .. } = std::mem::replace(&mut self.mode, Mode::Live) {
            let seq = self.history.push(key.clone(), weight);
            self.last_emitted = Some(Emitted { key, seq, at: cand.at, novel: false });
            out.emit.push(cand);
        }
    }

    /// Store an emitted novel / low-info line. Fully covered hard segments
    /// (e.g. NVL page redraw + new sentence) are not stored again.
    fn accept(&mut self, n: &Normalized, a: &Analysis, now: Instant) {
        let split = n.seg_count() > 1 && a.seg_fully_covered.iter().any(|&f| f);
        let seq = if split {
            let mut last = None;
            for i in 0..n.seg_count() {
                if !a.seg_fully_covered[i] {
                    let k = n.key[n.seg_range(i)].to_string();
                    let w = weight_of(&k);
                    last = Some(self.history.push(k, w));
                }
            }
            last.or(self.history.tail())
        } else {
            Some(self.history.push(n.key.clone(), n.weight))
        };
        if let Some(seq) = seq {
            self.last_emitted = Some(Emitted { key: n.key.clone(), seq, at: now, novel: true });
        }
    }

    fn suppress(&mut self, out: &mut Outcome, c: Candidate, reason: Reason) {
        if self.suppressed_ring.len() == self.cfg.suppressed_ring_cap {
            self.suppressed_ring.pop_front();
        }
        self.suppressed_ring.push_back(c.clone());
        out.suppressed.push((c, reason));
    }

    fn note_thread(&mut self, t: ThreadKey, class: Class) {
        let window = self.cfg.thread_window;
        let st = self.threads.entry(t).or_default();
        if matches!(class, Class::Novel) {
            st.novel += 1;
        }
        st.recent.push_back(matches!(class, Class::Repeat));
        if st.recent.len() > window {
            st.recent.pop_front();
        }
    }
}

// ───────────────────────────── tests ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: ThreadKey = ThreadKey(1);
    const SIDE: ThreadKey = ThreadKey(2);

    const STORY: [&str; 30] = [
        "「今日はいい天気だね、散歩でもしようか」",
        "「うん、駅前の公園まで歩いてみよう」",
        "二人は並んで坂道をゆっくりと下っていった。",
        "「そういえば、昨日の宿題はもう終わった？」",
        "「まだ半分しか終わってないんだよね」",
        "風が吹いて、桜の花びらが舞い上がった。",
        "「見て、あそこに猫がいるよ」",
        "「本当だ、白くてふわふわしてるね」",
        "猫はこちらを一瞥すると、塀の向こうへ消えた。",
        "「逃げられちゃったね」",
        "「お腹が空いたから、何か食べに行こう」",
        "「駅前に新しいパン屋ができたらしいよ」",
        "店の前にはすでに長い行列ができていた。",
        "「すごい人だね、少し待つことになりそう」",
        "「せっかく来たんだし、並んでみようよ」",
        "列はゆっくりと進み、やがて店の中に入れた。",
        "「どれも美味しそうで迷っちゃうな」",
        "「私はこのクロワッサンにしようかな」",
        "焼きたての香りが店内いっぱいに広がっている。",
        "「じゃあ僕はメロンパンにするよ」",
        "会計を済ませて、二人は店の外に出た。",
        "「公園のベンチで食べようか」",
        "「いいね、ちょうど日陰になってるし」",
        "遠くで子供たちの笑い声が聞こえる。",
        "「こういう休日も悪くないね」",
        "「また来週も一緒に来ようよ」",
        "空にはいくつもの白い雲が浮かんでいた。",
        "「そろそろ帰ろうか、日が暮れてきた」",
        "「うん、今日は本当に楽しかった」",
        "夕焼けが街全体を赤く染めていた。",
    ];

    struct Sim {
        det: BacklogDetector,
        t0: Instant,
    }

    impl Sim {
        fn new() -> Self {
            Self::with(Config::default())
        }
        fn with(cfg: Config) -> Self {
            Self { det: BacklogDetector::new(cfg), t0: Instant::now() }
        }
        fn at(&self, ms: u64) -> Instant {
            self.t0 + Duration::from_millis(ms)
        }
        fn line_on(&mut self, ms: u64, thread: ThreadKey, text: &str) -> Outcome {
            let at = self.at(ms);
            self.det.on_line(Candidate::new(text, thread, at))
        }
        fn line(&mut self, ms: u64, text: &str) -> Outcome {
            self.line_on(ms, MAIN, text)
        }
        fn tick(&mut self, ms: u64) -> Outcome {
            let at = self.at(ms);
            self.det.on_tick(at)
        }
        /// Live reading: every line must be emitted immediately. Returns last timestamp.
        fn read(&mut self, start_ms: u64, gap_ms: u64, lines: &[&str]) -> u64 {
            let mut t = start_ms;
            for (i, l) in lines.iter().enumerate() {
                t = start_ms + i as u64 * gap_ms;
                let o = self.line(t, l);
                assert_eq!(texts(&o.emit), vec![*l], "live line not emitted immediately: {l}");
                assert!(!o.held && o.suppressed.is_empty());
            }
            t
        }
    }

    fn texts(v: &[Candidate]) -> Vec<&str> {
        v.iter().map(|c| c.text.as_str()).collect()
    }
    fn reasons(o: &Outcome) -> Vec<Reason> {
        o.suppressed.iter().map(|(_, r)| *r).collect()
    }

    // 1 ─ Case 1: multi-line buffer dump.
    #[test]
    fn s1_multiline_dump_is_suppressed() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..8]);
        let dump = STORY[3..8].join("\n");
        let o = s.line(t + 1000, &dump);
        assert!(o.emit.is_empty() && !o.held);
        assert_eq!(reasons(&o), vec![Reason::BacklogDump]);
        assert!(s.tick(t + 1500).emit.is_empty());
        // Reading resumes: new line goes straight through.
        let o = s.line(t + 4000, STORY[8]);
        assert_eq!(texts(&o.emit), vec![STORY[8]]);
    }

    // 2 ─ Case 2: Kirikiri/BGI line-by-line burst – zero leaks thanks to the hold.
    #[test]
    fn s2_line_by_line_burst_is_suppressed_without_leak() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..10]);
        let base = t + 7000;
        let mut emitted = 0;
        let mut suppressed = 0;
        for (k, idx) in (5..=9).rev().enumerate() {
            let o = s.line(base + k as u64 * 2, STORY[idx]);
            emitted += o.emit.len();
            suppressed += o.suppressed.len();
        }
        emitted += s.tick(base + 500).emit.len();
        assert_eq!(emitted, 0, "backlog burst leaked into the hooker");
        assert_eq!(suppressed, 5);
        let o = s.line(base + 5000, STORY[10]);
        assert_eq!(texts(&o.emit), vec![STORY[10]]);
    }

    // 3 ─ Slow wheel scrolling (1 tick/s, no input hook): leaks only the first tick.
    #[test]
    fn s3_slow_wheel_leaks_only_first_tick() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..10]);
        let base = t + 7000;
        let o = s.line(base, STORY[8]);
        assert!(o.held);
        assert_eq!(texts(&s.tick(base + 200).emit), vec![STORY[8]]);
        for (k, idx) in [7usize, 6, 5, 4].into_iter().enumerate() {
            let o = s.line(base + 1000 * (k as u64 + 1), STORY[idx]);
            assert!(o.emit.is_empty(), "tick {k} leaked");
            assert_eq!(o.suppressed.len(), 1);
        }
    }

    // 4 ─ Case 3: quoted flashbacks (one and two quotes) are never muted.
    #[test]
    fn s4_quoted_flashbacks_are_emitted() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..10]);
        let one = format!("{}——あの時の言葉が、今も胸の奥に残っている。", STORY[1]);
        assert_eq!(texts(&s.line(t + 2000, &one).emit), vec![one.as_str()]);
        let two = format!("{}{}あの日の約束を、僕はまだ覚えている。", STORY[3], STORY[4]);
        assert_eq!(texts(&s.line(t + 4000, &two).emit), vec![two.as_str()]);

        // The literal short example: 「行くな」「待ってる」——あの言葉が。
        let mut s = Sim::new();
        let t = s.read(0, 2000, &["「行くな」", STORY[0], "「待ってる」", STORY[1]]);
        let lit = "「行くな」「待ってる」——あの言葉が。";
        assert_eq!(texts(&s.line(t + 2000, lit).emit), vec![lit]);
    }

    // 5 ─ Hesitation merged into one flowing sentence (JP + EN).
    #[test]
    fn s5_hesitation_merge_is_emitted() {
        let mut s = Sim::new();
        let frags = ["「ちょっと待ってくれよ」", "「まだ話は終わってない」", "「行かないでほしいんだ」"];
        let t = s.read(0, 2000, &STORY[..3]);
        let t = s.read(t + 2000, 2000, &frags);
        let merged = "「ちょっと待ってくれよ、まだ話は終わってない、行かないでほしいんだ」";
        let o = s.line(t + 2000, merged);
        assert!(o.suppressed.is_empty());
        let mut out = texts(&o.emit).into_iter().map(String::from).collect::<Vec<_>>();
        out.extend(texts(&s.tick(t + 2300).emit).into_iter().map(String::from));
        assert_eq!(out, vec![merged.to_string()]);

        let mut s = Sim::new();
        let t = s.read(0, 2000, &["Hi", "I am", "X"]);
        let o = s.line(t + 2000, "Hi I am X");
        assert_eq!(texts(&o.emit), vec!["Hi I am X"]);
    }

    // 6 ─ Catchphrase repeated ~20 lines later; flushed in order before the next line.
    #[test]
    fn s6_catchphrase_later_is_emitted_in_order() {
        const CATCH: &str = "「俺は絶対に諦めないからな！」";
        let mut s = Sim::new();
        let mut script: Vec<&str> = STORY[..5].to_vec();
        script.push(CATCH);
        script.extend_from_slice(&STORY[5..24]);
        let t = s.read(0, 2000, &script);
        let o = s.line(t + 2000, CATCH);
        assert!(o.held && o.emit.is_empty() && o.suppressed.is_empty());
        // Fast click to a brand-new line before the hold deadline.
        let o = s.line(t + 2080, STORY[24]);
        assert_eq!(texts(&o.emit), vec![CATCH, STORY[24]]);
        assert!(o.suppressed.is_empty());
    }

    // 7 ─ Ctrl-skip through unread text: every novel line passes with zero delay.
    #[test]
    fn s7_skip_through_unread_text_is_emitted() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..5]);
        s.read(t + 2000, 30, &STORY[5..]);
    }

    // 8 ─ Ctrl-skip through recently read text (load save): policy toggle.
    #[test]
    fn s8_skip_through_recent_reread_follows_policy() {
        let replay = &STORY[5..13];

        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..20]);
        let base = t + 5000;
        let (mut emitted, mut suppressed) = (0, 0);
        for (k, l) in replay.iter().enumerate() {
            let o = s.line(base + 30 * k as u64, l);
            emitted += o.emit.len();
            suppressed += o.suppressed.len();
        }
        emitted += s.tick(base + 1000).emit.len();
        assert_eq!((emitted, suppressed), (0, replay.len()));
        assert_eq!(texts(&s.line(base + 3000, STORY[20]).emit), vec![STORY[20]]);

        let mut s = Sim::with(Config { suppress_rereads: false, ..Config::default() });
        let t = s.read(0, 2000, &STORY[..20]);
        let base = t + 5000;
        let mut out: Vec<String> = vec![];
        for (k, l) in replay.iter().enumerate() {
            let o = s.line(base + 30 * k as u64, l);
            assert!(o.suppressed.is_empty());
            out.extend(o.emit.into_iter().map(|c| c.text));
        }
        out.extend(s.tick(base + 1000).emit.into_iter().map(|c| c.text));
        assert_eq!(out, replay.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    }

    // 9 ─ Redraws: backlog-close redraw of the current line, and focus redraw in Live.
    #[test]
    fn s9_redraws_are_dropped() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..10]);
        let base = t + 7000;
        for (k, idx) in (6..=8).rev().enumerate() {
            s.line(base + k as u64 * 2, STORY[idx]);
        }
        let o = s.line(base + 300, STORY[9]); // backlog closed -> current line redrawn
        assert!(o.emit.is_empty());
        assert_eq!(reasons(&o), vec![Reason::Redraw]);
        assert_eq!(texts(&s.line(base + 3000, STORY[10]).emit), vec![STORY[10]]);

        let o = s.line(base + 4000, STORY[10]); // focus regained
        assert!(o.emit.is_empty());
        assert_eq!(reasons(&o), vec![Reason::Redraw]);
    }

    // ── Argon fixes ──

    #[test]
    fn a1_shared_thread_is_never_blacklisted() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..15]);
        let base = t + 5000;
        for (k, idx) in (3..=14).rev().enumerate() {
            let o = s.line(base + k as u64 * 3, STORY[idx]);
            assert!(o.emit.is_empty());
        }
        let t = s.read(base + 5000, 2000, &STORY[15..20]);
        assert_eq!(s.det.thread_role(MAIN), ThreadRole::Primary);
        let o = s.line(t + 2000, STORY[16]); // legit repeat on the same thread
        assert!(o.suppressed.is_empty());
        assert_eq!(texts(&s.tick(t + 2300).emit), vec![STORY[16]]);
    }

    #[test]
    fn a1_secondary_backlog_thread_is_learned_and_demoted() {
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..20]);
        let base = t + 5000;
        for (k, idx) in (9..=19).rev().enumerate() {
            let o = s.line_on(base + k as u64 * 2, SIDE, STORY[idx]);
            assert!(o.emit.is_empty());
        }
        s.tick(base + 500);
        assert_eq!(s.det.thread_role(SIDE), ThreadRole::Backlog);
        let o = s.line_on(base + 10_000, SIDE, STORY[2]);
        assert_eq!(reasons(&o), vec![Reason::BacklogThread]);
        let fresh = "「これは新しい台詞です、初めて見るよ」";
        assert_eq!(texts(&s.line_on(base + 12_000, SIDE, fresh).emit), vec![fresh]);
        assert_eq!(s.det.thread_role(SIDE), ThreadRole::Primary);
    }

    #[test]
    fn a2_overlapping_history_matches_use_range_union() {
        let mut det = BacklogDetector::default();
        det.seed_history(["今日は朝から雨が降っていた", "雨が降っていたので傘を持って出かけた"]);
        let n = normalize("今日は朝から雨が降っていたので傘を持って出かけた");
        let a = analyze(&n, &det.history, &det.names, &det.cfg);
        assert_eq!(a.residual_w, 0.0);
        assert!(a.is_repeat(&det.cfg));
    }

    #[test]
    fn a3_unbracketed_speaker_names_in_dump_are_labels() {
        let lines = ["「おはよう、今日は早いね」", "「うん、ちょっと用事があってさ」", "「私も一緒に行っていい？」"];
        let mut s = Sim::new();
        let t = s.read(0, 2000, &STORY[..5]);
        let t = s.read(t + 2000, 2000, &lines);
        let dump = format!("直哉\n{}\n優美\n{}\n藍\n{}", lines[0], lines[1], lines[2]);
        let o = s.line(t + 1000, &dump);
        assert!(o.emit.is_empty());
        assert_eq!(reasons(&o), vec![Reason::BacklogDump]);
        assert!(s.det.names.contains("直哉") && s.det.names.contains("藍"));
    }

    #[test]
    fn a4_real_world_vn_backlog_dump_without_newlines() {
        let line1 = "「お嬢様なのに、趣味はおじさんなのか」";
        let line2 = "「おじさんってぇのは心外だが……まぁそうだな、こいつは家族の影響だろうから、そう言われても仕方がない」";
        let line3 = "組長は、その辺りに転がっていたボールを拾い上げた。";
        let dump_from_game = "柊一郎「お嬢様なのに、趣味はおじさんなのか」妙花「おじさんってぇのは心外だが……まぁそうだな、こいつは家族の影響だろうから、そう言われても仕方がない」組長は、その辺りに転がっていたボールを拾い上げた。";

        let mut s = Sim::new();
        let t = s.read(0, 2000, &[line1, line2, line3]);
        let o = s.line(t + 1000, dump_from_game);
        assert!(o.emit.is_empty(), "Real world backlog dump leaked!");
        assert_eq!(reasons(&o), vec![Reason::BacklogDump]);

        // After backlog closes, next novel dialogue emits cleanly:
        let next_line = "「これからどうするんだ？」";
        let o_next = s.line(t + 4000, next_line);
        assert_eq!(texts(&o_next.emit), vec![next_line]);
    }

    #[test]
    fn a5_narration_followed_by_dialogue_dump() {
        let line1 = "「お医者さん嫌いみたいなね。でも性格の問題かしら、あたし必要ないって言われると追いかけたくなっちゃうのよ……！」";
        let line2 = "オレは先生に会いたくて来てるので、会いたくない人の気持ちが分からなかった。";
        let line3 = "絵理子「まぁ！　可愛いらしいこと言ってくれるじゃねーの、お主は～！」";
        let dump_from_game = "絵理子「お医者さん嫌いみたいなね。でも性格の問題かしら、あたし必要ないって言われると追いかけたくなっちゃうのよ……！」オレは先生に会いたくて来てるので、会いたくない人の気持ちが分からなかった。絵理子「まぁ！　可愛いらしいこと言ってくれるじゃねーの、お主は～！」";

        let mut s = Sim::new();
        let t = s.read(0, 2000, &[line1, line2, line3]);
        let o = s.line(t + 1000, dump_from_game);
        assert!(o.emit.is_empty(), "Mixed narration/dialogue backlog dump leaked!");
        assert_eq!(reasons(&o), vec![Reason::BacklogDump]);

        // After backlog closes, next novel dialogue emits cleanly:
        let next_line = "「次の患者さんどうぞ」";
        let o_next = s.line(t + 4000, next_line);
        assert_eq!(texts(&o_next.emit), vec![next_line]);
    }

    // 10 ─ Fast-Skip Wave (3+ novel lines in rapid succession):
    //      Line 1 emits at 0ms, intermediate lines suppressed as SkipBurst, final landing line emits after settle window.
    #[test]
    fn s10_skip_wave_suppresses_intermediate_lines_and_emits_landing_line() {
        let mut s = Sim::with(Config {
            skip_filter_enabled: true,
            skip_settle_window: Duration::from_millis(120),
            ..Config::default()
        });
        let t = s.read(0, 2000, &STORY[..5]);

        // Start fast-skip at t + 2000: 6 unread lines spaced 25ms apart
        let base = t + 2000;
        let o0 = s.line(base, STORY[5]);
        assert_eq!(texts(&o0.emit), vec![STORY[5]], "First line must emit at 0ms");
        assert!(o0.suppressed.is_empty());

        let mut suppressed_lines = Vec::new();
        for (k, &line) in STORY[6..11].iter().enumerate() {
            let o = s.line(base + (k as u64 + 1) * 25, line);
            assert!(o.emit.is_empty(), "Intermediate skip line must not emit immediately");
            assert!(o.held);
            for (cand, reason) in o.suppressed {
                assert_eq!(reason, Reason::SkipBurst);
                suppressed_lines.push(cand.text);
            }
        }

        // STORY[6..10] (4 lines) should have been suppressed as SkipBurst while STORY[10] is held as landing line
        assert_eq!(
            suppressed_lines,
            STORY[6..10].iter().map(|s| s.to_string()).collect::<Vec<_>>()
        );

        // Once 120ms settle window expires after the last line (base + 5*25 + 130), STORY[10] emits!
        let o_land = s.tick(base + 5 * 25 + 130);
        assert_eq!(texts(&o_land.emit), vec![STORY[10]], "Landing line must emit after settle timer");
    }

    // 11 ─ Quick 2-line double-tap (only 2 lines within settle window) does NOT suppress the 2nd line.
    #[test]
    fn s11_quick_double_tap_two_lines_does_not_suppress() {
        let mut s = Sim::with(Config {
            skip_filter_enabled: true,
            skip_settle_window: Duration::from_millis(120),
            ..Config::default()
        });
        let o1 = s.line(1000, STORY[0]);
        assert_eq!(texts(&o1.emit), vec![STORY[0]]);

        let o2 = s.line(1050, STORY[1]);
        assert!(o2.emit.is_empty() && o2.held && o2.suppressed.is_empty());

        let o_tick = s.tick(1200);
        assert_eq!(texts(&o_tick.emit), vec![STORY[1]]);
        assert!(o_tick.suppressed.is_empty());
    }

    // 12 ─ Single-event concatenated multi-turn skip dump suppresses bulk turns and emits only the landing turn.
    #[test]
    fn s12_concatenated_multiturn_skip_dump_suppresses_bulk_and_emits_tail() {
        let mut s = Sim::with(Config {
            skip_filter_enabled: true,
            skip_settle_window: Duration::from_millis(120),
            ..Config::default()
        });
        let dump = "柊一郎「お嬢様なのに、趣味はおじさんなのか」妙花「おじさんってぇのは心外だが……まぁそうだな」組長「さて、そろそろ本題に入ろうか」";
        let o = s.line(1000, dump);
        assert!(o.emit.is_empty() && o.held);
        assert_eq!(reasons(&o), vec![Reason::SkipDump]);
        assert_eq!(
            o.suppressed[0].0.text,
            "柊一郎「お嬢様なのに、趣味はおじさんなのか」\n妙花「おじさんってぇのは心外だが……まぁそうだな」"
        );

        // After settle window expires, only the final landing turn is emitted:
        let o_land = s.tick(1150);
        assert_eq!(texts(&o_land.emit), vec!["組長「さて、そろそろ本題に入ろうか」"]);
    }

    // 13 ─ Single long monologue (350+ chars, no multi-turn skip dump structure) is never split or suppressed.
    #[test]
    fn s13_long_monologue_over_300_chars_is_never_split_or_suppressed() {
        let mut s = Sim::with(Config {
            skip_filter_enabled: true,
            skip_settle_window: Duration::from_millis(120),
            ..Config::default()
        });
        let long_line = "「あの日の夕暮れ、駅のホームで君が言った言葉を今でも鮮明に覚えている。風が強く吹いていて、遠くの踏切の警報機が鳴り響く中で、僕たちは何も言えずにただ立ち尽くしていた。季節がいくつも巡って、街の景色がどれだけ変わってしまっても、あの瞬間の温度だけは決して消えることがないのだと、今になってようやく気がついたんだ。これからもずっと、忘れることはないだろう。」";
        assert!(long_line.chars().count() > 160);
        let o = s.line(2000, long_line);
        assert_eq!(texts(&o.emit), vec![long_line], "Long single-turn dialogue must emit at 0ms untouched");
        assert!(o.suppressed.is_empty() && !o.held);
    }
}
