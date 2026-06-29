use std::collections::HashSet;

use ab_database::models::bangumi::Bangumi;
use ab_database::models::rss::RSSItem;
use ab_database::models::torrent::Torrent;
use ab_network::NetworkClient;
use ab_parser::enricher::mikan::ImageSaver;
use ab_parser::raw_parser::Episode;
use ab_parser::title_parser::ParserConfig;

use crate::engine::RSSEngine;
use crate::error::RssError;

pub struct RSSAnalyser;

impl RSSAnalyser {
    pub async fn official_title_parser(
        bangumi: &mut Bangumi,
        rss: &RSSItem,
        torrent: &Torrent,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<(), RssError> {
        match rss.parser.as_str() {
            "mikan" => {
                if let Some(homepage) = &torrent.homepage {
                    let (poster, official_title) =
                        ab_parser::enricher::mikan::mikan_parse(network, homepage, image_saver)
                            .await?;
                    bangumi.poster_link = Some(poster);
                    bangumi.official_title = official_title;
                }
            }
            "tmdb" => {
                let api_key = config.tmdb_api_key.as_deref().unwrap_or("");
                let tmdb_info = ab_parser::enricher::tmdb::tmdb_search(
                    network,
                    &bangumi.official_title,
                    &config.language,
                    api_key,
                )
                .await?;
                if let Some(info) = tmdb_info {
                    bangumi.official_title = info.title.clone();
                    bangumi.year = Some(info.year.clone());
                    bangumi.season = info.last_season;
                    bangumi.poster_link = info.poster_link.clone();
                }
            }
            _ => {}
        }
        bangumi.official_title = bangumi
            .official_title
            .chars()
            .map(|c| if "/:.\\".contains(c) { ' ' } else { c })
            .collect();
        Ok(())
    }

    pub async fn get_rss_torrents(
        &self,
        rss_link: &str,
        _full_parse: bool,
        network: &NetworkClient,
    ) -> Result<Vec<Torrent>, RssError> {
        let entries = network.get_torrents(rss_link, None, None).await?;
        let torrents: Vec<Torrent> = entries
            .into_iter()
            .map(|e| Torrent {
                id: 0,
                bangumi_id: None,
                rss_id: None,
                name: Some(e.title),
                url: e.url,
                homepage: Some(e.homepage),
                downloaded: false,
                qb_hash: None,
            })
            .collect();
        Ok(torrents)
    }

    pub async fn torrents_to_data(
        &self,
        torrents: &[Torrent],
        rss: &RSSItem,
        full_parse: bool,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Vec<Bangumi>, RssError> {
        let mut new_data = Vec::new();
        let mut seen_titles = HashSet::new();
        for torrent in torrents {
            let name = match torrent.name.as_deref() {
                Some(n) => n,
                None => continue,
            };
            let episode = match ab_parser::raw_parser::raw_parser(name) {
                Some(ep) => ep,
                None => continue,
            };
            let title_raw = episode
                .title_en
                .clone()
                .or_else(|| episode.title_zh.clone())
                .or_else(|| episode.title_jp.clone())
                .unwrap_or_default();
            if title_raw.is_empty() || seen_titles.contains(&title_raw) {
                continue;
            }
            seen_titles.insert(title_raw.clone());

            let mut bangumi = build_bangumi_from_episode(&episode, config);
            Self::official_title_parser(&mut bangumi, rss, torrent, network, config, image_saver)
                .await?;
            if !full_parse {
                return Ok(vec![bangumi]);
            }
            new_data.push(bangumi);
        }
        Ok(new_data)
    }

    pub async fn torrent_to_data(
        &self,
        torrent: &Torrent,
        rss: &RSSItem,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Option<Bangumi>, RssError> {
        let name = match torrent.name.as_deref() {
            Some(n) => n,
            None => return Ok(None),
        };
        let episode = match ab_parser::raw_parser::raw_parser(name) {
            Some(ep) => ep,
            None => return Ok(None),
        };
        let mut bangumi = build_bangumi_from_episode(&episode, config);
        Self::official_title_parser(&mut bangumi, rss, torrent, network, config, image_saver)
            .await?;
        Ok(Some(bangumi))
    }

    pub async fn rss_to_data(
        &self,
        rss: &RSSItem,
        engine: &RSSEngine,
        full_parse: bool,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Vec<Bangumi>, RssError> {
        let rss_torrents = self.get_rss_torrents(&rss.url, full_parse, network).await?;
        let unmatched = engine
            .bangumi_repo()
            .match_list(&rss_torrents, &rss.url)
            .await?;
        if unmatched.is_empty() {
            return Ok(vec![]);
        }
        let new_data = self
            .torrents_to_data(&unmatched, rss, full_parse, network, config, image_saver)
            .await?;
        if !new_data.is_empty() {
            engine.bangumi_repo().add_all(&new_data).await?;
        }
        Ok(new_data)
    }

    pub async fn link_to_data(
        &self,
        rss: &RSSItem,
        network: &NetworkClient,
        config: &ParserConfig,
        image_saver: &dyn ImageSaver,
    ) -> Result<Option<Bangumi>, RssError> {
        let torrents = self.get_rss_torrents(&rss.url, false, network).await?;
        for torrent in &torrents {
            if let Some(bangumi) = self
                .torrent_to_data(torrent, rss, network, config, image_saver)
                .await?
            {
                return Ok(Some(bangumi));
            }
        }
        Ok(None)
    }
}

fn build_bangumi_from_episode(episode: &Episode, config: &ParserConfig) -> Bangumi {
    let titles = [&episode.title_zh, &episode.title_en, &episode.title_jp];
    let official_title = titles
        .iter()
        .find_map(|t| t.as_ref())
        .cloned()
        .unwrap_or_default();
    let title_raw = episode
        .title_en
        .clone()
        .or_else(|| episode.title_zh.clone())
        .or_else(|| episode.title_jp.clone())
        .unwrap_or_default();
    Bangumi {
        id: 0,
        official_title,
        year: None,
        title_raw,
        season: episode.season,
        season_raw: Some(episode.season_raw.clone()),
        group_name: Some(episode.group.clone()),
        dpi: Some(episode.resolution.clone()),
        source: Some(episode.source.clone()),
        subtitle: Some(episode.sub.clone()),
        eps_collect: episode.episode <= 1,
        episode_offset: 0,
        season_offset: 0,
        filter: config.filters.join(","),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_episode(
        title_zh: Option<&str>,
        title_en: Option<&str>,
        title_jp: Option<&str>,
        episode: i32,
        season: i32,
    ) -> Episode {
        Episode {
            title_en: title_en.map(String::from),
            title_zh: title_zh.map(String::from),
            title_jp: title_jp.map(String::from),
            season,
            season_raw: season.to_string(),
            episode,
            sub: "chs".into(),
            group: "TestGroup".into(),
            resolution: "1080p".into(),
            source: "WebRip".into(),
        }
    }

    fn default_config() -> ParserConfig {
        ParserConfig {
            language: "zh-CN".into(),
            filters: vec!["720".into()],
            tmdb_api_key: None,
            openai_enable: false,
        }
    }

    #[test]
    fn test_build_bangumi_episode_title_zh_preferred() {
        let ep = make_episode(Some("中文标题"), Some("English Title"), Some("日本語タイトル"), 1, 1);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert_eq!(bangumi.official_title, "中文标题");
        assert_eq!(bangumi.title_raw, "English Title");
    }

    #[test]
    fn test_build_bangumi_episode_title_en_fallback() {
        let ep = make_episode(None, Some("English Title"), Some("日本語タイトル"), 1, 1);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert_eq!(bangumi.official_title, "English Title");
        assert_eq!(bangumi.title_raw, "English Title");
    }

    #[test]
    fn test_build_bangumi_episode_title_jp_fallback() {
        let ep = make_episode(None, None, Some("日本語タイトル"), 1, 1);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert_eq!(bangumi.official_title, "日本語タイトル");
        assert_eq!(bangumi.title_raw, "日本語タイトル");
    }

    #[test]
    fn test_build_bangumi_eps_collect_true_when_episode_one() {
        let ep = make_episode(Some("Title"), None, None, 1, 1);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert!(bangumi.eps_collect);
    }

    #[test]
    fn test_build_bangumi_eps_collect_true_when_episode_zero() {
        let ep = make_episode(Some("Title"), None, None, 0, 1);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert!(bangumi.eps_collect);
    }

    #[test]
    fn test_build_bangumi_eps_collect_false_when_episode_greater_than_one() {
        let ep = make_episode(Some("Title"), None, None, 2, 1);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert!(!bangumi.eps_collect);
    }

    #[test]
    fn test_build_bangumi_sets_season_and_group() {
        let ep = make_episode(Some("Title"), None, None, 1, 3);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert_eq!(bangumi.season, 3);
        assert_eq!(bangumi.group_name.as_deref(), Some("TestGroup"));
        assert_eq!(bangumi.dpi.as_deref(), Some("1080p"));
        assert_eq!(bangumi.source.as_deref(), Some("WebRip"));
        assert_eq!(bangumi.subtitle.as_deref(), Some("chs"));
    }

    #[test]
    fn test_build_bangumi_filter_from_config() {
        let ep = make_episode(Some("Title"), None, None, 1, 1);
        let mut config = default_config();
        config.filters = vec!["720".into(), "x265".into()];
        let bangumi = build_bangumi_from_episode(&ep, &config);
        assert_eq!(bangumi.filter, "720,x265");
    }

    #[test]
    fn test_official_title_parser_sanitizes_chars() {
        let mut bangumi = Bangumi {
            id: 0,
            official_title: "Test: Title/With.Bad\\Chars".into(),
            year: None,
            title_raw: "test".into(),
            season: 1,
            season_raw: None,
            group_name: None,
            dpi: None,
            source: None,
            subtitle: None,
            eps_collect: false,
            episode_offset: 0,
            season_offset: 0,
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
        };
        let rss = RSSItem {
            id: 0,
            name: None,
            url: "http://example.com".into(),
            aggregate: false,
            parser: "none".into(),
            enabled: true,
            connection_status: None,
            last_checked_at: None,
            last_error: None,
        };
        let torrent = Torrent {
            id: 0,
            bangumi_id: None,
            rss_id: None,
            name: Some("Test Torrent".into()),
            url: "http://example.com/t.torrent".into(),
            homepage: None,
            downloaded: false,
            qb_hash: None,
        };
        let network = NetworkClient::new(None, None, None);
        let config = default_config();
        let image_saver = TestImageSaver;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(RSSAnalyser::official_title_parser(
            &mut bangumi, &rss, &torrent, &network, &config, &image_saver,
        )).unwrap();
        assert_eq!(bangumi.official_title, "Test  Title With Bad Chars");
    }

    struct TestImageSaver;
    impl ImageSaver for TestImageSaver {
        fn save(&self, _data: &[u8], _suffix: &str) -> String {
            "/posters/test.jpg".into()
        }
    }

    #[test]
    fn test_build_bangumi_empty_titles() {
        let ep = make_episode(None, None, None, 1, 1);
        let bangumi = build_bangumi_from_episode(&ep, &default_config());
        assert_eq!(bangumi.official_title, "");
        assert_eq!(bangumi.title_raw, "");
    }

    #[test]
    fn test_torrent_no_name_skipped() {
        let _torrent = Torrent {
            id: 0,
            bangumi_id: None,
            rss_id: None,
            name: None,
            url: "http://example.com/t.torrent".into(),
            homepage: None,
            downloaded: false,
            qb_hash: None,
        };
        let bangumi = build_bangumi_from_episode(
            &make_episode(Some("Title"), None, None, 1, 1),
            &default_config(),
        );
        assert_eq!(bangumi.official_title, "Title");
    }
}
