# SQLite Migrations

## Persistent Duplicate Search - Version 10 / PLAN-250

0010_duplicate_cache.sql adds versioned per-media fingerprints with cascading deletion and scope-keyed generation-protected JSON search checkpoints. Existing migrations, media identities, metadata and version-9 dissimilar decisions are preserved. Fresh/reopen, version-9 upgrade/history preservation and rollback fixtures cover ten migrations and sixteen application tables; synthetic pending/failure work uses version 11. Fingerprint and snapshot SQL writes never span filesystem hashing or revision checks.

## Metadata Presets - Version 8 / PLAN-215

0008_metadata_presets.sql adds the thirteenth application table for independent reusable metadata snapshots, with unique normalized names and JSON validity enforcement. Versions 1-7 remain byte-for-byte unchanged. Fresh/reopen, version-7 upgrade preservation, history checks and rollback regressions cover the new migration; synthetic pending migrations use version 9.

## Replacement Cleanup — Version 7

0007_replacement_cleanup.sql adds a default-zero delete_old policy to replacement intent/history. Previously recorded operations retain their policy; new replacements explicitly request old-original deletion after commit and temporary-backup removal after cleanup. Existing schema/history is preserved. Custom pending/broken migration fixtures now use version 8; application table count stays twelve.

## Recoverable Media Replacement — Version 6

The migration also adds a nullable normalized video-title search key and a trigger that clears it after title changes. Keys are populated lazily in bounded transactions; existing titles remain unchanged.

0006_media_replacements.sql adds durable replacement intent/provenance, commit/completion phases and lookup indexes for pending recovery and retained originals. Main identity and annotations survive Stage retirement and physical rebinding. Disk-off completed rows suppress unchanged originals during scans; disk-on rows retain backup locations. Fresh and version-one upgrade tests include the schema; replacement tests cover reopen recovery and transactional rollback. Custom migration-integrity fixtures now use version 7; they do not edit or load a real application database.

## Duplicate File Order — Version 5

0005_duplicate_order.sql adds nullable media.naming_order_key and nonnegative-or-NULL naming_order. Existing rows preserve stable ID ordering. Matching Library-root/generated-name keys activate saved positions; unmatched/unpositioned records use ID order after positioned members. Domain commands validate group membership and persist all positions transactionally; filesystem changes remain journaled disk reconciliation. Versions 1–4 remain unchanged. Upgrade, rollback and migration-history fixtures now include version 5 and use version 6 for custom pending migrations.

The Rust database service embeds the `.sql` migrations with `sqlx::migrate!("./migrations")`. No SQLx CLI, DATABASE_URL, schema database or migration directory is required on the user's machine. The existing build script watches this directory so new migration files trigger recompilation.

## Adding a Migration

1. Add the next unique, increasing version, for example `0002_add_preview_records.sql`.
2. Write a forward schema/data change. SQLx runs each migration in a transaction by default; do not include manual BEGIN/COMMIT, disable foreign keys or opt out of transactions. Keep connection PRAGMAs in database.rs.
3. Never modify, delete or renumber a migration already shipped. Use a new migration for corrections; SQLx compares stored checksums and rejects changed/missing history.
4. Test both a new empty database and an upgrade from the previous version, including preservation of user metadata and rollback on failure.
5. Run strict all-target Clippy, Rust tests and formatting checks.

## Visual Review Decisions — Version 9

0009_duplicate_rejections.sql adds canonical distinct lowercase SHA-256 pair exclusions and their creation timestamp. It preserves all earlier migrations and media metadata. Decisions survive record deletion/renaming because they describe content, not file identity. Fresh, reopen, previous-version upgrade, migration-history and rollback tests cover nine migrations and fourteen application tables; custom pending/failure fixtures use version 10.

`0001_init.sql` initializes ten application tables, indexes and video-kind integrity triggers. SQLx records applied versions/checksums in its own `_sqlx_migrations` table. Do not maintain a second schema version or use CREATE TABLE IF NOT EXISTS to conceal schema conflicts.

## Runtime Behavior

PLAN-086 canonicalizes embedded SQL to LF before applying new migrations; `.gitattributes` also pins SQL checkout endings to LF. Existing versions 1-4 accept exact SHA-384 matches for the same SQL with LF or CRLF, plus the shipped version-2 LF body/final CRLF variant. Compatibility adjusts only the in-memory migrator checksum, never stored history or user metadata. Changed SQL, missing migrations, dirty history and failed pending migrations remain SQLx errors. Setup logs and flushes failures before Tauri can abort the release process. Do not delete a database to recover from a checksum mismatch.

Startup opens only the derived NexaNew database path, creates its parent directory when needed, validates migration history and applies pending migrations before publishing managed database state. It refuses a nonempty schema without SQLx history, rather than assuming it is an empty Nexa database. Failed initialization closes the pool and returns an explicit startup error; it does not delete, reset or fall back to an in-memory database. Migrations are forward-only; automatic downgrade/reset is not supported.

SQLite runs with WAL, synchronous FULL, foreign keys on every connection, a five-second busy timeout and a pool of at most four connections. SQL statement logging is disabled to avoid logging media metadata. Native repositories can use Database::pool() for bound queries and Database::begin() for transactions; they own domain validation and operation errors. Normal Tauri exit explicitly awaits Database::close(). Do not copy only the main database file for a live backup in WAL mode; a future backup feature must use a SQLite-aware protocol.

The initial schema permits an unknown artist/title on unprocessed discoveries. A future managed save/distribution service must enforce the required single artist and applicable video title. Character/copyright/tag order is derived alphabetically by future domain queries; SQL row order is never a presentation guarantee. Filesystem-aware path comparison, Unicode key normalization, media indexing and source/config reconciliation are outside this foundation.

## Redistribution Workflow — Version 2

0002_redistribution.sql adds the persistent media.needs_distribution boolean and its partial index without modifying version 1 or adding tables. Copyright/artist deletion sets this flag within the same transaction that removes dictionary relations. Physical source/path identity and technical metadata_state stay unchanged. The version-1 upgrade test verifies existing records and metadata survive with the default zero flag. Future Library/scanner/distribution services must respect this workflow flag; the current preview wizard does not clear it.

## Stage Indexing — Version 3

0003_stage_scan.sql adds source revision, extractor version, searchable path keys and a Stage browse index. It changes no prior migration or table count. Fresh, reopen, version-1 upgrade and migration integrity tests cover the current three-version schema; custom pending/failing migration fixtures now use version 4. The Stage repository fills search keys on discovery or lazily for imported/redistribution records without filesystem reads.
