//! Native SQLite foundation. Domain repositories use the pool and transactions;
//! arbitrary SQL is never exposed to the webview.
use std::{path::Path, time::Duration};

use sqlx::{
    AssertSqlSafe, ConnectOptions, SqlSafeStr, Sqlite, SqlitePool, Transaction,
    migrate::{Migration, Migrator},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database path must be absolute and have a filename")]
    InvalidPath,
    #[error("Cannot create database directory: {0}")]
    Directory(#[source] std::io::Error),
    #[error("Cannot open SQLite database: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("Cannot inspect database schema: {0}")]
    Inspect(#[source] sqlx::Error),
    #[error("Existing database has an unrecognized schema; refusing to initialize it")]
    UnmanagedSchema,
    #[error("Cannot apply database migrations: {0}")]
    Migration(#[source] sqlx::migrate::MigrateError),
}

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create/open the isolated database and validate/apply embedded migrations.
    /// Startup owns reporting returned failures, avoiding duplicate error events.
    #[tracing::instrument(skip_all, fields(database_path = %path.display()))]
    pub async fn open(path: &Path) -> Result<Self, DatabaseError> {
        Self::open_with_migrator(path, &MIGRATOR).await
    }

    async fn open_with_migrator(path: &Path, migrator: &Migrator) -> Result<Self, DatabaseError> {
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(DatabaseError::InvalidPath);
        }

        let parent = path.parent().ok_or(DatabaseError::InvalidPath)?;

        tracing::info!("Opening SQLite database");
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(DatabaseError::Directory)?;

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .collation("NEXA_FILENAME", crate::naming::compare_names)
            .busy_timeout(Duration::from_secs(5))
            // SQL and bound values can contain personal media metadata.
            .disable_statement_logging();

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(options)
            .await
            .map_err(DatabaseError::Connect)?;

        if let Err(error) = Self::initialize(&pool, migrator).await {
            pool.close().await;

            return Err(error);
        }

        tracing::info!(max_connections = 4, "SQLite database ready");
        Ok(Self { pool })
    }

    async fn initialize(pool: &SqlitePool, migrator: &Migrator) -> Result<(), DatabaseError> {
        let (tables, tracked): (i64, i64) = sqlx::query_as(
            "SELECT count(*), coalesce(sum(name = '_sqlx_migrations'), 0)
             FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(pool)
        .await
        .map_err(DatabaseError::Inspect)?;

        if tables > 0 && tracked == 0 {
            return Err(DatabaseError::UnmanagedSchema);
        }

        tracing::info!(
            embedded_migrations = migrator.iter().count(),
            "Validating and applying database migrations"
        );

        let compatible = Self::compatible_migrator(pool, migrator, tracked != 0).await?;
        compatible
            .run(pool)
            .await
            .map_err(DatabaseError::Migration)?;

        let version: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(version), 0) FROM _sqlx_migrations WHERE success = 1",
        )
        .fetch_one(pool)
        .await
        .map_err(DatabaseError::Inspect)?;

        tracing::info!(schema_version = version, "Database migrations completed");
        Ok(())
    }

    /// Accept only byte-proven line-ending variants of the shipped SQL. Keep
    /// recorded checksums untouched; SQLx still validates all other history.
    async fn compatible_migrator(
        pool: &SqlitePool,
        migrator: &Migrator,
        tracked: bool,
    ) -> Result<Migrator, DatabaseError> {
        let applied: Vec<(i64, Vec<u8>)> = if tracked {
            sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations WHERE success = 1")
                .fetch_all(pool)
                .await
                .map_err(DatabaseError::Inspect)?
        } else {
            Vec::new()
        };

        let mut migrations = Vec::new();

        for migration in migrator.iter() {
            let lf = migration.sql.as_str().replace("\r\n", "\n");

            let mut canonical = migration_with_sql(migration, lf.clone());

            if let Some((_, checksum)) = applied.iter().find(|(v, _)| *v == migration.version)
                && checksum.as_slice() != canonical.checksum.as_ref()
                && (1..=4).contains(&migration.version)
            {
                let mut variants = vec![lf.replace('\n', "\r\n")];
                // Local pre-CI version 2 had LF lines and a final CRLF.
                if migration.version == 2
                    && let Some(body) = lf.strip_suffix('\n')
                {
                    variants.push(format!("{body}\r\n"));
                }

                if variants.iter().any(|sql| {
                    migration_with_sql(migration, sql.clone()).checksum.as_ref()
                        == checksum.as_slice()
                }) {
                    canonical.checksum = checksum.clone().into();

                    tracing::warn!(
                        migration_version = migration.version,
                        "Accepted historical migration line endings; stored history preserved"
                    );
                }
            }
            migrations.push(canonical);
        }
        Ok(Migrator {
            migrations: migrations.into(),
            ignore_missing: migrator.ignore_missing,
            locking: migrator.locking,
            no_tx: migrator.no_tx,
            table_name: migrator.table_name.clone(),
            create_schemas: migrator.create_schemas.clone(),
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Dropping an uncommitted transaction rolls it back through SQLx.
    pub async fn begin(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
        self.pool.begin().await
    }

    #[tracing::instrument(skip_all)]
    pub async fn close(&self) {
        tracing::info!("Closing SQLite database");
        self.pool.close().await;

        tracing::info!("SQLite database closed");
    }
}

fn migration_with_sql(migration: &Migration, sql: String) -> Migration {
    Migration::new(
        migration.version,
        migration.description.clone(),
        migration.migration_type,
        // Only trusted embedded migration SQL with line-ending substitutions.
        AssertSqlSafe(sql).into_sql_str(),
        migration.no_tx,
    )
}

#[cfg(test)]
mod tests;
