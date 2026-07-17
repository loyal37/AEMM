use std::{path::Path, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use crate::errors::AppError;

#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn connect(path: &Path) -> Result<Self, AppError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;
        let database = Self { pool };
        database.verify_integrity().await?;
        database.health_check().await?;
        Ok(database)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn health_check(&self) -> Result<(), AppError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn verify_integrity(&self) -> Result<(), AppError> {
        let quick_check: String = sqlx::query_scalar("PRAGMA quick_check(1)")
            .fetch_one(&self.pool)
            .await?;
        if quick_check != "ok" {
            return Err(AppError::DataIntegrity(format!(
                "SQLite quick_check failed: {quick_check}"
            )));
        }
        if sqlx::query("PRAGMA foreign_key_check")
            .fetch_optional(&self.pool)
            .await?
            .is_some()
        {
            return Err(AppError::DataIntegrity(
                "SQLite foreign-key integrity check failed.".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Database;

    #[tokio::test]
    async fn applies_migrations_and_seeds_default_profile() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;

        let profile_name: String = sqlx::query_scalar(
            "SELECT name FROM profiles WHERE id = '00000000-0000-0000-0000-000000000001'",
        )
        .fetch_one(database.pool())
        .await?;

        assert_eq!(profile_name, "默认配置");

        let modified_at_columns: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('mod_files') WHERE name = 'modified_at'",
        )
        .fetch_one(database.pool())
        .await?;
        let tags_json_columns: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('mod_local_metadata') WHERE name = 'tags_json'",
        )
        .fetch_one(database.pool())
        .await?;

        assert_eq!(modified_at_columns, 1);
        assert_eq!(tags_json_columns, 1);

        let active_profile_id: String =
            sqlx::query_scalar("SELECT active_profile_id FROM app_state WHERE singleton = 1")
                .fetch_one(database.pool())
                .await?;
        assert_eq!(active_profile_id, "00000000-0000-0000-0000-000000000001");
        Ok(())
    }

    #[tokio::test]
    async fn detects_foreign_key_corruption() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let database = Database::connect(&directory.path().join("mods.db")).await?;
        let mut connection = database.pool().acquire().await?;
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *connection)
            .await?;
        sqlx::query(
            "INSERT INTO profile_mods (profile_id, mod_id, enabled, load_order)
             VALUES ('missing-profile', 'missing-mod', 0, 0)",
        )
        .execute(&mut *connection)
        .await?;
        drop(connection);

        assert!(database.verify_integrity().await.is_err());
        Ok(())
    }
}
