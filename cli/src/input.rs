use std::io::Read;

pub const TRANSLATE_MAX_CHARS: usize = 5000;
pub const AI_MAX_CHARS: usize = 1000;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("テキストを入力してください")]
    Empty,
    #[error(
        "英文を \"...\" で囲んで渡すか、パイプで標準入力に渡してください（例: tfb fix \"I can not do it.\" / pbpaste | tfb fix）"
    )]
    Terminal,
    #[error("入力が長すぎます ({0}文字、上限5000文字)")]
    TooLong(usize),
    #[error("解説・添削は 1000 文字までです。短くしてください")]
    AiTooLong,
    #[error("英文を入力してください")]
    NotEnglish,
    #[error("標準入力を読めませんでした: {0}")]
    Read(String),
}

/// 引数があればそれを、なければ標準入力を読む。入力は 1 行として扱い、改行を含む空白の連続を半角スペース 1 つにまとめる。
pub fn read_text(
    arg: Option<&str>,
    mut stdin: impl Read,
    stdin_is_terminal: bool,
) -> Result<String, InputError> {
    let raw = match arg {
        Some(arg) => arg.to_string(),
        None if stdin_is_terminal => return Err(InputError::Terminal),
        None => {
            let mut buf = String::new();
            stdin
                .read_to_string(&mut buf)
                .map_err(|e| InputError::Read(e.to_string()))?;
            buf
        }
    };
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        Err(InputError::Empty)
    } else {
        Ok(text)
    }
}

pub fn check_translate(text: &str) -> Result<(), InputError> {
    let chars = text.chars().count();
    if chars > TRANSLATE_MAX_CHARS {
        return Err(InputError::TooLong(chars));
    }
    Ok(())
}

pub fn check_ai(text: &str) -> Result<(), InputError> {
    if text.chars().count() > AI_MAX_CHARS {
        return Err(InputError::AiTooLong);
    }
    if has_kana(text) {
        return Err(InputError::NotEnglish);
    }
    Ok(())
}

pub fn has_kana(text: &str) -> bool {
    text.chars().any(|c| ('\u{3040}'..='\u{30FF}').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_single_argument_and_ignores_stdin() {
        let text = read_text(Some("I can not do it."), "ignored".as_bytes(), false).unwrap();
        assert_eq!(text, "I can not do it.");
    }

    #[test]
    fn accepts_hyphen_leading_text() {
        assert_eq!(
            read_text(Some("-5 degrees is cold."), "".as_bytes(), true).unwrap(),
            "-5 degrees is cold."
        );
    }

    #[test]
    fn reads_stdin_when_no_argument_and_trims() {
        let text = read_text(None, "  Hello, world.\n\n".as_bytes(), false).unwrap();
        assert_eq!(text, "Hello, world.");
    }

    #[test]
    fn joins_lines_and_collapses_whitespace() {
        let text = read_text(
            None,
            "I can not\nreproduce   it\r\n\tin my local.\n".as_bytes(),
            false,
        )
        .unwrap();
        assert_eq!(text, "I can not reproduce it in my local.");
        assert_eq!(
            read_text(Some("Hello,\n  world."), "".as_bytes(), false).unwrap(),
            "Hello, world."
        );
    }

    #[test]
    fn refuses_to_wait_on_terminal_stdin() {
        assert_eq!(
            read_text(None, "".as_bytes(), true),
            Err(InputError::Terminal)
        );
    }

    #[test]
    fn rejects_blank_input() {
        assert_eq!(
            read_text(Some("  "), "".as_bytes(), false),
            Err(InputError::Empty)
        );
        assert_eq!(
            read_text(None, " \n".as_bytes(), false),
            Err(InputError::Empty)
        );
    }

    #[test]
    fn terminal_message_shows_quoted_example() {
        assert_eq!(
            InputError::Terminal.to_string(),
            "英文を \"...\" で囲んで渡すか、パイプで標準入力に渡してください（例: tfb fix \"I can not do it.\" / pbpaste | tfb fix）"
        );
    }

    #[test]
    fn translate_limit_is_5000_chars() {
        assert_eq!(check_translate(&"a".repeat(5000)), Ok(()));
        assert_eq!(
            check_translate(&"a".repeat(5001)),
            Err(InputError::TooLong(5001))
        );
    }

    #[test]
    fn ai_limit_is_1000_chars() {
        assert_eq!(check_ai(&"a".repeat(1000)), Ok(()));
        assert_eq!(check_ai(&"a".repeat(1001)), Err(InputError::AiTooLong));
    }

    #[test]
    fn counts_multibyte_characters_as_one() {
        // 「é」は UTF-8 で 2 バイトだが 1 文字として数える
        assert_eq!(check_ai(&"é".repeat(1000)), Ok(()));
        assert_eq!(check_translate(&"漢".repeat(5000)), Ok(()));
    }

    #[test]
    fn detects_hiragana_and_katakana_only() {
        assert!(has_kana("これはペンです"));
        assert!(has_kana("カタカナ"));
        assert!(!has_kana("東京"));
        assert!(!has_kana("This is a pen."));
        assert_eq!(check_ai("これは pen です"), Err(InputError::NotEnglish));
    }

    #[test]
    fn messages_match_spec() {
        assert_eq!(
            InputError::TooLong(5001).to_string(),
            "入力が長すぎます (5001文字、上限5000文字)"
        );
        assert_eq!(
            InputError::AiTooLong.to_string(),
            "解説・添削は 1000 文字までです。短くしてください"
        );
    }
}
