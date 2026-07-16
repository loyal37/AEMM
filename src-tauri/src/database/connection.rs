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
}
