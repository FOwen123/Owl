//! Owl: spoken numbers written as digits ("five point five" → 5.5, 百分之二十
//! → 20%). The speech models Owl uses for English and Chinese write numbers
//! as words, and the AI cleanup doesn't always fix them. Only clear cases are
//! changed: small numbers on their own ("one of them", 一個) and idioms
//! (萬一, 十分) stay words.

/// English and Chinese numbers in `text`, as digits where the meaning is clear.
pub fn format_numbers(text: &str) -> String {
    chinese(&english(text))
}

// --- English ------------------------------------------------------------

fn small(word: &str) -> Option<u64> {
    const WORDS: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 8] = [
        "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    let w = word.to_ascii_lowercase();
    if let Some(i) = WORDS.iter().position(|x| *x == w) {
        return Some(i as u64);
    }
    TENS.iter()
        .position(|x| *x == w)
        .map(|i| (i as u64 + 2) * 10)
}

fn multiplier(word: &str) -> Option<u64> {
    match word.to_ascii_lowercase().as_str() {
        "hundred" => Some(100),
        "thousand" => Some(1_000),
        "million" => Some(1_000_000),
        "billion" => Some(1_000_000_000),
        _ => None,
    }
}

/// "first" … "twentieth", "hundredth": ordinal number words.
fn is_ordinal(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    if matches!(
        w.as_str(),
        "first" | "second" | "third" | "fifth" | "eighth" | "ninth" | "twelfth"
    ) {
        return true;
    }
    let stem = |suffix: &str| w.strip_suffix(suffix).map(str::to_string);
    stem("ieth").is_some_and(|s| small(&format!("{s}y")).is_some())
        || stem("th").is_some_and(|s| small(&s).is_some() || multiplier(&s).is_some())
}

fn is_number_word(w: &str) -> bool {
    small(w).is_some() || multiplier(w).is_some()
}

/// A word of the text and where it sits, so the text between words survives.
struct Word<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn words(text: &str) -> Vec<Word<'_>> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        let part = c.is_ascii_alphabetic();
        match (part, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push(Word {
                    text: &text[s..i],
                    start: s,
                    end: i,
                });
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(Word {
            text: &text[s..],
            start: s,
            end: text.len(),
        });
    }
    out
}

/// The value of a run of number words, or `None` if it isn't one number
/// ("one two", or a run that starts with "hundred").
fn value(run: &[&str]) -> Option<u64> {
    if run.is_empty() || multiplier(run[0]).is_some() {
        return None;
    }
    let (mut total, mut current) = (0u64, 0u64);
    let mut last_small: Option<u64> = None;
    for w in run {
        if let Some(n) = small(w) {
            // "twenty five" adds; "five twenty" or "one two" isn't one number.
            if let Some(prev) = last_small {
                if !(prev >= 20 && prev % 10 == 0 && n < 10) {
                    return None;
                }
            }
            current += n;
            last_small = Some(n);
        } else if let Some(m) = multiplier(w) {
            if m == 100 {
                current = current.max(1) * 100;
            } else {
                total += current.max(1) * m;
                current = 0;
            }
            last_small = None;
        }
    }
    Some(total + current)
}

/// Two-digit pairs read as a year or a code: "nineteen eighty four" → 1984,
/// "eighty eighty" → 8080.
fn pairs(run: &[&str]) -> Option<String> {
    if run.iter().any(|w| multiplier(w).is_some()) {
        return None;
    }
    let mut groups: Vec<u64> = Vec::new();
    let mut i = 0;
    while i < run.len() {
        let n = small(run[i])?;
        if n >= 20 && n % 10 == 0 && i + 1 < run.len() && small(run[i + 1])? < 10 {
            groups.push(n + small(run[i + 1])?);
            i += 2;
        } else {
            groups.push(n);
            i += 1;
        }
    }
    (groups.len() == 2 && groups.iter().all(|g| (10..100).contains(g)))
        .then(|| format!("{}{:02}", groups[0], groups[1]))
}

