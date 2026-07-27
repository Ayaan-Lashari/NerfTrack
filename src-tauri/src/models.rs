use serde::{Deserialize, Serialize};

pub const ALGORITHM_VERSION: &str = "nerfify-token-api-equivalent-v3";

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub estimated_weekly_value_usd: Option<f64>,
    pub change_value_usd: Option<f64>,
    pub change_percent: Option<f64>,
    pub observed_cost_usd: Option<f64>,
    pub weekly_used_percent: Option<f64>,
    pub reset_at: Option<i64>,
    pub reset_reason: Option<String>,
    pub status: QuoteStatus,
    pub algorithm_version: String,
    pub confidence: Confidence,
    pub valid_observation_count: u64,
    pub percentage_coverage: Option<f64>,
    pub pricing_source: Option<String>,
    pub model_status: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub timestamp: i64,
    pub estimated_weekly_value_usd: Option<f64>,
    pub raw_estimated_weekly_value_usd: Option<f64>,
    pub observed_cost_usd: Option<f64>,
    pub weekly_used_percent: Option<f64>,
    pub reset_at: Option<i64>,
    pub reset_reason: Option<String>,
    pub is_finalized: bool,
    pub is_heartbeat: bool,
    pub epoch: Option<i64>,
    pub confidence: Confidence,
    pub percentage_coverage: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeStatistics {
    pub range: Range,
    pub baseline_estimated_weekly_value_usd: Option<f64>,
    pub baseline_timestamp: Option<i64>,
    pub current_estimated_weekly_value_usd: Option<f64>,
    pub delta_value_usd: Option<f64>,
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
    pub reduced_motion: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 10,
            reconciliation_interval_hours: 1,
            monitoring_gap_minutes: 5,
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
    #[serde(default)]
    pub custom_pricing: Vec<CustomPriceOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomPriceOverride {
    pub model_id: String,
    #[serde(default)]
    pub alias: Option<String>,
    pub input_usd_per_million: f64,
    pub cached_input_usd_per_million: f64,
    pub output_usd_per_million: f64,
}

impl CustomPriceOverride {
    pub fn validate(&self) -> Result<(), String> {
        if self.model_id.trim().is_empty() {
            return Err("custom pricing model ID is required".into());
        }
        for value in [
            self.input_usd_per_million,
            self.cached_input_usd_per_million,
            self.output_usd_per_million,
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err("custom token prices must be finite non-negative USD amounts".into());
            }
        }
        Ok(())
    }
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
            custom_pricing: Vec::new(),
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.advanced.validate()?;
        for price in &self.custom_pricing {
            price.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactedSelection {
    pub selected: bool,
    pub status: DiscoveryStatus,
}
