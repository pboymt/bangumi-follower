use ab_network::NetworkClient;
use crate::raw_parser::{raw_parser, Episode};
use crate::enricher::mikan::ImageSaver;
use crate::error::ParserError;

#[derive(Debug, Clone)]
pub struct ParserConfig {
    pub language: String,
    pub filters: Vec<String>,
    pub tmdb_api_key: Option<String>,
    pub openai_enable: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedBangumi {
    pub official_title: String,
    pub title_raw: String,
    pub season: i32,
    pub season_raw: String,
    pub group_name: Option<String>,
    pub dpi: Option<String>,
    pub source: Option<String>,
    pub subtitle: Option<String>,
    pub eps_collect: bool,
    pub poster_link: Option<String>,
    pub year: Option<String>,
}

pub fn quick_parse(raw_title: &str) -> Option<Episode> {
    raw_parser(raw_title)
}

pub async fn full_parse(
    client: &NetworkClient,
    raw_title: &str,
    config: &ParserConfig,
    _image_saver: &dyn ImageSaver,
) -> Result<ParsedBangumi, ParserError> {
    let episode = raw_parser(raw_title)
        .ok_or_else(|| ParserError::NoMatch(format!("raw_parser returned None for: {raw_title}")))?;

    let title_raw = episode.title_en.clone()
        .or_else(|| episode.title_zh.clone())
        .or_else(|| episode.title_jp.clone())
        .unwrap_or_default();

    let official_title = episode.title_zh.clone()
        .or_else(|| episode.title_en.clone())
        .or_else(|| episode.title_jp.clone())
        .unwrap_or_default();

    let api_key = config.tmdb_api_key.as_deref().unwrap_or("");
    let search_title = official_title.as_str();

    let (poster_link, year) = if let Ok(Some(tmdb_info)) = crate::enricher::tmdb::tmdb_search(
        client,
        search_title,
        &config.language,
        api_key,
    ).await {
        (tmdb_info.poster_link.clone(), Some(tmdb_info.year.clone()))
    } else {
        (None, None)
    };

    Ok(ParsedBangumi {
        official_title,
        title_raw,
        season: episode.season,
        season_raw: episode.season_raw,
        group_name: if episode.group.is_empty() { None } else { Some(episode.group) },
        dpi: if episode.resolution.is_empty() { None } else { Some(episode.resolution) },
        source: if episode.source.is_empty() { None } else { Some(episode.source) },
        subtitle: if episode.sub.is_empty() { None } else { Some(episode.sub) },
        eps_collect: episode.episode <= 1,
        poster_link,
        year,
    })
}
