//! The `[scorer]` table of `approvals.toml` (RFC §2.5). It is read with the
//! rest of the file: one bad value rejects the whole file, as a bad rule does.
//! The endpoint and the key never reach an error message.

use std::time::Duration;

use serde::Deserialize;

use crate::model_approval::Risk;
use crate::policy::Category;

/// The model the local backend asks for when the file does not name one.
const DEFAULT_MODEL: &str = "clef-flash";
/// The longest the scorer may take, in ms. The RFC's fail-closed budget.
pub const MAX_TIMEOUT_MS: u64 = 200;
const DEFAULT_TIMEOUT_MS: u64 = MAX_TIMEOUT_MS;
const DEFAULT_LOW_BELOW: f64 = 0.2;
const DEFAULT_HIGH_FROM: f64 = 0.7;

/// Which scorer answers, and where. `None` is the default: nothing is asked.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Backend {
    #[default]
    None,
    /// An OpenAI-compatible server on loopback, plain HTTP.
    LocalHttp {
        host: String,
        port: u16,
        model: String,
    },
    /// A remote server. Parsed so the file can name it; this build has no TLS
    /// client, so it never answers and every consultation fails closed.
    Cloud {
        endpoint: String,
        api_key_env: String,
        model: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScorerConfig {
    pub backend: Backend,
    pub timeout: Duration,
    /// Categories a `low` score may approve. Anything else goes to the lead.
    pub auto_ok_if_low: Vec<Category>,
    /// Scores below this are `low`; from `high_from` up they are `high`.
    pub low_below: f64,
    pub high_from: f64,
}

impl Default for ScorerConfig {
    fn default() -> Self {
        Self {
            backend: Backend::None,
            timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            auto_ok_if_low: Vec::new(),
            low_below: DEFAULT_LOW_BELOW,
            high_from: DEFAULT_HIGH_FROM,
        }
    }
}

impl ScorerConfig {
    /// The label of a score under the thresholds of the file.
    pub fn label(&self, risk_score: f64) -> Risk {
        if risk_score < self.low_below {
            Risk::Low
        } else if risk_score >= self.high_from {
            Risk::High
        } else {
            Risk::Medium
        }
    }

    /// The model and the prompt revision, as the audit names the scorer.
    /// `None` when there is no scorer to name.
    pub fn version(&self) -> Option<String> {
        match &self.backend {
            Backend::None => None,
            Backend::LocalHttp { model, .. } | Backend::Cloud { model, .. } => {
                Some(format!("{model}@{}", super::PROMPT_VERSION))
            }
        }
    }

    pub fn auto_ok(&self, category: Category) -> bool {
        self.auto_ok_if_low.contains(&category)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawScorer {
    backend: String,
    endpoint: Option<String>,
    model: Option<String>,
    api_key_env: Option<String>,
    timeout_ms: Option<u64>,
    #[serde(default)]
    auto_ok_if_low: Vec<String>,
    low_below: Option<f64>,
    high_from: Option<f64>,
}

impl RawScorer {
    pub fn into_config(self) -> Result<ScorerConfig, String> {
        let model = self.model.unwrap_or_else(|| DEFAULT_MODEL.to_string());
        if model.trim().is_empty() {
            return Err("model is empty".to_string());
        }
        let backend = match self.backend.as_str() {
            "none" => Backend::None,
            "local_http" => {
                let endpoint = self.endpoint.ok_or("local_http needs an endpoint")?;
                let (host, port) = local_endpoint(&endpoint)
                    .ok_or("local_http endpoint must be http://127.0.0.1:<port> or http://localhost:<port>")?;
                Backend::LocalHttp { host, port, model }
            }
            "cloud" => {
                let endpoint = self.endpoint.ok_or("cloud needs an endpoint")?;
                if !endpoint.starts_with("https://") {
                    return Err("cloud endpoint must be https://".to_string());
                }
                let api_key_env = self.api_key_env.ok_or("cloud needs api_key_env")?;
                if api_key_env.trim().is_empty() {
                    return Err("api_key_env is empty".to_string());
                }
                Backend::Cloud {
                    endpoint,
                    api_key_env,
                    model,
                }
            }
            other => {
                return Err(format!(
                    "backend `{other}` is not none, local_http or cloud"
                ))
            }
        };
        let timeout_ms = self.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        if !(1..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(format!("timeout_ms must be between 1 and {MAX_TIMEOUT_MS}"));
        }
        let auto_ok_if_low = self
            .auto_ok_if_low
            .iter()
            .map(|name| {
                Category::from_config(name)
                    .filter(|category| !category.never_delegate())
                    .ok_or_else(|| format!("auto_ok_if_low `{name}` is not a delegable category"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let low_below = self.low_below.unwrap_or(DEFAULT_LOW_BELOW);
        let high_from = self.high_from.unwrap_or(DEFAULT_HIGH_FROM);
        if !(0.0 <= low_below && low_below < high_from && high_from <= 1.0) {
            return Err("thresholds must satisfy 0 <= low_below < high_from <= 1".to_string());
        }
        Ok(ScorerConfig {
            backend,
            timeout: Duration::from_millis(timeout_ms),
            auto_ok_if_low,
            low_below,
            high_from,
        })
    }
}

/// `http://127.0.0.1:<port>` or `http://localhost:<port>`, with no path: the
/// local backend appends its own route.
fn local_endpoint(endpoint: &str) -> Option<(String, u16)> {
    let rest = endpoint.strip_prefix("http://")?;
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    if authority.contains('/') {
        return None;
    }
    let (host, port) = authority.rsplit_once(':')?;
    let port = port.parse().ok()?;
    matches!(host, "127.0.0.1" | "localhost").then(|| (host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<ScorerConfig, String> {
        toml::from_str::<RawScorer>(text)
            .map_err(|error| error.to_string())?
            .into_config()
    }

    #[test]
    fn no_table_means_no_scorer() {
        let config = parse("backend = \"none\"").expect("config");
        assert_eq!(config.backend, Backend::None);
        assert_eq!(config.version(), None);
    }

    #[test]
    fn the_local_backend_takes_loopback_only() {
        let config = parse(
            "backend = \"local_http\"\nendpoint = \"http://127.0.0.1:8765\"\nauto_ok_if_low = [\"read\", \"test\"]",
        )
        .expect("config");
        assert_eq!(
            config.backend,
            Backend::LocalHttp {
                host: "127.0.0.1".to_string(),
                port: 8765,
                model: DEFAULT_MODEL.to_string(),
            }
        );
        assert!(config.auto_ok(Category::Read));
        assert!(!config.auto_ok(Category::Build));
        assert_eq!(config.version().as_deref(), Some("clef-flash@p1"));
        assert!(parse("backend = \"local_http\"\nendpoint = \"http://10.0.0.5:8765\"").is_err());
    }

    #[test]
    fn the_timeout_cannot_exceed_the_fail_closed_budget() {
        assert!(parse("backend = \"none\"\ntimeout_ms = 201").is_err());
        assert!(parse("backend = \"none\"\ntimeout_ms = 0").is_err());
        assert_eq!(
            parse("backend = \"none\"").expect("config").timeout,
            Duration::from_millis(200)
        );
    }

    #[test]
    fn a_money_category_cannot_be_auto_approved() {
        assert!(parse("backend = \"none\"\nauto_ok_if_low = [\"money\"]").is_err());
    }

    #[test]
    fn cloud_needs_https_and_names_its_key_without_holding_it() {
        let config = parse(
            "backend = \"cloud\"\nendpoint = \"https://api.example.test/v1\"\napi_key_env = \"HN_SCORER_KEY\"",
        )
        .expect("config");
        assert!(
            matches!(config.backend, Backend::Cloud { ref api_key_env, .. } if api_key_env == "HN_SCORER_KEY")
        );
        assert!(
            parse("backend = \"cloud\"\nendpoint = \"http://x.test\"\napi_key_env = \"K\"")
                .is_err()
        );
    }

    #[test]
    fn labels_follow_the_thresholds() {
        let config = ScorerConfig::default();
        assert_eq!(config.label(0.05), Risk::Low);
        assert_eq!(config.label(0.4), Risk::Medium);
        assert_eq!(config.label(0.9), Risk::High);
    }
}
