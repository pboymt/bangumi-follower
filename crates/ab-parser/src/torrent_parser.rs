use lru::LruCache;
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Mutex;

static PARSER_CACHE: Lazy<Mutex<LruCache<String, Option<ParsedFile>>>> = Lazy::new(|| {
    Mutex::new(LruCache::new(512.try_into().unwrap()))
});

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub media_path: String,
    pub group: Option<String>,
    pub title: String,
    pub season: i32,
    pub episode: String,
    pub suffix: String,
}

#[derive(Debug, Clone)]
pub struct ParsedSubtitle {
    pub media_path: String,
    pub group: Option<String>,
    pub title: String,
    pub season: i32,
    pub episode: String,
    pub suffix: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    Media,
    Subtitle,
}

const SUBTITLE_LANG: &[(&str, &[&str])] = &[
    ("zh-tw", &["tc", "cht", "繁", "zh-tw"]),
    ("zh", &["sc", "chs", "简", "zh"]),
];

pub fn get_group(group_and_title: &str) -> (Option<String>, String) {
    if let Some(pos) = group_and_title.rfind(|c| c == ']' || c == '】') {
        let group = &group_and_title[1..pos];
        let title = group_and_title[pos+1..].trim().to_string();
        (Some(group.to_string()), title)
    } else {
        (None, group_and_title.to_string())
    }
}

pub fn get_season_and_title(season_and_title: &str) -> (String, i32) {
    let re = Lazy::new(|| Regex::new(r"(?i)S(\d+)|Season\s*(\d+)|第(\d+)[季期]").unwrap());
    if let Some(caps) = re.captures(season_and_title) {
        let season: i32 = caps.iter().skip(1).flatten()
            .filter_map(|m| m.as_str().parse::<i32>().ok())
            .next().unwrap_or(1);
        let title = re.replace(season_and_title, "").trim().to_string();
        (title, season)
    } else {
        (season_and_title.to_string(), 1)
    }
}

pub fn get_subtitle_lang(filename: &str) -> Option<&'static str> {
    for (lang, keywords) in SUBTITLE_LANG {
        if keywords.iter().any(|k| filename.contains(k)) {
            return Some(lang);
        }
    }
    None
}

pub fn torrent_parser(
    torrent_path: &str,
    torrent_name: Option<&str>,
    season: Option<i32>,
    file_type: FileType,
) -> Option<ParsedFile> {
    let cache_key = format!("{}|{:?}|{:?}|{:?}", torrent_path, torrent_name, season, file_type);
    {
        let mut cache = PARSER_CACHE.lock().unwrap();
        if let Some(result) = cache.get(&cache_key) {
            return result.clone();
        }
    }

    let filename = std::path::Path::new(torrent_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(torrent_path);
    let name = torrent_name.unwrap_or(filename);

    let rules: &[&str] = &[
        r"(?i)(.*) - (\d{1,4}(?:\.\d{1,2})?(?!\d|p))(?:v\d{1,2})?(?: )?(?:END)?(.*)",
        r"(?i)(.*)[\[\ E](\d{1,4}(?:\.\d{1,2})?)(?:v\d{1,2})?(?: )?(?:END)?[\]\ ](.*)",
        r"(?i)(.*)\[(?:第)?(\d{1,4}(?:\.\d{1,2})?)[话集話](?:END)?\](.*)",
        r"(?i)(.*)第?(\d{1,4}(?:\.\d{1,2})?)[话話集](?:END)?(.*)",
        r"(?i)(.*)(?:S\d{2})?EP?(\d{1,4}(?:\.\d{1,2})?)(.*)",
    ];

    let suffix = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    for rule in rules {
        if let Ok(re) = Regex::new(rule) {
            if let Some(caps) = re.captures(name) {
                let title_part = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let (group, mut title) = get_group(title_part);
                let (parsed_title, parsed_season) = get_season_and_title(&title);
                title = parsed_title;
                let effective_season = season.unwrap_or(parsed_season);
                let episode = caps.get(2).map(|m| m.as_str()).unwrap_or("1").to_string();

                let result = ParsedFile {
                    media_path: torrent_path.to_string(),
                    group,
                    title: title.trim().to_string(),
                    season: effective_season,
                    episode,
                    suffix,
                };

                let mut cache = PARSER_CACHE.lock().unwrap();
                cache.put(cache_key, Some(result.clone()));
                return Some(result);
            }
        }
    }

    let mut cache = PARSER_CACHE.lock().unwrap();
    cache.put(cache_key, None);
    None
}

pub fn subtitle_parser(
    torrent_path: &str,
    torrent_name: Option<&str>,
    season: Option<i32>,
) -> Option<ParsedSubtitle> {
    let parsed = torrent_parser(torrent_path, torrent_name, season, FileType::Subtitle)?;
    let lang = get_subtitle_lang(torrent_path);
    Some(ParsedSubtitle {
        media_path: parsed.media_path,
        group: parsed.group,
        title: parsed.title,
        season: parsed.season,
        episode: parsed.episode,
        suffix: parsed.suffix,
        language: lang.map(|l| l.to_string()),
    })
}
