use sqlx::SqlitePool;
use crate::models::passkey::{Passkey, PasskeyCreate};
use crate::error::DbError;

pub struct PasskeyRepo {
    pool: SqlitePool,
}

impl PasskeyRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_passkey(&self, data: &PasskeyCreate) -> Result<Passkey, DbError> {
        sqlx::query(
            "INSERT INTO passkey (user_id, name, credential_id, public_key, aaguid, transports) \
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(data.user_id)
        .bind(&data.name)
        .bind(&data.credential_id)
        .bind(&data.public_key)
        .bind(&data.aaguid)
        .bind(&data.transports)
        .execute(&self.pool)
        .await?;

        let id: (i32,) = sqlx::query_as("SELECT last_insert_rowid()")
            .fetch_one(&self.pool).await?;
        self.get_passkey_by_id(id.0, data.user_id).await
    }

    pub async fn get_passkey_by_credential_id(&self, credential_id: &str) -> Result<Option<Passkey>, DbError> {
        let pk = sqlx::query_as::<_, Passkey>(
            "SELECT * FROM passkey WHERE credential_id=?"
        )
        .bind(credential_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(pk)
    }

    pub async fn get_passkeys_by_user_id(&self, user_id: i32) -> Result<Vec<Passkey>, DbError> {
        let keys = sqlx::query_as::<_, Passkey>("SELECT * FROM passkey WHERE user_id=?")
            .bind(user_id).fetch_all(&self.pool).await?;
        Ok(keys)
    }

    pub async fn get_passkey_by_id(&self, passkey_id: i32, user_id: i32) -> Result<Passkey, DbError> {
        let pk = sqlx::query_as::<_, Passkey>(
            "SELECT * FROM passkey WHERE id=? AND user_id=?"
        )
        .bind(passkey_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| DbError::NotFound(format!("passkey {passkey_id}")))?;
        Ok(pk)
    }

    pub async fn update_passkey_usage(&self, passkey: &Passkey, new_sign_count: i32) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE passkey SET sign_count=?1, last_used_at=CURRENT_TIMESTAMP WHERE id=?2"
        )
        .bind(new_sign_count)
        .bind(passkey.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_passkey(&self, passkey_id: i32, user_id: i32) -> Result<bool, DbError> {
        let rows = sqlx::query("DELETE FROM passkey WHERE id=? AND user_id=?")
            .bind(passkey_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(rows.rows_affected() > 0)
    }
}
