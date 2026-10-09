# SQLite Migrations

The Rust database service embeds the `.sql` migrations with `sqlx::migrate!("./migrations")`. No SQLx CLI, DATABASE_URL, schema database or migration directory is required on the user's machine. The existing build script watches this directory so new migration files trigger recompilation.

## Adding a Migration

1. Add the next unique, increasing version, for example `0002_add_preview_records.sql`.
2. Write a forward schema/data change. SQLx runs each migration in a transaction by default; do not include manual BEGIN/COMMIT, disable foreign keys or opt out of transactions. Keep connection PRAGMAs in database.rs.
3. Never modify, delete or renumber a migration already shipped. Use a new migration for corrections; SQLx compares stored checksums and rejects changed/missing history.
4. Test both a new empty database and an upgrade from the previous version, including preservation of user metadata and rollback on failure.
5. Run strict all-target Clippy, Rust tests and formatting checks.

`0001_init.sql` initializes ten application tables, indexes and video-kind integrity triggers. SQLx records applied versions/checksums in its own `_sqlx_migrations` table. Do not maintain a second schema version or use CREATE TABLE IF NOT EXISTS to conceal schema conflicts.

## Runtime Behavior

Startup opens only the derived NexaNew database path, creates its parent directory when needed, validates migration history and applies pending migrations before publishing managed database state. It refuses a nonempty schema without SQLx history, rather than assuming it is an empty Nexa database. Failed initialization closes the pool and returns an explicit startup error; it does not delete, reset or fall back to an in-memory database. Migrations are forward-only; automatic downgrade/reset is not supported.

SQLite runs with WAL, synchronous FULL, foreign keys on every connection, a five-second busy timeout and a pool of at most four connections. SQL statement logging is disabled to avoid logging media metadata. Native repositories can use Database::pool() for bound queries and Database::begin() for transactions; they own domain validation and operation errors. Normal Tauri exit explicitly awaits Database::close(). Do not copy only the main database file for a live backup in WAL mode; a future backup feature must use a SQLite-aware protocol.

The initial schema permits an unknown artist/title on unprocessed discoveries. A future managed save/distribution service must enforce the required single artist and applicable video title. Character/copyright/tag order is derived alphabetically by future domain queries; SQL row order is never a presentation guarantee. Filesystem-aware path comparison, Unicode key normalization, media indexing and source/config reconciliation are outside this foundation.

## Redistribution Workflow — Version 2

0002_redistribution.sql adds the persistent media.needs_distribution boolean and its partial index without modifying version 1 or adding tables. Copyright/artist deletion sets this flag within the same transaction that removes dictionary relations. Physical source/path identity and technical metadata_state stay unchanged. The version-1 upgrade test verifies existing records and metadata survive with the default zero flag. Future Library/scanner/distribution services must respect this workflow flag; the current preview wizard does not clear it.
