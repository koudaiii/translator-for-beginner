//! `tfb` を実際に起動し、wiremock で立てた偽の Azure に向けて動作を確かめる。

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SECRET: &str = "test-secret-key";

fn tfb() -> Command {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("tfb"));
    cmd.env_clear();
    cmd
}

fn with_aoai(mut cmd: Command, server: &MockServer) -> Command {
    cmd.env("TFB_AOAI_ENDPOINT", format!("{}/openai/v1/", server.uri()))
        .env("TFB_AOAI_KEY", SECRET);
    cmd
}

fn with_translator(mut cmd: Command, server: &MockServer) -> Command {
    cmd.env("TFB_TRANSLATOR_ENDPOINT", server.uri())
        .env("TFB_TRANSLATOR_KEY", SECRET);
    cmd
}

fn completion(data: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "choices": [{ "finish_reason": "stop", "message": { "content": data.to_string() } }] }))
}

async fn mock_task(server: &MockServer, task: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .and(body_partial_json(
            json!({ "response_format": { "json_schema": { "name": task } } }),
        ))
        .respond_with(response)
        .expect(1)
        .mount(server)
        .await;
}

fn variants() -> Value {
    json!({ "chat": "Can't repro locally.", "email": "I cannot reproduce it.", "spoken": "So, I can't reproduce it.", "github": "Can't repro locally." })
}

fn corrections() -> Value {
    json!({ "corrections": [{ "before": "can not", "after": "cannot", "reason": "1 語で書きます。" }] })
}

#[tokio::test(flavor = "multi_thread")]
async fn translate_prints_translation_from_args() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/translate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "detectedLanguage": { "language": "en" }, "translations": [{ "text": "こんにちは。" }] }])))
        .expect(1)
        .mount(&server)
        .await;
    with_translator(tfb(), &server)
        .args(["translate", "Hello."])
        .assert()
        .success()
        .stdout("こんにちは。\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn translate_json_reads_stdin() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/translate"))
        .and(body_partial_json(json!([{ "Text": "Hello." }])))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "detectedLanguage": { "language": "en" }, "translations": [{ "text": "こんにちは。" }] }])))
        .expect(1)
        .mount(&server)
        .await;
    let output = with_translator(tfb(), &server)
        .args(["translate", "--json"])
        .write_stdin("  Hello.\n")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value,
        json!({ "translation": { "text": "こんにちは。", "detected": "en" }, "errors": {} })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fix_prints_corrections_and_variants() {
    let server = MockServer::start().await;
    mock_task(&server, "corrections", completion(corrections())).await;
    mock_task(&server, "variants", completion(variants())).await;
    with_aoai(tfb(), &server)
        .args(["fix", "I", "can", "not", "reproduce", "it."])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "■ 直した箇所\n  can not → cannot\n    1 語で書きます。\n",
        ))
        .stdout(predicate::str::contains("[GitHub]\nCan't repro locally.\n"))
        .stdout(predicate::str::contains("\u{1b}[").not());
}

#[tokio::test(flavor = "multi_thread")]
async fn explain_json_has_reading_and_grammar() {
    let server = MockServer::start().await;
    mock_task(
        &server,
        "reading",
        completion(json!({ "literal": "私は / 再現できない", "vocabulary": [] })),
    )
    .await;
    mock_task(
        &server,
        "grammar",
        completion(json!({ "grammar": [{ "point": "cannot", "explanation": "1 語", "examples": [{ "en": "I cannot.", "ja": "できない。" }, { "en": "We cannot.", "ja": "できない。" }] }] })),
    )
    .await;
    let output = with_aoai(tfb(), &server)
        .args(["explain", "--json", "I cannot reproduce it."])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["reading"]["literal"], "私は / 再現できない");
    assert_eq!(value["grammar"][0]["point"], "cannot");
    assert_eq!(value["errors"], json!({}));
}

#[tokio::test(flavor = "multi_thread")]
async fn one_failure_keeps_the_other_section_and_exits_1() {
    let server = MockServer::start().await;
    mock_task(
        &server,
        "corrections",
        ResponseTemplate::new(429)
            .set_body_json(json!({ "error": { "message": "Rate limit exceeded." } })),
    )
    .await;
    mock_task(&server, "variants", completion(variants())).await;
    with_aoai(tfb(), &server)
        .args(["fix", "I can not reproduce it."])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("■ 言い回し"))
        .stdout(predicate::str::contains("■ 直した箇所").not())
        .stderr("直した箇所: Azure OpenAI がエラーを返しました (429) Rate limit exceeded.\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn json_failure_goes_to_errors_not_stderr() {
    let server = MockServer::start().await;
    mock_task(&server, "corrections", completion(corrections())).await;
    mock_task(&server, "variants", ResponseTemplate::new(404).set_body_json(json!({ "error": { "code": "DeploymentNotFound", "message": "The API deployment for this resource does not exist." } }))).await;
    let assert = with_aoai(tfb(), &server)
        .args(["fix", "--json", "I can not reproduce it."])
        .assert()
        .code(1)
        .stderr("");
    let value: Value = serde_json::from_slice(&assert.get_output().stdout).unwrap();
    assert_eq!(value["variants"], Value::Null);
    assert_eq!(
        value["errors"]["variants"],
        "Azure OpenAI がエラーを返しました (404) The API deployment for this resource does not exist."
    );
    assert_eq!(value["corrections"][0]["after"], "cannot");
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_error_shows_message_without_key() {
    let server = MockServer::start().await;
    for task in ["reading", "grammar"] {
        mock_task(&server, task, ResponseTemplate::new(401).set_body_json(json!({ "error": { "code": "401", "message": "Access denied due to invalid subscription key." } }))).await;
    }
    with_aoai(tfb(), &server)
        .args(["explain", "Hello."])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("直訳と単語: Azure OpenAI がエラーを返しました (401) Access denied due to invalid subscription key.\n"))
        .stderr(predicate::str::contains(SECRET).not())
        .stdout("");
}

#[tokio::test(flavor = "multi_thread")]
async fn usage_errors_exit_2_without_calling_azure() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;

    with_aoai(tfb(), &server)
        .args(["fix", "これはペンです"])
        .assert()
        .code(2)
        .stderr("tfb: 英文を入力してください\n");
    with_aoai(tfb(), &server)
        .args(["explain", &"a".repeat(1001)])
        .assert()
        .code(2)
        .stderr("tfb: 解説・添削は 1000 文字までです。短くしてください\n");
    with_translator(tfb(), &server)
        .args(["translate", &"a".repeat(5001)])
        .assert()
        .code(2)
        .stderr("tfb: 入力が長すぎます (5001文字、上限5000文字)\n");
    with_aoai(tfb(), &server)
        .args(["fix", "   "])
        .assert()
        .code(2)
        .stderr("tfb: テキストを入力してください\n");
    tfb()
        .env("TFB_AOAI_KEY", SECRET)
        .args(["fix", "Hello."])
        .assert()
        .code(2)
        .stderr("tfb: 環境変数 TFB_AOAI_ENDPOINT を設定してください\n");
    tfb()
        .args(["translate", "Hello."])
        .assert()
        .code(2)
        .stderr("tfb: 環境変数 TFB_TRANSLATOR_KEY を設定してください\n");
}

#[test]
fn bad_arguments_exit_2() {
    tfb().args(["unknown"]).assert().code(2);
    tfb().assert().code(2);
}
