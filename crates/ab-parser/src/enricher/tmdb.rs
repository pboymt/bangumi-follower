use std::sync::Mutex;
use std::collections::HashMap;
use lru::LruCache;
use once_cell::sync::Lazy;
use ab_core::config::consts::TMDB_API;
use ab_network::NetworkClient;
use crate::offset_detector::{TMDBInfo, TMDBSeason, EpisodeAirDate, detect_virtual_seasons};
use crate::error::ParserError;

static TMDB_CACHE: Lazy<Mutex<LruCache<String, TMDBInfo>>> = Lazy::new(|| {
    Mutex::new(LruCache::new(512.try_into().unwrap()))
});

const TMDB_BASE: &str = "https://api.themoviedb.org/3";

fn resolve_api_key(key: &str) -> String {
    if !key.is_empty() {
        return key.to_string();
    }
    std::env::var("AB_TMDB_API_KEY")
        .unwrap_or_else(|_| TMDB_API.to_string())
}

pub async fn tmdb_search(
    client: &NetworkClient,
    title: &str,
    language: &str,
    api_key: &str,
) -> Result<Option<TMDBInfo>, ParserError> {
    let api_key = resolve_api_key(api_key);
    let cache_key = format!("{title}:{language}");
    {
        let mut cache = TMDB_CACHE.lock().unwrap();
        if let Some(info) = cache.get(&cache_key) {
            return Ok(Some(info.clone()));
        }
    }

    let search_url = format!("{TMDB_BASE}/search/tv?api_key={api_key}&query={}&include_adult=false&language={language}", urlencoding::encode(title));
    let result: serde_json::Value = client.get_json(&search_url).await.map_err(ParserError::Network)?;

    let results = result["results"].as_array().map(|r| r.to_vec()).unwrap_or_default();
    if results.is_empty() {
        return Ok(None);
    }

    for item in &results {
        let genre_ids = item["genre_ids"].as_array().map(|g| {
            g.iter().filter_map(|id| id.as_i64()).collect::<Vec<_>>()
        }).unwrap_or_default();
        if !genre_ids.contains(&16) {
            continue;
        }

        let tv_id = item["id"].as_i64().unwrap_or(0) as i32;
        let detail_url = format!("{TMDB_BASE}/tv/{tv_id}?api_key={api_key}&language={language}");
        let detail: serde_json::Value = client.get_json(&detail_url).await.map_err(ParserError::Network)?;

        let title = detail["name"].as_str().unwrap_or("").to_string();
        let original_title = detail["original_name"].as_str().unwrap_or("").to_string();
        let year = item["first_air_date"].as_str().unwrap_or("").to_string();
        let series_status = detail["status"].as_str().map(|s| s.to_string());

        let mut seasons = Vec::new();
        let mut season_episode_counts = HashMap::new();
        let mut last_season = 0;

        if let Some(s_list) = detail["seasons"].as_array() {
            for s in s_list {
                let season_num = s["season_number"].as_i64().unwrap_or(0) as i32;
                if season_num == 0 { continue; }
                let episode_count = s["episode_count"].as_i64().unwrap_or(0) as i32;
                let air_date = s["air_date"].as_str().map(|s| s.to_string());
                let poster_path = s["poster_path"].as_str().map(|s| s.to_string());

                seasons.push(TMDBSeason {
                    season: season_num.to_string(),
                    air_date,
                    poster_path: poster_path.map(|p| format!("https://image.tmdb.org/t/p/w780{p}")),
                });
                season_episode_counts.insert(season_num, episode_count);
                if season_num > last_season { last_season = season_num; }
            }
        }

        let poster_path = detail["poster_path"].as_str()
            .map(|p| format!("https://image.tmdb.org/t/p/w780{p}"));

        let mut info = TMDBInfo {
            id: tv_id,
            title,
            original_title,
            seasons,
            last_season,
            year: year[..4].to_string(),
            poster_link: poster_path,
            series_status,
            season_episode_counts,
            virtual_season_starts: HashMap::new(),
        };

        for (season_num, _) in &info.season_episode_counts {
            let eps_url = format!("{TMDB_BASE}/tv/{tv_id}/season/{season_num}?api_key={api_key}&language={language}");
            if let Ok(eps_data) = client.get_json(&eps_url).await {
                if let Some(episodes) = eps_data["episodes"].as_array() {
                    let air_dates: Vec<EpisodeAirDate> = episodes.iter().filter_map(|ep| {
                        Some(EpisodeAirDate {
                            episode_number: ep["episode_number"].as_i64()? as i32,
                            air_date: ep["air_date"].as_str().map(|s| s.to_string()),
                        })
                    }).collect();
                    let boundaries = detect_virtual_seasons(&air_dates, 6);
                    if !boundaries.is_empty() {
                        info.virtual_season_starts.insert(*season_num, boundaries);
                    }
                }
            }
        }

        let mut cache = TMDB_CACHE.lock().unwrap();
        cache.put(cache_key, info.clone());
        return Ok(Some(info));
    }

    Ok(None)
}


