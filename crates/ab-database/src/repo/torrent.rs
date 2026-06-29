use sqlx::SqlitePool;
use crate::models::torrent::Torrent;
use crate::error::DbError;

pub struct TorrentRepo {
    pool: SqlitePool,
}

impl TorrentRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn add(&self, data: &Torrent) -> Result<(), DbError> {
        sqlx::query(
            "INSERT OR IGNORE INTO torrent (refer_id, rss_id, name, url, homepage, downloaded, qb_hash) \
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(data.bangumi_id)
        .bind(data.rss_id)
        .bind(&data.name)
        .bind(&data.url)
        .bind(&data.homepage)
        .bind(data.downloaded)
        .bind(&data.qb_hash)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn add_all(&self, datas: &[Torrent]) -> Result<(), DbError> {
        for data in datas {
            self.add(data).await?;
        }
        Ok(())
    }

    pub async fn update(&self, data: &Torrent) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE torrent SET refer_id=?1, rss_id=?2, name=?3, url=?4, homepage=?5, \
             downloaded=?6, qb_hash=?7 WHERE id=?8"
        )
        .bind(data.bangumi_id)
        .bind(data.rss_id)
        .bind(&data.name)
        .bind(&data.url)
        .bind(&data.homepage)
        .bind(data.downloaded)
        .bind(&data.qb_hash)
        .bind(data.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_all(&self, datas: &[Torrent]) -> Result<(), DbError> {
        for data in datas {
            self.update(data).await?;
        }
        Ok(())
    }

    pub async fn update_one_user(&self, data: &Torrent) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE torrent SET downloaded=?1 WHERE id=?2"
        )
        .bind(data.downloaded)
        .bind(data.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn search(&self, id: i32) -> Result<Option<Torrent>, DbError> {
        let t = sqlx::query_as::<_, Torrent>("SELECT * FROM torrent WHERE id=?")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(t)
    }

    pub async fn search_all(&self) -> Result<Vec<Torrent>, DbError> {
        let items = sqlx::query_as::<_, Torrent>("SELECT * FROM torrent")
            .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn search_rss(&self, rss_id: i32) -> Result<Vec<Torrent>, DbError> {
        let items = sqlx::query_as::<_, Torrent>("SELECT * FROM torrent WHERE rss_id=?")
            .bind(rss_id).fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn check_new(&self, torrents: &[Torrent]) -> Result<Vec<Torrent>, DbError> {
        let mut new_torrents = Vec::new();
        for t in torrents {
            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT url FROM torrent WHERE url=?"
            )
            .bind(&t.url)
            .fetch_optional(&self.pool)
            .await?;
            if existing.is_none() {
                new_torrents.push(t.clone());
            }
        }
        Ok(new_torrents)
    }

    pub async fn search_by_qb_hash(&self, qb_hash: &str) -> Result<Option<Torrent>, DbError> {
        let t = sqlx::query_as::<_, Torrent>("SELECT * FROM torrent WHERE qb_hash=?")
            .bind(qb_hash).fetch_optional(&self.pool).await?;
        Ok(t)
    }

    pub async fn search_by_qb_hashes(&self, qb_hashes: &[String]) -> Result<Vec<Torrent>, DbError> {
        if qb_hashes.is_empty() {
            return Ok(Vec::new());
        }
        let params: Vec<String> = qb_hashes.iter().map(|_| "?".to_string()).collect();
        let sql = format!("SELECT * FROM torrent WHERE qb_hash IN ({})", params.join(","));
        let mut query = sqlx::query_as::<_, Torrent>(&sql);
        for hash in qb_hashes {
            query = query.bind(hash);
        }
        let results = query.fetch_all(&self.pool).await?;
        Ok(results)
    }

    pub async fn delete_by_bangumi_id(&self, bangumi_id: i32) -> Result<i32, DbError> {
        let result = sqlx::query("DELETE FROM torrent WHERE refer_id=?")
            .bind(bangumi_id).execute(&self.pool).await?;
        Ok(result.rows_affected() as i32)
    }

    pub async fn search_by_url(&self, url: &str) -> Result<Option<Torrent>, DbError> {
        let t = sqlx::query_as::<_, Torrent>("SELECT * FROM torrent WHERE url=?")
            .bind(url).fetch_optional(&self.pool).await?;
        Ok(t)
    }

    pub async fn update_qb_hash(&self, torrent_id: i32, qb_hash: &str) -> Result<bool, DbError> {
        let rows = sqlx::query("UPDATE torrent SET qb_hash=? WHERE id=?")
            .bind(qb_hash).bind(torrent_id).execute(&self.pool).await?;
        Ok(rows.rows_affected() > 0)
    }
}
