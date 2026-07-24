use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::collector::{CollectionSummary, PersistedCheckpoint};
use crate::models::{
    AdvancedSettings, Annotation, AnnotationKind, AppSettings, Confidence, CurrentQuote,
    DiagnosticReason, DiagnosticsSummary, HistoryPoint, HistoryResponse, QuoteStatus, Range,
    RangeStatistics, ALGORITHM_VERSION,
};
use crate::parser::{event_fingerprint, UsageEvent};
use crate::pricing::{
    classify_provider_eligibility, normalize_model_id, price_event, Eligibility, PricingSnapshot,
};

pub struct Database {
    pub path: PathBuf,
    connection: Connection,
}

pub struct LatestQuotaObservation {
    pub used_percent: f64,
    pub reset_at_ms: Option<i64>,
    pub plan: Option<String>,
}

#[derive(Clone)]
struct QuotaPoint {
    account_key: Option<String>,
    limit_id: Option<String>,
    observed_at_ms: i64,
    reset_at_ms: i64,
    used_percent: f64,
}

struct WeightedQuote {
    point: HistoryPoint,
    epoch: i64,
    observed_cost_usd: f64,
}

type EpochKey = (Option<String>, Option<String>, i64);

#[derive(Default)]
struct EpochAggregate {
    cost_usd: f64,
    quota_points: f64,
    model: Option<String>,
    mixed: bool,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn comparison_baseline(quotes: &[WeightedQuote], cutoff_ms: i64, tolerance_ms: i64) -> Option<f64> {
    let candidate = quotes
        .iter()
        .rev()
        .find(|quote| quote.point.timestamp <= cutoff_ms)?;
    (cutoff_ms - candidate.point.timestamp <= tolerance_ms)
        .then_some(candidate.point.value_usd)
        .flatten()
}

fn graph_points(
    quotes: &[WeightedQuote],
    since_ms: i64,
    current_epoch: Option<i64>,
    keep_current_detail: bool,
) -> Vec<HistoryPoint> {
    let mut completed = HashMap::new();
    let mut current = Vec::new();
    for quote in quotes
        .iter()
        .filter(|quote| quote.point.timestamp >= since_ms)
    {
        if keep_current_detail && Some(quote.epoch) == current_epoch {
            current.push(quote.point.clone());
        } else {
            completed.insert(quote.epoch, quote.point.clone());
        }
    }
    let mut points = completed.into_values().chain(current).collect::<Vec<_>>();
    points.sort_by_key(|point| point.timestamp);
    for point in &mut points {
        point.epoch = None;
    }
    points
}

fn empty_history(range: Range) -> HistoryResponse {
    HistoryResponse {
        bucket: range.bucket().into(),
        statistics: RangeStatistics {
            range,
            baseline_value_usd: None,
            current_value_usd: None,
            delta_usd: None,
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
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/Nerfify");
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
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/Nerfify");
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
    for suffix in ["", "-wal", "-shm"] {
        let source = if suffix.is_empty() {
            path.to_path_buf()
        } else {
            PathBuf::from(format!("{}{}", path.display(), suffix))
        };
        if !source.exists() {
            continue;
        }
        let target = if suffix.is_empty() {
            recovery.clone()
        } else {
            PathBuf::from(format!("{}{}", recovery.display(), suffix))
        };
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
        database.rebuild_quotes()?;
        database.record_app_run()?;
        Ok(database)
    }

    fn migrate(&mut self) -> Result<(), String> {
        let previous_version = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap_or_default();
        if self
            .connection
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
                pricing_status TEXT NOT NULL,
                cost_usd REAL,
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
                boundary_reason TEXT
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
                algorithm_version TEXT NOT NULL
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
            CREATE INDEX IF NOT EXISTS idx_usage_events_account_pricing_time
                ON usage_events(account_key, pricing_status, timestamp_ms);
            CREATE INDEX IF NOT EXISTS idx_quota_snapshots_account_limit_time
                ON quota_snapshots(account_key, limit_id, observed_at_ms, id);
            CREATE INDEX IF NOT EXISTS idx_quota_snapshots_time
                ON quota_snapshots(observed_at_ms, id);
            CREATE INDEX IF NOT EXISTS idx_quotes_algorithm_time
                ON quotes(algorithm_version, timestamp_ms);
            INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (1, strftime('%s','now') * 1000);
            COMMIT;",
            )
            .is_err()
        {
            let _ = self.connection.execute_batch("ROLLBACK;");
            return Err("database migration failed".into());
        }
        if previous_version < 5 {
            self.connection
                .execute_batch(
                    "BEGIN IMMEDIATE;
                    DELETE FROM source_checkpoints;
                    DELETE FROM usage_events;
                    DELETE FROM pricing_snapshots;
                    DELETE FROM quota_snapshots;
                    DELETE FROM epochs;
                    DELETE FROM measurements;
                    DELETE FROM quotes;
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
        if self.load_settings().is_err() {
            self.save_settings(&AppSettings::default())?;
        }
        Ok(())
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
        settings.advanced.validate()?;
        let json = serde_json::to_string(settings)
            .map_err(|_| "unable to serialize settings".to_string())?;
        self.connection.execute("INSERT INTO settings (key, value_json, updated_at_ms) VALUES ('app', ?1, ?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json, updated_at_ms=excluded.updated_at_ms", params![json, now_ms()]).map_err(|_| "unable to save settings".to_string())?;
        Ok(())
    }

    pub fn load_checkpoints(&self) -> Result<std::collections::HashMap<String, u64>, String> {
        Ok(self
            .load_checkpoint_states()?
            .into_iter()
            .map(|(key, checkpoint)| (key, checkpoint.byte_offset))
            .collect())
    }

    pub fn load_checkpoint_states(
        &self,
    ) -> Result<std::collections::HashMap<String, PersistedCheckpoint>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT source_key, byte_offset, parser_state_json FROM source_checkpoints")
            .map_err(|_| "unable to read source checkpoints".to_string())?;
        let rows = statement
            .query_map([], |row| {
                let key: String = row.get(0)?;
                let offset: i64 = row.get(1)?;
                let parser_state_json: String = row.get(2)?;
                Ok((
                    key,
                    PersistedCheckpoint {
                        byte_offset: offset.max(0) as u64,
                        parser_state_json,
                    },
                ))
            })
            .map_err(|_| "unable to read source checkpoints".to_string())?;
        rows.map(|row| row.map_err(|_| "unable to decode source checkpoint".to_string()))
            .collect()
    }

    pub fn persist_collection(
        &mut self,
        collection: &CollectionSummary,
        account_key: Option<&str>,
        pricing_snapshot: Option<&PricingSnapshot>,
    ) -> Result<usize, String> {
        let settings = self.load_settings()?.advanced;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| "unable to start collection transaction".to_string())?;
        if let Some(snapshot) = pricing_snapshot.filter(|_| !collection.events.is_empty()) {
            transaction
                .execute(
                    "INSERT INTO pricing_snapshots (source, observed_at_ms, version, etag, sha256, is_current) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                    params![snapshot.source, snapshot.observed_at_ms, snapshot.version, snapshot.etag, snapshot.sha256],
                )
                .map_err(|_| "unable to persist pricing snapshot".to_string())?;
        }
        for checkpoint in &collection.checkpoints {
            transaction
                .execute(
                    "INSERT INTO source_checkpoints (source_key, byte_offset, parser_state_json, source_active, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(source_key) DO UPDATE SET byte_offset=excluded.byte_offset, parser_state_json=excluded.parser_state_json, source_active=excluded.source_active, updated_at_ms=excluded.updated_at_ms",
                    params![checkpoint.source_key, checkpoint.byte_offset as i64, checkpoint.parser_state_json, i64::from(checkpoint.source_active), now_ms()],
                )
                .map_err(|_| "unable to persist source checkpoint".to_string())?;
        }
        let mut inserted = 0;
        for event in &collection.events {
            if Self::persist_event(&transaction, event, account_key, pricing_snapshot)? {
                inserted += 1;
            }
        }
        Self::rebuild_quotes_in_transaction(&transaction, &settings)?;
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
        transaction: &rusqlite::Transaction<'_>,
        event: &UsageEvent,
        account_key: Option<&str>,
        pricing_snapshot: Option<&PricingSnapshot>,
    ) -> Result<bool, String> {
        let eligibility = classify_provider_eligibility(event);
        let eligible = matches!(eligibility, Eligibility::Eligible);
        let (pricing_status, cost_usd): (&str, Option<f64>) = if !eligible {
            match eligibility {
                Eligibility::Rejected(_) => ("rejected", None),
                Eligibility::Eligible | Eligibility::Pending(_) => ("pending", None),
            }
        } else if let Some(snapshot) = pricing_snapshot {
            match price_event(event, snapshot) {
                Ok(cost) => ("priced", Some(cost)),
                Err(_) => ("pending", None),
            }
        } else {
            ("pending", None)
        };
        let inserted = transaction
            .execute(
                "INSERT OR IGNORE INTO usage_events (fingerprint, account_key, timestamp_ms, model_id, input_tokens, cached_input_tokens, output_tokens, eligible, pricing_status, cost_usd) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![event_fingerprint(event), account_key, event.timestamp_ms, normalize_model_id(&event.model), event.input_tokens as i64, event.cached_input_tokens as i64, event.output_tokens as i64, i64::from(eligible && cost_usd.is_some()), pricing_status, cost_usd],
            )
            .map_err(|_| "unable to persist usage event".to_string())?;
        if inserted == 1 {
            if let (Some(used_percent), Some(reset_at_ms), Some(duration_minutes)) = (
                event.quota_used_percent,
                event.quota_reset_at_ms,
                event.quota_window_minutes,
            ) {
                if used_percent.is_finite()
                    && (0.0..=100.0).contains(&used_percent)
                    && duration_minutes.is_finite()
                    && duration_minutes > 0.0
                {
                    transaction
                        .execute(
                            "INSERT INTO quota_snapshots (account_key, observed_at_ms, reset_at_ms, duration_minutes, limit_id, plan, used_percent, connection_quality) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'good')",
                            params![account_key, event.timestamp_ms, reset_at_ms, duration_minutes, event.quota_limit_id, event.plan, used_percent],
                        )
                        .map_err(|_| "unable to persist quota snapshot".to_string())?;
                }
            }
        }
        if inserted == 1 && pricing_status != "priced" {
            add_diagnostic(transaction, pricing_status, 1)?;
        }
        Ok(inserted == 1)
    }

    pub fn rebuild_quotes(&mut self) -> Result<(), String> {
        let settings = self.load_settings()?.advanced;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| "unable to start quote rebuild".to_string())?;
        Self::rebuild_quotes_in_transaction(&transaction, &settings)?;
        transaction
            .commit()
            .map_err(|_| "unable to commit quote rebuild".to_string())
    }

    fn rebuild_quotes_in_transaction(
        transaction: &rusqlite::Transaction<'_>,
        settings: &AdvancedSettings,
    ) -> Result<(), String> {
        let quota_points = {
            let mut statement = transaction
                .prepare(
                    "WITH observed AS (
                         SELECT quota.*,
                                LAG(used_percent) OVER (
                                    PARTITION BY account_key, limit_id
                                    ORDER BY observed_at_ms, id
                                ) AS previous_used_percent,
                                LAG(reset_at_ms) OVER (
                                    PARTITION BY account_key, limit_id
                                    ORDER BY observed_at_ms, id
                                ) AS previous_reset_at_ms
                         FROM quota_snapshots quota
                         WHERE used_percent > 0
                           AND reset_at_ms IS NOT NULL
                           AND ABS(duration_minutes - 10080.0) <= 240.0
                     )
                     SELECT account_key, limit_id, observed_at_ms, reset_at_ms, used_percent
                     FROM observed
                     WHERE previous_used_percent IS NULL
                        OR used_percent<>previous_used_percent
                        OR reset_at_ms<>previous_reset_at_ms
                     ORDER BY observed_at_ms",
                )
                .map_err(|_| "unable to read weekly quota observations".to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok(QuotaPoint {
                        account_key: row.get(0)?,
                        limit_id: row.get(1)?,
                        observed_at_ms: row.get(2)?,
                        reset_at_ms: row.get(3)?,
                        used_percent: row.get(4)?,
                    })
                })
                .map_err(|_| "unable to read weekly quota observations".to_string())?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| "unable to decode weekly quota observations".to_string())?
        };

        transaction
            .execute("DELETE FROM quotes", [])
            .map_err(|_| "unable to clear stale quotes".to_string())?;

        let thresholds = crate::estimator::Thresholds {
            refresh_seconds: settings.refresh_interval_seconds,
            monitoring_gap_minutes: settings.monitoring_gap_minutes,
            settlement_seconds: settings.settlement_window_seconds,
            hard_settlement_seconds: settings.settlement_window_seconds,
            minimum_decimal_quota_points: settings.minimum_quota_movement_points,
            minimum_whole_quota_points: settings.minimum_quota_movement_points,
            minimum_eligible_cost_usd: settings.minimum_eligible_cost_usd,
            minimum_events: settings.minimum_events,
            low_usage_quarantine_percent: settings.low_usage_quarantine_percent,
        };
        let rebuilt_at_ms = now_ms();
        let mut previous_by_epoch: HashMap<EpochKey, QuotaPoint> = HashMap::new();

        for current in quota_points {
            let epoch_key = (
                current.account_key.clone(),
                current.limit_id.clone(),
                (current.reset_at_ms + 30_000).div_euclid(60_000),
            );
            let Some(previous) = previous_by_epoch.get(&epoch_key).cloned() else {
                previous_by_epoch.insert(epoch_key, current);
                continue;
            };
            if previous.used_percent >= 100.0 || current.used_percent <= previous.used_percent {
                continue;
            }

            let (cost_delta_usd, event_count): (f64, i64) = transaction
                .query_row(
                    "SELECT COALESCE(SUM(cost_usd), 0), COUNT(*)
                     FROM usage_events
                     WHERE pricing_status='priced'
                       AND timestamp_ms > ?1
                       AND timestamp_ms <= ?2
                       AND account_key IS ?3",
                    params![
                        previous.observed_at_ms,
                        current.observed_at_ms,
                        current.account_key
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|_| "unable to measure interval cost".to_string())?;
            let quota_delta_points = current.used_percent - previous.used_percent;
            let decimal_quota =
                current.used_percent.fract() != 0.0 || previous.used_percent.fract() != 0.0;
            let decision = crate::estimator::settle_interval(
                crate::estimator::SettlementInput {
                    cost_delta_usd,
                    quota_delta_points,
                    events: event_count.max(0) as u64,
                    decimal_quota,
                    sources_unchanged_for_seconds: rebuilt_at_ms
                        .saturating_sub(current.observed_at_ms)
                        .max(0) as u64
                        / 1_000,
                    monotonic: true,
                    complete: true,
                    low_usage_percent: current.used_percent,
                },
                thresholds,
            );
            let crate::estimator::MeasurementDecision::Valid {
                quote_usd,
                confidence,
            } = decision
            else {
                continue;
            };
            previous_by_epoch.insert(epoch_key, current.clone());
            let dominant_model: Option<String> = transaction
                .query_row(
                    "SELECT model_id
                     FROM usage_events
                     WHERE pricing_status='priced'
                       AND timestamp_ms > ?1
                       AND timestamp_ms <= ?2
                       AND account_key IS ?3
                     GROUP BY model_id
                     ORDER BY SUM(cost_usd) DESC
                     LIMIT 1",
                    params![
                        previous.observed_at_ms,
                        current.observed_at_ms,
                        current.account_key
                    ],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| "unable to identify interval model".to_string())?;
            let confidence = match confidence {
                crate::estimator::Confidence::High => "high",
                crate::estimator::Confidence::Medium => "medium",
                crate::estimator::Confidence::Low => "low",
            };
            transaction
                .execute(
                    "INSERT INTO quotes (
                        timestamp_ms, value_usd, raw_value_usd, observed_cost_usd,
                        weekly_used_percent, dominant_model, confidence, status,
                        is_finalized, algorithm_version
                     ) VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, 'valid', 1, ?7)",
                    params![
                        current.observed_at_ms,
                        quote_usd,
                        cost_delta_usd,
                        current.used_percent,
                        dominant_model,
                        confidence,
                        ALGORITHM_VERSION
                    ],
                )
                .map_err(|_| "unable to persist settled quote".to_string())?;
        }
        Ok(())
    }

    pub fn latest_quota_observation(&self) -> Result<Option<LatestQuotaObservation>, String> {
        self.connection
            .query_row(
                "SELECT used_percent, reset_at_ms, plan FROM quota_snapshots ORDER BY observed_at_ms DESC LIMIT 1",
                [],
                |row| {
                    Ok(LatestQuotaObservation {
                        used_percent: row.get(0)?,
                        reset_at_ms: row.get(1)?,
                        plan: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(|_| "unable to read current quota".to_string())
    }

    fn weighted_quotes(&self) -> Result<Vec<WeightedQuote>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT quotes.timestamp_ms, quotes.value_usd, quotes.raw_value_usd,
                        quotes.observed_cost_usd, quotes.weekly_used_percent,
                        quotes.is_finalized, quotes.dominant_model,
                        (SELECT (quota.reset_at_ms + 30000) / 60000
                         FROM quota_snapshots quota
                         WHERE quota.observed_at_ms=quotes.timestamp_ms
                           AND quota.reset_at_ms IS NOT NULL
                           AND ABS(quota.duration_minutes - 10080.0) <= 240.0
                         ORDER BY quota.id DESC
                         LIMIT 1)
                 FROM quotes
                 WHERE quotes.algorithm_version=?1
                 ORDER BY quotes.timestamp_ms",
            )
            .map_err(|_| "unable to read weighted quote history".to_string())?;
        let rows = statement
            .query_map(params![ALGORITHM_VERSION], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, i64>(5)? != 0,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })
            .map_err(|_| "unable to read weighted quote history".to_string())?;
        let mut aggregate_by_epoch: HashMap<i64, EpochAggregate> = HashMap::new();
        let mut weighted = Vec::new();
        for row in rows {
            let (
                timestamp,
                value,
                raw_value,
                observed_cost,
                weekly_used,
                is_finalized,
                model,
                epoch,
            ) = row.map_err(|_| "unable to decode weighted quote history".to_string())?;
            let Some(epoch) = epoch else {
                continue;
            };
            if !value.is_finite() || value <= 0.0 || !observed_cost.is_finite() {
                continue;
            }
            let quota_points = observed_cost / value * 100.0;
            if !quota_points.is_finite() || quota_points <= 0.0 {
                continue;
            }
            let aggregate = aggregate_by_epoch.entry(epoch).or_default();
            aggregate.cost_usd += observed_cost;
            aggregate.quota_points += quota_points;
            if let Some(model) = model {
                match aggregate.model.as_deref() {
                    None => aggregate.model = Some(model),
                    Some(existing) if existing != model => aggregate.mixed = true,
                    _ => {}
                }
            }
            let display_model = if aggregate.mixed {
                Some("mixed".to_string())
            } else {
                aggregate.model.clone()
            };
            weighted.push(WeightedQuote {
                point: HistoryPoint {
                    timestamp,
                    value_usd: Some(aggregate.cost_usd / aggregate.quota_points * 100.0),
                    raw_value_usd: raw_value,
                    weekly_used_percent: weekly_used,
                    is_finalized,
                    is_heartbeat: false,
                    dominant_model: display_model,
                    epoch: Some(epoch),
                },
                epoch,
                observed_cost_usd: aggregate.cost_usd,
            });
        }
        Ok(weighted)
    }

    pub fn latest_quote(&self) -> Result<Option<CurrentQuote>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT timestamp_ms, value_usd, observed_cost_usd, weekly_used_percent,
                        dominant_model, confidence, status
                 FROM quotes ORDER BY timestamp_ms DESC LIMIT 5",
            )
            .map_err(|_| "unable to read current quote".to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(|_| "unable to read current quote".to_string())?;
        let recent = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| "unable to decode current quote".to_string())?;
        let Some(latest) = recent.first() else {
            return Ok(None);
        };
        let latest_timestamp = latest.0;
        let latest_reset_group: Option<i64> = self
            .connection
            .query_row(
                "SELECT (reset_at_ms + 30000) / 60000
                 FROM quota_snapshots
                 WHERE observed_at_ms=?1
                   AND reset_at_ms IS NOT NULL
                   AND ABS(duration_minutes - 10080.0) <= 240.0
                 ORDER BY id DESC
                 LIMIT 1",
                params![latest_timestamp],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "unable to identify current quota epoch".to_string())?;
        let weighted = self.weighted_quotes()?;
        let current = latest_reset_group
            .and_then(|epoch| weighted.iter().rev().find(|quote| quote.epoch == epoch));
        let current_value = current.and_then(|quote| quote.point.value_usd);
        let previous_value = current_value.and_then(|_| {
            comparison_baseline(
                &weighted,
                latest_timestamp - Range::W1.duration_ms(),
                Range::W1.duration_ms() / 2,
            )
        });
        let observed_cost_usd = current.map(|quote| quote.observed_cost_usd);
        let weekly_used_percent = latest.3;
        let confidence = latest.5.as_str();
        let status = latest.6.as_str();
        let reset_at: Option<i64> = self
            .connection
            .query_row(
                "SELECT reset_at_ms FROM quota_snapshots ORDER BY observed_at_ms DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "unable to read current reset time".to_string())?
            .flatten();
        Ok(Some(CurrentQuote {
            value_usd: current_value,
            change_usd: current_value
                .zip(previous_value)
                .map(|(current, previous)| current - previous),
            change_percent: current_value
                .zip(previous_value)
                .filter(|(_, previous)| *previous != 0.0)
                .map(|(current, previous)| ((current - previous) / previous) * 100.0),
            observed_cost_usd,
            weekly_used_percent,
            reset_at,
            status: if current_value.is_none() && status == "valid" {
                QuoteStatus::Pending
            } else {
                match status {
                    "valid" => QuoteStatus::Valid,
                    "pending" => QuoteStatus::Pending,
                    "unsupported" => QuoteStatus::Unsupported,
                    "error" => QuoteStatus::Error,
                    _ => QuoteStatus::Empty,
                }
            },
            dominant_model: current.and_then(|quote| quote.point.dominant_model.clone()),
            algorithm_version: ALGORITHM_VERSION.into(),
            confidence: match confidence {
                "high" => Confidence::High,
                "medium" => Confidence::Medium,
                "low" => Confidence::Low,
                _ => Confidence::None,
            },
            note: Some("Weighted from every settled priced model in the current weekly quota cycle; unknown-price usage remains pending.".into()),
        }))
    }

    pub fn history(&self, range: Range) -> Result<HistoryResponse, String> {
        let latest: Option<(i64, Option<i64>)> = self
            .connection
            .query_row(
                "SELECT quotes.timestamp_ms,
                        (SELECT (quota.reset_at_ms + 30000) / 60000
                         FROM quota_snapshots quota
                         WHERE quota.observed_at_ms=quotes.timestamp_ms
                           AND quota.reset_at_ms IS NOT NULL
                           AND ABS(quota.duration_minutes - 10080.0) <= 240.0
                         ORDER BY quota.id DESC
                         LIMIT 1)
                 FROM quotes
                 ORDER BY quotes.timestamp_ms DESC
                 LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| "unable to identify current quote history".to_string())?;
        let Some((latest_timestamp, latest_epoch)) = latest else {
            return Ok(empty_history(range));
        };
        let weighted = self.weighted_quotes()?;
        let since = latest_timestamp - range.duration_ms();
        let current = latest_epoch.and_then(|epoch| {
            weighted
                .iter()
                .rev()
                .find(|quote| quote.epoch == epoch)
                .and_then(|quote| quote.point.value_usd)
        });
        let baseline =
            current.and_then(|_| comparison_baseline(&weighted, since, range.duration_ms() / 2));
        let points = graph_points(
            &weighted,
            since,
            latest_epoch,
            matches!(range, Range::D1 | Range::W1),
        );
        let delta = current
            .zip(baseline)
            .map(|(current, baseline)| current - baseline);
        let delta_percent = current
            .zip(baseline)
            .filter(|(_, baseline)| *baseline != 0.0)
            .map(|(current, baseline)| ((current - baseline) / baseline) * 100.0);
        let tolerance_ms = match &range {
            Range::D1 => 30 * 60 * 1_000,
            Range::W1 => 60 * 60 * 1_000,
            Range::M1 => 4 * 60 * 60 * 1_000,
            Range::M3 | Range::M6 => 8 * 60 * 60 * 1_000,
        };
        let partial = points
            .first()
            .map_or(true, |point| point.timestamp > since + tolerance_ms);
        Ok(HistoryResponse {
            statistics: RangeStatistics {
                range: range.clone(),
                baseline_value_usd: baseline,
                current_value_usd: current,
                delta_usd: delta,
                delta_percent,
                point_count: points.len(),
                partial,
            },
            bucket: range.bucket().into(),
            points,
        })
    }

    pub fn annotations(&self) -> Result<Vec<Annotation>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, timestamp_ms, label, kind FROM annotations ORDER BY timestamp_ms ASC",
            )
            .map_err(|_| "unable to read annotations".to_string())?;
        let rows = statement
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
            .map_err(|_| "unable to read annotations".to_string())?;
        rows.map(|row| row.map_err(|_| "unable to decode annotation".to_string()))
            .collect()
    }

    pub fn reset_annotations(&mut self) -> Result<(), String> {
        self.connection
            .execute("DELETE FROM annotations", [])
            .map_err(|_| "unable to reset annotations".to_string())?;
        Ok(())
    }

    pub fn diagnostics(&self) -> Result<DiagnosticsSummary, String> {
        let total_events: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))
            .unwrap_or(0);
        let priced_events: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE eligible=1 AND pricing_status='priced'",
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
        let partial_line_retries: i64 = self.diagnostic_count("partial final line");
        let monitoring_gaps: i64 = self.diagnostic_count("monitoring gap");
        let hidden_resets: i64 = self.diagnostic_count("hidden reset");
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
            partial_line_retries,
            monitoring_gaps,
            hidden_resets,
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
    transaction: &rusqlite::Transaction<'_>,
    reason: &str,
    increment: i64,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO diagnostics (reason, count, updated_at_ms) VALUES (?1, ?2, ?3) ON CONFLICT(reason) DO UPDATE SET count=count+excluded.count, updated_at_ms=excluded.updated_at_ms",
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
    use crate::models::AdvancedSettings;
    use crate::pricing::embedded_codex_snapshot;

    #[test]
    fn account_key_is_salted_and_non_reversible() {
        let first = hash_account_key(b"salt-a", " account-primary ");
        let second = hash_account_key(b"salt-b", "account-primary");
        assert_ne!(first, second);
        assert!(!first.contains("account-primary"));
        assert!(first.starts_with("acct_"));
    }

    #[test]
    fn settings_validate_defaults() {
        assert!(AdvancedSettings::default().validate().is_ok());
    }

    #[test]
    fn migration_v5_discards_stale_pricing_for_reimport() {
        let path = std::env::temp_dir().join(format!(
            "nerfify-parser-migration-{}-{}.db",
            std::process::id(),
            now_ms()
        ));
        let mut database = Database {
            path: path.clone(),
            connection: open_connection(&path).expect("temporary database"),
        };
        database.migrate().expect("schema");
        database
            .connection
            .execute(
                "INSERT INTO usage_events (
                    fingerprint, timestamp_ms, model_id, input_tokens,
                    cached_input_tokens, output_tokens, pricing_status
                 ) VALUES ('old', 1, 'gpt-5.6-sol', 1, 0, 0, 'priced')",
                [],
            )
            .expect("old usage");
        database
            .connection
            .pragma_update(None, "user_version", 3)
            .expect("old schema version");

        database.migrate().expect("parser migration");

        let events: i64 = database
            .connection
            .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))
            .expect("usage count");
        assert_eq!(events, 0);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn live_collection_produces_current_quote_and_history() {
        let path = std::env::temp_dir().join(format!(
            "nerfify-live-{}-{}.db",
            std::process::id(),
            now_ms()
        ));
        let mut database = Database {
            path: path.clone(),
            connection: open_connection(&path).expect("temporary database"),
        };
        database.migrate().expect("schema");
        assert_eq!(
            database
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("schema version"),
            5
        );
        let hot_path_indexes: i64 = database
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='index' AND name IN (
                     'idx_usage_events_account_pricing_time',
                     'idx_quota_snapshots_account_limit_time',
                     'idx_quota_snapshots_time',
                     'idx_quotes_algorithm_time'
                 )",
                [],
                |row| row.get(0),
            )
            .expect("hot path indexes");
        assert_eq!(hot_path_indexes, 4);
        let bucket_start = now_ms().div_euclid(1_800_000) * 1_800_000 - 10_800_000;
        let reset_at_ms = bucket_start + 604_800_000;
        let events = vec![
            UsageEvent {
                timestamp_ms: bucket_start + 60_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 1_000_000,
                output_tokens: 100_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(10.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 1_860_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000,
                output_tokens: 10_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(12.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 1_920_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000,
                output_tokens: 10_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(15.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 3_660_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000,
                output_tokens: 10_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(20.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 3_720_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000,
                output_tokens: 10_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(25.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 5_460_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000,
                output_tokens: 10_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(95.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 5_520_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000,
                output_tokens: 10_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(100.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 7_260_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000_000,
                output_tokens: 10_000_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(100.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
            UsageEvent {
                timestamp_ms: bucket_start + 7_320_000,
                model: "gpt-5.6-sol".into(),
                input_tokens: 100_000_000,
                output_tokens: 10_000_000,
                authenticated_official_codex: true,
                quota_used_percent: Some(100.0),
                quota_reset_at_ms: Some(reset_at_ms),
                quota_window_minutes: Some(10_080.0),
                quota_limit_id: Some("codex".into()),
                ..UsageEvent::default()
            },
        ];
        let pricing = embedded_codex_snapshot(&events, now_ms());
        database
            .persist_collection(
                &CollectionSummary {
                    events,
                    ..CollectionSummary::default()
                },
                None,
                Some(&pricing),
            )
            .expect("persist live data");
        let quote = database
            .latest_quote()
            .expect("quote")
            .expect("current quote");
        assert!(quote.value_usd.is_some_and(|value| value > 0.0));
        assert_eq!(quote.weekly_used_percent, Some(100.0));
        assert_eq!(quote.change_usd, None);
        assert_eq!(quote.algorithm_version, "nerfify-estimator-v3");
        let history = database.history(Range::W1).expect("history");
        assert_eq!(history.points.len(), 3);
        assert_eq!(history.statistics.current_value_usd, quote.value_usd);
        assert!(history
            .points
            .iter()
            .all(|point| point.value_usd.is_some_and(|value| value < 100.0)));
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn small_movements_accumulate_and_heartbeats_do_not_delay_settlement() {
        let path = std::env::temp_dir().join(format!(
            "nerfify-heartbeats-{}-{}.db",
            std::process::id(),
            now_ms()
        ));
        let mut database = Database {
            path: path.clone(),
            connection: open_connection(&path).expect("temporary database"),
        };
        database.migrate().expect("schema");
        let bucket_start = now_ms().div_euclid(1_800_000) * 1_800_000 - 10_800_000;
        let first_observation = bucket_start + 60_000;
        let first_heartbeat = bucket_start + 120_000;
        let eleven_observation = bucket_start + 180_000;
        let eleven_heartbeat = bucket_start + 240_000;
        let twelve_observation = bucket_start + 300_000;
        let changed_observation = bucket_start + 360_000;
        let changed_heartbeat = bucket_start + 420_000;
        let reset_at_ms = bucket_start + Range::W1.duration_ms();
        let events = [
            (first_observation, 10.0),
            (first_heartbeat, 10.0),
            (eleven_observation, 11.0),
            (eleven_heartbeat, 11.0),
            (twelve_observation, 12.0),
            (changed_observation, 13.0),
            (changed_heartbeat, 13.0),
        ]
        .into_iter()
        .map(|(timestamp_ms, quota_used_percent)| UsageEvent {
            timestamp_ms,
            model: "gpt-5.6-sol".into(),
            input_tokens: 100_000,
            output_tokens: 10_000,
            authenticated_official_codex: true,
            quota_used_percent: Some(quota_used_percent),
            quota_reset_at_ms: Some(reset_at_ms),
            quota_window_minutes: Some(10_080.0),
            quota_limit_id: Some("codex".into()),
            ..UsageEvent::default()
        })
        .collect::<Vec<_>>();
        let pricing = embedded_codex_snapshot(&events, now_ms());
        database
            .persist_collection(
                &CollectionSummary {
                    events,
                    ..CollectionSummary::default()
                },
                None,
                Some(&pricing),
            )
            .expect("persist heartbeat data");

        let history = database.history(Range::W1).expect("history");
        assert_eq!(history.points.len(), 1);
        assert_eq!(history.points[0].timestamp, changed_observation);

        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn history_weights_single_mixed_and_future_models() {
        let path = std::env::temp_dir().join(format!(
            "nerfify-history-{}-{}.db",
            std::process::id(),
            now_ms()
        ));
        let mut database = Database {
            path: path.clone(),
            connection: open_connection(&path).expect("temporary database"),
        };
        database.migrate().expect("schema");
        let current = now_ms();
        let prior_reset = current - 2 * Range::W1.duration_ms();
        let current_reset = current + Range::W1.duration_ms();
        let insert_quote = |connection: &Connection,
                            timestamp: i64,
                            cost: f64,
                            quota_points: f64,
                            used_percent: f64,
                            model: &str,
                            reset: i64| {
            let value = cost / quota_points * 100.0;
            connection
                .execute(
                    "INSERT INTO quota_snapshots (
                            observed_at_ms, reset_at_ms, duration_minutes, limit_id,
                            used_percent, connection_quality
                         ) VALUES (?1, ?2, 10080.0, 'codex', ?3, 'good')",
                    params![timestamp, reset, used_percent],
                )
                .expect("quota");
            connection
                .execute(
                    "INSERT INTO quotes (
                            timestamp_ms, value_usd, raw_value_usd, observed_cost_usd,
                            weekly_used_percent, dominant_model, confidence, status,
                            is_finalized, algorithm_version
                         ) VALUES (?1, ?2, ?2, ?3, ?4, ?5, 'high', 'valid', 1, ?6)",
                    params![
                        timestamp,
                        value,
                        cost,
                        used_percent,
                        model,
                        ALGORITHM_VERSION
                    ],
                )
                .expect("quote");
        };
        for (index, (cost, model)) in [
            (0.9, "gpt-5.5"),
            (1.0, "gpt-future-official"),
            (0.944, "gpt-5.5"),
        ]
        .into_iter()
        .enumerate()
        {
            insert_quote(
                &database.connection,
                current - 10 * 86_400_000 + index as i64 * 21_600_000,
                cost,
                10.0,
                (index + 1) as f64 * 10.0,
                model,
                prior_reset,
            );
        }
        let current_intervals = [
            (2.959_801_5, 3.0, "gpt-5.6-luna"),
            (1.240_624_5, 8.0, "gpt-5.6-sol"),
            (0.716_424_5, 10.0, "gpt-5.6-sol"),
            (0.871_220_5, 8.0, "gpt-5.6-sol"),
            (0.305_190_75, 3.0, "gpt-5.6-sol"),
        ];
        let mut used_percent = 0.0;
        for (index, (cost, quota_points, model)) in current_intervals.into_iter().enumerate() {
            used_percent += quota_points;
            insert_quote(
                &database.connection,
                current - 4 * 3_600_000 + index as i64 * 3_600_000,
                cost,
                quota_points,
                used_percent,
                model,
                current_reset,
            );
        }

        let expected_current =
            current_intervals.iter().map(|item| item.0).sum::<f64>() / used_percent * 100.0;
        let expected_previous = (0.9 + 1.0 + 0.944) / 30.0 * 100.0;
        let quote = database
            .latest_quote()
            .expect("quote")
            .expect("current quote");
        assert!(quote
            .value_usd
            .is_some_and(|value| (value - expected_current).abs() < 0.000_001));
        assert!(quote
            .observed_cost_usd
            .is_some_and(|value| (value - 6.093_261_75).abs() < 0.000_001));
        assert_eq!(quote.dominant_model.as_deref(), Some("mixed"));
        assert!(quote
            .change_usd
            .is_some_and(
                |value| (value - (expected_current - expected_previous)).abs() < 0.000_001
            ));

        let week = database.history(Range::W1).expect("week");
        assert_eq!(week.points.len(), 5);
        assert!(week
            .statistics
            .baseline_value_usd
            .is_some_and(|value| (value - expected_previous).abs() < 0.000_001));
        assert_eq!(
            week.points.last().and_then(|point| point.value_usd),
            Some(expected_current)
        );
        assert_eq!(
            week.points
                .last()
                .and_then(|point| point.dominant_model.as_deref()),
            Some("mixed")
        );

        let day = database.history(Range::D1).expect("day");
        assert_eq!(day.statistics.baseline_value_usd, None);
        assert_eq!(day.statistics.delta_percent, None);
        drop(database);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn graph_keeps_manual_resets_exact_without_transient_old_cycles() {
        let quote = |timestamp, value, epoch| WeightedQuote {
            point: HistoryPoint {
                timestamp,
                value_usd: Some(value),
                raw_value_usd: Some(value),
                weekly_used_percent: Some(10.0),
                is_finalized: true,
                is_heartbeat: false,
                dominant_model: None,
                epoch: Some(epoch),
            },
            epoch,
            observed_cost_usd: value / 10.0,
        };
        let quotes = vec![
            quote(1, 40.0, 10),
            quote(2, 20.0, 10),
            quote(3, 30.0, 11),
            quote(4, 25.0, 11),
            quote(5, 18.0, 12),
            quote(6, 19.0, 12),
        ];

        let points = graph_points(&quotes, 0, Some(12), true);

        assert_eq!(
            points
                .iter()
                .map(|point| (point.timestamp, point.value_usd))
                .collect::<Vec<_>>(),
            vec![
                (2, Some(20.0)),
                (4, Some(25.0)),
                (5, Some(18.0)),
                (6, Some(19.0))
            ]
        );
        assert!(points.iter().all(|point| point.epoch.is_none()));

        let long_range = graph_points(&quotes, 0, Some(12), false);
        assert_eq!(
            long_range
                .iter()
                .map(|point| (point.timestamp, point.value_usd))
                .collect::<Vec<_>>(),
            vec![(2, Some(20.0)), (4, Some(25.0)), (6, Some(19.0))]
        );
    }
}
