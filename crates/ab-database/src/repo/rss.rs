use sqlx::SqlitePool;
use crate::models::rss::{RSSItem, RSSUpdate};
use crate::error::DbError;

pub struct RssRepo {
    pool: SqlitePool,
}

impl RssRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn add(&self, data: &RSSItem) -> Result<bool, DbError> {
        let existing: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM rssitem WHERE url = ?"
        )
        .bind(&data.url)
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Ok(false);
        }

        sqlx::query(
            "INSERT INTO rssitem (name, url, aggregate, parser, enabled) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&data.name)
        .bind(&data.url)
        .bind(data.aggregate)
        .bind(&data.parser)
        .bind(data.enabled)
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    pub async fn add_all(&self, data: &[RSSItem]) -> Result<(), DbError> {
        for item in data {
            self.add(item).await?;
        }
        Ok(())
    }

    pub async fn update(&self, id: i32, data: &RSSUpdate) -> Result<bool, DbError> {
        let rows = sqlx::query(
            "UPDATE rssitem SET name=COALESCE(?1, name), url=COALESCE(?2, url), \
             aggregate=COALESCE(?3, aggregate), parser=COALESCE(?4, parser), \
             enabled=COALESCE(?5, enabled), connection_status=COALESCE(?6, connection_status), \
             last_checked_at=COALESCE(?7, last_checked_at), last_error=COALESCE(?8, last_error) \
             WHERE id=?9"
        )
        .bind(&data.name)
        .bind(&data.url)
        .bind(data.aggregate)
        .bind(&data.parser)
        .bind(data.enabled)
        .bind(&data.connection_status)
        .bind(&data.last_checked_at)
        .bind(&data.last_error)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(rows.rows_affected() > 0)
    }

    pub async fn enable(&self, id: i32) -> Result<bool, DbError> {
        let rows = sqlx::query("UPDATE rssitem SET enabled=1 WHERE id=?")
            .bind(id).execute(&self.pool).await?;
        Ok(rows.rows_affected() > 0)
    }

    pub async fn enable_batch(&self, ids: &[i32]) -> Result<(), DbError> {
        for id in ids {
            self.enable(*id).await?;
        }
        Ok(())
    }

    pub async fn disable(&self, id: i32) -> Result<bool, DbError> {
        let rows = sqlx::query("UPDATE rssitem SET enabled=0 WHERE id=?")
            .bind(id).execute(&self.pool).await?;
        Ok(rows.rows_affected() > 0)
    }

    pub async fn disable_batch(&self, ids: &[i32]) -> Result<(), DbError> {
        for id in ids {
            self.disable(*id).await?;
        }
        Ok(())
    }

    pub async fn search_id(&self, id: i32) -> Result<Option<RSSItem>, DbError> {
        let item = sqlx::query_as::<_, RSSItem>("SELECT * FROM rssitem WHERE id=?")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(item)
    }

    pub async fn search_all(&self) -> Result<Vec<RSSItem>, DbError> {
        let items = sqlx::query_as::<_, RSSItem>("SELECT * FROM rssitem")
            .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn search_active(&self) -> Result<Vec<RSSItem>, DbError> {
        let items = sqlx::query_as::<_, RSSItem>("SELECT * FROM rssitem WHERE enabled=1")
            .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn search_aggregate(&self) -> Result<Vec<RSSItem>, DbError> {
        let items = sqlx::query_as::<_, RSSItem>(
            "SELECT * FROM rssitem WHERE aggregate=1 AND enabled=1"
        )
        .fetch_all(&self.pool).await?;
        Ok(items)
    }

    pub async fn delete(&self, id: i32) -> Result<bool, DbError> {
        let rows = sqlx::query("DELETE FROM rssitem WHERE id=?")
            .bind(id).execute(&self.pool).await?;
        Ok(rows.rows_affected() > 0)
    }

    pub async fn delete_all(&self) -> Result<(), DbError> {
        sqlx::query("DELETE FROM rssitem").execute(&self.pool).await?;
        Ok(())
    }
}
