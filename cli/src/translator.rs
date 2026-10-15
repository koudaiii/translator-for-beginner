use crate::config::TranslatorConfig;
use crate::http::{CallError, Http};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Translation {
    pub text: String,
    pub detected: Option<String>,
}

pub fn parse_translation(response: &Value) -> Option<Translation> {
    let item = response.get(0)?;
    Some(Translation {
        text: item.pointer("/translations/0/text")?.as_str()?.to_string(),
        detected: item
            .pointer("/detectedLanguage/language")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub async fn translate(
    http: &Http,
    config: &TranslatorConfig,
    text: &str,
) -> Result<Translation, CallError> {
    let url = format!("{}/translate?api-version=3.0&to=ja", config.endpoint);
    let mut headers = vec![("Ocp-Apim-Subscription-Key", config.key.as_str())];
    if let Some(region) = &config.region {
        headers.push(("Ocp-Apim-Subscription-Region", region.as_str()));
    }
    let response = http
        .post_json(
            "Azure Translator",
            &url,
            &headers,
            &json!([{ "Text": text }]),
        )
        .await?;
    parse_translation(&response).ok_or(CallError::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::TIMEOUT;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn parses_text_and_detected_language() {
        let response = json!([{ "detectedLanguage": { "language": "en", "score": 1.0 }, "translations": [{ "text": "こんにちは", "to": "ja" }] }]);
        assert_eq!(
            parse_translation(&response),
            Some(Translation {
                text: "こんにちは".into(),
                detected: Some("en".into())
            })
        );
        assert_eq!(parse_translation(&json!([])), None);
    }

    #[tokio::test]
    async fn sends_key_region_and_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/translate"))
            .and(query_param("api-version", "3.0"))
            .and(query_param("to", "ja"))
            .and(header("Ocp-Apim-Subscription-Key", "secret"))
            .and(header("Ocp-Apim-Subscription-Region", "japaneast"))
            .and(body_json(json!([{ "Text": "Hello." }])))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "detectedLanguage": { "language": "en" }, "translations": [{ "text": "こんにちは。" }] }])))
            .expect(1)
            .mount(&server)
            .await;
        let config = TranslatorConfig {
            endpoint: server.uri(),
            key: "secret".into(),
            region: Some("japaneast".into()),
        };
        let translation = translate(&Http::new(TIMEOUT), &config, "Hello.")
            .await
            .unwrap();
        assert_eq!(translation.text, "こんにちは。");
    }

    #[tokio::test]
    async fn reports_translator_errors_with_service_name() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(
                json!({ "error": { "code": 401000, "message": "The request is not authorized." } }),
            ))
            .mount(&server)
            .await;
        let config = TranslatorConfig {
            endpoint: server.uri(),
            key: "bad".into(),
            region: None,
        };
        let err = translate(&Http::new(TIMEOUT), &config, "Hello.")
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Azure Translator がエラーを返しました (401) The request is not authorized."
        );
    }
}
