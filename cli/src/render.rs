use crate::http::CallError;
use crate::tasks::{Corrections, Grammar, Reading, Variants};
use crate::translator::Translation;
use anstyle::{AnsiColor, Style};
use serde::Serialize;
use serde_json::{Map, Value, json};

const HEADING: Style = AnsiColor::Cyan.on_default().bold();
const LABEL: Style = Style::new().bold();
const VARIANT_LABELS: [(&str, &str); 4] = [
    ("chat", "チャット"),
    ("email", "メール"),
    ("spoken", "対面・通話"),
    ("github", "GitHub"),
];

fn heading(title: &str) -> String {
    format!("{HEADING}■ {title}{HEADING:#}\n")
}

fn bold(text: &str) -> String {
    format!("{LABEL}{text}{LABEL:#}")
}

pub fn translation_text(translation: &Translation) -> String {
    if translation.detected.as_deref() == Some("ja") {
        "すでに日本語です\n".to_string()
    } else {
        format!("{}\n", translation.text)
    }
}

pub fn explain_text(reading: Option<&Reading>, grammar: Option<&Grammar>) -> String {
    let mut blocks = Vec::new();
    if let Some(reading) = reading {
        blocks.push(format!("{}{}\n", heading("直訳"), reading.literal));
        if !reading.vocabulary.is_empty() {
            let lines: String = reading
                .vocabulary
                .iter()
                .map(|v| format!("  {} — {}\n", v.term, v.meaning))
                .collect();
            blocks.push(format!("{}{lines}", heading("単語・熟語")));
        }
    }
    if let Some(grammar) = grammar {
        let points: Vec<String> = grammar
            .grammar
            .iter()
            .map(|g| {
                let examples: String = g
                    .examples
                    .iter()
                    .map(|e| format!("    {}\n      {}\n", e.en, e.ja))
                    .collect();
                format!(
                    "{}\n  {}\n{examples}",
                    bold(&format!("● {}", g.point)),
                    g.explanation
                )
            })
            .collect();
        blocks.push(format!("{}{}", heading("文法ポイント"), points.join("\n")));
    }
    blocks.join("\n")
}

pub fn fix_text(corrections: Option<&Corrections>, variants: Option<&Variants>) -> String {
    let mut blocks = Vec::new();
    if let Some(corrections) = corrections {
        let body = if corrections.corrections.is_empty() {
            "  文法の誤りは見つかりませんでした\n".to_string()
        } else {
            corrections
                .corrections
                .iter()
                .map(|c| format!("  {} → {}\n    {}\n", c.before, c.after, c.reason))
                .collect()
        };
        blocks.push(format!("{}{body}", heading("直した箇所")));
    }
    if let Some(variants) = variants {
        let items: Vec<String> = VARIANT_LABELS
            .iter()
            .map(|(key, label)| {
                let text = match *key {
                    "chat" => &variants.chat,
                    "email" => &variants.email,
                    "spoken" => &variants.spoken,
                    _ => &variants.github,
                };
                format!("{}\n{text}\n", bold(&format!("[{label}]")))
            })
            .collect();
        blocks.push(format!("{}{}", heading("言い回し"), items.join("\n")));
    }
    blocks.join("\n")
}

/// 処理名ごとの結果を JSON にまとめる。失敗した処理は `null` にして `errors` に入れる。
pub struct JsonReport {
    fields: Map<String, Value>,
    errors: Map<String, Value>,
}

impl JsonReport {
    pub fn new() -> Self {
        Self {
            fields: Map::new(),
            errors: Map::new(),
        }
    }

    pub fn add<T: Serialize>(mut self, name: &str, result: Result<T, &CallError>) -> Self {
        match result {
            Ok(value) => {
                self.fields.insert(
                    name.to_string(),
                    serde_json::to_value(value).expect("結果を JSON にできませんでした"),
                );
            }
            Err(error) => {
                self.fields.insert(name.to_string(), Value::Null);
                self.errors
                    .insert(name.to_string(), json!(error.to_string()));
            }
        }
        self
    }

