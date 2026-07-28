use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::collector::{CollectionSummary, PersistedCheckpoint};
use crate::models::{
    Annotation, AnnotationKind, AppSettings, Confidence, CurrentQuote, DiagnosticReason,
    DiagnosticsSummary, HistoryPoint, HistoryResponse, QuoteStatus, Range, RangeStatistics,
    ALGORITHM_VERSION,
};
use crate::parser::{event_fingerprint, UsageEvent};

const WEEKLY_WINDOW_MINUTES: f64 = 10_080.0;
const WEEKLY_WINDOW_TOLERANCE_MINUTES: f64 = 240.0;
const RESET_TIMESTAMP_JITTER_MS: i64 = 5 * 60 * 1_000;

#[derive(Clone, Copy)]
struct ApiPrice {
    input: f64,
    cached_input: f64,
    output: f64,
}

// Verified 2026-07-24 from OpenAI's model catalog and individual model pages;
// see docs/CALCULATION.md for the source links. Rates are USD / 1M text tokens.
fn official_price(model: &str) -> Option<ApiPrice> {
    match model.trim().to_ascii_lowercase().as_str() {
        "gpt-5.6" | "gpt-5.6-sol" | "chat-latest" => Some(ApiPrice {
            input: 5.0,
            cached_input: 0.5,
            output: 30.0,
        }),
        "gpt-5.6-terra" | "gpt-5.4" => Some(ApiPrice {
            input: 2.5,
            cached_input: 0.25,
            output: 15.0,
        }),
        "gpt-5.6-luna" => Some(ApiPrice {
            input: 1.0,
            cached_input: 0.1,
            output: 6.0,
        }),
        "gpt-5.5" => Some(ApiPrice {
            input: 5.0,
            cached_input: 0.5,
            output: 30.0,
        }),
        "gpt-5.5-pro" | "gpt-5.4-pro" => Some(ApiPrice {
            input: 30.0,
            cached_input: 0.0,
            output: 180.0,
        }),
        "gpt-5.4-mini" => Some(ApiPrice {
            input: 0.75,
            cached_input: 0.075,
            output: 4.5,
        }),
        "gpt-5.4-nano" => Some(ApiPrice {
            input: 0.2,
            cached_input: 0.02,
            output: 1.25,
        }),
        "gpt-5.3-codex" | "gpt-5.2-codex" => Some(ApiPrice {
            input: 1.75,
            cached_input: 0.175,
            output: 14.0,
        }),
        "gpt-5" | "gpt-5-codex" | "gpt-5.1-codex" | "gpt-5.1-codex-max" | "gpt-5-chat-latest" => {
            Some(ApiPrice {
                input: 1.25,
                cached_input: 0.125,
                output: 10.0,
            })
        }
        "gpt-5.1-codex-mini" | "gpt-5-mini" => Some(ApiPrice {
            input: 0.25,
            cached_input: 0.025,
            output: 2.0,
        }),
        "gpt-5-nano" => Some(ApiPrice {
            input: 0.05,
            cached_input: 0.005,
            output: 0.4,
        }),
        "codex-mini-latest" => Some(ApiPrice {
            input: 1.50,
            cached_input: 0.375,
            output: 6.0,
        }),
        "gpt-4.1" => Some(ApiPrice {
            input: 2.0,
            cached_input: 0.5,
            output: 8.0,
        }),
        "gpt-4.1-mini" => Some(ApiPrice {
            input: 0.4,
            cached_input: 0.1,
            output: 1.6,
        }),
        "gpt-4.1-nano" => Some(ApiPrice {
            input: 0.1,
            cached_input: 0.025,
            output: 0.4,
        }),
        "gpt-4o" => Some(ApiPrice {
            input: 2.5,
            cached_input: 1.25,
            output: 10.0,
        }),
        "gpt-4o-mini" => Some(ApiPrice {
            input: 0.15,
            cached_input: 0.075,
            output: 0.6,
        }),
        "o1" => Some(ApiPrice {
            input: 15.0,
            cached_input: 7.5,
            output: 60.0,
        }),
        "o3" => Some(ApiPrice {
            input: 2.0,
            cached_input: 0.5,
            output: 8.0,
        }),
        "o3-mini" | "o4-mini" => Some(ApiPrice {
            input: 1.1,
            cached_input: 0.275,
            output: 4.4,
        }),
        _ => None,
    }
}

fn event_cost(event: &UsageEvent, settings: &AppSettings) -> Result<(f64, &'static str), String> {
    let model = event.model.trim().to_ascii_lowercase().replace('/', "-");
    let custom = settings.custom_pricing.iter().find(|override_price| {
        override_price.model_id.trim().eq_ignore_ascii_case(&model)
            || override_price
                .alias
                .as_deref()
                .is_some_and(|alias| alias.trim().eq_ignore_ascii_case(&model))
    });
    let (price, source) = if let Some(price) = custom {
        (
            ApiPrice {
                input: price.input_usd_per_million,
                cached_input: price.cached_input_usd_per_million,
                output: price.output_usd_per_million,
            },
            "custom",
        )
    } else if let Some(price) = official_price(&model) {
        (price, "official")
    } else {
        return Err(format!(
            "unknown API price for model {model}; add a local custom price override"
        ));
    };
    // Official GPT-5.4/5.5/5.6 long-context rules apply 2x input and 1.5x output
    // above 272K input tokens. Cache-write token counts are not present in Codex JSONL,
    // so they remain pending in the cached-input bucket instead of being guessed.
    let long_context = event.long_context || event.input_tokens > 272_000;
    let documented_long_context = model.starts_with("gpt-5.4")
        || model.starts_with("gpt-5.5")
        || model.starts_with("gpt-5.6");
    let multiplier_input = if long_context && documented_long_context {
        2.0
    } else {
        1.0
    };
    let multiplier_output = if long_context && documented_long_context {
        1.5
    } else {
        1.0
    };
    // `input_tokens` includes cached input and `reasoning_tokens` is an output
    // detail in Codex/Responses records. Charge each physical token once.
    let uncached_input = event.input_tokens.saturating_sub(event.cached_input_tokens);
    let billed_output = if event.output_tokens > 0 {
        event.output_tokens
    } else {
        event.reasoning_tokens
    };
    let cost = (uncached_input as f64 * price.input * multiplier_input
        + event.cached_input_tokens as f64 * price.cached_input
        + billed_output as f64 * price.output * multiplier_output)
        / 1_000_000.0;
    if cost.is_finite() && cost >= 0.0 {
        Ok((cost, source))
    } else {
        Err("non-finite token-derived API cost".into())
    }
}

#[derive(Clone)]
struct StoredUsageForPricing {
    fingerprint: String,
    event: UsageEvent,
}

pub struct Database {
    pub path: PathBuf,
    connection: Connection,
}

pub struct LatestQuotaObservation {
    pub account_key: Option<String>,
    pub limit_id: Option<String>,
    pub observed_at_ms: i64,
    pub used_percent: f64,
    pub reset_at_ms: Option<i64>,
    pub plan: Option<String>,
}

#[derive(Clone, Debug)]
struct QuotaPoint {
    account_key: Option<String>,
    limit_id: Option<String>,
    observed_at_ms: i64,
    reset_at_ms: Option<i64>,
    used_percent: f64,
}

#[derive(Clone)]
struct WindowGroup {
    account_key: Option<String>,
    limit_id: Option<String>,
    reset_at_ms: Option<i64>,
    started_at_ms: i64,
    ended_at_ms: i64,
    reset_reason: String,
    points: Vec<QuotaPoint>,
}

#[derive(Clone)]
struct StoredPoint {
    point: HistoryPoint,
    window_id: i64,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn empty_history(range: Range) -> HistoryResponse {
    HistoryResponse {
        bucket: range.bucket().into(),
        statistics: RangeStatistics {
            range,
            baseline_estimated_weekly_value_usd: None,
            baseline_timestamp: None,
            current_estimated_weekly_value_usd: None,
            delta_value_usd: None,
            delta_percent: None,
            point_count: 0,
            partial: true,
        },
        points: Vec::new(),
    }
}

pub fn data_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("NERFIFY_DATA_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = crate::discovery::home_dir() {
        return home
            .join("Library")
            .join("Application Support")
            .join("Nerfify");
    }
    #[cfg(target_os = "windows")]
    if let Some(app_data) = std::env::var_os("APPDATA") {
        return PathBuf::from(app_data).join("Nerfify");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home).join("Nerfify");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(home) = crate::discovery::home_dir() {
        return home.join(".local").join("share").join("Nerfify");
    }
    PathBuf::from(".nerfify")
}

fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(connection)
}

fn preserve_database_files(path: &Path) -> Result<(), String> {
    let recovery = path.with_file_name(format!("nerfify.recovery-{}.db", now_ms()));
    let base_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("nerfify.db");
    for suffix in ["", "-wal", "-shm"] {
        let source = path.with_file_name(format!("{base_name}{suffix}"));
        if !source.exists() {
            continue;
        }
        let recovery_name = recovery
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("nerfify.recovery.db");
        let target = recovery.with_file_name(format!("{recovery_name}{suffix}"));
        fs::rename(&source, &target).map_err(|_| {
            "database is corrupt and recovery copy could not be created".to_string()
        })?;
    }
    Ok(())
}

impl Database {
    pub fn open() -> Result<Self, String> {
        let directory = data_directory();
        fs::create_dir_all(&directory)
            .map_err(|_| "unable to create local data directory".to_string())?;
        let path = directory.join("nerfify.db");
        let mut database = match open_connection(&path) {
            Ok(connection) => Self {
                path: path.clone(),
                connection,
            },
            Err(_) if path.exists() => {
                preserve_database_files(&path)?;
                Self {
                    path: path.clone(),
                    connection: open_connection(&path)
                        .map_err(|_| "unable to create clean local database".to_string())?,
                }
            }
            Err(error) => return Err(format!("unable to open local database: {error}")),
        };
        if database.migrate().is_err() {
            drop(database);
            preserve_database_files(&path)?;
            database = Self {
                path: path.clone(),
                connection: open_connection(&path)
                    .map_err(|_| "unable to create clean local database".to_string())?,
            };
            database.migrate()?;
        }
        database.restrict_directory_permissions(&directory);
        database.restrict_file_permissions();
        database.record_app_run()?;
        Ok(database)
    }

