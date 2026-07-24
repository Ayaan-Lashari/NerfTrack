// Derived from the pinned ccusage Codex pricing behavior (MIT), commit
// 31e084afbca3981af97ab6b55abe4f38f451bad4. Nerfify retains only pricing behavior.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::parser::UsageEvent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub input_per_million: f64,
    pub cached_input_per_million: f64,
    pub output_per_million: f64,
}

#[derive(Debug, Clone)]
pub struct PricingSnapshot {
    pub source: String,
    pub version: Option<String>,
    pub etag: Option<String>,
    pub observed_at_ms: i64,
    pub sha256: String,
    pub prices: std::collections::BTreeMap<String, Price>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Eligibility {
    Eligible,
    Pending(String),
    Rejected(String),
}

pub const OFFICIAL_CODEX_ALLOWLIST: &[&str] = &["gpt-5-codex", "gpt-5-codex-mini", "codex-1"];
pub const PRICING_REFRESH_INTERVAL_MS: i64 = 24 * 60 * 60 * 1_000;
const LONG_CONTEXT_THRESHOLD_TOKENS: u64 = 272_000;

#[derive(Clone, Copy)]
struct EmbeddedRate {
    model: &'static str,
    input: f64,
    cached_input: f64,
    output: f64,
    long_context: bool,
}

const fn rate(
    model: &'static str,
    input: f64,
    cached_input: f64,
    output: f64,
    long_context: bool,
) -> EmbeddedRate {
    EmbeddedRate {
        model,
        input,
        cached_input,
        output,
        long_context,
    }
}

// Standard API-equivalent text-token prices per million tokens. Exact model
// names prevent a mini, nano, or pro tier from inheriting its parent rate.
const EMBEDDED_CODEX_RATES: &[EmbeddedRate] = &[
    rate("gpt-5.6-sol", 5.0, 0.5, 30.0, true),
    rate("gpt-5.6-terra", 2.5, 0.25, 15.0, true),
    rate("gpt-5.6-luna", 1.0, 0.1, 6.0, true),
    rate("gpt-5.6", 5.0, 0.5, 30.0, true),
    rate("gpt-5.5-pro", 30.0, 30.0, 180.0, true),
    rate("gpt-5.5", 5.0, 0.5, 30.0, true),
    rate("gpt-5.4-pro", 30.0, 30.0, 180.0, true),
    rate("gpt-5.4-mini", 0.75, 0.075, 4.5, false),
    rate("gpt-5.4-nano", 0.2, 0.02, 1.25, false),
    rate("gpt-5.4", 2.5, 0.25, 15.0, true),
    rate("gpt-5.3-codex", 1.75, 0.175, 14.0, false),
    rate("gpt-5.2-codex", 1.75, 0.175, 14.0, false),
    rate("gpt-5.2", 1.75, 0.175, 14.0, false),
    rate("gpt-5.1-codex-mini", 0.25, 0.025, 2.0, false),
    rate("gpt-5.1-codex-max", 1.25, 0.125, 10.0, false),
    rate("gpt-5.1-codex", 1.25, 0.125, 10.0, false),
    rate("gpt-5.1", 1.25, 0.125, 10.0, false),
    rate("gpt-5-codex", 1.25, 0.125, 10.0, false),
    rate("gpt-5-mini", 0.25, 0.025, 2.0, false),
    rate("gpt-5-nano", 0.05, 0.005, 0.4, false),
    rate("gpt-5", 1.25, 0.125, 10.0, false),
    rate("codex-mini-latest", 1.5, 0.375, 6.0, false),
];

pub fn should_refresh_pricing(last_observed_at_ms: Option<i64>, now_ms: i64) -> bool {
    last_observed_at_ms
        .map(|observed| now_ms.saturating_sub(observed) >= PRICING_REFRESH_INTERVAL_MS)
        .unwrap_or(true)
}

pub fn normalize_model_id(model: &str) -> String {
    model.trim().to_ascii_lowercase().replace("/", "-")
}

pub fn model_is_allowlisted(model: &str) -> bool {
    OFFICIAL_CODEX_ALLOWLIST.contains(&normalize_model_id(model).as_str())
}

pub fn classify_provider_eligibility(event: &UsageEvent) -> Eligibility {
    if event.explicit_backend.as_deref().is_some_and(|backend| {
        let value = backend.to_ascii_lowercase();
        value.contains("bedrock")
            || value.contains("anthropic")
            || value.contains("api-key")
            || value.contains("api_key")
    }) {
        return Eligibility::Rejected("unsupported backend".into());
    }
    if event.explicit_provider.as_deref().is_some_and(|provider| {
        matches!(
            provider.to_ascii_lowercase().as_str(),
            "bedrock" | "anthropic" | "azure" | "api-key" | "api_key"
        )
    }) {
        return Eligibility::Rejected("unsupported provider".into());
    }
    if event.authenticated_official_codex {
        return Eligibility::Eligible;
    }
    if event
        .explicit_provider
        .as_deref()
        .is_some_and(|provider| provider.eq_ignore_ascii_case("openai"))
        && event
            .explicit_backend
            .as_deref()
            .is_some_and(|backend| backend.eq_ignore_ascii_case("codex"))
    {
        return Eligibility::Eligible;
    }
    if model_is_allowlisted(&event.model) {
        return Eligibility::Eligible;
    }
    if event.custom_alias {
        return Eligibility::Pending(
            "custom alias resolves pricing but cannot establish provider eligibility".into(),
        );
    }
    Eligibility::Pending("provider evidence or exact Codex model allowlist is missing".into())
}

pub fn price_event(event: &UsageEvent, snapshot: &PricingSnapshot) -> Result<f64, String> {
    let eligibility = classify_provider_eligibility(event);
    if !matches!(eligibility, Eligibility::Eligible) {
        return Err(match eligibility {
            Eligibility::Eligible => unreachable!(),
            Eligibility::Pending(reason) | Eligibility::Rejected(reason) => reason,
        });
    }
    let model = normalize_model_id(&event.model);
    let price = snapshot
        .prices
        .get(&model)
        .ok_or_else(|| "missing pricing snapshot".to_string())?;
    let cached_input = event.cached_input_tokens.min(event.input_tokens);
    let uncached_input = event.input_tokens.saturating_sub(cached_input);
    let inferred_long_context = embedded_rate(&model).is_some_and(|rate| {
        rate.long_context && event.input_tokens > LONG_CONTEXT_THRESHOLD_TOKENS
    });
    let long_context = event.long_context || inferred_long_context;
    let input_multiplier = if long_context { 2.0 } else { 1.0 };
    let output_multiplier = if long_context { 1.5 } else { 1.0 };
    let mut cost = ((uncached_input as f64 * price.input_per_million / 1_000_000.0)
        + (cached_input as f64 * price.cached_input_per_million / 1_000_000.0))
        * input_multiplier
        + (event.output_tokens as f64 * price.output_per_million / 1_000_000.0) * output_multiplier;
    if event.fast_mode {
        cost *= event.fast_mode_multiplier;
    }
    if !cost.is_finite() {
        return Err("non-finite price result".into());
    }
    Ok(cost)
}

pub fn embedded_codex_snapshot(events: &[UsageEvent], observed_at_ms: i64) -> PricingSnapshot {
    let prices = events
        .iter()
        .filter_map(|event| {
            let model = normalize_model_id(&event.model);
            codex_family_price(&model).map(|price| (model, price))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let serialized = serde_json::to_string(&prices).unwrap_or_default();
    PricingSnapshot {
        source: "embedded OpenAI model pricing".into(),
        version: Some("2026-07-exact".into()),
        etag: None,
        observed_at_ms,
        sha256: snapshot_hash(&serialized),
        prices,
    }
}

fn codex_family_price(model: &str) -> Option<Price> {
    let rate = embedded_rate(model)?;
    Some(Price {
        input_per_million: rate.input,
        cached_input_per_million: rate.cached_input,
        output_per_million: rate.output,
    })
}

fn embedded_rate(model: &str) -> Option<EmbeddedRate> {
    EMBEDDED_CODEX_RATES
        .iter()
        .copied()
        .find(|rate| model_matches_rate(model, rate.model))
}

fn model_matches_rate(model: &str, canonical: &str) -> bool {
    if model == canonical {
        return true;
    }
    let Some(snapshot) = model.strip_prefix(canonical) else {
        return false;
    };
    let bytes = snapshot.as_bytes();
    bytes.len() == 11
        && bytes[0] == b'-'
        && bytes[1..5].iter().all(u8::is_ascii_digit)
        && bytes[5] == b'-'
        && bytes[6..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'-'
        && bytes[9..11].iter().all(u8::is_ascii_digit)
}

pub fn snapshot_hash(serialized_pricing: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(serialized_pricing.as_bytes());
    format!("{:x}", digest.finalize())
}

pub fn filtered_snapshot(
    source: &str,
    version: Option<String>,
    observed_at_ms: i64,
    value: &Value,
) -> PricingSnapshot {
    filtered_snapshot_with_metadata(source, version, None, observed_at_ms, value)
}

pub fn filtered_snapshot_with_metadata(
    source: &str,
    version: Option<String>,
    etag: Option<String>,
    observed_at_ms: i64,
    value: &Value,
) -> PricingSnapshot {
    let mut prices = std::collections::BTreeMap::new();
    if let Some(models) = value.get("models").and_then(Value::as_object) {
        for (model, entry) in models {
            if !model_is_allowlisted(model) {
                continue;
            }
            let input = entry.get("input").and_then(Value::as_f64).unwrap_or(0.0);
            let cached = entry
                .get("cache_read")
                .or_else(|| entry.get("cached_input"))
                .and_then(Value::as_f64)
                .unwrap_or(input);
            let output = entry.get("output").and_then(Value::as_f64).unwrap_or(0.0);
            if input.is_finite() && cached.is_finite() && output.is_finite() {
                prices.insert(
                    normalize_model_id(model),
                    Price {
                        input_per_million: input,
                        cached_input_per_million: cached,
                        output_per_million: output,
                    },
                );
            }
        }
    }
    let serialized = serde_json::to_string(&prices).unwrap_or_default();
    PricingSnapshot {
        source: source.into(),
        version,
        etag,
        observed_at_ms,
        sha256: snapshot_hash(&serialized),
        prices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(model: &str) -> UsageEvent {
        UsageEvent {
            model: model.into(),
            input_tokens: 1_000_000,
            cached_input_tokens: 200_000,
            output_tokens: 100_000,
            long_context_multiplier: 1.0,
            fast_mode_multiplier: 1.0,
            ..UsageEvent::default()
        }
    }

    #[test]
    fn explicit_unsupported_provider_never_enters_quotes() {
        let mut usage = event("gpt-5-codex");
        usage.explicit_provider = Some("bedrock".into());
        assert_eq!(
            classify_provider_eligibility(&usage),
            Eligibility::Rejected("unsupported provider".into())
        );
    }

    #[test]
    fn authenticated_official_backend_can_support_new_model() {
        let mut usage = event("gpt-5-codex-next");
        usage.authenticated_official_codex = true;
        assert_eq!(classify_provider_eligibility(&usage), Eligibility::Eligible);
    }

    #[test]
    fn custom_alias_can_price_but_not_establish_eligibility() {
        let mut usage = event("my-codex-alias");
        usage.custom_alias = true;
        assert!(
            matches!(classify_provider_eligibility(&usage), Eligibility::Pending(reason) if reason.contains("custom alias"))
        );
    }

    #[test]
    fn cached_and_uncached_tokens_are_priced_separately() {
        let usage = event("gpt-5-codex");
        let snapshot = PricingSnapshot {
            source: "test".into(),
            version: None,
            etag: None,
            observed_at_ms: 0,
            sha256: "hash".into(),
            prices: std::collections::BTreeMap::from([(
                "gpt-5-codex".into(),
                Price {
                    input_per_million: 2.0,
                    cached_input_per_million: 0.5,
                    output_per_million: 4.0,
                },
            )]),
        };
        let cost = price_event(&usage, &snapshot).expect("cost");
        assert!((cost - 2.1).abs() < f64::EPSILON);
    }

    #[test]
    fn pricing_refresh_is_at_most_daily() {
        assert!(should_refresh_pricing(None, 0));
        assert!(!should_refresh_pricing(
            Some(1_000),
            1_000 + PRICING_REFRESH_INTERVAL_MS - 1
        ));
        assert!(should_refresh_pricing(
            Some(1_000),
            1_000 + PRICING_REFRESH_INTERVAL_MS
        ));
    }

    #[test]
    fn prices_current_codex_tiers_at_their_distinct_rates() {
        for (model, expected) in [
            ("gpt-5.6-sol", 12.7),
            ("gpt-5.6-terra", 6.35),
            ("gpt-5.6-luna", 2.54),
        ] {
            let mut usage = event(model);
            usage.authenticated_official_codex = true;
            let snapshot = embedded_codex_snapshot(&[usage.clone()], 1);
            let cost = price_event(&usage, &snapshot).expect("cost");
            assert!((cost - expected).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn exact_model_rates_do_not_inherit_parent_tiers() {
        for (model, expected) in [
            ("gpt-5.5", (5.0, 0.5, 30.0)),
            ("gpt-5.4", (2.5, 0.25, 15.0)),
            ("gpt-5.4-mini", (0.75, 0.075, 4.5)),
            ("gpt-5.4-nano", (0.2, 0.02, 1.25)),
            ("gpt-5.1-codex-mini", (0.25, 0.025, 2.0)),
            ("gpt-5-mini", (0.25, 0.025, 2.0)),
            ("gpt-5-nano", (0.05, 0.005, 0.4)),
            ("codex-mini-latest", (1.5, 0.375, 6.0)),
        ] {
            let price = codex_family_price(model).expect("supported model");
            assert_eq!(
                (
                    price.input_per_million,
                    price.cached_input_per_million,
                    price.output_per_million
                ),
                expected
            );
        }
    }

    #[test]
    fn official_date_snapshots_use_their_exact_model_rate() {
        let mini = codex_family_price("gpt-5.4-mini-2026-03-17").expect("mini snapshot");
        assert_eq!(mini.input_per_million, 0.75);
        assert!(codex_family_price("gpt-5.4-mini-preview").is_none());
    }

    #[test]
    fn long_context_uses_distinct_input_and_output_multipliers() {
        let mut usage = event("gpt-5.6-sol");
        usage.authenticated_official_codex = true;
        let snapshot = embedded_codex_snapshot(&[usage.clone()], 1);
        let long_cost = price_event(&usage, &snapshot).expect("long-context cost");
        assert!((long_cost - 12.7).abs() < f64::EPSILON);

        usage.input_tokens = LONG_CONTEXT_THRESHOLD_TOKENS;
        usage.cached_input_tokens = 0;
        usage.output_tokens = 100_000;
        let base_cost = price_event(&usage, &snapshot).expect("base cost");
        assert!((base_cost - 4.36).abs() < f64::EPSILON);
    }
}