    pub fn finish(mut self) -> String {
        self.fields
            .insert("errors".to_string(), Value::Object(self.errors));
        format!("{}\n", Value::Object(self.fields))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{Correction, Example, GrammarPoint, Vocabulary};

    fn plain(text: &str) -> String {
        anstream::adapter::strip_str(text).to_string()
    }

    fn reading(vocabulary: Vec<Vocabulary>) -> Reading {
        Reading {
            literal: "私は / 知りたい".into(),
            vocabulary,
        }
    }

    fn grammar() -> Grammar {
        Grammar {
            grammar: vec![GrammarPoint {
                point: "cannot".into(),
                explanation: "1 語で書きます。".into(),
                examples: vec![
                    Example {
                        en: "I cannot go.".into(),
                        ja: "行けません。".into(),
                    },
                    Example {
                        en: "She cannot swim.".into(),
                        ja: "泳げません。".into(),
                    },
                ],
            }],
        }
    }

    fn variants() -> Variants {
        Variants {
            chat: "c".into(),
            email: "e".into(),
            spoken: "s".into(),
            github: "g".into(),
        }
    }

    #[test]
    fn translation_text_says_already_japanese() {
        assert_eq!(
            translation_text(&Translation {
                text: "訳".into(),
                detected: Some("en".into())
            }),
            "訳\n"
        );
        assert_eq!(
            translation_text(&Translation {
                text: "これ".into(),
                detected: Some("ja".into())
            }),
            "すでに日本語です\n"
        );
    }

    #[test]
    fn explain_text_layout() {
        let text = plain(&explain_text(
            Some(&reading(vec![Vocabulary {
                term: "reason".into(),
                meaning: "理由".into(),
            }])),
            Some(&grammar()),
        ));
        assert_eq!(
            text,
            "■ 直訳\n私は / 知りたい\n\n■ 単語・熟語\n  reason — 理由\n\n■ 文法ポイント\n● cannot\n  1 語で書きます。\n    I cannot go.\n      行けません。\n    She cannot swim.\n      泳げません。\n"
        );
    }

    #[test]
    fn explain_text_skips_empty_vocabulary_and_failed_sections() {
        let text = plain(&explain_text(Some(&reading(vec![])), None));
        assert_eq!(text, "■ 直訳\n私は / 知りたい\n");
    }

    #[test]
    fn fix_text_layout() {
        let corrections = Corrections {
            corrections: vec![Correction {
                before: "can not".into(),
                after: "cannot".into(),
                reason: "1 語".into(),
            }],
        };
        let text = plain(&fix_text(Some(&corrections), Some(&variants())));
        assert_eq!(
            text,
            "■ 直した箇所\n  can not → cannot\n    1 語\n\n■ 言い回し\n[チャット]\nc\n\n[メール]\ne\n\n[対面・通話]\ns\n\n[GitHub]\ng\n"
        );
    }

    #[test]
    fn fix_text_reports_no_errors() {
        let text = plain(&fix_text(
            Some(&Corrections {
                corrections: vec![],
            }),
            None,
        ));
        assert_eq!(text, "■ 直した箇所\n  文法の誤りは見つかりませんでした\n");
    }

    #[test]
    fn headings_are_colored_before_stripping() {
        assert!(explain_text(Some(&reading(vec![])), None).contains("\u{1b}["));
    }

    #[test]
    fn json_report_nulls_failures_and_lists_errors() {
        let failure = CallError::Unreadable;
        let report = JsonReport::new()
            .add("corrections", Err::<Corrections, _>(&failure))
            .add("variants", Ok(&variants()))
            .finish();
        let value: Value = serde_json::from_str(&report).unwrap();
        assert_eq!(value["corrections"], Value::Null);
        assert_eq!(value["variants"]["github"], "g");
        assert_eq!(
            value["errors"],
            json!({ "corrections": "結果を読み取れませんでした" })
        );
        assert!(report.ends_with('\n'));
    }

    #[test]
    fn json_report_has_empty_errors_on_success() {
        let report = JsonReport::new().add("variants", Ok(&variants())).finish();
        let value: Value = serde_json::from_str(&report).unwrap();
        assert_eq!(value["errors"], json!({}));
    }
}
