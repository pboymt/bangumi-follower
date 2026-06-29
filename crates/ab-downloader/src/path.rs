use ab_database::models::bangumi::Bangumi;

const MEDIA_SUFFIXES: &[&str] = &[".mp4", ".mkv"];
const SUBTITLE_SUFFIXES: &[&str] = &[".ass", ".srt"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub size: i64,
}

pub fn check_files(files: &[FileInfo]) -> (Vec<String>, Vec<String>) {
    let mut media = Vec::new();
    let mut subtitles = Vec::new();
    for file in files {
        let lower = file.name.to_lowercase();
        if MEDIA_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
            media.push(file.name.clone());
        } else if SUBTITLE_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
            subtitles.push(file.name.clone());
        }
    }
    (media, subtitles)
}

pub fn path_to_bangumi(save_path: &str, torrent_name: &str) -> (String, i32) {
    let parts: Vec<&str> = save_path.split('/').filter(|s| !s.is_empty()).collect();
    let folder = parts.last().copied().unwrap_or("");
    let season = folder
        .strip_prefix("Season ")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let name = if parts.len() >= 2 {
        parts[parts.len() - 2].to_string()
    } else {
        torrent_name.to_string()
    };
    (name, season)
}

pub fn file_depth(file_path: &str) -> usize {
    file_path.matches('/').count()
}

pub fn is_ep(file_path: &str) -> bool {
    file_depth(file_path) <= 2
}

pub fn gen_save_path(data: &Bangumi, downloader_path: &str) -> String {
    let folder = if let Some(ref year) = data.year {
        format!("{} ({})", data.official_title, year)
    } else {
        data.official_title.clone()
    };
    let mut adjusted_season = data.season + data.season_offset;
    if adjusted_season < 1 {
        adjusted_season = data.season;
    }
    join_path(&[downloader_path, &folder, &format!("Season {}", adjusted_season)])
}

pub fn rule_name(data: &Bangumi, group_tag: bool) -> String {
    if group_tag {
        if let Some(ref group) = data.group_name {
            format!("[{}] {} S{}", group, data.official_title, data.season)
        } else {
            format!("{} S{}", data.official_title, data.season)
        }
    } else {
        format!("{} S{}", data.official_title, data.season)
    }
}

pub fn join_path(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_end_matches('/'))
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_database::models::bangumi::Bangumi;

    fn make_bangumi(
        official_title: &str,
        year: Option<&str>,
        season: i32,
        season_offset: i32,
        group_name: Option<&str>,
    ) -> Bangumi {
        Bangumi {
            id: 1,
            official_title: official_title.to_string(),
            year: year.map(|s| s.to_string()),
            title_raw: String::new(),
            season,
            season_raw: None,
            group_name: group_name.map(|s| s.to_string()),
            dpi: None,
            source: None,
            subtitle: None,
            eps_collect: false,
            episode_offset: 0,
            season_offset,
            filter: String::new(),
            rss_link: String::new(),
            poster_link: None,
            added: false,
            rule_name: None,
            save_path: None,
            deleted: false,
            archived: false,
            air_weekday: None,
            weekday_locked: false,
            needs_review: false,
            needs_review_reason: None,
            suggested_season_offset: None,
            suggested_episode_offset: None,
            title_aliases: None,
        }
    }

    #[test]
    fn check_files_classifies_media() {
        let files = vec![
            FileInfo { name: "ep01.mkv".to_string(), size: 100 },
            FileInfo { name: "ep02.mp4".to_string(), size: 200 },
            FileInfo { name: "subs.ass".to_string(), size: 10 },
            FileInfo { name: "subs.srt".to_string(), size: 5 },
            FileInfo { name: "cover.jpg".to_string(), size: 50 },
        ];
        let (media, subs) = check_files(&files);
        assert_eq!(media.len(), 2);
        assert!(media.contains(&"ep01.mkv".to_string()));
        assert!(media.contains(&"ep02.mp4".to_string()));
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&"subs.ass".to_string()));
        assert!(subs.contains(&"subs.srt".to_string()));
    }

    #[test]
    fn check_files_handles_mixed_case() {
        let files = vec![
            FileInfo { name: "EP01.MKV".to_string(), size: 100 },
            FileInfo { name: "subs.ASS".to_string(), size: 10 },
        ];
        let (media, subs) = check_files(&files);
        assert_eq!(media.len(), 1);
        assert_eq!(subs.len(), 1);
    }

    #[test]
    fn gen_save_path_with_year() {
        let data = make_bangumi("Test Anime", Some("2024"), 1, 0, None);
        let path = gen_save_path(&data, "/downloads");
        assert_eq!(path, "/downloads/Test Anime (2024)/Season 1");
    }

    #[test]
    fn gen_save_path_without_year() {
        let data = make_bangumi("Test Anime", None, 1, 0, None);
        let path = gen_save_path(&data, "/downloads");
        assert_eq!(path, "/downloads/Test Anime/Season 1");
    }

    #[test]
    fn gen_save_path_with_season_offset() {
        let data = make_bangumi("Test Anime", None, 1, 1, None);
        let path = gen_save_path(&data, "/downloads");
        assert_eq!(path, "/downloads/Test Anime/Season 2");
    }

    #[test]
    fn gen_save_path_adjusted_season_below_one_reverts_to_original() {
        let data = make_bangumi("Test Anime", None, 2, -3, None);
        let path = gen_save_path(&data, "/downloads");
        assert_eq!(path, "/downloads/Test Anime/Season 2");
    }

    #[test]
    fn rule_name_with_group_tag() {
        let data = make_bangumi("Test Anime", None, 1, 0, Some("SubGroup"));
        let name = rule_name(&data, true);
        assert_eq!(name, "[SubGroup] Test Anime S1");
    }

    #[test]
    fn rule_name_without_group_tag() {
        let data = make_bangumi("Test Anime", None, 1, 0, Some("SubGroup"));
        let name = rule_name(&data, false);
        assert_eq!(name, "Test Anime S1");
    }

    #[test]
    fn rule_name_without_group_name_falls_back() {
        let data = make_bangumi("Test Anime", None, 1, 0, None);
        let name = rule_name(&data, true);
        assert_eq!(name, "Test Anime S1");
    }

    #[test]
    fn path_to_bangumi_extracts_name_and_season() {
        let (name, season) = path_to_bangumi("/downloads/Test Anime/Season 1", "torrent");
        assert_eq!(name, "Test Anime");
        assert_eq!(season, 1);
    }

    #[test]
    fn is_ep_with_shallow_path() {
        assert!(is_ep("ep01.mkv"));
        assert!(is_ep("folder/ep01.mkv"));
        assert!(is_ep("a/b/ep01.mkv"));
        assert!(!is_ep("a/b/c/ep01.mkv"));
    }

    #[test]
    fn join_path_combines_components() {
        let path = join_path(&["/base", "dir1", "dir2"]);
        assert_eq!(path, "/base/dir1/dir2");
    }

    #[test]
    fn join_path_handles_empty_parts() {
        let path = join_path(&["/base", "", "dir"]);
        assert_eq!(path, "/base/dir");
    }

    #[test]
    fn join_path_strips_trailing_slashes() {
        let path = join_path(&["/base/", "dir/"]);
        assert_eq!(path, "/base/dir");
    }

    #[test]
    fn file_depth_counts_separators() {
        assert_eq!(file_depth("a.mkv"), 0);
        assert_eq!(file_depth("a/b.mkv"), 1);
        assert_eq!(file_depth("a/b/c.mkv"), 2);
    }
}
