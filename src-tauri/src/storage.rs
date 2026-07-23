use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::collector::{CollectionSummary, PersistedCheckpoint};
use crate::models::{
    Annotation, AnnotationKind, AppSettings, Confidence, CurrentQuote, DiagnosticReason,
    DiagnosticsSummary, HistoryPoint, HistoryResponse, QuoteStatus, Range, RangeStatistics,
    ALGORITHM_VERSION,
};
use crate::parser::{event_fingerprint, UsageEvent};
use crate::pricing::{
    classify_provider_eligibility, normalize_model_id, price_event, Eligibility, PricingSnapshot,
};

pub struct Database {
    pub path: PathBuf,
    connection: Connection,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
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
        database.record_app_run()?;
        Ok(database)
    }

    fn migrate(&mut self) -> Result<(), String> {
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
            INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (1, strftime('%s','now') * 1000);
            PRAGMA user_version=1;
            COMMIT;",
            )
            .is_err()
        {
            let _ = self.connection.execute_batch("ROLLBACK;");
            return Err("database migration failed".into());
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
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| "unable to start collection transaction".to_string())?;
        if let Some(snapshot) = pricing_snapshot {
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
        let account_eligible =
            account_key.is_some() && matches!(eligibility, Eligibility::Eligible);
        let (pricing_status, cost_usd): (&str, Option<f64>) = if !account_eligible {
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
                params![event_fingerprint(event), account_key, event.timestamp_ms, normalize_model_id(&event.model), event.input_tokens as i64, event.cached_input_tokens as i64, event.output_tokens as i64, i64::from(account_eligible && cost_usd.is_some()), pricing_status, cost_usd],
            )
            .map_err(|_| "unable to persist usage event".to_string())?;
        if inserted == 1 && pricing_status != "priced" {
            add_diagnostic(transaction, pricing_status, 1)?;
        }
        Ok(inserted == 1)
    }

    pub fn latest_quote(&self) -> Result<Option<CurrentQuote>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT value_usd, observed_cost_usd, weekly_used_percent, dominant_model, confidence, status
                 FROM quotes ORDER BY timestamp_ms DESC LIMIT 2",
            )
            .map_err(|_| "unable to read current quote".to_string())?;
        let mut rows = statement
            .query([])
            .map_err(|_| "unable to read current quote".to_string())?;
        let Some(current_row) = rows
            .next()
            .map_err(|_| "unable to read current quote".to_string())?
        else {
            return Ok(None);
        };
        let (
            current_value,
            observed_cost_usd,
            weekly_used_percent,
            dominant_model,
            confidence,
            status,
        ) = (
            current_row.get(0).unwrap_or(None),
            current_row.get(1).unwrap_or(None),
            current_row.get(2).unwrap_or(None),
            current_row.get(3).ok(),
            current_row
                .get::<_, String>(4)
                .unwrap_or_else(|_| "none".into()),
            current_row
                .get::<_, String>(5)
                .unwrap_or_else(|_| "empty".into()),
        );
        let previous_value: Option<f64> = rows
            .next()
            .map_err(|_| "unable to read current quote".to_string())?
            .and_then(|row| row.get(0).ok().flatten());
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
            change_usd: current_value.zip(previous_value).map(|(current, previous)| current - previous),
            change_percent: current_value
                .zip(previous_value)
                .filter(|(_, previous)| *previous != 0.0)
                .map(|(current, previous)| ((current - previous) / previous) * 100.0),
            observed_cost_usd,
            weekly_used_percent,
            reset_at,
            status: match status.as_str() {
                "valid" => QuoteStatus::Valid,
                "pending" => QuoteStatus::Pending,
                "unsupported" => QuoteStatus::Unsupported,
                "error" => QuoteStatus::Error,
                _ => QuoteStatus::Empty,
            },
            dominant_model,
            algorithm_version: ALGORITHM_VERSION.into(),
            confidence: match confidence.as_str() {
                "high" => Confidence::High,
                "medium" => Confidence::Medium,
                "low" => Confidence::Low,
                _ => Confidence::None,
            },
            note: Some(
                "Values are estimates based on observed local usage and may differ from actual API pricing."
                    .into(),
            ),
        }))
    }

    pub fn history(&self, range: Range) -> Result<HistoryResponse, String> {
        let since = now_ms() - range.duration_ms();
        let mut statement = self.connection.prepare("SELECT timestamp_ms, value_usd, raw_value_usd, weekly_used_percent, is_finalized, dominant_model, 0 AS is_heartbeat FROM quotes WHERE timestamp_ms >= ?1 UNION ALL SELECT timestamp_ms, value_usd, value_usd, weekly_used_percent, 1, NULL, 1 AS is_heartbeat FROM chart_heartbeats WHERE timestamp_ms >= ?1 ORDER BY timestamp_ms ASC").map_err(|_| "unable to read history".to_string())?;
        let rows = statement
            .query_map(params![since], |row| {
                Ok(HistoryPoint {
                    timestamp: row.get(0)?,
                    value_usd: row.get(1)?,
                    raw_value_usd: row.get(2)?,
                    weekly_used_percent: row.get(3)?,
                    is_finalized: row.get::<_, i64>(4)? != 0,
                    is_heartbeat: row.get::<_, i64>(6)? != 0,
                    dominant_model: row.get(5)?,
                })
            })
            .map_err(|_| "unable to read history".to_string())?;
        let mut points = Vec::new();
        for row in rows {
            points.push(row.map_err(|_| "unable to decode history".to_string())?);
        }
        let first = points.first().and_then(|point| point.value_usd);
        let current = points.last().and_then(|point| point.value_usd);
        let delta = current.zip(first).map(|(current, first)| current - first);
        let delta_percent = current
            .zip(first)
            .filter(|(_, first)| *first != 0.0)
            .map(|(current, first)| ((current - first) / first) * 100.0);
        Ok(HistoryResponse {
            statistics: RangeStatistics {
                range: range.clone(),
                baseline_value_usd: first,
                current_value_usd: current,
                delta_usd: delta,
                delta_percent,
                point_count: points.len(),
                partial: !points.is_empty() && !matches!(range, Range::D1),
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
    use crate::models::AdvancedSettings;

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
}
