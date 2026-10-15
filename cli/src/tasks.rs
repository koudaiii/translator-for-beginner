//! Azure OpenAI に依頼する 4 処理のプロンプト、スキーマ、結果の型。
//! プロンプトは拡張機能の `ai.js` の `AI_TASKS` と一字一句同じに保つ（`tests/prompts-sync.test.mjs` で照合する）。

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const READING_PROMPT: &str = "あなたは英語初心者の日本人に教える英語教師です。与えられた英文について、日本語で簡潔に解説してください。\n- literal: 英語の語順のまま、意味のまとまりごとに「 / 」で区切った日本語の直訳。英単語は書かない。例: 「私は / ずっと住んでいる / 東京に / 〜以来 / 私が / 卒業した / 大学を」\n- vocabulary: 初心者がつまずきそうな単語・熟語を最大5個。固有名詞や中学1年レベルの基本語は入れない。meaning は文中での意味。";
pub const GRAMMAR_PROMPT: &str = "あなたは英語初心者の日本人に教える英語教師です。与えられた英文について、日本語で簡潔に解説してください。\n- grammar: 文中の重要な文法項目を1〜3個。point は項目名、explanation は2〜3文の説明、examples には同じ文法を使った短い英文（en）と和訳（ja）を必ず2つ入れる。空にしない。";
pub const CORRECTIONS_PROMPT: &str = "あなたは英語初心者の日本人が書いた英文を直す英語の編集者です。入力の英文の文法・語法の誤りを1つずつ挙げる。before は元の表現、after は直した表現、reason は初心者向けの短い日本語の理由。誤りがなければ空配列にする（言い回しの好みは誤りに含めない）。";
pub const VARIANTS_PROMPT: &str = "あなたは英語初心者の日本人が書いた英文を直す英語の編集者です。入力の英文の文法・語法の誤りを直し、意味を変えずに、次の4つの用途に合う英文をそれぞれ1つ書いてください。用途ごとに語調・長さ・言い回しをはっきり変え、同じ文を繰り返さない。情報を足したり削ったりしない。\n- chat: Slack や Teams のチャット。短く、フレンドリーな口語。短縮形を使い、文を削ってよい（例: \"Checked your PR — looks good!\"）。挨拶や署名は付けない。\n- email: 仕事のメール本文。完全な文で丁寧に。短縮形を避け、丁寧な定型表現を使う（例: \"I apologize for the delayed response.\"）。入力に挨拶や署名がなければ付けない。\n- spoken: 対面や通話で口に出して言う英語。言いやすい短い文に分け、自然なつなぎ言葉を使う（例: \"So, ...\", \"Actually, ...\"）。\n- github: Issue や Pull Request のコメント。簡潔で明確、中立的な技術文。開発者の慣用表現を使ってよい（例: LGTM, repro）。コード・ファイル名・コマンドはバッククォートで囲む。";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Reading,
    Grammar,
    Corrections,
    Variants,
}

impl Task {
    #[cfg(test)]
    pub const ALL: [Task; 4] = [
        Task::Reading,
        Task::Grammar,
        Task::Corrections,
        Task::Variants,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Task::Reading => "reading",
            Task::Grammar => "grammar",
            Task::Corrections => "corrections",
            Task::Variants => "variants",
        }
    }

    pub fn system_prompt(self) -> &'static str {
        match self {
            Task::Reading => READING_PROMPT,
            Task::Grammar => GRAMMAR_PROMPT,
            Task::Corrections => CORRECTIONS_PROMPT,
            Task::Variants => VARIANTS_PROMPT,
        }
    }

    pub fn user_content(self, text: &str) -> String {
        match self {
            Task::Reading | Task::Grammar => format!("英文: {text}"),
            Task::Corrections | Task::Variants => text.to_string(),
        }
    }

    pub fn schema(self) -> Value {
        let string = json!({ "type": "string" });
        match self {
            Task::Reading => object(&[
                ("literal", string.clone()),
                (
                    "vocabulary",
                    array(
                        object(&[("term", string.clone()), ("meaning", string)]),
                        None,
                        Some(5),
                    ),
                ),
            ]),
            Task::Grammar => object(&[(
                "grammar",
                array(
                    object(&[
                        ("point", string.clone()),
                        ("explanation", string.clone()),
                        (
                            "examples",
                            array(
                                object(&[("en", string.clone()), ("ja", string)]),
                                Some(2),
                                Some(2),
                            ),
                        ),
                    ]),
                    Some(1),
                    Some(3),
                ),
            )]),
            Task::Corrections => object(&[(
                "corrections",
                array(
                    object(&[
                        ("before", string.clone()),
                        ("after", string.clone()),
                        ("reason", string),
                    ]),
                    None,
                    Some(5),
                ),
            )]),
            Task::Variants => object(&[
                ("chat", string.clone()),
                ("email", string.clone()),
                ("spoken", string.clone()),
                ("github", string),
            ]),
        }
    }
}

