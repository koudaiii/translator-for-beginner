use std::io::Read;

pub const TRANSLATE_MAX_CHARS: usize = 5000;
pub const AI_MAX_CHARS: usize = 1000;

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("テキストを入力してください")]
    Empty,
    #[error(
        "英文を引数で渡すか、パイプで標準入力に渡してください（例: tfb fix I can not do it. / pbpaste | tfb fix）"
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

/// 引数があれば半角スペースでつなぎ、なければ標準入力を読む。前後の空白は取り除く。
pub fn read_text(
    args: &[String],
    mut stdin: impl Read,
    stdin_is_terminal: bool,
) -> Result<String, InputError> {
    let raw = if !args.is_empty() {
        args.join(" ")
    } else if stdin_is_terminal {
        return Err(InputError::Terminal);
    } else {
        let mut buf = String::new();
        stdin
            .read_to_string(&mut buf)
            .map_err(|e| InputError::Read(e.to_string()))?;
        buf
    };
    let text = raw.trim();
    if text.is_empty() {
        Err(InputError::Empty)
    } else {
        Ok(text.to_string())
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

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn joins_args_with_single_spaces_and_ignores_stdin() {
        let text = read_text(
            &args(&["I", "can", "not", "do", "it."]),
            "ignored".as_bytes(),
            false,
        )
        .unwrap();
        assert_eq!(text, "I can not do it.");
    }

    #[test]
    fn reads_stdin_when_no_args_and_trims() {
        let text = read_text(&[], "  Hello, world.\n\n".as_bytes(), false).unwrap();
        assert_eq!(text, "Hello, world.");
    }

    #[test]
    fn refuses_to_wait_on_terminal_stdin() {
        assert_eq!(
            read_text(&[], "".as_bytes(), true),
            Err(InputError::Terminal)
        );
    }

    #[test]
    fn rejects_blank_input() {
        assert_eq!(
            read_text(&args(&["  "]), "".as_bytes(), false),
            Err(InputError::Empty)
        );
        assert_eq!(
            read_text(&[], " \n".as_bytes(), false),
            Err(InputError::Empty)
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
