use std::sync::Mutex;
use std::time::Instant;
use sqlx::SqlitePool;
use crate::models::bangumi::{Bangumi, BangumiUpdate};
use crate::models::torrent::Torrent;
use crate::error::DbError;

pub enum UpdateData {
    Full(Bangumi),
    Partial(i32, BangumiUpdate),
}

const CACHE_TTL: u64 = 300;

pub struct BangumiRepo {
    pool: SqlitePool,
    cache: Mutex<Option<(Vec<Bangumi>, Instant)>>,
}

impl BangumiRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            cache: Mutex::new(None),
        }
    }

    fn invalidate_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            *cache = None;
        }
    }

    pub async fn add(&self, data: &Bangumi) -> Result<bool, DbError> {
        let existing: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM bangumi WHERE title_raw=? AND official_title=?"
        )
        .bind(&data.title_raw)
        .bind(&data.official_title)
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Ok(false);
        }

        sqlx::query(
            "INSERT INTO bangumi (official_title, year, title_raw, season, season_raw, group_name, \
             dpi, source, subtitle, eps_collect, episode_offset, season_offset, filter, rss_link, \
             poster_link, added, rule_name, save_path, deleted, archived, air_weekday, \
             weekday_locked, needs_review, needs_review_reason, title_aliases) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)"
        )
        .bind(&data.official_title).bind(&data.year).bind(&data.title_raw)
        .bind(data.season).bind(&data.season_raw).bind(&data.group_name)
        .bind(&data.dpi).bind(&data.source).bind(&data.subtitle)
        .bind(data.eps_collect).bind(data.episode_offset).bind(data.season_offset)
        .bind(&data.filter).bind(&data.rss_link).bind(&data.poster_link)
        .bind(data.added).bind(&data.rule_name).bind(&data.save_path)
        .bind(data.deleted).bind(data.archived).bind(data.air_weekday)
        .bind(data.weekday_locked).bind(data.needs_review).bind(&data.needs_review_reason)
        .bind(&data.title_aliases)
        .execute(&self.pool)
        .await?;

        self.invalidate_cache();
        Ok(true)
    }

    pub async fn add_all(&self, datas: &[Bangumi]) -> Result<u32, DbError> {
        let mut count = 0u32;
        for data in datas {
            if self.add(data).await? {
                count += 1;
            }
        }
        Ok(count)
    }

    async fn update_inner(&self, data: UpdateData) -> Result<bool, DbError> {
        match data {
            UpdateData::Full(b) => {
                let rows = sqlx::query(
                    "UPDATE bangumi SET official_title=?1, year=?2, title_raw=?3, season=?4, \
                     season_raw=?5, group_name=?6, dpi=?7, source=?8, subtitle=?9, \
                     eps_collect=?10, episode_offset=?11, season_offset=?12, filter=?13, \
                     rss_link=?14, poster_link=?15, added=?16, rule_name=?17, save_path=?18, \
                     deleted=?19, archived=?20, air_weekday=?21, weekday_locked=?22, \
                     needs_review=?23, needs_review_reason=?24, title_aliases=?25 WHERE id=?26"
                )
                .bind(&b.official_title).bind(&b.year).bind(&b.title_raw)
                .bind(b.season).bind(&b.season_raw).bind(&b.group_name)
                .bind(&b.dpi).bind(&b.source).bind(&b.subtitle)
                .bind(b.eps_collect).bind(b.episode_offset).bind(b.season_offset)
                .bind(&b.filter).bind(&b.rss_link).bind(&b.poster_link)
                .bind(b.added).bind(&b.rule_name).bind(&b.save_path)
                .bind(b.deleted).bind(b.archived).bind(b.air_weekday)
                .bind(b.weekday_locked).bind(b.needs_review).bind(&b.needs_review_reason)
                .bind(&b.title_aliases).bind(b.id)
                .execute(&self.pool).await?;
                Ok(rows.rows_affected() > 0)
            }
            UpdateData::Partial(id, u) => {
                let rows = sqlx::query(
                    "UPDATE bangumi SET \
                     official_title=COALESCE(?1, official_title), \
                     year=COALESCE(?2, year), \
                     title_raw=COALESCE(?3, title_raw), \
                     season=COALESCE(?4, season), \
                     group_name=COALESCE(?5, group_name), \
                     dpi=COALESCE(?6, dpi), \
                     source=COALESCE(?7, source), \
                     subtitle=COALESCE(?8, subtitle), \
                     eps_collect=COALESCE(?9, eps_collect), \
                     episode_offset=COALESCE(?10, episode_offset), \
                     season_offset=COALESCE(?11, season_offset), \
                     filter=COALESCE(?12, filter), \
                     rss_link=COALESCE(?13, rss_link), \
                     poster_link=COALESCE(?14, poster_link), \
                     added=COALESCE(?15, added), \
                     rule_name=COALESCE(?16, rule_name), \
                     save_path=COALESCE(?17, save_path), \
                     deleted=COALESCE(?18, deleted), \
                     archived=COALESCE(?19, archived), \
                     air_weekday=COALESCE(?20, air_weekday), \
                     weekday_locked=COALESCE(?21, weekday_locked), \
                     needs_review=COALESCE(?22, needs_review), \
                     needs_review_reason=COALESCE(?23, needs_review_reason), \
                     title_aliases=COALESCE(?24, title_aliases) \
                     WHERE id=?25"
                )
                .bind(&u.official_title).bind(&u.year).bind(&u.title_raw)
                .bind(u.season).bind(&u.group_name).bind(&u.dpi)
                .bind(&u.source).bind(&u.subtitle).bind(u.eps_collect)
                .bind(u.episode_offset).bind(u.season_offset).bind(&u.filter)
                .bind(&u.rss_link).bind(&u.poster_link).bind(u.added)
                .bind(&u.rule_name).bind(&u.save_path).bind(u.deleted)
                .bind(u.archived).bind(u.air_weekday).bind(u.weekday_locked)
                .bind(u.needs_review).bind(&u.needs_review_reason).bind(&u.title_aliases)
                .bind(id)
                .execute(&self.pool).await?;
                Ok(rows.rows_affected() > 0)
            }
        }
    }

    pub async fn update(&self, data: UpdateData) -> Result<bool, DbError> {
        let result = self.update_inner(data).await?;
        self.invalidate_cache();
        Ok(result)
    }

    pub async fn update_all(&self, datas: &[Bangumi]) -> Result<(), DbError> {
        for data in datas {
            self.update_inner(UpdateData::Full(data.clone())).await?;
        }
        self.invalidate_cache();
        Ok(())
    }

    pub async fn update_rss(&self, title_raw: &str, rss_set: &str) -> Result<(), DbError> {
        sqlx::query("UPDATE bangumi SET rss_link=?1 WHERE title_raw=?2")
            .bind(rss_set).bind(title_raw).execute(&self.pool).await?;
        self.invalidate_cache();
        Ok(())
    }

    pub async fn update_poster(&self, title_raw: &str, poster_link: &str) -> Result<(), DbError> {
        sqlx::query("UPDATE bangumi SET poster_link=?1 WHERE title_raw=?2")
            .bind(poster_link).bind(title_raw).execute(&self.pool).await?;
        self.invalidate_cache();
        Ok(())
    }

    pub async fn delete_one(&self, id: i32) -> Result<(), DbError> {
        sqlx::query("DELETE FROM bangumi WHERE id=?")
            .bind(id).execute(&self.pool).await?;
        self.invalidate_cache();
        Ok(())
    }

    pub async fn delete_all(&self) -> Result<(), DbError> {
        sqlx::query("DELETE FROM bangumi").execute(&self.pool).await?;
        self.invalidate_cache();
        Ok(())
    }

    pub async fn disable_rule(&self, id: i32) -> Result<(), DbError> {
        sqlx::query("UPDATE bangumi SET deleted=1 WHERE id=?")
            .bind(id).execute(&self.pool).await?;
        self.invalidate_cache();
        Ok(())
    }

    pub async fn search_id(&self, id: i32) -> Result<Option<Bangumi>, DbError> {
        let b = sqlx::query_as::<_, Bangumi>("SELECT * FROM bangumi WHERE id=?")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(b)
    }

    pub async fn search_official_title(&self, official_title: &str) -> Result<Option<Bangumi>, DbError> {
        let b = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE official_title=?"
        )
        .bind(official_title)
        .fetch_optional(&self.pool)
        .await?;
        Ok(b)
    }

    pub async fn search_ids(&self, ids: &[i32]) -> Result<Vec<Bangumi>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let params: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!("SELECT * FROM bangumi WHERE id IN ({})", params.join(","));
        let mut query = sqlx::query_as::<_, Bangumi>(&sql);
        for id in ids {
            query = query.bind(id);
        }
        let results = query.fetch_all(&self.pool).await?;
        Ok(results)
    }

    pub async fn search_all(&self) -> Result<Vec<Bangumi>, DbError> {
        {
            let cache = self.cache.lock().unwrap();
            if let Some((ref bangumis, ref time)) = *cache {
                if time.elapsed().as_secs() < CACHE_TTL {
                    return Ok(bangumis.clone());
                }
            }
        }
        let bangumis = sqlx::query_as::<_, Bangumi>("SELECT * FROM bangumi")
            .fetch_all(&self.pool).await?;
        let mut cache = self.cache.lock().unwrap();
        *cache = Some((bangumis.clone(), Instant::now()));
        Ok(bangumis)
    }

    pub async fn not_complete(&self) -> Result<Vec<Bangumi>, DbError> {
        let items = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE eps_collect=0 AND deleted=0"
        )
        .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn not_added(&self) -> Result<Vec<Bangumi>, DbError> {
        let items = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE added=0 AND deleted=0"
        )
        .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn search_rss(&self, rss_link: &str) -> Result<Vec<Bangumi>, DbError> {
        let items = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE rss_link=?"
        )
        .bind(rss_link)
        .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn get_needs_review(&self) -> Result<Vec<Bangumi>, DbError> {
        let items = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE needs_review=1 AND deleted=0"
        )
        .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn get_active_for_scan(&self) -> Result<Vec<Bangumi>, DbError> {
        let items = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE deleted=0 AND archived=0"
        )
        .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn match_torrent(&self, torrent_name: &str) -> Result<Option<Bangumi>, DbError> {
        let bangumis = self.search_all().await?;
        let mut best: Option<Bangumi> = None;
        let mut best_len = 0usize;
        for b in &bangumis {
            if b.deleted { continue; }
            if torrent_name.contains(&b.title_raw) && b.title_raw.len() > best_len {
                best = Some(b.clone());
                best_len = b.title_raw.len();
            } else {
                for alias in b.aliases() {
                    if torrent_name.contains(&alias) && alias.len() > best_len {
                        best = Some(b.clone());
                        best_len = alias.len();
                    }
                }
            }
        }
        Ok(best)
    }

    pub async fn match_by_save_path(&self, save_path: &str) -> Result<Option<Bangumi>, DbError> {
        let normalized = save_path.replace('\\', "/").trim_end_matches('/').to_string();
        let b = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE save_path=? OR save_path LIKE ?"
        )
        .bind(&normalized)
        .bind(format!("{}%", normalized))
        .fetch_optional(&self.pool)
        .await?;
        Ok(b)
    }

    pub async fn match_poster(&self, bangumi_name: &str) -> Result<String, DbError> {
        let b = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE official_title=? AND poster_link IS NOT NULL LIMIT 1"
        )
        .bind(bangumi_name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(b.and_then(|b| b.poster_link).unwrap_or_default())
    }

    pub async fn match_list(&self, torrent_list: &[Torrent], rss_link: &str) -> Result<Vec<Torrent>, DbError> {
        let bangumis = self.search_all().await?;
        let mut unmatched = Vec::new();
        for torrent in torrent_list {
            let mut matched = false;
            for b in &bangumis {
                if torrent.name.as_deref().map_or(false, |n| n.contains(&b.title_raw)) {
                    matched = true;
                    sqlx::query("UPDATE bangumi SET rss_link=?1 WHERE id=?2")
                        .bind(rss_link).bind(b.id).execute(&self.pool).await?;
                    break;
                }
                for alias in b.aliases() {
                    if torrent.name.as_deref().map_or(false, |n| n.contains(&alias)) {
                        matched = true;
                        sqlx::query("UPDATE bangumi SET rss_link=?1 WHERE id=?2")
                            .bind(rss_link).bind(b.id).execute(&self.pool).await?;
                        break;
                    }
                }
                if matched { break; }
            }
            if !matched {
                unmatched.push(torrent.clone());
            }
        }
        self.invalidate_cache();
        Ok(unmatched)
    }

    pub async fn find_semantic_duplicate(&self, data: &Bangumi) -> Result<Option<Bangumi>, DbError> {
        let candidates = sqlx::query_as::<_, Bangumi>(
            "SELECT * FROM bangumi WHERE official_title=? AND deleted=0"
        )
        .bind(&data.official_title)
        .fetch_all(&self.pool)
        .await?;

        for c in candidates {
            if c.dpi == data.dpi && c.subtitle == data.subtitle && c.source == data.source {
                let g1 = c.group_name.as_deref().unwrap_or("");
                let g2 = data.group_name.as_deref().unwrap_or("");
                if g1.contains(g2) || g2.contains(g1) || g1 == g2 {
                    return Ok(Some(c));
                }
            }
        }
        Ok(None)
    }

    pub async fn add_title_alias(&self, bangumi_id: i32, new_title_raw: &str) -> Result<bool, DbError> {
        let b = self.search_id(bangumi_id).await?;
        match b {
            Some(mut b) => {
                let mut aliases = b.aliases();
                if aliases.contains(&new_title_raw.to_string()) {
                    return Ok(false);
                }
                aliases.push(new_title_raw.to_string());
                b.set_aliases(aliases);
                self.update(UpdateData::Full(b)).await?;
                Ok(true)
            }
            None => Err(DbError::NotFound(format!("bangumi {bangumi_id}"))),
        }
    }

    pub fn get_all_title_patterns(&self, bangumi: &Bangumi) -> Vec<String> {
        let mut patterns = vec![bangumi.title_raw.clone()];
        patterns.extend(bangumi.aliases());
        patterns.sort_by(|a, b| b.len().cmp(&a.len()));
        patterns
    }

    pub async fn archive_one(&self, id: i32) -> Result<bool, DbError> {
        let rows = sqlx::query("UPDATE bangumi SET archived=1 WHERE id=?")
            .bind(id).execute(&self.pool).await?;
        self.invalidate_cache();
        Ok(rows.rows_affected() > 0)
    }

    pub async fn unarchive_one(&self, id: i32) -> Result<bool, DbError> {
        let rows = sqlx::query("UPDATE bangumi SET archived=0 WHERE id=?")
            .bind(id).execute(&self.pool).await?;
        self.invalidate_cache();
        Ok(rows.rows_affected() > 0)
    }

    pub async fn set_needs_review(&self, id: i32, reason: &str, suggested_season_offset: Option<i32>, suggested_episode_offset: Option<i32>) -> Result<bool, DbError> {
        let rows = sqlx::query(
            "UPDATE bangumi SET needs_review=1, needs_review_reason=?1, \
             suggested_season_offset=?2, suggested_episode_offset=?3 WHERE id=?4"
        )
        .bind(reason)
        .bind(suggested_season_offset)
        .bind(suggested_episode_offset)
        .bind(id)
        .execute(&self.pool).await?;
        Ok(rows.rows_affected() > 0)
    }

    pub async fn clear_needs_review(&self, id: i32) -> Result<bool, DbError> {
        let rows = sqlx::query(
            "UPDATE bangumi SET needs_review=0, needs_review_reason=NULL, \
             suggested_season_offset=NULL, suggested_episode_offset=NULL WHERE id=?"
        )
        .bind(id).execute(&self.pool).await?;
        Ok(rows.rows_affected() > 0)
    }

    pub async fn set_weekday(&self, id: i32, weekday: Option<i32>) -> Result<bool, DbError> {
        let rows = sqlx::query("UPDATE bangumi SET air_weekday=? WHERE id=?")
            .bind(weekday).bind(id).execute(&self.pool).await?;
        Ok(rows.rows_affected() > 0)
    }
}
