use sqlx::SqlitePool;
use sha2::{Sha256, Digest};
use crate::models::user::{User, UserUpdate};
use crate::error::DbError;

pub trait PasswordHasher: Send + Sync {
    fn hash_password(&self, password: &str) -> String;
    fn verify_password(&self, password: &str, hash: &str) -> bool;
}

pub struct DefaultHasher;

impl PasswordHasher for DefaultHasher {
    fn hash_password(&self, password: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn verify_password(&self, password: &str, hash: &str) -> bool {
        self.hash_password(password) == hash
    }
}

pub struct UserRepo {
    pool: SqlitePool,
    hasher: Box<dyn PasswordHasher>,
}

impl UserRepo {
    pub fn new(pool: SqlitePool, hasher: Box<dyn PasswordHasher>) -> Self {
        Self { pool, hasher }
    }

    pub async fn get_user(&self, username: &str) -> Result<User, DbError> {
        let user = sqlx::query_as::<_, User>("SELECT * FROM user WHERE username=?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("user {username}")))?;
        Ok(user)
    }

    pub async fn auth_user(&self, username: &str, password: &str) -> Result<bool, DbError> {
        let user = self.get_user(username).await?;
        Ok(self.hasher.verify_password(password, &user.password))
    }

    pub async fn update_user(&self, username: &str, update: &UserUpdate) -> Result<User, DbError> {
        let user = self.get_user(username).await?;
        let new_username = update.username.as_deref().unwrap_or(&user.username);
        let new_password = match &update.password {
            Some(p) => self.hasher.hash_password(p),
            None => user.password,
        };
        sqlx::query("UPDATE user SET username=?1, password=?2 WHERE id=?3")
            .bind(new_username)
            .bind(&new_password)
            .bind(user.id)
            .execute(&self.pool)
            .await?;
        self.get_user(new_username).await
    }

    pub async fn add_default_user(&self) -> Result<(), DbError> {
        let existing: Option<(i32,)> = sqlx::query_as("SELECT id FROM user LIMIT 1")
            .fetch_optional(&self.pool).await?;
        if existing.is_some() {
            return Ok(());
        }
        let default_password = self.hasher.hash_password("admin");
        sqlx::query("INSERT INTO user (username, password) VALUES ('admin', ?)")
            .bind(&default_password)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