fn english(text: &str) -> String {
    let ws = words(text);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < ws.len() {
        if !is_number_word(ws[i].text) {
            i += 1;
            continue;
        }
        // The run: number words joined by spaces, hyphens or "and" after a
        // hundred ("one hundred and five").
        let mut j = i + 1;
        while j < ws.len() {
            let gap = &text[ws[j - 1].end..ws[j].start];
            let joined = gap == " " || gap == "-";
            if joined && is_number_word(ws[j].text) {
                j += 1;
            } else if joined
                && ws[j].text.eq_ignore_ascii_case("and")
                && multiplier(ws[j - 1].text) == Some(100)
                && j + 1 < ws.len()
                && text[ws[j].end..ws[j + 1].start] == *" "
                && small(ws[j + 1].text).is_some()
            {
                j += 2;
            } else {
                break;
            }
        }
        let run: Vec<&str> = ws[i..j]
            .iter()
            .map(|w| w.text)
            .filter(|w| !w.eq_ignore_ascii_case("and"))
            .collect();
        let next = |k: usize| ws.get(k).filter(|w| text[ws[k - 1].end..w.start] == *" ");
        // "twenty-first", "one hundred second": an ordinal stays words.
        let joined_next = ws
            .get(j)
            .filter(|w| matches!(&text[ws[j - 1].end..w.start], " " | "-"));
        if joined_next.is_some_and(|w| is_ordinal(w.text)) {
            i = j + 1;
            continue;
        }

        let mut digits = None;
        let mut end = j;
        // "five point five", "three point one four"
        if next(j).is_some_and(|w| w.text.eq_ignore_ascii_case("point")) {
            let mut k = j + 1;
            let mut decimals = String::new();
            while let Some(d) = next(k).and_then(|w| small(w.text)).filter(|d| *d < 10) {
                decimals.push_str(&d.to_string());
                k += 1;
            }
            if let (Some(whole), false) = (value(&run), decimals.is_empty()) {
                digits = Some(format!("{whole}.{decimals}"));
                end = k;
            }
        }
        if digits.is_none() {
            digits = value(&run)
                .map(|v| v.to_string())
                .or_else(|| pairs(&run))
                .or_else(|| {
                    // "eight zero eight zero": three or more single digits.
                    let ds: Option<Vec<u64>> =
                        run.iter().map(|w| small(w).filter(|d| *d < 10)).collect();
                    ds.filter(|d| d.len() >= 3)
                        .map(|d| d.iter().map(|x| x.to_string()).collect())
                });
        }
        let Some(mut digits) = digits else {
            i = j;
            continue;
        };
        let unit = next(end).map(|w| w.text.to_ascii_lowercase());
        let clear = match unit.as_deref() {
            Some("percent") => {
                digits.push('%');
                end += 1;
                true
            }
            Some("dollars" | "dollar") => {
                digits.insert(0, '$');
                end += 1;
                true
            }
            // A single small word ("one of them", "two people") stays a word.
            _ => digits.contains('.') || run.len() > 1 || value(&run).is_some_and(|v| v >= 10),
        };
        if clear {
            out.push_str(&text[copied..ws[i].start]);
            out.push_str(&digits);
            copied = ws[end - 1].end;
        }
        i = end;
    }
    out.push_str(&text[copied..]);
    out
}

// --- Chinese ------------------------------------------------------------

fn digit(c: char) -> Option<u64> {
    Some(match c {
        '零' | '〇' => 0,
        '一' => 1,
        '二' | '兩' | '两' => 2,
        '三' => 3,
        '四' => 4,
        '五' => 5,
        '六' => 6,
        '七' => 7,
        '八' => 8,
        '九' => 9,
        _ => return None,
    })
}

fn unit(c: char) -> Option<u64> {
    Some(match c {
        '十' => 10,
        '百' => 100,
        '千' => 1_000,
        '萬' | '万' => 10_000,
        '億' | '亿' => 100_000_000,
        _ => return None,
    })
}

fn cn_value(s: &[char]) -> Option<u64> {
    let (mut total, mut section, mut current) = (0u64, 0u64, 0u64);
    for &c in s {
        if let Some(d) = digit(c) {
            current = d;
            continue;
        }
        let u = unit(c)?;
        if u >= 10_000 {
            total += (section + current) * u;
            section = 0;
        } else {
            // 十 alone is ten; 百 or 千 need a digit before them.
            section += current.max(if u == 10 { 1 } else { 0 }) * u;
        }
        current = 0;
    }
    Some(total + section + current)
}

/// Measure words and units after which a number is clearly a quantity.
const MEASURES: &[&str] = &[
    "個", "个", "人", "位", "元", "塊", "块", "年", "月", "日", "號", "号", "天", "歲", "岁", "次",
    "分鐘", "分钟", "小時", "小时", "秒", "公里", "公斤", "頁", "页", "張", "张", "本", "件", "倍",
    "週", "周", "條", "条", "台", "家", "間", "间",
];

fn chinese(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let is_num = |c: char| digit(c).is_some() || unit(c).is_some();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        // 百分之二十 → 20%
        if chars[i..].starts_with(&['百', '分', '之']) {
            let start = i + 3;
            let (n, end) = cn_number(&chars, start);
            if let Some(n) = n {
                out.push_str(&n);
                out.push('%');
                i = end;
                continue;
            }
        }
        if !is_num(chars[i]) || unit(chars[i]).is_some_and(|u| u > 10) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && is_num(chars[j]) {
            j += 1;
        }
        let run = &chars[i..j];
        let rest: String = chars[j..].iter().take(2).collect();
        let decimal = chars.get(j) == Some(&'點') || chars.get(j) == Some(&'点');
        let converted = if decimal && chars.get(j + 1).is_some_and(|c| digit(*c).is_some()) {
            // Only a whole decimal; a clock time or idiom stays as said.
            let (n, end) = cn_number(&chars, i);
            n.filter(|n| n.contains('.')).map(|n| (n, end))
        } else if run.len() == 4
            && run.iter().all(|c| digit(*c).is_some())
            && rest.starts_with('年')
        {
            Some((
                run.iter()
                    .filter_map(|c| digit(*c))
                    .map(|d| d.to_string())
                    .collect(),
                j,
            ))
        } else if run.iter().any(|c| unit(*c).is_some())
            && MEASURES.iter().any(|m| rest.starts_with(m))
        {
            cn_value(run).map(|v| (v.to_string(), j))
        } else {
            None
        };
        match converted {
            Some((n, end)) => {
                out.push_str(&n);
                i = end;
            }
            None => {
                out.extend(run);
                i = j;
            }
        }
    }
    out
}

