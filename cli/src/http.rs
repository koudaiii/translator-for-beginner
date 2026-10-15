use serde_json::Value;
use std::time::Duration;

pub const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_DETAIL_CHARS: usize = 300;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    #[error("{service} がエラーを返しました ({status}) {message}")]
    Status {
        service: &'static str,
        status: u16,
        message: String,
    },
    #[error("通信に失敗しました: {0} 秒以内に応答がありませんでした")]
    Timeout(String),
    #[error("通信に失敗しました: {0}")]
    Network(String),
    #[error("結果を読み取れませんでした")]
    Unreadable,
}

/// エラー応答の本文から表示用のメッセージを取り出す。JSON の `error.message` を優先し、なければ本文を 300 文字に切り詰める。
pub fn error_message(body: &str) -> String {
    if let Ok(value) = serde_json::from_str::<Value>(body)
        && let Some(message) = value.pointer("/error/message").and_then(Value::as_str)
    {
        return message.to_string();
    }
    let body = body.trim();
    if body.chars().count() > MAX_DETAIL_CHARS {
        format!(
            "{}…",
            body.chars().take(MAX_DETAIL_CHARS).collect::<String>()
        )
    } else {
        body.to_string()
    }
}

fn seconds(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        duration.as_secs().to_string()
    } else {
        format!("{:.1}", duration.as_secs_f64())
    }
}

pub struct Http {
    client: reqwest::Client,
    timeout: Duration,
}

impl Http {
    pub fn new(timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("HTTP クライアントを作成できませんでした");
        Self { client, timeout }
    }

    /// JSON を POST し、成功時は応答の JSON を返す。
    pub async fn post_json(
        &self,
        service: &'static str,
        url: &str,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> Result<Value, CallError> {
        let mut request = self.client.post(url).json(body);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request.send().await.map_err(|e| self.call_error(e))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CallError::Status {
                service,
                status: status.as_u16(),
                message: error_message(&body),
            });
        }
        response.json::<Value>().await.map_err(|e| {
            if e.is_timeout() {
                self.call_error(e)
            } else {
                CallError::Unreadable
            }
        })
    }

    fn call_error(&self, error: reqwest::Error) -> CallError {
        if error.is_timeout() {
            CallError::Timeout(seconds(self.timeout))
        } else {
            CallError::Network(error.without_url().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn extracts_error_message_from_json() {
        assert_eq!(
            error_message(r#"{"error":{"code":"401","message":"Access denied"}}"#),
            "Access denied"
        );
    }

    #[test]
    fn truncates_non_json_body_to_300_chars() {
        let message = error_message(&"x".repeat(500));
        assert_eq!(message.chars().count(), 301);
        assert!(message.ends_with('…'));
        assert_eq!(error_message(" short "), "short");
    }

    #[test]
    fn formats_timeout_seconds() {
        assert_eq!(
            CallError::Timeout(seconds(TIMEOUT)).to_string(),
            "通信に失敗しました: 30 秒以内に応答がありませんでした"
        );
        assert_eq!(seconds(Duration::from_millis(200)), "0.2");
    }

    #[tokio::test]
    async fn posts_json_with_headers_and_returns_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/echo"))
            .and(header("api-key", "secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .expect(1)
            .mount(&server)
            .await;
        let http = Http::new(TIMEOUT);
        let value = http
            .post_json(
                "Test",
                &format!("{}/echo", server.uri()),
                &[("api-key", "secret")],
                &json!({}),
            )
            .await
            .unwrap();
        assert_eq!(value, json!({"ok": true}));
    }

    #[tokio::test]
    async fn maps_http_errors_to_status_with_message() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_json(json!({"error": {"message": "Rate limit exceeded"}})),
            )
            .mount(&server)
            .await;
        let err = Http::new(TIMEOUT)
            .post_json("Azure OpenAI", &server.uri(), &[], &json!({}))
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Azure OpenAI がエラーを返しました (429) Rate limit exceeded"
        );
    }

    #[tokio::test]
    async fn times_out_slow_responses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(1)))
            .mount(&server)
            .await;
        let err = Http::new(Duration::from_millis(200))
            .post_json("Test", &server.uri(), &[], &json!({}))
            .await
            .unwrap_err();
        assert_eq!(err, CallError::Timeout("0.2".into()));
    }

    #[tokio::test]
    async fn non_json_success_body_is_unreadable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let err = Http::new(TIMEOUT)
            .post_json("Test", &server.uri(), &[], &json!({}))
            .await
            .unwrap_err();
        assert_eq!(err, CallError::Unreadable);
    }

    #[tokio::test]
    async fn connection_failure_is_network_error() {
        let err = Http::new(TIMEOUT)
            .post_json("Test", "http://127.0.0.1:9/", &[], &json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, CallError::Network(_)), "{err:?}");
        assert!(err.to_string().starts_with("通信に失敗しました: "));
    }
}
