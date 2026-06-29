use std::collections::HashMap;
use chrono::Datelike;

#[derive(Debug, Clone)]
pub struct OffsetSuggestion {
    pub season_offset: i32,
    pub episode_offset: Option<i32>,
    pub reason: String,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone)]
pub struct TMDBInfo {
    pub id: i32,
    pub title: String,
    pub original_title: String,
    pub seasons: Vec<TMDBSeason>,
    pub last_season: i32,
    pub year: String,
    pub poster_link: Option<String>,
    pub series_status: Option<String>,
    pub season_episode_counts: HashMap<i32, i32>,
    pub virtual_season_starts: HashMap<i32, Vec<i32>>,
}

#[derive(Debug, Clone)]
pub struct TMDBSeason {
    pub season: String,
    pub air_date: Option<String>,
    pub poster_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EpisodeAirDate {
    pub episode_number: i32,
    pub air_date: Option<String>,
}

pub fn detect_offset_mismatch(
    parsed_season: i32,
    parsed_episode: i32,
    tmdb_info: &TMDBInfo,
) -> Option<OffsetSuggestion> {
    if tmdb_info.seasons.is_empty() || tmdb_info.last_season == 0 {
        return None;
    }

    if parsed_season > tmdb_info.last_season {
        let season_offset = tmdb_info.last_season - parsed_season;
        let mut reasons = vec![format!("Season offset: {season_offset} (parsed S{parsed_season}, TMDB has {})", tmdb_info.last_season)];

        let total_eps_before = (1..parsed_season)
            .filter_map(|s| tmdb_info.season_episode_counts.get(&s))
            .sum::<i32>();

        let episode_offset = if total_eps_before > 0 && parsed_episode <= total_eps_before {
            reasons.push(format!("Episode offset: {total_eps_before} (total eps before S{parsed_season})"));
            Some(total_eps_before)
        } else {
            None
        };

        let confidence = match tmdb_info.series_status.as_deref() {
            Some("Ended") => Confidence::High,
            Some("Returning Series") => Confidence::Medium,
            _ => Confidence::Low,
        };

        return Some(OffsetSuggestion {
            season_offset,
            episode_offset,
            reason: reasons.join("; "),
            confidence,
        });
    }

    if let Some(starts) = tmdb_info.virtual_season_starts.get(&parsed_season) {
        if let Some(&ep_start) = starts.iter().find(|&&s| s > parsed_episode) {
            let episode_offset = ep_start - 1;
            return Some(OffsetSuggestion {
                season_offset: 0,
                episode_offset: Some(episode_offset),
                reason: format!("Virtual season detected: S{parsed_season} starts at episode {ep_start}"),
                confidence: Confidence::Medium,
            });
        }
    }

    if let Some(&total_eps) = tmdb_info.season_episode_counts.get(&parsed_season) {
        if parsed_episode > total_eps {
            return Some(OffsetSuggestion {
                season_offset: 0,
                episode_offset: Some(total_eps - parsed_episode),
                reason: format!("Episode {parsed_episode} exceeds S{parsed_season} total ({total_eps})"),
                confidence: Confidence::Low,
            });
        }
    }

    None
}

pub fn detect_virtual_seasons(episodes: &[EpisodeAirDate], gap_months: u32) -> Vec<i32> {
    let mut boundaries = Vec::new();
    for i in 1..episodes.len() {
        if let (Some(ref prev), Some(ref curr)) = (&episodes[i-1].air_date, &episodes[i].air_date) {
            if let (Ok(p), Ok(c)) = (chrono::NaiveDate::parse_from_str(prev, "%Y-%m-%d"),
                                     chrono::NaiveDate::parse_from_str(curr, "%Y-%m-%d")) {
                let months = (c.year() - p.year()) * 12 + (c.month() as i32 - p.month() as i32);
                if months > gap_months as i32 {
                    boundaries.push(episodes[i].episode_number);
                }
            }
        }
    }
    boundaries
}
