pub const DEFAULT_DEPLOYMENT: &str = "gpt-6-luna";
pub const DEFAULT_TRANSLATOR_ENDPOINT: &str = "https://api.cognitive.microsofttranslator.com";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslatorConfig {
    pub endpoint: String,
    pub key: String,
    pub region: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AoaiConfig {
    pub endpoint: String,
    pub key: String,
    pub deployment: String,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("環境変数 {} を設定してください", .0.join(", "))]
    Missing(Vec<&'static str>),
    #[error("環境変数 {0} は http:// か https:// で始まる URL にしてください")]
    InvalidUrl(&'static str),
}

/// 実際の環境変数を読む。前後の空白を除き、空なら未設定として扱う。
pub fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn lookup(get: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    get(name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 前後の空白、末尾の `/`、末尾の `/openai/v1` を取り除く。
pub fn normalize_endpoint(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    trimmed
        .strip_suffix("/openai/v1")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string()
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

pub fn translator_config(
    get: impl Fn(&str) -> Option<String>,
) -> Result<TranslatorConfig, ConfigError> {
    let key = lookup(&get, "TFB_TRANSLATOR_KEY")
        .ok_or(ConfigError::Missing(vec!["TFB_TRANSLATOR_KEY"]))?;
    let endpoint = lookup(&get, "TFB_TRANSLATOR_ENDPOINT")
        .map(|v| v.trim_end_matches('/').to_string())
        .unwrap_or_else(|| DEFAULT_TRANSLATOR_ENDPOINT.to_string());
    if !is_http_url(&endpoint) {
        return Err(ConfigError::InvalidUrl("TFB_TRANSLATOR_ENDPOINT"));
    }
    Ok(TranslatorConfig {
        endpoint,
        key,
        region: lookup(&get, "TFB_TRANSLATOR_REGION"),
    })
}

pub fn aoai_config(get: impl Fn(&str) -> Option<String>) -> Result<AoaiConfig, ConfigError> {
    let endpoint = lookup(&get, "TFB_AOAI_ENDPOINT");
    let key = lookup(&get, "TFB_AOAI_KEY");
    let missing: Vec<&'static str> = [
        ("TFB_AOAI_ENDPOINT", endpoint.is_none()),
        ("TFB_AOAI_KEY", key.is_none()),
    ]
    .into_iter()
    .filter(|(_, absent)| *absent)
    .map(|(name, _)| name)
    .collect();
    if !missing.is_empty() {
        return Err(ConfigError::Missing(missing));
    }
    let endpoint = normalize_endpoint(&endpoint.unwrap());
    if !is_http_url(&endpoint) {
        return Err(ConfigError::InvalidUrl("TFB_AOAI_ENDPOINT"));
    }
    Ok(AoaiConfig {
        endpoint,
        key: key.unwrap(),
        deployment: lookup(&get, "TFB_AOAI_DEPLOYMENT")
            .unwrap_or_else(|| DEFAULT_DEPLOYMENT.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn aoai_lists_every_missing_variable() {
        let err = aoai_config(env(&[])).unwrap_err();
        assert_eq!(
            err,
            ConfigError::Missing(vec!["TFB_AOAI_ENDPOINT", "TFB_AOAI_KEY"])
        );
        assert_eq!(
            err.to_string(),
            "環境変数 TFB_AOAI_ENDPOINT, TFB_AOAI_KEY を設定してください"
        );
    }

    #[test]
    fn blank_values_count_as_missing() {
        let err = aoai_config(env(&[
            ("TFB_AOAI_ENDPOINT", "https://x.openai.azure.com"),
            ("TFB_AOAI_KEY", "  "),
        ]))
        .unwrap_err();
        assert_eq!(err, ConfigError::Missing(vec!["TFB_AOAI_KEY"]));
    }

    #[test]
    fn aoai_defaults_deployment_and_normalizes_endpoint() {
        let cfg = aoai_config(env(&[
            (
                "TFB_AOAI_ENDPOINT",
                " https://x.openai.azure.com/openai/v1/ ",
            ),
            ("TFB_AOAI_KEY", "k"),
        ]))
        .unwrap();
        assert_eq!(
            cfg,
            AoaiConfig {
                endpoint: "https://x.openai.azure.com".into(),
                key: "k".into(),
                deployment: "gpt-6-luna".into()
            }
        );
    }

    #[test]
    fn normalize_endpoint_handles_each_suffix() {
        assert_eq!(
            normalize_endpoint("https://x.openai.azure.com/"),
            "https://x.openai.azure.com"
        );
        assert_eq!(
            normalize_endpoint("https://x.openai.azure.com/openai/v1"),
            "https://x.openai.azure.com"
        );
        assert_eq!(
            normalize_endpoint("https://x.openai.azure.com/openai/v1/"),
            "https://x.openai.azure.com"
        );
        assert_eq!(
            normalize_endpoint("https://x.openai.azure.com"),
            "https://x.openai.azure.com"
        );
    }

    #[test]
    fn rejects_non_http_endpoints() {
        let err = aoai_config(env(&[
            ("TFB_AOAI_ENDPOINT", "x.openai.azure.com"),
            ("TFB_AOAI_KEY", "k"),
        ]))
        .unwrap_err();
        assert_eq!(err, ConfigError::InvalidUrl("TFB_AOAI_ENDPOINT"));
        let err = translator_config(env(&[
            ("TFB_TRANSLATOR_KEY", "k"),
            ("TFB_TRANSLATOR_ENDPOINT", "ftp://x"),
        ]))
        .unwrap_err();
        assert_eq!(err, ConfigError::InvalidUrl("TFB_TRANSLATOR_ENDPOINT"));
    }

    #[test]
    fn translator_defaults_endpoint_and_optional_region() {
        let cfg = translator_config(env(&[("TFB_TRANSLATOR_KEY", "k")])).unwrap();
        assert_eq!(
            cfg,
            TranslatorConfig {
                endpoint: DEFAULT_TRANSLATOR_ENDPOINT.into(),
                key: "k".into(),
                region: None
            }
        );
        let cfg = translator_config(env(&[
            ("TFB_TRANSLATOR_KEY", "k"),
            ("TFB_TRANSLATOR_REGION", "japaneast"),
            ("TFB_TRANSLATOR_ENDPOINT", "http://127.0.0.1:9/"),
        ]))
        .unwrap();
        assert_eq!(cfg.endpoint, "http://127.0.0.1:9");
        assert_eq!(cfg.region.as_deref(), Some("japaneast"));
    }

    #[test]
    fn translator_requires_key() {
        assert_eq!(
            translator_config(env(&[])).unwrap_err(),
            ConfigError::Missing(vec!["TFB_TRANSLATOR_KEY"])
        );
    }
}