    fn migrate(&mut self) -> Result<(), String> {
        let previous_version = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap_or_default();
        self.connection
            .execute_batch(
                "PRAGMA foreign_keys=ON;
                BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS accounts (
                    account_key TEXT PRIMARY KEY,
                    plan TEXT,
                    created_at_ms INTEGER NOT NULL,
                    last_seen_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS source_checkpoints (
                    source_key TEXT PRIMARY KEY,
                    byte_offset INTEGER NOT NULL DEFAULT 0,
                    parser_state_json TEXT NOT NULL DEFAULT '{}',
                    source_active INTEGER NOT NULL DEFAULT 1,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS usage_events (
                    fingerprint TEXT PRIMARY KEY,
                    account_key TEXT,
                    timestamp_ms INTEGER NOT NULL,
                    model_id TEXT NOT NULL,
                    input_tokens INTEGER NOT NULL,
                    cached_input_tokens INTEGER NOT NULL,
                    output_tokens INTEGER NOT NULL,
                    eligible INTEGER NOT NULL DEFAULT 0,
                    pricing_status TEXT NOT NULL DEFAULT 'not_applicable',
                    cost_usd REAL,
                    credits REAL,
                    logged_charge_usd REAL,
                    credit_source TEXT NOT NULL DEFAULT 'unavailable',
                    credit_status TEXT NOT NULL DEFAULT 'pending',
                    quota_reset_at_ms INTEGER,
                    quota_limit_id TEXT,
                    FOREIGN KEY(account_key) REFERENCES accounts(account_key)
                );
                CREATE TABLE IF NOT EXISTS pricing_snapshots (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source TEXT NOT NULL,
                    observed_at_ms INTEGER NOT NULL,
                    version TEXT,
                    etag TEXT,
                    sha256 TEXT NOT NULL,
                    is_current INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS quota_snapshots (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    account_key TEXT,
                    observed_at_ms INTEGER NOT NULL,
                    reset_at_ms INTEGER,
                    duration_minutes REAL,
                    limit_id TEXT,
                    plan TEXT,
                    used_percent REAL,
                    connection_quality TEXT,
                    FOREIGN KEY(account_key) REFERENCES accounts(account_key)
                );
                CREATE TABLE IF NOT EXISTS epochs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    account_key TEXT,
                    plan TEXT,
                    limit_id TEXT,
                    reset_at_ms INTEGER,
                    started_at_ms INTEGER NOT NULL,
                    ended_at_ms INTEGER,
                    boundary_reason TEXT,
                    reset_reason TEXT NOT NULL DEFAULT 'uncertain_reset'
                );
                CREATE TABLE IF NOT EXISTS measurements (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    epoch_id INTEGER,
                    measured_at_ms INTEGER NOT NULL,
                    cost_delta_usd REAL,
                    quota_delta_points REAL,
                    event_count INTEGER,
                    status TEXT NOT NULL,
                    diagnostic_reason TEXT,
                    previous_observed_at_ms INTEGER,
                    credits_delta REAL,
                    percent_delta REAL,
                    credits_per_1_percent REAL,
                    estimated_weekly_credits REAL,
                    estimated_weekly_value_usd REAL,
                    FOREIGN KEY(epoch_id) REFERENCES epochs(id)
                );
                CREATE TABLE IF NOT EXISTS quotes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp_ms INTEGER NOT NULL,
                    value_usd REAL,
                    raw_value_usd REAL,
                    observed_cost_usd REAL,
                    weekly_used_percent REAL,
                    dominant_model TEXT,
                    confidence TEXT NOT NULL,
                    status TEXT NOT NULL,
                    is_finalized INTEGER NOT NULL DEFAULT 1,
                    algorithm_version TEXT NOT NULL,
                    estimated_weekly_credits REAL,
                    estimated_weekly_value_usd REAL,
                    credits_observed_this_window REAL,
                    percentage_coverage REAL,
                    valid_observation_count INTEGER NOT NULL DEFAULT 0,
                    window_id INTEGER,
                    window_start_ms INTEGER,
                    window_end_ms INTEGER,
                    reported_reset_at_ms INTEGER,
                    reset_reason TEXT,
                    credit_source TEXT
                );
                CREATE TABLE IF NOT EXISTS annotations (
                    id TEXT PRIMARY KEY,
                    timestamp_ms INTEGER NOT NULL,
                    label TEXT NOT NULL,
                    kind TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS chart_heartbeats (
                    timestamp_ms INTEGER PRIMARY KEY,
                    value_usd REAL,
                    weekly_used_percent REAL
                );
                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value_json TEXT NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS app_runs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    started_at_ms INTEGER NOT NULL,
                    ended_at_ms INTEGER,
                    version TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS diagnostics (
                    reason TEXT PRIMARY KEY,
                    count INTEGER NOT NULL DEFAULT 0,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_usage_events_credit_time
                    ON usage_events(account_key, credit_status, timestamp_ms);
                CREATE INDEX IF NOT EXISTS idx_usage_events_estimation
                    ON usage_events(account_key, timestamp_ms)
                    WHERE eligible=1 AND pricing_status IN ('official', 'custom');
                CREATE INDEX IF NOT EXISTS idx_quota_snapshots_account_limit_time
                    ON quota_snapshots(account_key, limit_id, observed_at_ms, id);
                CREATE INDEX IF NOT EXISTS idx_quota_snapshots_time
                    ON quota_snapshots(observed_at_ms, id);
                CREATE INDEX IF NOT EXISTS idx_quotes_algorithm_time
                    ON quotes(algorithm_version, timestamp_ms);
                INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms)
                    VALUES (1, strftime('%s','now') * 1000);
                COMMIT;",
            )
            .map_err(|_| "database schema migration failed".to_string())?;

        if previous_version < 5 {
            self.connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                    DELETE FROM pricing_snapshots;
                    DELETE FROM quotes;
                    DELETE FROM measurements;
                    DELETE FROM epochs;
                    DELETE FROM chart_heartbeats;
                    DELETE FROM diagnostics;
                    INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (2, strftime('%s','now') * 1000);
                    INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (3, strftime('%s','now') * 1000);
                    INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (4, strftime('%s','now') * 1000);
                    INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (5, strftime('%s','now') * 1000);
                    PRAGMA user_version=5;
                    COMMIT;",
                )
                .map_err(|_| "database live-data migration failed".to_string())?;
        }

        if previous_version < 6 {
            for (table, column, definition) in [
                ("usage_events", "credits", "REAL"),
                ("usage_events", "logged_charge_usd", "REAL"),
                (
                    "usage_events",
                    "credit_source",
                    "TEXT NOT NULL DEFAULT 'unavailable'",
                ),
                (
                    "usage_events",
                    "credit_status",
                    "TEXT NOT NULL DEFAULT 'pending'",
                ),
                ("usage_events", "quota_reset_at_ms", "INTEGER"),
                ("usage_events", "quota_limit_id", "TEXT"),
                (
                    "epochs",
                    "reset_reason",
                    "TEXT NOT NULL DEFAULT 'uncertain_reset'",
                ),
                ("measurements", "previous_observed_at_ms", "INTEGER"),
                ("measurements", "credits_delta", "REAL"),
                ("measurements", "percent_delta", "REAL"),
                ("measurements", "credits_per_1_percent", "REAL"),
                ("measurements", "estimated_weekly_credits", "REAL"),
                ("measurements", "estimated_weekly_value_usd", "REAL"),
                ("quotes", "estimated_weekly_credits", "REAL"),
                ("quotes", "estimated_weekly_value_usd", "REAL"),
                ("quotes", "credits_observed_this_window", "REAL"),
                ("quotes", "percentage_coverage", "REAL"),
                (
                    "quotes",
                    "valid_observation_count",
                    "INTEGER NOT NULL DEFAULT 0",
                ),
                ("quotes", "window_id", "INTEGER"),
                ("quotes", "window_start_ms", "INTEGER"),
                ("quotes", "window_end_ms", "INTEGER"),
                ("quotes", "reported_reset_at_ms", "INTEGER"),
                ("quotes", "reset_reason", "TEXT"),
                ("quotes", "credit_source", "TEXT"),
            ] {
                if !self.column_exists(table, column)? {
                    self.connection
                        .execute(
                            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                            [],
                        )
                        .map_err(|_| format!("unable to migrate {table}.{column}"))?;
                }
            }
            self.connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                    DELETE FROM pricing_snapshots;
                    DELETE FROM quotes;
                    DELETE FROM measurements;
                    DELETE FROM epochs;
                    DELETE FROM chart_heartbeats;
                    DELETE FROM diagnostics;
                    INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (6, strftime('%s','now') * 1000);
                    PRAGMA user_version=6;
                    COMMIT;",
                )
                .map_err(|error| format!("credit estimator migration failed: {error}"))?;
        }
        if previous_version < 7 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 DELETE FROM quotes;
                 DELETE FROM measurements;
                 DELETE FROM epochs;
                 DELETE FROM chart_heartbeats;
                 DELETE FROM diagnostics;
                 UPDATE usage_events SET cost_usd=NULL, pricing_status='pending', eligible=0;
                 INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (7, strftime('%s','now') * 1000);
                 PRAGMA user_version=7;
                 COMMIT;",
            ).map_err(|error| format!("token estimator migration failed: {error}"))?;
        }
        if previous_version < 8 {
            self.connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                     DELETE FROM quotes;
                     DELETE FROM measurements;
                     DELETE FROM epochs;
                     DELETE FROM chart_heartbeats;
                     INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms)
                         VALUES (8, strftime('%s','now') * 1000);
                     PRAGMA user_version=8;
                     COMMIT;",
                )
                .map_err(|error| format!("usage history correction migration failed: {error}"))?;
        }
        if self.load_settings().is_err() {
            self.save_settings(&AppSettings::default())?;
        }
        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool, String> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|_| "unable to inspect database schema".to_string())?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|_| "unable to inspect database schema".to_string())?;
        let exists = columns.filter_map(Result::ok).any(|name| name == column);
        Ok(exists)
    }

    fn restrict_file_permissions(&self) {
        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(&self.path) {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            let _ = fs::set_permissions(&self.path, permissions);
        }
    }

    fn restrict_directory_permissions(&self, directory: &Path) {
        #[cfg(unix)]
        if let Ok(metadata) = fs::metadata(directory) {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o700);
            let _ = fs::set_permissions(directory, permissions);
        }
    }

    fn record_app_run(&mut self) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO app_runs (started_at_ms, version) VALUES (?1, ?2)",
                params![now_ms(), env!("CARGO_PKG_VERSION")],
            )
            .map_err(|_| "unable to record app run".into())
            .map(|_| ())
    }

    pub fn load_settings(&self) -> Result<AppSettings, String> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT value_json FROM settings WHERE key='app'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "unable to read settings".to_string())?;
        value
            .map(|json| {
                serde_json::from_str(&json).map_err(|_| "stored settings are invalid".to_string())
            })
            .unwrap_or_else(|| Ok(AppSettings::default()))
    }

    pub fn save_settings(&mut self, settings: &AppSettings) -> Result<(), String> {
        settings.validate()?;
        let json = serde_json::to_string(settings)
            .map_err(|_| "unable to serialize settings".to_string())?;
        self.connection
            .execute(
                "INSERT INTO settings (key, value_json, updated_at_ms)
                 VALUES ('app', ?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,
                 updated_at_ms=excluded.updated_at_ms",
                params![json, now_ms()],
            )
            .map_err(|_| "unable to save settings".to_string())?;
        Ok(())
    }

    pub fn load_checkpoints(&self) -> Result<HashMap<String, u64>, String> {
        Ok(self
            .load_checkpoint_states()?
            .into_iter()
            .map(|(key, checkpoint)| (key, checkpoint.byte_offset))
            .collect())
    }

    pub fn load_checkpoint_states(&self) -> Result<HashMap<String, PersistedCheckpoint>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT source_key, byte_offset, parser_state_json FROM source_checkpoints")
            .map_err(|_| "unable to read source checkpoints".to_string())?;
        let rows = statement
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let offset: i64 = row.get(1)?;
                Ok((
                    key,
                    PersistedCheckpoint {
                        byte_offset: offset.max(0) as u64,
                        parser_state_json: row.get(2)?,
                    },
                ))
            })
            .map_err(|_| "unable to read source checkpoints".to_string())?;
        rows.map(|row| row.map_err(|_| "unable to decode source checkpoint".to_string()))
            .collect()
    }

    // harn:assume persisted-incremental-indexing ref=incremental-persistence scope=function
    pub fn persist_collection<P>(
        &mut self,
        collection: &CollectionSummary,
        account_key: Option<&str>,
        _unused_pricing_snapshot: Option<&P>,
    ) -> Result<usize, String> {
        let settings = self.load_settings()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| "unable to start collection transaction".to_string())?;
        for checkpoint in &collection.checkpoints {
            transaction
                .execute(
                    "INSERT INTO source_checkpoints (
                        source_key, byte_offset, parser_state_json, source_active, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(source_key) DO UPDATE SET
                        byte_offset=excluded.byte_offset,
                        parser_state_json=excluded.parser_state_json,
                        source_active=excluded.source_active,
                        updated_at_ms=excluded.updated_at_ms",
                    params![
                        checkpoint.source_key,
                        checkpoint.byte_offset as i64,
                        checkpoint.parser_state_json,
                        i64::from(checkpoint.source_active),
                        now_ms()
                    ],
                )
                .map_err(|_| "unable to persist source checkpoint".to_string())?;
        }
        let mut inserted = 0;
        let mut earliest_inserted_at_ms = None;
        for event in &collection.events {
            if Self::persist_event(&transaction, event, account_key, &settings)? {
                inserted += 1;
                earliest_inserted_at_ms = Some(
                    earliest_inserted_at_ms.map_or(event.timestamp_ms, |timestamp: i64| {
                        timestamp.min(event.timestamp_ms)
                    }),
                );
            }
        }
        if let Some(affected_from_ms) = earliest_inserted_at_ms {
            Self::rebuild_quotes_incrementally(&transaction, affected_from_ms)?;
        }
        if collection.stats.partial_line_retries > 0 {
            add_diagnostic(
                &transaction,
                "partial final line",
                collection.stats.partial_line_retries as i64,
            )?;
        }
        if !collection.interrupted_sources.is_empty() {
            add_diagnostic(
                &transaction,
                "monitoring gap",
                collection.interrupted_sources.len() as i64,
            )?;
        }
        transaction
            .commit()
            .map_err(|_| "unable to commit collection transaction".to_string())?;
        Ok(inserted)
    }

    fn persist_event(
        transaction: &Transaction<'_>,
        event: &UsageEvent,
        account_key: Option<&str>,
        settings: &AppSettings,
    ) -> Result<bool, String> {
        let pricing = event_cost(event, settings);
        let (cost_usd, pricing_status, eligible) = match pricing {
            Ok((cost, source)) => (Some(cost), source, 1_i64),
            Err(reason) => {
                add_diagnostic(transaction, &reason, 1)?;
                (None, "pending", 0_i64)
            }
        };
        let inserted = transaction
            .execute(
                "INSERT OR IGNORE INTO usage_events (
                    fingerprint, account_key, timestamp_ms, model_id, input_tokens,
                    cached_input_tokens, output_tokens, eligible, pricing_status, cost_usd,
                    quota_reset_at_ms, quota_limit_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12)",
                params![
                    event_fingerprint(event),
                    account_key,
                    event.timestamp_ms,
                    event.model.trim().to_ascii_lowercase().replace('/', "-"),
                    event.input_tokens as i64,
                    event.cached_input_tokens as i64,
                    event.output_tokens as i64,
                    eligible,
                    pricing_status,
                    cost_usd,
                    event.quota_reset_at_ms,
                    event.quota_limit_id,
                ],
            )
            .map_err(|_| "unable to persist usage event".to_string())?;
        if inserted == 1 {
            if let (Some(used_percent), Some(duration_minutes)) =
                (event.quota_used_percent, event.quota_window_minutes)
            {
                if used_percent.is_finite()
                    && (0.0..=100.0).contains(&used_percent)
                    && duration_minutes.is_finite()
                    && (duration_minutes - WEEKLY_WINDOW_MINUTES).abs()
                        <= WEEKLY_WINDOW_TOLERANCE_MINUTES
                {
                    transaction
                        .execute(
                            "INSERT INTO quota_snapshots (
                                account_key, observed_at_ms, reset_at_ms, duration_minutes,
                                limit_id, plan, used_percent, connection_quality
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'good')",
                            params![
                                account_key,
                                event.timestamp_ms,
                                event.quota_reset_at_ms,
                                duration_minutes,
                                event.quota_limit_id,
                                event.plan,
                                used_percent
                            ],
                        )
                        .map_err(|_| "unable to persist weekly observation".to_string())?;
                }
            }
        }
        Ok(inserted == 1)
    }

    pub fn rebuild_quotes(&mut self) -> Result<(), String> {
        let settings = self.load_settings()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| "unable to start token estimate rebuild".to_string())?;
        Self::reprice_usage_events(&transaction, &settings)?;
        Self::rebuild_quotes_in_transaction(&transaction)?;
        transaction
            .commit()
            .map_err(|_| "unable to commit token estimate rebuild".to_string())
    }

    fn reprice_usage_events(
        transaction: &Transaction<'_>,
        settings: &AppSettings,
    ) -> Result<(), String> {
        let rows = {
            let mut statement = transaction
                .prepare(
                    "SELECT fingerprint, model_id, input_tokens, cached_input_tokens, output_tokens
                     FROM usage_events",
                )
                .map_err(|_| "unable to read imported usage for repricing".to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok(StoredUsageForPricing {
                        fingerprint: row.get(0)?,
                        event: UsageEvent {
                            model: row.get(1)?,
                            input_tokens: row.get::<_, i64>(2)?.max(0) as u64,
                            cached_input_tokens: row.get::<_, i64>(3)?.max(0) as u64,
                            output_tokens: row.get::<_, i64>(4)?.max(0) as u64,
                            ..UsageEvent::default()
                        },
                    })
                })
                .map_err(|_| "unable to read imported usage for repricing".to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| "unable to decode imported usage for repricing".to_string())?;
            rows
        };
        for row in rows {
            match event_cost(&row.event, settings) {
                Ok((cost, source)) => {
                    transaction
                        .execute(
                            "UPDATE usage_events
                             SET eligible=1, pricing_status=?2, cost_usd=?3
                             WHERE fingerprint=?1",
                            params![row.fingerprint, source, cost],
                        )
                        .map_err(|_| "unable to update repriced usage".to_string())?;
                }
                Err(_) => {
                    transaction
                        .execute(
                            "UPDATE usage_events
                             SET eligible=0, pricing_status='pending', cost_usd=NULL
                             WHERE fingerprint=?1",
                            params![row.fingerprint],
                        )
                        .map_err(|_| "unable to update pending usage price".to_string())?;
                }
            }
        }
        Ok(())
    }

    fn rebuild_quotes_in_transaction(transaction: &Transaction<'_>) -> Result<(), String> {
        let observations = Self::weekly_observations(transaction)?;
        transaction
            .execute("DELETE FROM quotes", [])
            .map_err(|_| "unable to clear stale token estimates".to_string())?;
        transaction
            .execute("DELETE FROM measurements", [])
            .map_err(|_| "unable to clear stale token measurements".to_string())?;
        transaction
            .execute("DELETE FROM epochs", [])
            .map_err(|_| "unable to clear stale weekly windows".to_string())?;

        let (groups, stale_regressions) = Self::window_groups(observations);
        if stale_regressions > 0 {
            add_diagnostic(
                transaction,
                "stale pre-reset weekly usage regression",
                stale_regressions as i64,
            )?;
        }
        for group in groups {
            Self::persist_window_group(transaction, &group)?;
        }
        Ok(())
    }

    fn rebuild_quotes_incrementally(
        transaction: &Transaction<'_>,
        affected_from_ms: i64,
    ) -> Result<(), String> {
        let observations = Self::weekly_observations(transaction)?;
        let (groups, stale_regressions) = Self::window_groups(observations);
        if stale_regressions > 0 {
            add_diagnostic(
                transaction,
                "stale pre-reset weekly usage regression",
                stale_regressions as i64,
            )?;
        }

        let mut cutoffs = HashMap::<(Option<String>, Option<String>), i64>::new();
        for group in &groups {
            if group.ended_at_ms >= affected_from_ms {
                cutoffs
                    .entry((group.account_key.clone(), group.limit_id.clone()))
                    .or_insert(group.started_at_ms);
            }
        }
        for ((account_key, limit_id), cutoff_ms) in &cutoffs {
            transaction
                .execute(
                    "DELETE FROM quotes
                     WHERE window_id IN (
                        SELECT id FROM epochs
                        WHERE account_key IS ?1 AND limit_id IS ?2
                          AND started_at_ms >= ?3
                     )",
                    params![account_key, limit_id, cutoff_ms],
                )
                .map_err(|_| "unable to clear affected token estimates".to_string())?;
            transaction
                .execute(
                    "DELETE FROM measurements
                     WHERE epoch_id IN (
                        SELECT id FROM epochs
                        WHERE account_key IS ?1 AND limit_id IS ?2
                          AND started_at_ms >= ?3
                     )",
                    params![account_key, limit_id, cutoff_ms],
                )
                .map_err(|_| "unable to clear affected token measurements".to_string())?;
            transaction
                .execute(
                    "DELETE FROM epochs
                     WHERE account_key IS ?1 AND limit_id IS ?2
                       AND started_at_ms >= ?3",
                    params![account_key, limit_id, cutoff_ms],
                )
                .map_err(|_| "unable to clear affected weekly windows".to_string())?;
        }

        for group in groups {
            let key = (group.account_key.clone(), group.limit_id.clone());
            if cutoffs
                .get(&key)
                .is_some_and(|cutoff_ms| group.started_at_ms >= *cutoff_ms)
            {
                Self::persist_window_group(transaction, &group)?;
            }
        }
        Ok(())
    }

    fn persist_window_group(
        transaction: &Transaction<'_>,
        group: &WindowGroup,
    ) -> Result<(), String> {
        let window_id = transaction
            .query_row(
                "INSERT INTO epochs (
                    account_key, limit_id, reset_at_ms, started_at_ms, ended_at_ms,
                    boundary_reason, reset_reason
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 RETURNING id",
                params![
                    group.account_key,
                    group.limit_id,
                    group.reset_at_ms,
                    group.started_at_ms,
                    group.ended_at_ms,
                    group.reset_reason,
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| "unable to persist weekly window".to_string())?;
        Self::rebuild_group(transaction, group, window_id)
    }

    fn weekly_observations(transaction: &Transaction<'_>) -> Result<Vec<QuotaPoint>, String> {
        let mut statement = transaction
            .prepare(
                "SELECT account_key, limit_id, observed_at_ms, reset_at_ms, used_percent
                 FROM quota_snapshots
                 WHERE used_percent IS NOT NULL
                   AND used_percent BETWEEN 0.0 AND 100.0
                   AND duration_minutes IS NOT NULL
                   AND ABS(duration_minutes - 10080.0) <= 240.0
                 ORDER BY observed_at_ms, id",
            )
            .map_err(|_| "unable to read weekly observations".to_string())?;
        let observations = statement
            .query_map([], |row| {
                Ok(QuotaPoint {
                    account_key: row.get(0)?,
                    limit_id: row.get(1)?,
                    observed_at_ms: row.get(2)?,
                    reset_at_ms: row.get(3)?,
                    used_percent: row.get(4)?,
                })
            })
            .map_err(|_| "unable to read weekly observations".to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| "unable to decode weekly observations".to_string())?;
        Ok(observations)
    }

    // harn:assume jitter-safe-weekly-windows ref=window-reconstruction scope=function
    fn window_groups(observations: Vec<QuotaPoint>) -> (Vec<WindowGroup>, usize) {
        let mut groups_by_stream =
            HashMap::<(Option<String>, Option<String>), Vec<WindowGroup>>::new();
        let mut stale_regressions = 0;
        for current in observations {
            let groups = groups_by_stream
                .entry((current.account_key.clone(), current.limit_id.clone()))
                .or_default();
            let new_reason = groups.last().and_then(|group| {
                let previous = group.points.last()?;
                let reset_changed = match (group.reset_at_ms, current.reset_at_ms) {
                    (Some(left), Some(right)) => {
                        left.abs_diff(right) > RESET_TIMESTAMP_JITTER_MS as u64
                    }
                    _ => false,
                };
                if reset_changed {
                    return Some("reported_reset_changed");
                }
                if current.used_percent
                    < previous.used_percent - crate::estimator::MATERIAL_USAGE_DECREASE_PERCENT
                {
                    return previous
                        .reset_at_ms
                        .is_some_and(|reset| current.observed_at_ms >= reset)
                        .then_some("scheduled_reset");
                }
                None
            });
            if let Some(reason) = new_reason {
                groups.push(WindowGroup {
                    account_key: current.account_key.clone(),
                    limit_id: current.limit_id.clone(),
                    reset_at_ms: current.reset_at_ms,
                    started_at_ms: current.observed_at_ms,
                    ended_at_ms: current.observed_at_ms,
                    reset_reason: reason.into(),
                    points: vec![current],
                });
            } else if let Some(group) = groups.last_mut() {
                if group.points.last().is_some_and(|previous| {
                    current.used_percent
                        < previous.used_percent - crate::estimator::MATERIAL_USAGE_DECREASE_PERCENT
                }) {
                    stale_regressions += 1;
                    continue;
                }
                group.ended_at_ms = current.observed_at_ms;
                group.points.push(current);
            } else {
                groups.push(WindowGroup {
                    account_key: current.account_key.clone(),
                    limit_id: current.limit_id.clone(),
                    reset_at_ms: current.reset_at_ms,
                    started_at_ms: current.observed_at_ms,
                    ended_at_ms: current.observed_at_ms,
                    // The first observation is a baseline, not evidence that a
                    // scheduled reset happened at that instant.
                    reset_reason: "uncertain_reset".into(),
                    points: vec![current],
                });
            }
        }
        let mut groups = groups_by_stream.into_values().flatten().collect::<Vec<_>>();
        groups.sort_by_key(|group| group.started_at_ms);
        (groups, stale_regressions)
    }

    // harn:assume raw-history-stable-headline ref=history-signal-contract scope=function
    fn rebuild_group(
        transaction: &Transaction<'_>,
        group: &WindowGroup,
        window_id: i64,
    ) -> Result<(), String> {
        let first = group
            .points
            .first()
            .expect("window has a first observation");
        let mut previous = first.clone();
        let baseline_cost = Self::cost_through(
            transaction,
            group,
            group.started_at_ms,
            first.observed_at_ms,
        )?;
        let mut previous_cost = baseline_cost;
        let mut rates = Vec::new();
        for current in group.points.iter().skip(1) {
            let current_cost = Self::cost_through(
                transaction,
                group,
                group.started_at_ms,
                current.observed_at_ms,
            )?;
            let interval = crate::estimator::TokenInterval {
                previous_cost_usd: previous_cost,
                current_cost_usd: current_cost,
                previous_used_percent: previous.used_percent,
                current_used_percent: current.used_percent,
            };
            let decision = crate::estimator::measure_interval(interval);
            match decision {
                crate::estimator::MeasurementDecision::Valid {
                    cost_delta_usd,
                    percent_delta,
                    estimated_weekly_value_usd,
                } => {
                    let cumulative_estimate =
                        match crate::estimator::measure_interval(crate::estimator::TokenInterval {
                            previous_cost_usd: baseline_cost,
                            current_cost_usd: current_cost,
                            previous_used_percent: first.used_percent,
                            current_used_percent: current.used_percent,
                        }) {
                            crate::estimator::MeasurementDecision::Valid {
                                estimated_weekly_value_usd,
                                ..
                            } => estimated_weekly_value_usd,
                            _ => estimated_weekly_value_usd,
                        };
                    rates.push(cumulative_estimate);
                    let smoothed_value = crate::estimator::median_recent(
                        &rates,
                        crate::estimator::MEDIAN_SAMPLE_COUNT,
                    )
                    .expect("valid rate");
                    let coverage = (current.used_percent - first.used_percent).max(0.0);
                    let relative_deviation = crate::estimator::relative_median_deviation(
                        &rates,
                        crate::estimator::MEDIAN_SAMPLE_COUNT,
                    )
                    .unwrap_or(f64::INFINITY);
                    let confidence =
                        crate::estimator::confidence(rates.len(), coverage, relative_deviation);
                    let observed_cost = current_cost;
                    transaction
                        .execute(
                            "INSERT INTO measurements (
                                epoch_id, measured_at_ms, cost_delta_usd, quota_delta_points,
                                event_count, status, diagnostic_reason, previous_observed_at_ms,
                                percent_delta, estimated_weekly_value_usd
                             ) VALUES (?1, ?2, ?3, ?4, ?5, 'valid', NULL, ?6, ?4, ?7)",
                            params![
                                window_id,
                                current.observed_at_ms,
                                cost_delta_usd,
                                percent_delta,
                                Self::priced_event_count(
                                    transaction,
                                    group,
                                    previous.observed_at_ms,
                                    current.observed_at_ms
                                )?,
                                previous.observed_at_ms,
                                estimated_weekly_value_usd,
                            ],
                        )
                        .map_err(|_| "unable to persist token measurement".to_string())?;
                    transaction
                        .execute(
                            "INSERT INTO quotes (
                                timestamp_ms, value_usd, raw_value_usd, observed_cost_usd,
                                weekly_used_percent, dominant_model, confidence, status,
                                is_finalized, algorithm_version, estimated_weekly_value_usd,
                                percentage_coverage, valid_observation_count, window_id,
                                window_start_ms, window_end_ms, reported_reset_at_ms,
                                reset_reason, credit_source
                             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, 'valid', 1, ?7,
                                ?2, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                            params![
                                current.observed_at_ms,
                                smoothed_value,
                                cumulative_estimate,
                                observed_cost,
                                current.used_percent,
                                confidence,
                                ALGORITHM_VERSION,
                                coverage,
                                rates.len() as i64,
                                window_id,
                                group.started_at_ms,
                                group.ended_at_ms,
                                group.reset_at_ms,
                                group.reset_reason,
                                Self::model_status(
                                    transaction,
                                    group,
                                    group.started_at_ms,
                                    current.observed_at_ms
                                )?,
                            ],
                        )
                        .map_err(|_| "unable to persist weekly token estimate".to_string())?;
                }
                crate::estimator::MeasurementDecision::Pending(reason) => {
                    Self::persist_non_valid_measurement(
                        transaction,
                        window_id,
                        &previous,
                        current,
                        previous_cost,
                        current_cost,
                        group,
                        "pending",
                        &reason,
                    )?;
                }
                crate::estimator::MeasurementDecision::Rejected(reason) => {
                    Self::persist_non_valid_measurement(
                        transaction,
                        window_id,
                        &previous,
                        current,
                        previous_cost,
                        current_cost,
                        group,
                        "rejected",
                        &reason,
                    )?;
                }
            }
            previous = current.clone();
            previous_cost = current_cost;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_non_valid_measurement(
        transaction: &Transaction<'_>,
        window_id: i64,
        previous: &QuotaPoint,
        current: &QuotaPoint,
        previous_cost: f64,
        current_cost: f64,
        group: &WindowGroup,
        status: &str,
        reason: &str,
    ) -> Result<(), String> {
        transaction
            .execute(
                "INSERT INTO measurements (
                    epoch_id, measured_at_ms, cost_delta_usd, quota_delta_points,
                    event_count, status, diagnostic_reason, previous_observed_at_ms,
                    percent_delta
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?4)",
                params![
                    window_id,
                    current.observed_at_ms,
                    current_cost - previous_cost,
                    current.used_percent - previous.used_percent,
                    Self::priced_event_count(
                        transaction,
                        group,
                        previous.observed_at_ms,
                        current.observed_at_ms
                    )?,
                    status,
                    reason,
                    previous.observed_at_ms,
                ],
            )
            .map_err(|_| "unable to persist pending token measurement".to_string())?;
        Ok(())
    }

    fn cost_through(
        transaction: &Transaction<'_>,
        group: &WindowGroup,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<f64, String> {
        let value: f64 = transaction
            .query_row(
                "SELECT COALESCE(SUM(cost_usd), 0.0)
                 FROM usage_events
                 WHERE eligible=1 AND pricing_status IN ('official', 'custom')
                   AND timestamp_ms > ?1 AND timestamp_ms <= ?2
                   AND account_key IS ?3
                   AND (quota_limit_id IS ?4 OR quota_limit_id IS NULL)",
                params![start_ms, end_ms, group.account_key, group.limit_id],
                |row| row.get(0),
            )
            .map_err(|_| "unable to read token-derived costs".to_string())?;
        Ok(if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        })
    }

    fn priced_event_count(
        transaction: &Transaction<'_>,
        group: &WindowGroup,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<i64, String> {
        transaction
            .query_row(
                "SELECT COUNT(*) FROM usage_events
                 WHERE eligible=1 AND pricing_status IN ('official', 'custom')
                   AND timestamp_ms > ?1 AND timestamp_ms <= ?2
                   AND account_key IS ?3
                   AND (quota_limit_id IS ?4 OR quota_limit_id IS NULL)",
                params![start_ms, end_ms, group.account_key, group.limit_id],
                |row| row.get(0),
            )
            .map_err(|_| "unable to count priced token events".to_string())
    }

    fn model_status(
        transaction: &Transaction<'_>,
        group: &WindowGroup,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<String, String> {
        let mut statement = transaction
            .prepare(
                "SELECT DISTINCT pricing_status FROM usage_events
                 WHERE eligible=1 AND pricing_status IN ('official', 'custom')
                   AND timestamp_ms > ?1 AND timestamp_ms <= ?2
                   AND account_key IS ?3
                   AND (quota_limit_id IS ?4 OR quota_limit_id IS NULL)
                 ORDER BY pricing_status",
            )
            .map_err(|_| "unable to read pricing status".to_string())?;
        let sources = statement
            .query_map(
                params![start_ms, end_ms, group.account_key, group.limit_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| "unable to read pricing status".to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| "unable to decode pricing status".to_string())?;
        Ok(match sources.as_slice() {
            [] => "pending".into(),
            [source] => source.clone(),
            _ => "mixed".into(),
        })
    }

    fn stored_points(&self) -> Result<Vec<StoredPoint>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT timestamp_ms, estimated_weekly_value_usd, raw_value_usd, observed_cost_usd,
                        weekly_used_percent, reported_reset_at_ms, reset_reason,
                        is_finalized, window_id, confidence, percentage_coverage
                 FROM quotes
                 WHERE algorithm_version=?1 AND status='valid' AND is_finalized=1
                 ORDER BY timestamp_ms, id",
            )
            .map_err(|_| "unable to read token estimate history".to_string())?;
        let rows = statement
            .query_map(params![ALGORITHM_VERSION], |row| {
                let window_id: i64 = row.get(8)?;
                Ok(StoredPoint {
                    point: HistoryPoint {
                        timestamp: row.get(0)?,
                        estimated_weekly_value_usd: row.get(1)?,
                        raw_estimated_weekly_value_usd: row.get(2)?,
                        observed_cost_usd: row.get(3)?,
                        weekly_used_percent: row.get(4)?,
                        reset_at: row.get(5)?,
                        reset_reason: row.get(6)?,
                        is_finalized: row.get::<_, i64>(7)? != 0,
                        is_heartbeat: false,
                        epoch: Some(window_id),
                        confidence: match row.get::<_, String>(9)?.as_str() {
                            "high" => Confidence::High,
                            "medium" => Confidence::Medium,
                            "low" => Confidence::Low,
                            _ => Confidence::None,
                        },
                        percentage_coverage: row.get(10)?,
                    },
                    window_id: row.get(8)?,
                })
            })
            .map_err(|_| "unable to read token estimate history".to_string())?;
        rows.map(|row| row.map_err(|_| "unable to decode token estimate history".to_string()))
            .collect()
    }

    pub fn latest_quota_observation(&self) -> Result<Option<LatestQuotaObservation>, String> {
        self.connection
            .query_row(
                "SELECT account_key, limit_id, observed_at_ms, used_percent, reset_at_ms, plan
                 FROM quota_snapshots
                 WHERE used_percent IS NOT NULL
                 ORDER BY observed_at_ms DESC, id DESC LIMIT 1",
                [],
                |row| {
                    Ok(LatestQuotaObservation {
                        account_key: row.get(0)?,
                        limit_id: row.get(1)?,
                        observed_at_ms: row.get(2)?,
                        used_percent: row.get(3)?,
                        reset_at_ms: row.get(4)?,
                        plan: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|_| "unable to read current weekly usage".to_string())
    }

    fn active_window_id(&self, latest: &LatestQuotaObservation) -> Result<Option<i64>, String> {
        self.connection
            .query_row(
                "SELECT id FROM epochs
                 WHERE account_key IS ?1 AND limit_id IS ?2
                   AND started_at_ms <= ?3
                 ORDER BY started_at_ms DESC, id DESC LIMIT 1",
                params![latest.account_key, latest.limit_id, latest.observed_at_ms],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "unable to identify active weekly window".to_string())
    }

    fn active_window_metadata(&self, window_id: i64) -> Result<(i64, Option<i64>, String), String> {
        self.connection
            .query_row(
                "SELECT started_at_ms, reset_at_ms, reset_reason FROM epochs WHERE id=?1",
                params![window_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| "unable to read active weekly window".to_string())
    }

    pub fn latest_quote(&self) -> Result<Option<CurrentQuote>, String> {
        let latest = self.latest_quota_observation()?;
        let points = self.stored_points()?;
        let Some(latest) = latest else {
            return Ok(points.last().map(|stored| CurrentQuote {
                estimated_weekly_value_usd: stored.point.estimated_weekly_value_usd,
                change_value_usd: None,
                change_percent: None,
                observed_cost_usd: stored.point.observed_cost_usd,
                weekly_used_percent: stored.point.weekly_used_percent,
                reset_at: stored.point.reset_at,
                reset_reason: stored.point.reset_reason.clone(),
                status: QuoteStatus::Valid,
                algorithm_version: ALGORITHM_VERSION.into(),
                confidence: Confidence::Low,
                valid_observation_count: 1,
                percentage_coverage: None,
                pricing_source: None,
                model_status: None,
                note: Some("Estimated from local token usage and API prices.".into()),
            }));
        };
        let active_window_id = self.active_window_id(&latest)?;
        let current = active_window_id.and_then(|window_id| {
            points
                .iter()
                .rev()
                .find(|stored| {
                    stored.window_id == window_id && stored.point.timestamp <= latest.observed_at_ms
                })
                .cloned()
        });
        let (reset_reason, window_start, reset_at) = active_window_id
            .map(|window_id| self.active_window_metadata(window_id))
            .transpose()?
            .map(|(start, reset, reason)| (Some(reason), Some(start), reset))
            .unwrap_or((None, None, latest.reset_at_ms));
        let observed_cost = window_start
            .map(|start| self.sum_window_cost(start, latest.observed_at_ms, &latest))
            .transpose()?;
        let (valid_observation_count, percentage_coverage, pricing_source) = active_window_id
            .map(|window_id| self.window_summary(window_id))
            .transpose()?
            .unwrap_or((0, None, None));
        let previous = current.as_ref().and_then(|item| {
            points.iter().rev().find(|candidate| {
                candidate.point.timestamp <= item.point.timestamp - Range::W1.duration_ms()
            })
        });
        let change_value_usd = current
            .as_ref()
            .zip(previous)
            .and_then(|(current, previous)| {
                current
                    .point
                    .estimated_weekly_value_usd
                    .zip(previous.point.estimated_weekly_value_usd)
                    .map(|(current, previous)| current - previous)
            });
        let change_percent = current
            .as_ref()
            .zip(previous)
            .and_then(|(current, previous)| {
                current
                    .point
                    .estimated_weekly_value_usd
                    .zip(previous.point.estimated_weekly_value_usd)
                    .filter(|(_, previous)| *previous != 0.0)
                    .map(|(current, previous)| ((current - previous) / previous) * 100.0)
            });
        let (estimated_weekly_value_usd, confidence, status, note) = if let Some(current) =
            current.as_ref()
        {
            (
                current.point.estimated_weekly_value_usd,
                match self
                    .connection
                    .query_row(
                        "SELECT confidence FROM quotes WHERE timestamp_ms=?1 AND window_id=?2
                             ORDER BY id DESC LIMIT 1",
                        params![current.point.timestamp, current.window_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|_| "unable to read confidence".to_string())?
                    .as_deref()
                {
                    Some("high") => Confidence::High,
                    Some("medium") => Confidence::Medium,
                    Some("low") => Confidence::Low,
                    _ => Confidence::None,
                },
                QuoteStatus::Valid,
                Some(
                    "Rolling median of cumulative weekly cost-per-percent estimates; short intervals do not set the headline."
                        .into(),
                ),
            )
        } else {
            (
                None,
                Confidence::None,
                QuoteStatus::Pending,
                Some(
                    "Waiting for a positive weekly-usage change paired with local token cost."
                        .into(),
                ),
            )
        };
        Ok(Some(CurrentQuote {
            estimated_weekly_value_usd,
            change_value_usd,
            change_percent,
            observed_cost_usd: observed_cost,
            weekly_used_percent: Some(latest.used_percent),
            reset_at,
            reset_reason,
            status,
            algorithm_version: ALGORITHM_VERSION.into(),
            confidence,
            valid_observation_count,
            percentage_coverage,
            pricing_source: pricing_source.clone(),
            model_status: pricing_source,
            note,
        }))
    }

    fn sum_window_cost(
        &self,
        start_ms: i64,
        end_ms: i64,
        latest: &LatestQuotaObservation,
    ) -> Result<f64, String> {
        self.connection
            .query_row(
                "SELECT COALESCE(SUM(cost_usd), 0.0) FROM usage_events
                 WHERE eligible=1 AND pricing_status IN ('official', 'custom') AND timestamp_ms >= ?1 AND timestamp_ms <= ?2
                   AND account_key IS ?3
                   AND (quota_limit_id IS ?4 OR quota_limit_id IS NULL)",
                params![
                    start_ms,
                    end_ms,
                    latest.account_key,
                    latest.limit_id
                ],
                |row| row.get(0),
            )
            .map_err(|_| "unable to read observed token cost".to_string())
    }

    fn window_summary(&self, window_id: i64) -> Result<(u64, Option<f64>, Option<String>), String> {
        let (count, coverage): (i64, Option<f64>) = self
            .connection
            .query_row(
                "SELECT COUNT(*), MAX(estimated_weekly_value_usd * 0 + percent_delta)
                 FROM measurements WHERE epoch_id=?1 AND status='valid'",
                params![window_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "unable to read measurement summary".to_string())?;
        let source: Option<String> = self
            .connection
            .query_row(
                "SELECT CASE WHEN COUNT(DISTINCT pricing_status)=1 THEN MAX(pricing_status)
                             WHEN COUNT(DISTINCT pricing_status)>1 THEN 'mixed' END
                 FROM usage_events AS event
                 JOIN epochs AS window ON window.id=?1
                 WHERE event.eligible=1 AND event.pricing_status IN ('official', 'custom')
                   AND event.timestamp_ms >= window.started_at_ms
                   AND event.timestamp_ms <= COALESCE(window.ended_at_ms, window.started_at_ms)
                   AND event.account_key IS window.account_key
                   AND (event.quota_limit_id IS window.limit_id OR event.quota_limit_id IS NULL)",
                params![window_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "unable to read measurement source".to_string())?
            .flatten();
        let coverage = self
            .connection
            .query_row(
                "SELECT COALESCE(SUM(percent_delta), 0.0) FROM measurements
                 WHERE epoch_id=?1 AND status='valid'",
                params![window_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "unable to read percentage coverage".to_string())?
            .flatten()
            .or(coverage);
        Ok((count.max(0) as u64, coverage, source))
    }

    // harn:assume reliable-range-comparisons ref=range-comparison-selection scope=function
    pub fn history(&self, range: Range) -> Result<HistoryResponse, String> {
        let latest_timestamp: Option<i64> = self
            .connection
            .query_row(
                "SELECT observed_at_ms FROM quota_snapshots
                 ORDER BY observed_at_ms DESC, id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "unable to identify current weekly history".to_string())?;
        let Some(latest_timestamp) = latest_timestamp else {
            return Ok(empty_history(range));
        };
        let stored = self.stored_points()?;
        let since = latest_timestamp - range.duration_ms();
        let points = stored
            .iter()
            .filter(|stored| {
                stored.point.timestamp >= since && stored.point.timestamp <= latest_timestamp
            })
            .map(|stored| stored.point.clone())
            .collect::<Vec<_>>();
        let active_window = self
            .latest_quota_observation()?
            .and_then(|latest| self.active_window_id(&latest).ok().flatten());
        let current_point = active_window.and_then(|window_id| {
            stored.iter().rev().find(|stored| {
                stored.window_id == window_id
                    && matches!(
                        stored.point.confidence,
                        Confidence::Medium | Confidence::High
                    )
            })
        });
        let baseline_point = current_point.and_then(|current| {
            stored.iter().find(|candidate| {
                candidate.point.timestamp >= since
                    && candidate.point.timestamp < current.point.timestamp
                    && matches!(
                        candidate.point.confidence,
                        Confidence::Medium | Confidence::High
                    )
            })
        });
        let current = current_point.and_then(|stored| stored.point.estimated_weekly_value_usd);
        let baseline = baseline_point.and_then(|stored| stored.point.estimated_weekly_value_usd);
        let baseline_timestamp = baseline_point.map(|stored| stored.point.timestamp);
        let delta_value_usd = current
            .zip(baseline)
            .map(|(current, baseline)| current - baseline);
        let delta_percent = current
            .zip(baseline)
            .filter(|(_, baseline)| *baseline != 0.0)
            .map(|(current, baseline)| ((current - baseline) / baseline) * 100.0);
        Ok(HistoryResponse {
            statistics: RangeStatistics {
                range: range.clone(),
                baseline_estimated_weekly_value_usd: baseline,
                baseline_timestamp,
                current_estimated_weekly_value_usd: current,
                delta_value_usd,
                delta_percent,
                point_count: points.len(),
                partial: points.first().map_or(true, |point| point.timestamp > since),
            },
            bucket: range.bucket().into(),
            points,
        })
    }

    pub fn annotations(&self) -> Result<Vec<Annotation>, String> {
        let mut annotations = self
            .connection
            .prepare(
                "SELECT id, timestamp_ms, label, kind FROM annotations ORDER BY timestamp_ms ASC",
            )
            .map_err(|_| "unable to read annotations".to_string())?
            .query_map([], |row| {
                let kind: String = row.get(3)?;
                Ok(Annotation {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    label: row.get(2)?,
                    kind: match kind.as_str() {
                        "diagnostic" => AnnotationKind::Diagnostic,
                        "note" => AnnotationKind::Note,
                        _ => AnnotationKind::Reset,
                    },
                })
            })
            .map_err(|_| "unable to read annotations".to_string())?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        let mut statement = self
            .connection
            .prepare(
                "SELECT window.id, window.started_at_ms, window.reset_reason
                 FROM epochs AS window
                 WHERE EXISTS (
                    SELECT 1 FROM quotes
                    WHERE quotes.window_id=window.id
                      AND quotes.algorithm_version=?1
                      AND quotes.status='valid'
                      AND quotes.is_finalized=1
                 )
                 ORDER BY window.started_at_ms ASC",
            )
            .map_err(|_| "unable to read weekly windows".to_string())?;
        let resets = statement
            .query_map(params![ALGORITHM_VERSION], |row| {
                let id: i64 = row.get(0)?;
                let reason: String = row.get(2)?;
                Ok(Annotation {
                    id: format!("weekly-window-{id}"),
                    timestamp: row.get(1)?,
                    label: format!("Weekly window · {reason}"),
                    kind: AnnotationKind::Reset,
                })
            })
            .map_err(|_| "unable to read weekly windows".to_string())?;
        annotations.extend(resets.filter_map(Result::ok));
        annotations.sort_by_key(|annotation| annotation.timestamp);
        Ok(annotations)
    }

    pub fn reset_annotations(&mut self) -> Result<(), String> {
        self.connection
            .execute("DELETE FROM annotations", [])
            .map_err(|_| "unable to reset annotations".to_string())?;
        Ok(())
    }

    pub fn reset_all_data(&mut self) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| "unable to start data reset".to_string())?;
        transaction
            .execute_batch(
                "DELETE FROM source_checkpoints;
                 DELETE FROM usage_events;
                 DELETE FROM pricing_snapshots;
                 DELETE FROM quota_snapshots;
                 DELETE FROM measurements;
                 DELETE FROM epochs;
                 DELETE FROM quotes;
                 DELETE FROM chart_heartbeats;
                 DELETE FROM annotations;
                 DELETE FROM diagnostics;
                 DELETE FROM accounts;
                 DELETE FROM app_runs;",
            )
            .map_err(|_| "unable to reset local data".to_string())?;
        transaction
            .commit()
            .map_err(|_| "unable to commit data reset".to_string())
    }

    pub fn diagnostics(&self) -> Result<DiagnosticsSummary, String> {
        let total_events: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))
            .unwrap_or(0);
        let priced_events: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE eligible=1 AND pricing_status IN ('official', 'custom')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let pending_events: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE pricing_status='pending'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let rejected_events: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE pricing_status='rejected'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let unattributed_events: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE account_key IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let mut reasons_statement = self
            .connection
            .prepare("SELECT reason, count FROM diagnostics ORDER BY count DESC LIMIT 12")
            .map_err(|_| "unable to read diagnostics".to_string())?;
        let reasons = reasons_statement
            .query_map([], |row| {
                Ok(DiagnosticReason {
                    reason: row.get(0)?,
                    count: row.get(1)?,
                })
            })
            .map_err(|_| "unable to read diagnostics".to_string())?
            .filter_map(Result::ok)
            .collect();
        let mut model_statement = self
            .connection
            .prepare("SELECT DISTINCT model_id FROM usage_events ORDER BY model_id")
            .map_err(|_| "unable to read model IDs".to_string())?;
        let model_ids = model_statement
            .query_map([], |row| row.get(0))
            .map_err(|_| "unable to read model IDs".to_string())?
            .filter_map(Result::ok)
            .collect();
        Ok(DiagnosticsSummary {
            total_events,
            priced_events,
            pending_events,
            rejected_events,
            unattributed_events,
            partial_line_retries: self.diagnostic_count("partial final line"),
            monitoring_gaps: self.diagnostic_count("monitoring gap"),
            hidden_resets: self.diagnostic_count("hidden reset"),
            reasons,
            model_ids,
            privacy:
                "Prompts, account identifiers, and full local paths are never stored or returned."
                    .into(),
        })
    }

    fn diagnostic_count(&self, reason: &str) -> i64 {
        self.connection
            .query_row(
                "SELECT count FROM diagnostics WHERE reason=?1",
                params![reason],
                |row| row.get(0),
            )
            .unwrap_or(0)
    }
}

fn add_diagnostic(
    transaction: &Transaction<'_>,
    reason: &str,
    increment: i64,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO diagnostics (reason, count, updated_at_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(reason) DO UPDATE SET count=count+excluded.count,
             updated_at_ms=excluded.updated_at_ms",
            params![reason, increment, now_ms()],
        )
        .map_err(|_| "unable to persist diagnostic".to_string())?;
    Ok(())
}

pub fn hash_account_key(salt: &[u8], identity: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut normalized = identity.trim().to_ascii_lowercase();
    normalized.retain(|character| !character.is_whitespace());
    let mut digest = Sha256::new();
    digest.update(salt);
    digest.update(normalized.as_bytes());
    format!("acct_{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::CollectionSummary;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DATABASE_ID: AtomicU64 = AtomicU64::new(0);

    fn database() -> (Database, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "nerfify-token-test-{}-{}.db",
            std::process::id(),
            TEST_DATABASE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut database = Database {
            path: path.clone(),
            connection: open_connection(&path).expect("temporary database"),
        };
        database.migrate().expect("schema");
        (database, path)
    }

    fn event(
        timestamp_ms: i64,
        token_millions: Option<f64>,
        used_percent: f64,
        reset_at_ms: Option<i64>,
    ) -> UsageEvent {
        UsageEvent {
            timestamp_ms,
            model: "gpt-5.2-codex".into(),
            input_tokens: (token_millions.unwrap_or(0.0) * 1_000_000.0) as u64,
            output_tokens: 0,
            quota_used_percent: Some(used_percent),
            quota_reset_at_ms: reset_at_ms,
            quota_window_minutes: Some(WEEKLY_WINDOW_MINUTES),
            quota_limit_id: Some("codex".into()),
            authenticated_official_codex: true,
            ..UsageEvent::default()
        }
    }

    fn persist(database: &mut Database, events: Vec<UsageEvent>) {
        persist_for_account(database, events, None);
    }

    fn persist_for_account(
        database: &mut Database,
        events: Vec<UsageEvent>,
        account_key: Option<&str>,
    ) {
        database
            .persist_collection::<()>(
                &CollectionSummary {
                    events,
                    ..CollectionSummary::default()
                },
                account_key,
                None,
            )
            .expect("persist collection");
    }

    #[test]
    fn account_key_is_salted_and_non_reversible() {
        let first = hash_account_key(b"salt-a", " account-primary ");
        let second = hash_account_key(b"salt-b", "account-primary");
        assert_ne!(first, second);
        assert!(!first.contains("account-primary"));
    }

    #[test]
    fn cost_prices_cached_input_and_reasoning_output_once() {
        let event = UsageEvent {
            model: "gpt-5.2-codex".into(),
            input_tokens: 100,
            cached_input_tokens: 20,
            output_tokens: 8,
            reasoning_tokens: 3,
            ..UsageEvent::default()
        };
        let (cost, source) = event_cost(&event, &AppSettings::default()).expect("official price");
        // 80 normal input + 20 cached input + 8 output; reasoning is a subset of output.
        assert_eq!(source, "official");
        assert!((cost - ((80.0 * 1.75 + 20.0 * 0.175 + 8.0 * 14.0) / 1_000_000.0)).abs() < 1e-15);
    }

    #[test]
    fn token_interval_persists_full_week_estimate() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 42.0, Some(10_000)),
                event(2_000, Some(0.42), 43.0, Some(10_000)),
            ],
        );
        let quote = database
            .latest_quote()
            .expect("quote")
            .expect("current quote");
        assert_eq!(quote.estimated_weekly_value_usd, Some(73.5));
        assert_eq!(quote.observed_cost_usd, Some(0.735));
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn empty_incremental_scan_keeps_existing_estimates() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 42.0, Some(10_000)),
                event(2_000, Some(0.42), 43.0, Some(10_000)),
            ],
        );
        let quote_id: i64 = database
            .connection
            .query_row("SELECT MAX(id) FROM quotes", [], |row| row.get(0))
            .expect("quote id");

        persist(&mut database, Vec::new());

        let quote_id_after_refresh: i64 = database
            .connection
            .query_row("SELECT MAX(id) FROM quotes", [], |row| row.get(0))
            .expect("quote id after refresh");
        assert_eq!(quote_id_after_refresh, quote_id);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn incremental_scan_preserves_completed_window_rows() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 42.0, Some(10_000)),
                event(2_000, Some(0.42), 43.0, Some(10_000)),
                event(3_000, Some(0.0), 10.0, Some(20_000)),
                event(4_000, Some(0.2), 11.0, Some(20_000)),
            ],
        );
        let completed_window_id: i64 = database
            .connection
            .query_row(
                "SELECT id FROM epochs ORDER BY started_at_ms LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("completed window");
        let completed_quote_id: i64 = database
            .connection
            .query_row("SELECT id FROM quotes WHERE timestamp_ms=2000", [], |row| {
                row.get(0)
            })
            .expect("completed quote");

        persist(
            &mut database,
            vec![event(5_000, Some(0.3), 12.0, Some(20_000))],
        );

        let completed_window_id_after_refresh: i64 = database
            .connection
            .query_row(
                "SELECT id FROM epochs ORDER BY started_at_ms LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("completed window after refresh");
        let completed_quote_id_after_refresh: i64 = database
            .connection
            .query_row("SELECT id FROM quotes WHERE timestamp_ms=2000", [], |row| {
                row.get(0)
            })
            .expect("completed quote after refresh");
        assert_eq!(completed_window_id_after_refresh, completed_window_id);
        assert_eq!(completed_quote_id_after_refresh, completed_quote_id);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn estimate_reads_use_the_filtered_time_index() {
        let (database, path) = database();
        let plan: String = database
            .connection
            .query_row(
                "EXPLAIN QUERY PLAN
                 SELECT COALESCE(SUM(cost_usd), 0.0)
                 FROM usage_events
                 WHERE eligible=1 AND pricing_status IN ('official', 'custom')
                   AND timestamp_ms > ?1 AND timestamp_ms <= ?2
                   AND account_key IS ?3
                   AND (quota_limit_id IS ?4 OR quota_limit_id IS NULL)",
                params![
                    0_i64,
                    i64::MAX,
                    Option::<String>::None,
                    Option::<String>::None
                ],
                |row| row.get(3),
            )
            .expect("estimate query plan");
        assert!(plan.contains("idx_usage_events_estimation"), "{plan}");
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn zero_token_cost_leaves_current_estimate_pending() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, None, 42.0, Some(10_000)),
                event(2_000, None, 43.0, Some(10_000)),
            ],
        );
        let quote = database
            .latest_quote()
            .expect("quote")
            .expect("pending quote");
        assert_eq!(quote.status, QuoteStatus::Pending);
        assert!(quote.estimated_weekly_value_usd.is_none());
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rebuild_reprices_known_models_imported_before_rates_existed() {
        let (mut database, path) = database();
        let mut baseline = event(1_000, Some(0.0), 10.0, Some(10_000));
        baseline.model = "gpt-5.6-luna".into();
        let mut usage = event(2_000, Some(1.0), 11.0, Some(10_000));
        usage.model = "gpt-5.6-luna".into();
        persist(&mut database, vec![baseline, usage]);
        database
            .connection
            .execute(
                "UPDATE usage_events
                 SET eligible=0, pricing_status='not_applicable', cost_usd=NULL
                 WHERE model_id='gpt-5.6-luna'",
                [],
            )
            .expect("simulate stale import");

        database.rebuild_quotes().expect("reprice and rebuild");

        let priced: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM usage_events
                 WHERE model_id='gpt-5.6-luna' AND eligible=1 AND pricing_status='official'",
                [],
                |row| row.get(0),
            )
            .expect("priced events");
        assert_eq!(priced, 2);
        let quote = database
            .latest_quote()
            .expect("quote")
            .expect("current quote");
        assert_eq!(quote.estimated_weekly_value_usd, Some(200.0));
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reset_timestamp_jitter_stays_in_one_window() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 10.0, Some(10_000)),
                event(2_000, Some(0.4), 11.0, Some(10_000)),
                event(3_000, Some(0.5), 12.0, Some(20_000)),
            ],
        );
        let windows: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM epochs", [], |row| row.get(0))
            .expect("windows");
        assert_eq!(windows, 1);
        let measurements: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM measurements WHERE status='valid'",
                [],
                |row| row.get(0),
            )
            .expect("measurements");
        assert_eq!(measurements, 2);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn material_reset_timestamp_change_starts_a_new_window() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 10.0, Some(1_000_000)),
                event(2_000, Some(0.4), 11.0, Some(1_000_000)),
                event(
                    3_000,
                    Some(0.5),
                    12.0,
                    Some(1_000_000 + RESET_TIMESTAMP_JITTER_MS + 1),
                ),
            ],
        );
        let windows: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM epochs", [], |row| row.get(0))
            .expect("windows");
        assert_eq!(windows, 2);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn pre_reset_usage_regression_is_ignored_and_diagnosed() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 10.0, Some(1_000_000)),
                event(2_000, Some(0.1), 11.0, Some(1_000_000)),
                event(3_000, Some(0.1), 10.0, Some(1_000_000)),
                event(4_000, Some(0.1), 12.0, Some(1_000_000)),
            ],
        );
        let windows: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM epochs", [], |row| row.get(0))
            .expect("windows");
        let valid: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM measurements WHERE status='valid'",
                [],
                |row| row.get(0),
            )
            .expect("valid measurements");
        assert_eq!(windows, 1);
        assert_eq!(valid, 2);
        assert_eq!(
            database.diagnostic_count("stale pre-reset weekly usage regression"),
            1
        );
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn jittered_reset_events_contribute_to_one_raw_estimate() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 10.0, Some(1_000_000)),
                event(2_000, Some(0.1), 10.0, Some(1_010_000)),
                event(3_000, Some(0.3), 11.0, Some(1_000_000)),
            ],
        );
        let raw: f64 = database
            .connection
            .query_row(
                "SELECT raw_value_usd FROM quotes ORDER BY timestamp_ms DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("raw estimate");
        assert!((raw - 70.0).abs() < 1e-10);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn history_uses_reliable_in_range_baseline_across_windows() {
        let (database, path) = database();
        database
            .connection
            .execute(
                "INSERT INTO quota_snapshots (
                    observed_at_ms, reset_at_ms, duration_minutes, limit_id, used_percent
                 ) VALUES (10000, 20000, 10080, 'codex', 20)",
                [],
            )
            .expect("latest quota");
        database
            .connection
            .execute_batch(
                "INSERT INTO epochs (
                    id, limit_id, reset_at_ms, started_at_ms, ended_at_ms, reset_reason
                 ) VALUES
                    (1, 'codex', 9000, 0, 4000, 'uncertain_reset'),
                    (2, 'codex', 20000, 5000, 10000, 'reported_reset_changed');",
            )
            .expect("epochs");
        for (timestamp, value, confidence, coverage, window_id) in [
            (1_000, 3.28, "low", 1.0, 1),
            (2_000, 50.0, "medium", 5.0, 1),
            (10_000, 60.0, "high", 20.0, 2),
        ] {
            database
                .connection
                .execute(
                    "INSERT INTO quotes (
                        timestamp_ms, value_usd, raw_value_usd, estimated_weekly_value_usd,
                        confidence, status, is_finalized, algorithm_version,
                        percentage_coverage, window_id
                     ) VALUES (?1, ?2, ?2, ?2, ?3, 'valid', 1, ?4, ?5, ?6)",
                    params![
                        timestamp,
                        value,
                        confidence,
                        ALGORITHM_VERSION,
                        coverage,
                        window_id
                    ],
                )
                .expect("quote");
        }

        let history = database.history(Range::D1).expect("history");
        assert_eq!(history.statistics.baseline_timestamp, Some(2_000));
        assert_eq!(
            history.statistics.baseline_estimated_weekly_value_usd,
            Some(50.0)
        );
        assert_eq!(history.statistics.delta_value_usd, Some(10.0));
        assert_eq!(history.points[0].raw_estimated_weekly_value_usd, Some(3.28));
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn history_does_not_reuse_stale_baseline_outside_range() {
        let (database, path) = database();
        let current_timestamp = Range::D1.duration_ms() + 10_000;
        database
            .connection
            .execute(
                "INSERT INTO quota_snapshots (
                    observed_at_ms, reset_at_ms, duration_minutes, limit_id, used_percent
                 ) VALUES (?1, ?2, 10080, 'codex', 20)",
                params![
                    current_timestamp,
                    current_timestamp + Range::W1.duration_ms()
                ],
            )
            .expect("latest quota");
        database
            .connection
            .execute(
                "INSERT INTO epochs (
                    id, limit_id, reset_at_ms, started_at_ms, ended_at_ms, reset_reason
                 ) VALUES (1, 'codex', ?1, 0, ?2, 'uncertain_reset')",
                params![
                    current_timestamp + Range::W1.duration_ms(),
                    current_timestamp
                ],
            )
            .expect("epoch");
        for (timestamp, value) in [(1, 3.28), (current_timestamp, 60.0)] {
            database
                .connection
                .execute(
                    "INSERT INTO quotes (
                        timestamp_ms, value_usd, raw_value_usd, estimated_weekly_value_usd,
                        confidence, status, is_finalized, algorithm_version,
                        percentage_coverage, window_id
                     ) VALUES (?1, ?2, ?2, ?2, 'high', 'valid', 1, ?3, 20, 1)",
                    params![timestamp, value, ALGORITHM_VERSION],
                )
                .expect("quote");
        }

        let history = database.history(Range::D1).expect("history");
        assert_eq!(history.statistics.baseline_timestamp, None);
        assert_eq!(history.statistics.delta_value_usd, None);
        assert_eq!(history.statistics.delta_percent, None);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn scheduled_reset_and_usage_decrease_are_recorded() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 80.0, Some(2_000)),
                event(2_000, Some(0.1), 10.0, Some(2_000)),
            ],
        );
        let reason: String = database
            .connection
            .query_row(
                "SELECT reset_reason FROM epochs ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("reason");
        assert_eq!(reason, "scheduled_reset");
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn restart_rebuilds_the_same_active_window_and_measurement() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 42.0, Some(10_000)),
                event(2_000, Some(0.42), 43.0, Some(10_000)),
            ],
        );
        let before = database.latest_quote().expect("before").expect("quote");
        drop(database);
        let mut reopened = Database {
            path: path.clone(),
            connection: open_connection(&path).expect("reopen database"),
        };
        reopened.migrate().expect("reopen schema");
        reopened.rebuild_quotes().expect("rebuild");
        let after = reopened.latest_quote().expect("after").expect("quote");
        assert_eq!(
            before.estimated_weekly_value_usd,
            after.estimated_weekly_value_usd
        );
        let valid: i64 = reopened
            .connection
            .query_row(
                "SELECT COUNT(*) FROM measurements WHERE status='valid'",
                [],
                |row| row.get(0),
            )
            .expect("valid measurements");
        assert_eq!(valid, 1);
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn late_out_of_order_observation_stays_in_its_timestamp_window() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(3_000, Some(0.5), 5.0, Some(20_000)),
                event(1_000, Some(0.0), 10.0, Some(10_000)),
                event(2_000, Some(0.4), 11.0, Some(10_000)),
            ],
        );
        let valid: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM measurements WHERE status='valid'",
                [],
                |row| row.get(0),
            )
            .expect("valid measurements");
        assert_eq!(valid, 1);
        let delta: f64 = database
            .connection
            .query_row(
                "SELECT cost_delta_usd FROM measurements WHERE status='valid'",
                [],
                |row| row.get(0),
            )
            .expect("cost delta");
        assert_eq!(delta, 0.7);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn interleaved_account_observations_keep_each_account_window_intact() {
        let (mut database, path) = database();
        for account_key in ["account-a", "account-b"] {
            database
                .connection
                .execute(
                    "INSERT INTO accounts (account_key, created_at_ms, last_seen_at_ms)
                     VALUES (?1, 0, 0)",
                    params![account_key],
                )
                .expect("account");
        }
        persist_for_account(
            &mut database,
            vec![event(1_000, Some(0.0), 10.0, Some(10_000))],
            Some("account-a"),
        );
        persist_for_account(
            &mut database,
            vec![event(2_000, Some(0.1), 50.0, Some(10_000))],
            Some("account-b"),
        );
        persist_for_account(
            &mut database,
            vec![event(3_000, Some(0.4), 11.0, Some(10_000))],
            Some("account-a"),
        );
        let valid: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM measurements WHERE status='valid'",
                [],
                |row| row.get(0),
            )
            .expect("valid measurements");
        assert_eq!(valid, 1);
        let windows: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM epochs", [], |row| row.get(0))
            .expect("windows");
        assert_eq!(windows, 2);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn credit_migration_preserves_raw_events_but_invalidates_old_derived_rows() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 42.0, Some(10_000)),
                event(2_000, Some(0.42), 43.0, Some(10_000)),
            ],
        );
        database
            .connection
            .execute(
                "INSERT INTO quotes (timestamp_ms, value_usd, confidence, status, algorithm_version)
                 VALUES (1, 1, 'high', 'valid', 'legacy')",
                [],
            )
            .expect("legacy derived row");
        database
            .connection
            .pragma_update(None, "user_version", 5)
            .expect("legacy version");
        database.migrate().expect("credit migration");
        let events: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))
            .expect("raw events");
        let quotes: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM quotes", [], |row| row.get(0))
            .expect("quotes");
        assert_eq!(events, 2);
        assert_eq!(quotes, 0);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn history_v8_migration_preserves_raw_data_settings_and_user_annotations() {
        let (mut database, path) = database();
        persist(
            &mut database,
            vec![
                event(1_000, Some(0.0), 10.0, Some(100_000)),
                event(2_000, Some(0.4), 11.0, Some(100_000)),
            ],
        );
        database
            .connection
            .execute(
                "INSERT INTO annotations (id, timestamp_ms, label, kind)
                 VALUES ('user-note', 1500, 'Keep me', 'note')",
                [],
            )
            .expect("user annotation");
        database
            .save_settings(&AppSettings::default())
            .expect("settings");
        database
            .connection
            .pragma_update(None, "user_version", 7)
            .expect("simulate v7");

        database.migrate().expect("history correction migration");

        for (table, expected) in [
            ("usage_events", 2_i64),
            ("quota_snapshots", 2),
            ("settings", 1),
            ("annotations", 1),
            ("quotes", 0),
            ("measurements", 0),
            ("epochs", 0),
        ] {
            let count: i64 = database
                .connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("row count");
            assert_eq!(count, expected, "unexpected {table} count");
        }
        drop(database);
        let _ = fs::remove_file(path);
    }
}