fn object(properties: &[(&str, Value)]) -> Value {
    let required: Vec<&str> = properties.iter().map(|(name, _)| *name).collect();
    let properties: serde_json::Map<String, Value> = properties
        .iter()
        .map(|(name, schema)| (name.to_string(), schema.clone()))
        .collect();
    json!({ "type": "object", "additionalProperties": false, "required": required, "properties": properties })
}

fn array(items: Value, min: Option<u32>, max: Option<u32>) -> Value {
    let mut schema = json!({ "type": "array", "items": items });
    if let Some(min) = min {
        schema["minItems"] = json!(min);
    }
    if let Some(max) = max {
        schema["maxItems"] = json!(max);
    }
    schema
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading {
    pub literal: String,
    pub vocabulary: Vec<Vocabulary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vocabulary {
    pub term: String,
    pub meaning: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grammar {
    pub grammar: Vec<GrammarPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrammarPoint {
    pub point: String,
    pub explanation: String,
    pub examples: Vec<Example>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Example {
    pub en: String,
    pub ja: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corrections {
    pub corrections: Vec<Correction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Correction {
    pub before: String,
    pub after: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variants {
    pub chat: String,
    pub email: String,
    pub spoken: String,
    pub github: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_strict(schema: &Value, path: &str) {
        match schema["type"].as_str() {
            Some("object") => {
                assert_eq!(
                    schema["additionalProperties"],
                    json!(false),
                    "{path} additionalProperties"
                );
                let mut required: Vec<&str> = schema["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect();
                let mut keys: Vec<&str> = schema["properties"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect();
                required.sort();
                keys.sort();
                assert_eq!(required, keys, "{path} required");
                for (key, child) in schema["properties"].as_object().unwrap() {
                    assert_strict(child, &format!("{path}.{key}"));
                }
            }
            Some("array") => assert_strict(&schema["items"], &format!("{path}[]")),
            _ => {}
        }
    }

    #[test]
    fn every_schema_meets_strict_mode() {
        for task in Task::ALL {
            assert_strict(&task.schema(), task.name());
        }
    }

    #[test]
    fn schemas_carry_item_limits() {
        assert_eq!(
            Task::Reading.schema()["properties"]["vocabulary"]["maxItems"],
            json!(5)
        );
        let grammar = &Task::Grammar.schema()["properties"]["grammar"];
        assert_eq!(
            (grammar["minItems"].clone(), grammar["maxItems"].clone()),
            (json!(1), json!(3))
        );
        let examples = &grammar["items"]["properties"]["examples"];
        assert_eq!(
            (examples["minItems"].clone(), examples["maxItems"].clone()),
            (json!(2), json!(2))
        );
        assert_eq!(
            Task::Corrections.schema()["properties"]["corrections"]["maxItems"],
            json!(5)
        );
    }

    #[test]
    fn explain_tasks_prefix_user_content() {
        assert_eq!(Task::Reading.user_content("Hi."), "英文: Hi.");
        assert_eq!(Task::Grammar.user_content("Hi."), "英文: Hi.");
        assert_eq!(Task::Corrections.user_content("Hi."), "Hi.");
        assert_eq!(Task::Variants.user_content("Hi."), "Hi.");
    }

    #[test]
    fn result_types_match_their_schemas() {
        let reading: Reading = serde_json::from_value(
            json!({"literal": "x", "vocabulary": [{"term": "a", "meaning": "b"}]}),
        )
        .unwrap();
        assert_eq!(reading.vocabulary[0].term, "a");
        let variants: Variants = serde_json::from_value(
            json!({"chat": "a", "email": "b", "spoken": "c", "github": "d"}),
        )
        .unwrap();
        assert_eq!(variants.github, "d");
        assert!(serde_json::from_value::<Variants>(json!({"chat": "a"})).is_err());
    }
}
