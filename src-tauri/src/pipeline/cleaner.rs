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
static NAME_BEFORE_QUOTE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)^\s*[^「『。、！？!?,.\r\n]{1,15}?\s*(「.*」|『.*』)\s*$").unwrap()
});
static MULTI_SPACE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t\u{3000}]{2,}").unwrap());
static MULTI_NEWLINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{2,}").unwrap());

static JAPANESE_KANJI_SET: Lazy<HashSet<char>> = Lazy::new(|| {
    concat!(
        "亜哀挨愛曖悪握圧扱宛嵐安案暗以衣位囲医依委威為畏胃尉異移萎偉椅彙意違維慰遺緯域育一壱逸茨芋引印因咽姻員院陰飲隠韻",
        "右宇羽雨唄鬱運雲永泳英映栄営詠影鋭衛易疫益液駅悦謁越閲円園宴延怨沿炎垣演遠鉛塩縁艶汚於王欧殴桜翁奥横沖億屋憶臆虞乙俺卸音恩温穏",
        "下化仮何価佳加可夏嫁家寡科暇果架歌河火禍菓花荷華課貨過蚊我牙画臥芽賀雅餓介回灰会快戒改怪拐悔海界皆械絵開階塊楷解潰壊懐諧貝外劾害崖涯街慨蓋該概骸",
        "格核殻獲確穫覚角較隔閣革学岳楽額掛潟括活喝渇割葛滑褐轄且株釜鎌刈干刊甘汗缶完肝官冠巻看陥乾勘患貫寒喚堪換敢棺款間閑勧寛幹感漢慣管関歓監緩憾還館環簡観韓艦鑑",
        "丸含岸岩玩眼頑顔願企伎危机気希忌汽奇祈季紀軌既記起飢鬼帰基寄規亀喜幾揮期棋貴棄毀旗器畿輝機騎技宜偽欺儀戯擬犠議菊吉喫詰却客脚逆虐",
        "九久及弓丘旧休吸泣求球究窮級糾給居巨拒拠挙虚許距漁御凶共叫狂京享供協況峡挟教橋狭矯胸脅興郷鏡競響驚仰暁業局曲極玉桐均近金菌勤琴筋僅禁緊錦謹襟吟",
        "区句苦駆具惧愚空偶遇隅串屈掘窟熊繰桑勲君薫訓群軍郡径恵啓契型形慶憩掲携敬景桂渓経蛍計詣警鶏芸迎鯨劇撃激傑欠決潔穴結血月",
        "件倹剣圏堅嫌建憲懸検権献肩見謙賢軒遣険顕験幻弦減源玄現言限個古呼固孤弧故枯湖虎誇跨雇顧鼓互五午呉娯後御悟語誤護",
        "交侯候光公功効厚口向后好工巧康幸拘控攻更洪浩港硬絞綱耕考肯航荒行衡講貢購郊酵鋼降項香高剛号合拷豪克刻告国穀酷獄骨込今困昆根婚恨混魂墾懇",
        "左佐唆差仕再作削坐昨詐索鎖座査砂殺雑皿参山惨傘散桟暫士仕史司四市矢旨死糸至私使刺始姉姿思指支施師恣時枝止氏祉視詞試詩資飼誌雌嗣賜諮示字寺次耳自似児事辞治慈磁璽",
        "鹿式識軸七執失室湿嫉漆疾質実芝写社車捨遮謝邪蛇煮借若弱寂手主取守朱殊珠種趣酒首儒受呪寿授樹需囚収周宗拾秋臭舟衆習週就衝愁蹴集醜什住充十従柔汁渋獣縦叔祝宿淑粛縮塾熟出術述俊春瞬准循旬殉順純巡遵盾醇",
        "処初所書庶暑緒署諸女如助序叙徐除傷償升召商唱奨妨娼将小少尚床彰承抄招掌捷昇昭晶松梢標章粧紹肖訟証象像焦硝礁祥称笑詳鐘丈冗乗剰城場壌嬢常情条浄状畳穣蒸譲醸錠",
        "植殖織職色食触信伸侵唇娠審心振晋森深申真神秦紳臣芯薪親診身辛進針震人仁刃塵尽迅陣吹炊睡粋酔推髄崇数枢据杉澄寸",
        "世瀬是制勢姓征性成政整星晴正清生盛精聖声製西誠請逝青静斉税隻席惜斥析石積籍責赤跡切拙折窃接設雪摂節説舌絶千仙占先宣川戦扇栓泉浅洗染潜煎線繊薦践遷選銭銑鮮前善漸然全禅繕",
        "塑措組租素祖疎阻遡礎双壮争想倉喪掃操早槽曹巣奏窓装相走蒼草葬藻送贈造促捉即束測足速俗属賊族続卒率存村孫尊損",
        "他多汰打妥唾堕惰駄太対体耐待怠胎退帯泰滞態代大貸袋逮替第題択沢宅達脱奪棚谷旦担炭短端綻誕鍛団壇弾断暖段男談",
        "値知地弛恥智池痴稚置緻致遅馳築竹畜蓄逐秩窒茶嫡着中仲宙忠抽昼柱注虫衷鋳駐朝弔徴懲挑暢潮町眺聴脹超跳長頂鳥勅朕沈珍賃鎮陳",
        "墜追痛通塚漬坪釣亭低停偵貞呈堤定帝底庭廷弟抵締諦泥的笛摘滴適敵溺迭哲鉄徹撤天典店点展添転田伝殿澱電",
        "吐塗徒斗渡登賭途都怒土奴努度逃刀冬党凍唐塔島悼投搭東桃棟盗湯痘答等筒統稲踏糖頭謄藤闘騰同働堂導洞童胴道銅峠匿得徳毒独読凸突届豚曇",
        "那奈内謎鍋軟難二尼弐匂虹日入乳尿任妊忍認濡根寝年念燃粘悩",
        "把覇婆罵拝杯排敗廃輩培媒梅買売賠陪伯拍泊白舶薄迫漠爆縛麦発髪伐罰抜閥飯晩番盤般搬斑版板汎帆藩煩頒販範繁",
        "卑否妃彼悲扉批披比皮疲碑秘罷肥被費避非飛備尾微美鼻匹必泌筆百俵標氷漂票表評描秒病苗品浜貧賓頻敏",
        "府侮付婦富布怖扶敷普浮父符腐赴負賦複覆払沸仏物噴墳憤奮分文聞紛雰兵塀幣平弊柄米壁癖別蔑片変辺返遍編",
        "保圃捕歩補舗募墓暮母慕簿倣俸包呆報奉宝峰崩抱放方泡砲縫胞芳訪蜂邦飽乏亡傍剖坊妨忘忙房某冒帽望棒貌暴膨謀",
        "北木朴牧睦僕墨撲没本奔翻凡盆埋幕枚毎膜又魔磨摩麻繭満慢漫末抹万未味魅岬密蜜脈妙民眠矛務夢霧銘鳴迷滅免綿面",
        "茂模毛妄盲耗猛網目黙問紋門野冶夜爺厄役薬訳躍柔愉油癒諭輸唯優勇友幽悠憂有猶裕誘遊郵雄融与予余誉預幼用庸揺溶腰様洋羊葉要陽",
        "絡落拉裸頼雷乱卵覧濫藍利吏履理痢裏里離陸律略留流柳竜隆両量僚寮凌猟療糧陵領倫厘林臨輪隣塁涙累類",
        "令例冷励礼列劣烈裂廉練連恋錬炉賂路露老労郎朗浪漏惑枠湾腕"
    )
    .chars()
    .collect()
});

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

    // Strip unpaired UTF-16 surrogates, NULL chars, zero-width spaces, or BOM
    text.retain(|c| c != '\0' && c != '\u{200B}' && c != '\u{FEFF}' && c != '\u{FFFD}');

    if text.trim().is_empty() {
        return String::new();
    }

    // 1. Strip HTML / Ruby / Engine control tags first
    if config.strip_html_ruby_tags {
        text = RUBY_RT_RE.replace_all(&text, "").into_owned();
        text = HTML_TAG_RE.replace_all(&text, "").into_owned();
        text = ENGINE_TAG_RE.replace_all(&text, "").into_owned();
    }

    // 2. Optional speaker bracket & speaker name prefix removal BEFORE repeat dedup
    if config.remove_speaker_brackets {
        text = SPEAKER_BRACKET_RE.replace_all(&text, "").into_owned();
        text = NAME_BEFORE_QUOTE_RE.replace(&text, "$1").into_owned();
    }

    // 3. Remove repeated characters (Textractor "Remove Repeated Characters" equivalent)
    if config.remove_repeated_chars {
        text = remove_repeated_characters(&text);
    }

    // 4. Remove repeated phrases (Textractor "Remove Repeated Phrases" equivalent)
    if config.remove_repeated_phrases {
        text = remove_repeated_phrases(&text);
    }

    // 5. Filter dual-language Chinese segments while keeping Japanese segments
    if config.filter_chinese_segments {
        text = filter_chinese_segments(&text);
        if text.trim().is_empty() {
            return String::new();
        }
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

fn has_kana(segment: &str) -> bool {
    segment
        .chars()
        .any(|c| ('\u{3040}'..='\u{30FF}').contains(&c) || ('\u{FF66}'..='\u{FF9F}').contains(&c))
}

fn is_jp_kanji_segment(segment: &str) -> bool {
    let mut cjk_count = 0usize;
    for c in segment.chars() {
        if ('\u{4E00}'..='\u{9FAF}').contains(&c) {
            cjk_count += 1;
            if !JAPANESE_KANJI_SET.contains(&c) {
                return false;
            }
        }
    }
    cjk_count > 0
}

fn has_any_cjk(segment: &str) -> bool {
    segment
        .chars()
        .any(|c| ('\u{4E00}'..='\u{9FAF}').contains(&c))
}

/// Native Rust replacement for `mypost.py` dual-language JP+CN filter.
/// Splits hooked output on sentence/clause boundaries (`？！。」』\n`) and strips
/// Chinese-only translation segments while preserving Japanese kana/kanji segments.
pub fn filter_chinese_segments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut segment = String::new();
    let mut post_split = false;

    let flush_segment = |seg: &str, post_split_flag: bool, out: &mut String| {
        if seg.trim().is_empty() {
            return;
        }
        if has_kana(seg) || !has_any_cjk(seg) {
            out.push_str(seg);
        } else if !post_split_flag && is_jp_kanji_segment(seg) {
            out.push_str(seg);
        }
    };

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        segment.push(c);
        if matches!(c, '\u{FF1F}' | '\u{FF01}' | '\u{3002}' | '\u{300D}' | '\u{300F}' | '\n') {
            // Consume any immediately following closing brackets or sentence enders as part of the same clause
            while i + 1 < chars.len()
                && matches!(
                    chars[i + 1],
                    '\u{FF1F}' | '\u{FF01}' | '\u{3002}' | '\u{300D}' | '\u{300F}' | '\u{FF09}'
                )
            {
                i += 1;
                segment.push(chars[i]);
            }
            flush_segment(&segment, post_split, &mut result);
            segment.clear();
            post_split = true;
        }
        i += 1;
    }

    if !segment.is_empty() {
        flush_segment(&segment, post_split, &mut result);
    }

    result
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

    #[test]
    fn test_dual_language_chinese_filter_and_speaker_quote() {
        let mut cfg = AppConfig::default();
        cfg.remove_speaker_brackets = true;
        cfg.filter_chinese_segments = true;

        // 1. Dual-language JP + CN (both unbracketed and bracketed Chinese translation)
        assert_eq!(
            clean_sentence("「おはよう！」早上好！", None, &cfg),
            "「おはよう！」"
        );
        assert_eq!(
            clean_sentence("「おはよう！」「早上好！」", None, &cfg),
            "「おはよう！」"
        );

        // 2. Speaker name before 「...」 (from mypost.py #2) + invisible chars
        assert_eq!(
            clean_sentence("\u{FEFF}小鳥 「今日もいい天気だね！」\u{200B}", None, &cfg),
            "「今日もいい天気だね！」"
        );
    }
}
