use std::collections::HashMap;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const MODELS_DEV_API_URL: &str = "https://models.dev/api.json";
pub const PRICING_SOURCE: &str = "models_dev";
pub const CODEX_AUTO_REVIEW_MODEL_ID: &str = "codex-auto-review";
pub const CODEX_AUTO_REVIEW_API_MODEL_ID: &str = "gpt-5.6-luna";

const MAX_CATALOG_BYTES: usize = 32 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemotePriceTier {
    pub threshold_tokens: u64,
    pub input: f64,
    pub cached_input: f64,
    pub output: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemotePrice {
    pub input: f64,
    pub cached_input: f64,
    pub output: f64,
    pub long_context_tiers: Vec<RemotePriceTier>,
}

#[derive(Clone, Debug, Default)]
pub struct PricingCatalog {
    pub models: HashMap<String, RemotePrice>,
    pub digest: Option<String>,
}

impl PricingCatalog {
    pub fn find(&self, model: &str) -> Option<RemotePrice> {
        let normalized = normalize_model_id(model);
        let canonical = canonical_api_model_id(&normalized);
        self.models
            .get(&canonical)
            .cloned()
            .or_else(|| self.models.get(&normalized).cloned())
            .or_else(|| {
                normalized
                    .strip_prefix("openai-")
                    .and_then(|unprefixed| self.models.get(unprefixed).cloned())
            })
    }
}

pub enum FetchOutcome {
    Updated {
        payload: String,
        etag: Option<String>,
        digest: String,
        catalog: PricingCatalog,
    },
    NotModified {
        etag: Option<String>,
    },
}

pub fn normalize_model_id(model: &str) -> String {
    model.trim().to_ascii_lowercase().replace('/', "-")
}

/// Codex records the internal Auto Review worker name rather than the public
/// API model name. Keep the feature label for diagnostics, but price it as the
/// current GPT-5.6 Luna model.
pub fn canonical_api_model_id(model: &str) -> String {
    let normalized = normalize_model_id(model);
    let unprefixed = normalized.strip_prefix("openai-").unwrap_or(&normalized);
    if unprefixed == CODEX_AUTO_REVIEW_MODEL_ID {
        CODEX_AUTO_REVIEW_API_MODEL_ID.into()
    } else {
        unprefixed.into()
    }
}

pub fn parse_catalog(json: &str, digest: Option<String>) -> Result<PricingCatalog, String> {
    let root: Value = serde_json::from_str(json)
        .map_err(|error| format!("models.dev returned invalid JSON: {error}"))?;
    let openai_models = root
        .get("openai")
        .and_then(|provider| provider.get("models"))
        .and_then(Value::as_object)
        .ok_or_else(|| "models.dev catalog has no openai.models section".to_string())?;

    let mut models = HashMap::new();
    for (model_key, model_value) in openai_models {
        let Some(cost) = model_value.get("cost") else {
            continue;
        };
        let Some(input) = non_negative_number(cost.get("input")) else {
            continue;
        };
        let Some(output) = non_negative_number(cost.get("output")) else {
            continue;
        };
        let Some(cached_input) = optional_non_negative_number(cost.get("cache_read")) else {
            continue;
        };
        let price = RemotePrice {
            input,
            cached_input,
            output,
            long_context_tiers: parse_long_context_tiers(cost),
        };

        insert_model_aliases(&mut models, model_key, model_value, price);
    }

    if models.is_empty() {
        return Err("models.dev openai.models contains no token-priced models".to_string());
    }

    Ok(PricingCatalog { models, digest })
}

pub fn fetch_models_dev(etag: Option<&str>) -> Result<FetchOutcome, String> {
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("NerfTrack/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("unable to create models.dev client: {error}"))?;
    let mut request = client.get(MODELS_DEV_API_URL);
    if let Some(etag) = etag.filter(|value| !value.trim().is_empty()) {
        request = request.header(IF_NONE_MATCH, etag);
    }

    let response = request
        .send()
        .map_err(|error| format!("unable to fetch models.dev pricing: {error}"))?;
    let response_etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    if response.status() == StatusCode::NOT_MODIFIED {
        return Ok(FetchOutcome::NotModified {
            etag: response_etag.or_else(|| etag.map(ToOwned::to_owned)),
        });
    }
    if !response.status().is_success() {
        return Err(format!(
            "models.dev pricing request returned HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CATALOG_BYTES as u64)
    {
        return Err("models.dev pricing catalog is unexpectedly large".to_string());
    }

    let bytes = response
        .bytes()
        .map_err(|error| format!("unable to read models.dev pricing: {error}"))?;
    if bytes.len() > MAX_CATALOG_BYTES {
        return Err("models.dev pricing catalog is unexpectedly large".to_string());
    }
    let payload = String::from_utf8(bytes.to_vec())
        .map_err(|_| "models.dev pricing catalog was not UTF-8".to_string())?;
    let digest = format!("{:x}", Sha256::digest(payload.as_bytes()));
    let catalog = parse_catalog(&payload, Some(digest.clone()))?;

    Ok(FetchOutcome::Updated {
        payload,
        etag: response_etag,
        digest,
        catalog,
    })
}

fn insert_model_aliases(
    models: &mut HashMap<String, RemotePrice>,
    model_key: &str,
    model_value: &Value,
    price: RemotePrice,
) {
    let mut aliases = vec![model_key.to_string()];
    if let Some(model_id) = model_value.get("id").and_then(Value::as_str) {
        aliases.push(model_id.to_string());
    }
    for alias in aliases {
        let normalized = normalize_model_id(&alias);
        if normalized.is_empty() {
            continue;
        }
        models.insert(normalized.clone(), price.clone());
        if let Some(unprefixed) = normalized.strip_prefix("openai-") {
            models.insert(unprefixed.to_string(), price.clone());
        }
    }
}

fn non_negative_number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn parse_long_context_tiers(cost: &Value) -> Vec<RemotePriceTier> {
    if let Some(tiers) = cost.get("tiers").and_then(Value::as_array) {
        let mut parsed = tiers.iter().filter_map(parse_tier).collect::<Vec<_>>();
        parsed.sort_by_key(|tier| tier.threshold_tokens);
        if !parsed.is_empty() {
            return parsed;
        }
    }

    let Some(over_200k) = cost.get("context_over_200k") else {
        return Vec::new();
    };
    let Some(input) = non_negative_number(over_200k.get("input")) else {
        return Vec::new();
    };
    let Some(output) = non_negative_number(over_200k.get("output")) else {
        return Vec::new();
    };
    let Some(cached_input) = optional_non_negative_number(over_200k.get("cache_read")) else {
        return Vec::new();
    };
    vec![RemotePriceTier {
        threshold_tokens: 200_000,
        input,
        cached_input,
        output,
    }]
}

fn parse_tier(value: &Value) -> Option<RemotePriceTier> {
    let threshold = value
        .get("tier")
        .and_then(|tier| tier.get("size"))
        .and_then(Value::as_u64)
        .filter(|size| *size > 0)?;
    Some(RemotePriceTier {
        threshold_tokens: threshold,
        input: non_negative_number(value.get("input"))?,
        cached_input: optional_non_negative_number(value.get("cache_read"))?,
        output: non_negative_number(value.get("output"))?,
    })
}

fn optional_non_negative_number(value: Option<&Value>) -> Option<f64> {
    match value {
        None | Some(Value::Null) => Some(0.0),
        Some(value) => non_negative_number(Some(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_token_priced_openai_models_and_aliases() {
        let catalog = parse_catalog(
            r#"
            {
              "openai": {
                "models": {
                  "gpt-test": {
                    "id": "gpt-test",
                    "cost": {"input": 1, "cache_read": 0.1, "output": 2}
                  },
                  "image-test": {"cost": {"image": 4}}
                }
              },
              "anthropic": {
                "models": {
                  "claude-test": {"cost": {"input": 99, "output": 99}}
                }
              }
            }
            "#,
            Some("digest".into()),
        )
        .expect("catalog should parse");

        assert_eq!(catalog.digest.as_deref(), Some("digest"));
        assert_eq!(catalog.find("gpt-test").expect("model").input, 1.0);
        assert!(catalog.find("claude-test").is_none());
        assert!(catalog.find("image-test").is_none());
    }

    #[test]
    fn parses_context_tier_rates() {
        let catalog = parse_catalog(
            r#"
            {"openai":{"models":{
              "gpt-tier": {"cost": {
                "input": 5, "cache_read": 0.5, "output": 30,
                "tiers": [{
                  "tier": {"type": "context", "size": 272000},
                  "input": 10, "cache_read": 1, "output": 45
                }, {
                  "tier": {"type": "context", "size": 1000000},
                  "input": 12, "cache_read": 1.2, "output": 50
                }]
              }}
            }}}
            "#,
            None,
        )
        .expect("catalog should parse");
        let price = catalog.find("gpt-tier").expect("model");
        assert_eq!(price.long_context_tiers.len(), 2);
        assert_eq!(price.long_context_tiers[0].threshold_tokens, 272_000);
        assert_eq!(price.long_context_tiers[0].output, 45.0);
        assert_eq!(price.long_context_tiers[1].threshold_tokens, 1_000_000);
    }

    #[test]
    fn rejects_catalog_without_token_priced_openai_models() {
        let error = parse_catalog(
            r#"{"openai":{"models":{"image":{"cost":{"image":1}}}}}"#,
            None,
        )
        .expect_err("catalog should be rejected");
        assert!(error.contains("no token-priced"));
    }

    #[test]
    fn resolves_codex_auto_review_to_the_public_gpt_5_6_luna_catalog_entry() {
        let catalog = parse_catalog(
            r#"{"openai":{"models":{"gpt-5.6-luna":{"cost":{"input":0.2,"cache_read":0.02,"output":1.2}}}}}"#,
            None,
        )
        .expect("catalog should parse");
        let price = catalog.find(CODEX_AUTO_REVIEW_MODEL_ID).expect("model");
        assert_eq!(price.input, 0.2);
        assert_eq!(price.cached_input, 0.02);
        assert_eq!(price.output, 1.2);
        assert_eq!(
            canonical_api_model_id(CODEX_AUTO_REVIEW_MODEL_ID),
            "gpt-5.6-luna"
        );
    }
}