/// A Chinese number from `start`, with decimals after 點: its digits and where it ends.
fn cn_number(chars: &[char], start: usize) -> (Option<String>, usize) {
    let mut j = start;
    while j < chars.len() && (digit(chars[j]).is_some() || unit(chars[j]).is_some()) {
        j += 1;
    }
    let Some(whole) = cn_value(&chars[start..j]).filter(|_| j > start) else {
        return (None, start);
    };
    if matches!(chars.get(j), Some('點' | '点')) {
        let mut k = j + 1;
        let mut decimals = String::new();
        while let Some(d) = chars.get(k).and_then(|c| digit(*c)) {
            decimals.push_str(&d.to_string());
            k += 1;
        }
        if !decimals.is_empty() && decimal_ends(chars, k) {
            return (Some(format!("{whole}.{decimals}")), k);
        }
    }
    (Some(whole.to_string()), j)
}

/// Whether a decimal like 二點五 ends at `k`: at the end, before non-Chinese
/// text, a unit or a particle. Clock times (一點五十分, 三點五分) and idioms
/// (一點一滴) go on with something else and stay words.
fn decimal_ends(chars: &[char], k: usize) -> bool {
    const AFTER: &[&str] = &[
        "倍", "萬", "万", "億", "亿", "元", "塊", "块", "公里", "公斤", "公升", "米", "版", "秒",
        "小時", "小时", "度", "的", "了", "吧", "啊", "呢", "嗎", "吗", "和", "跟", "或", "到",
    ];
    let Some(&next) = chars.get(k) else {
        return true;
    };
    if !crate::correction::is_cjk(next) {
        return true;
    }
    let rest: String = chars[k..].iter().take(2).collect();
    AFTER.iter().any(|a| rest.starts_with(a))
}

#[cfg(test)]
mod tests {
    use super::format_numbers as f;

    #[test]
    fn english_numbers_become_digits_when_clear() {
        assert_eq!(f("use five point five high"), "use 5.5 high");
        assert_eq!(f("pi is three point one four"), "pi is 3.14");
        assert_eq!(f("about twenty percent of users"), "about 20% of users");
        assert_eq!(f("it costs five dollars"), "it costs $5");
        assert_eq!(f("we have twenty five people"), "we have 25 people");
        assert_eq!(f("one hundred and five days"), "105 days");
        assert_eq!(f("two thousand twenty six"), "2026");
        assert_eq!(f("back in nineteen eighty four"), "back in 1984");
        assert_eq!(f("set the port to eighty eighty"), "set the port to 8080");
        assert_eq!(f("the code is eight zero eight zero"), "the code is 8080");
        assert_eq!(f("Twenty-five, maybe"), "25, maybe");
        assert_eq!(f("top ten"), "top 10");
    }

    #[test]
    fn small_and_idiomatic_numbers_stay_words() {
        for s in [
            "one of them",
            "no one knows",
            "two people came",
            "a hundred times",
            "hundreds of users",
            "at one point we left",
            "Someone said one, two",
            "the twenty-first version",
            "the one hundred second time",
            "his twenty fifth birthday",
        ] {
            assert_eq!(f(s), s);
        }
    }

    #[test]
    fn chinese_numbers_become_digits_when_clear() {
        assert_eq!(f("大概百分之二十的人"), "大概20%的人");
        assert_eq!(f("版本二點五"), "版本2.5");
        assert_eq!(f("版本二點五的功能"), "版本2.5的功能");
        assert_eq!(f("快了二點五倍"), "快了2.5倍");
        assert_eq!(f("二零二六年的計畫"), "2026年的計畫");
        assert_eq!(f("我們有二十五個人"), "我們有25個人");
        assert_eq!(f("十二月十五號開會"), "12月15號開會");
        assert_eq!(f("三百塊"), "300塊");
        assert_eq!(f("等十分鐘"), "等10分鐘");
        assert_eq!(f("兩萬三千人"), "23000人");
    }

    #[test]
    fn chinese_idioms_and_small_numbers_stay() {
        for s in [
            "萬一下雨",
            "十分好看",
            "一個人",
            "一點點",
            "三點開會",
            "一起去",
            "三心二意",
            // Clock times and idioms aren't decimals.
            "一點五十分開會",
            "三點五分到",
            "一點一滴累積經驗",
        ] {
            assert_eq!(f(s), s);
        }
    }

    #[test]
    fn mixed_text_keeps_both() {
        assert_eq!(
            f("Owl 二點五 is about twenty percent faster"),
            "Owl 2.5 is about 20% faster"
        );
    }
}
