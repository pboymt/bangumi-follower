use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct Episode {
    pub title_en: Option<String>,
    pub title_zh: Option<String>,
    pub title_jp: Option<String>,
    pub season: i32,
    pub season_raw: String,
    pub episode: i32,
    pub sub: String,
    pub group: String,
    pub resolution: String,
    pub source: String,
}

static TITLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(.*?|\[.*])((?: ?-) ?\d+|\[\d+]|(.*))").unwrap()
});
static RESOLUTION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"1080|720|2160|4K").unwrap()
});
static SOURCE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"B-Global|[Bb]aha|[Bb]ilibili|AT-X|Web").unwrap()
});
static EPISODE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\d+").unwrap()
});
static SEASON_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)S(\d+)|Season\s*(\d+)|第(\d+)[季期]").unwrap()
});

fn has_jp(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}'))
}

fn has_zh(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '\u{4e00}'..='\u{9fff}'))
}

fn has_en(s: &str) -> bool {
    let has_alpha = s.chars().any(|c| c.is_ascii_alphabetic());
    has_alpha && s.chars().all(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || c.is_ascii_whitespace() || ":-.:'!?,\"&".contains(c))
}

fn pre_process(raw: &str) -> String {
    raw.replace('\u{3010}', "[").replace('\u{3011}', "]")
}

fn find_tags(text: &str) -> (String, String, String) {
    let resolution = RESOLUTION_RE.find(text).map(|m| m.as_str().to_string()).unwrap_or_default();
    let source = SOURCE_RE.find(text).map(|m| m.as_str().to_string()).unwrap_or_default();
    let sub = if text.contains("简繁") || text.contains("繁简") { "简繁".to_string() }
    else if text.contains("简") { "简".to_string() }
    else if text.contains("繁") || text.contains("cht") || text.contains("tc") { "繁".to_string() }
    else { String::new() };
    (resolution, source, sub)
}

fn name_process(name: &str) -> (Option<String>, Option<String>, Option<String>) {
    let has_en_chars = has_en(name);
    let has_jp_chars = has_jp(name);
    let has_zh_chars = has_zh(name);

    if has_en_chars && !has_zh_chars && !has_jp_chars {
        (Some(name.to_string()), None, None)
    } else if has_zh_chars && !has_en_chars {
        (None, Some(name.to_string()), None)
    } else if has_jp_chars && !has_zh_chars {
        (None, None, Some(name.to_string()))
    } else {
        (Some(name.to_string()), Some(name.to_string()), None)
    }
}

pub fn raw_parser(raw: &str) -> Option<Episode> {
    let processed = pre_process(raw);
    let (group, rest) = get_group(&processed);

    let caps = TITLE_RE.captures(&rest)?;
    let title_part = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
    let episode_str = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("1");
    let extra = caps.get(3).map(|m| m.as_str()).unwrap_or("");

    let title = prefix_process(title_part);

    let (season, season_raw) = season_process(&title);
    let clean_title = if season_raw.is_empty() {
        title.clone()
    } else {
        SEASON_RE.replace(&title, "").trim().to_string()
    };

    let (title_en, title_zh, title_jp) = name_process(&clean_title);

    let combined = format!("{episode_str} {extra}");
    let episode: i32 = EPISODE_RE.find(&combined)
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .unwrap_or(1);

    let (resolution, source, sub) = find_tags(&combined);

    Some(Episode {
        title_en,
        title_zh,
        title_jp,
        season,
        season_raw,
        episode,
        sub,
        group,
        resolution,
        source,
    })
}

fn get_group(text: &str) -> (String, String) {
    if let Some(pos) = text.rfind(|c| c == ']' || c == '\u{3011}') {
        let group = &text[1..pos];
        let rest = text[pos+1..].trim().to_string();
        (group.to_string(), rest)
    } else {
        (String::new(), text.to_string())
    }
}

fn prefix_process(title: &str) -> String {
    let prefixes = ["新番", "港澳台地區", "港澳台地区", "台湾", "繁"];
    let mut result = title.to_string();
    for p in prefixes {
        if result.starts_with(p) {
            result = result[p.len()..].trim().to_string();
            break;
        }
    }
    result
}

fn season_process(title: &str) -> (i32, String) {
    if let Some(caps) = SEASON_RE.captures(title) {
        let season: i32 = caps.iter().skip(1).flatten()
            .filter_map(|m| m.as_str().parse::<i32>().ok())
            .next().unwrap_or(1);
        (season, caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default())
    } else {
        (1, String::new())
    }
}
