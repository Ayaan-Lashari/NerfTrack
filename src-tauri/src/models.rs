use serde::{Deserialize, Serialize};

pub const ALGORITHM_VERSION: &str = "nerfify-estimator-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Range {
    D1,
    W1,
    M1,
    M3,
    M6,
}

impl Range {
    pub fn duration_ms(&self) -> i64 {
        match self {
            Self::D1 => 86_400_000,
            Self::W1 => 604_800_000,
            Self::M1 => 2_592_000_000,
            Self::M3 => 7_776_000_000,
            Self::M6 => 15_552_000_000,
        }
    }

    pub fn bucket(&self) -> &'static str {
        match self {
            Self::D1 => "raw",
            Self::W1 => "30m",
            Self::M1 => "2h",
            Self::M3 | Self::M6 => "4h",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatusState {
    Connected,
    Detecting,
    Settling,
    Recalibrating,
    Unsupported,
    NeedsSetup,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountState {
    Authenticated,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionQuality {
    Good,
    Degraded,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataQuality {
    Complete,
    Partial,
    Interrupted,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationMode {
    Cli,
    Gui,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryState {
    AutoDetected,
    Selected,
    Missing,
    Unsupported,
    Redacted,
    NotRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryStatus {
    pub state: DiscoveryState,
    pub redacted_location: Option<String>,
    pub message: String,
}

impl DiscoveryStatus {
    pub fn missing(message: impl Into<String>) -> Self {
        Self {
            state: DiscoveryState::Missing,
            redacted_location: None,
            message: message.into(),
        }
    }

    pub fn not_required(message: impl Into<String>) -> Self {
        Self {
            state: DiscoveryState::NotRequired,
            redacted_location: None,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub state: AppStatusState,
    pub label: String,
    pub detail: String,
    pub integration_mode: IntegrationMode,
    pub account_state: AccountState,
    pub connection_quality: ConnectionQuality,
    pub plan: Option<String>,
    pub reset_at: Option<i64>,
    pub last_updated_at: Option<i64>,
    pub codex_home: DiscoveryStatus,
    pub codex_executable: DiscoveryStatus,
    pub app_server: DiscoveryStatus,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteStatus {
    Valid,
    Pending,
    Empty,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentQuote {
    pub value_usd: Option<f64>,
    pub change_usd: Option<f64>,
    pub change_percent: Option<f64>,
    pub observed_cost_usd: Option<f64>,
    pub weekly_used_percent: Option<f64>,
    pub reset_at: Option<i64>,
    pub status: QuoteStatus,
    pub dominant_model: Option<String>,
    pub algorithm_version: String,
    pub confidence: Confidence,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub timestamp: i64,
    pub value_usd: Option<f64>,
    pub raw_value_usd: Option<f64>,
    pub weekly_used_percent: Option<f64>,
    pub is_finalized: bool,
    pub is_heartbeat: bool,
    pub dominant_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeStatistics {
    pub range: Range,
    pub baseline_value_usd: Option<f64>,
    pub current_value_usd: Option<f64>,
    pub delta_usd: Option<f64>,
    pub delta_percent: Option<f64>,
    pub point_count: usize,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    pub points: Vec<HistoryPoint>,
    pub statistics: RangeStatistics,
    pub bucket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Reset,
    Diagnostic,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: String,
    pub timestamp: i64,
    pub label: String,
    pub kind: AnnotationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReason {
    pub reason: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSummary {
    pub total_events: i64,
    pub priced_events: i64,
    pub pending_events: i64,
    pub rejected_events: i64,
    pub unattributed_events: i64,
    pub partial_line_retries: i64,
    pub monitoring_gaps: i64,
    pub hidden_resets: i64,
    pub reasons: Vec<DiagnosticReason>,
    pub model_ids: Vec<String>,
    pub privacy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedSettings {
    pub refresh_interval_seconds: u64,
    pub reconciliation_interval_hours: u64,
    pub monitoring_gap_minutes: u64,
    pub settlement_window_seconds: u64,
    pub minimum_quota_movement_points: f64,
    pub minimum_eligible_cost_usd: f64,
    pub minimum_events: u64,
    pub low_usage_quarantine_percent: f64,
    pub reduced_motion: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 10,
            reconciliation_interval_hours: 1,
            monitoring_gap_minutes: 5,
            settlement_window_seconds: 60,
            minimum_quota_movement_points: 3.0,
            minimum_eligible_cost_usd: 0.25,
            minimum_events: 2,
            low_usage_quarantine_percent: 3.0,
            reduced_motion: false,
        }
    }
}

impl AdvancedSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(5..=60).contains(&self.refresh_interval_seconds) {
            return Err("refresh interval must be between 5 and 60 seconds".into());
        }
        if !(1..=24).contains(&self.reconciliation_interval_hours) {
            return Err("reconciliation interval must be between 1 and 24 hours".into());
        }
        if !(1..=30).contains(&self.monitoring_gap_minutes) {
            return Err("monitoring gap must be between 1 and 30 minutes".into());
        }
        if !(30..=120).contains(&self.settlement_window_seconds) {
            return Err("settlement window must be between 30 and 120 seconds".into());
        }
        if !self.minimum_quota_movement_points.is_finite()
            || !(0.5..=25.0).contains(&self.minimum_quota_movement_points)
        {
            return Err("minimum quota movement must be between 0.5 and 25 points".into());
        }
        if !self.minimum_eligible_cost_usd.is_finite()
            || !(0.25..=100.0).contains(&self.minimum_eligible_cost_usd)
        {
            return Err("minimum eligible cost must be between 0.25 and 100 USD".into());
        }
        if !(1..=100).contains(&self.minimum_events) {
            return Err("minimum events must be between 1 and 100".into());
        }
        if !self.low_usage_quarantine_percent.is_finite()
            || !(0.0..=25.0).contains(&self.low_usage_quarantine_percent)
        {
            return Err("low-usage quarantine must be between 0 and 25 percent".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(flatten)]
    pub advanced: AdvancedSettings,
    pub appearance: String,
    pub currency: String,
    pub local_only: bool,
    pub telemetry: bool,
    pub auto_updater: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            advanced: AdvancedSettings::default(),
            appearance: "dark".into(),
            currency: "USD".into(),
            local_only: true,
            telemetry: false,
            auto_updater: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactedSelection {
    pub selected: bool,
    pub status: DiscoveryStatus,
}
