use crate::config::TranslatorConfig;
use crate::http::{CallError, Http};
use crate::input::has_kana;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Translation {
    pub text: String,
    pub detected: Option<String>,
    pub to: String,
}

pub fn parse_translation(response: &Value, to: &str) -> Option<Translation> {
    let item = response.get(0)?;
    Some(Translation {
        text: item.pointer("/translations/0/text")?.as_str()?.to_string(),
        detected: item
            .pointer("/detectedLanguage/language")
            .and_then(Value::as_str)
            .map(str::to_string),
        to: to.to_string(),
    })
}

/// 日本語なら英語へ、それ以外なら日本語へ翻訳する。
/// ひらがな・カタカナがあれば最初から英語へ訳す。漢字だけの日本語は、Translator が `ja` と判定したら英語で訳し直す。
pub async fn translate(
    http: &Http,
    config: &TranslatorConfig,
    text: &str,
) -> Result<Translation, CallError> {
    if has_kana(text) {
        return translate_to(http, config, text, "en").await;
    }
    let translation = translate_to(http, config, text, "ja").await?;
    if translation.detected.as_deref() == Some("ja") {
        return translate_to(http, config, text, "en").await;
    }
    Ok(translation)
}

async fn translate_to(
    http: &Http,
    config: &TranslatorConfig,
    text: &str,
    to: &str,
) -> Result<Translation, CallError> {
    let url = format!("{}/translate?api-version=3.0&to={to}", config.endpoint);
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
    parse_translation(&response, to).ok_or(CallError::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::TIMEOUT;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config(server: &MockServer) -> TranslatorConfig {
        TranslatorConfig {
            endpoint: server.uri(),
            key: "secret".into(),
            region: Some("japaneast".into()),
        }
    }

    fn response(detected: &str, text: &str, to: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!([{
            "detectedLanguage": { "language": detected, "score": 1.0 },
            "translations": [{ "text": text, "to": to }]
        }]))
    }

    async fn mock_to(server: &MockServer, to: &str, template: ResponseTemplate, times: u64) {
        Mock::given(method("POST"))
            .and(path("/translate"))
            .and(query_param("to", to))
            .respond_with(template)
            .expect(times)
            .mount(server)
            .await;
    }

    #[test]
    fn parses_text_and_detected_language() {
        let body = json!([{ "detectedLanguage": { "language": "en", "score": 1.0 }, "translations": [{ "text": "こんにちは", "to": "ja" }] }]);
        assert_eq!(
            parse_translation(&body, "ja"),
            Some(Translation {
                text: "こんにちは".into(),
                detected: Some("en".into()),
                to: "ja".into()
            })
        );
        assert_eq!(parse_translation(&json!([]), "ja"), None);
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
            .respond_with(response("en", "こんにちは。", "ja"))
            .expect(1)
            .mount(&server)
            .await;
        let translation = translate(&Http::new(TIMEOUT), &config(&server), "Hello.")
            .await
            .unwrap();
        assert_eq!(translation.text, "こんにちは。");
    }

    #[tokio::test]
    async fn english_is_translated_to_japanese() {
        let server = MockServer::start().await;
        mock_to(&server, "ja", response("en", "再現できません。", "ja"), 1).await;
        mock_to(&server, "en", response("ja", "unused", "en"), 0).await;
        let translation = translate(
            &Http::new(TIMEOUT),
            &config(&server),
            "I can not reproduce it.",
        )
        .await
        .unwrap();
        assert_eq!(
            translation,
            Translation {
                text: "再現できません。".into(),
                detected: Some("en".into()),
                to: "ja".into()
            }
        );
    }

    #[tokio::test]
    async fn kana_input_is_translated_to_english_in_one_call() {
        let server = MockServer::start().await;
        mock_to(
            &server,
            "en",
            response("ja", "I can't reproduce it locally.", "en"),
            1,
        )
        .await;
        mock_to(&server, "ja", response("ja", "unused", "ja"), 0).await;
        let translation = translate(
            &Http::new(TIMEOUT),
            &config(&server),
            "ローカルで再現できません",
        )
        .await
        .unwrap();
        assert_eq!(translation.to, "en");
        assert_eq!(translation.text, "I can't reproduce it locally.");
    }

    #[tokio::test]
    async fn kanji_only_japanese_is_retried_in_english() {
        let server = MockServer::start().await;
        mock_to(&server, "ja", response("ja", "東京大学", "ja"), 1).await;
        mock_to(
            &server,
            "en",
            response("ja", "University of Tokyo", "en"),
            1,
        )
        .await;
        let translation = translate(&Http::new(TIMEOUT), &config(&server), "東京大学")
            .await
            .unwrap();
        assert_eq!(
            translation,
            Translation {
                text: "University of Tokyo".into(),
                detected: Some("ja".into()),
                to: "en".into()
            }
        );
    }

    #[tokio::test]
    async fn kanji_only_chinese_stays_japanese() {
        let server = MockServer::start().await;
        mock_to(&server, "ja", response("zh-Hans", "愛してる", "ja"), 1).await;
        mock_to(&server, "en", response("zh-Hans", "unused", "en"), 0).await;
        let translation = translate(&Http::new(TIMEOUT), &config(&server), "我爱你")
            .await
            .unwrap();
        assert_eq!(translation.to, "ja");
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
        let err = translate(&Http::new(TIMEOUT), &config(&server), "Hello.")
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Azure Translator がエラーを返しました (401) The request is not authorized."
        );
    }
}
