use crate::config::AoaiConfig;
use crate::http::{CallError, Http};
use crate::tasks::Task;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub fn build_request(task: Task, text: &str, deployment: &str) -> Value {
    json!({
        "model": deployment,
        "reasoning_effort": "none",
        "messages": [
            { "role": "system", "content": task.system_prompt() },
            { "role": "user", "content": task.user_content(text) },
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": { "name": task.name(), "strict": true, "schema": task.schema() },
        },
    })
}

/// Chat Completions の応答から、モデルが返した JSON オブジェクトを取り出す。
pub fn parse_content(response: &Value) -> Result<Value, CallError> {
    let choice = response
        .pointer("/choices/0")
        .ok_or(CallError::Unreadable)?;
    if choice["finish_reason"] != "stop" {
        return Err(CallError::Unreadable);
    }
    let message = &choice["message"];
    if !message["refusal"].is_null() {
        return Err(CallError::Unreadable);
    }
    let content = message["content"].as_str().ok_or(CallError::Unreadable)?;
    let data: Value = serde_json::from_str(content).map_err(|_| CallError::Unreadable)?;
    if data.is_object() {
        Ok(data)
    } else {
        Err(CallError::Unreadable)
    }
}

pub async fn run<T: DeserializeOwned>(
    http: &Http,
    config: &AoaiConfig,
    task: Task,
    text: &str,
) -> Result<T, CallError> {
    let url = format!("{}/openai/v1/chat/completions", config.endpoint);
    let response = http
        .post_json(
            "Azure OpenAI",
            &url,
            &[("api-key", config.key.as_str())],
            &build_request(task, text, &config.deployment),
        )
        .await?;
    serde_json::from_value(parse_content(&response)?).map_err(|_| CallError::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::TIMEOUT;
    use crate::tasks::Variants;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn completion(content: Value, finish_reason: &str) -> Value {
        json!({ "choices": [{ "finish_reason": finish_reason, "message": { "content": content } }] })
    }

    #[test]
    fn builds_request_with_model_effort_and_strict_schema() {
        let body = build_request(Task::Variants, "I can not do it.", "gpt-6-luna");
        assert_eq!(body["model"], "gpt-6-luna");
        assert_eq!(body["reasoning_effort"], "none");
        assert_eq!(
            body["messages"][0],
            json!({ "role": "system", "content": Task::Variants.system_prompt() })
        );
        assert_eq!(
            body["messages"][1],
            json!({ "role": "user", "content": "I can not do it." })
        );
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["name"], "variants");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(
            body["response_format"]["json_schema"]["schema"],
            Task::Variants.schema()
        );
    }

    #[test]
    fn parses_stop_completion() {
        assert_eq!(
            parse_content(&completion(json!(r#"{"chat":"a"}"#), "stop")),
            Ok(json!({ "chat": "a" }))
        );
    }

    #[test]
    fn rejects_unusable_completions() {
        for response in [
            completion(json!(r#"{"chat":"a"}"#), "length"),
            completion(json!(null), "stop"),
            completion(json!("null"), "stop"),
            completion(json!(r#""text""#), "stop"),
            completion(json!(r#"{"chat":"#), "stop"),
            json!({ "choices": [{ "finish_reason": "stop", "message": { "content": null, "refusal": "I can't help." } }] }),
            json!({}),
        ] {
            assert_eq!(
                parse_content(&response),
                Err(CallError::Unreadable),
                "{response}"
            );
        }
    }

    #[tokio::test]
    async fn run_posts_to_v1_with_api_key_and_decodes_result() {
        let server = MockServer::start().await;
        let data = json!({ "chat": "a", "email": "b", "spoken": "c", "github": "d" });
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .and(header("api-key", "secret"))
            .and(body_partial_json(json!({ "model": "gpt-6-luna", "response_format": { "json_schema": { "name": "variants" } } })))
            .respond_with(ResponseTemplate::new(200).set_body_json(completion(json!(data.to_string()), "stop")))
            .expect(1)
            .mount(&server)
            .await;
        let config = AoaiConfig {
            endpoint: server.uri(),
            key: "secret".into(),
            deployment: "gpt-6-luna".into(),
        };
        let variants: Variants = run(&Http::new(TIMEOUT), &config, Task::Variants, "Hi.")
            .await
            .unwrap();
        assert_eq!(variants.chat, "a");
    }

    #[tokio::test]
    async fn run_rejects_result_of_wrong_shape() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(completion(json!(r#"{"chat":"a"}"#), "stop")),
            )
            .mount(&server)
            .await;
        let config = AoaiConfig {
            endpoint: server.uri(),
            key: "k".into(),
            deployment: "d".into(),
        };
        let result: Result<Variants, _> =
            run(&Http::new(TIMEOUT), &config, Task::Variants, "Hi.").await;
        assert_eq!(result, Err(CallError::Unreadable));
    }
}
