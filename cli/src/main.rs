mod aoai;
mod config;
mod http;
mod input;
mod render;
mod tasks;
mod translator;

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use http::{CallError, Http};
use render::JsonReport;
use std::fmt::Display;
use std::io::IsTerminal;
use std::process::ExitCode;
use tasks::{Corrections, Grammar, Reading, Task, Variants};

const AFTER_HELP: &str = "例:
  tfb fix \"I can not reproduce it in my local.\"
  tfb explain \"-5 degrees is too cold for me.\" --json
  tfb translate \"ローカルで再現できません\"   # 日本語は英語に翻訳する
  pbpaste | tfb translate
  source <(tfb completions zsh)   # ~/.zshrc に書くと Tab 補完が使える

環境変数:
  TFB_TRANSLATOR_KEY      Azure Translator のキー（translate で必須）
  TFB_TRANSLATOR_REGION   Azure Translator のリージョン（例: japaneast。リージョン付きのリソースでは必須）
  TFB_AOAI_ENDPOINT       Azure OpenAI のエンドポイント（explain / fix で必須。例: https://<リソース名>.openai.azure.com）
  TFB_AOAI_KEY            Azure OpenAI のキー（explain / fix で必須）
  TFB_AOAI_DEPLOYMENT     Azure OpenAI のデプロイ名（既定: gpt-6-luna）";

const QUOTE_HINT: &str = "英文は \"...\" で囲んでください（例: tfb fix \"I can not do it.\"）";

#[derive(Parser)]
#[command(
    name = "tfb",
    version,
    about = "英文を翻訳・解説・添削する（Azure Translator / Azure OpenAI）",
    after_help = AFTER_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 日本語と英語を相互に翻訳する（例: tfb translate "I can not do it." / tfb translate "再現できません"）
    Translate(Args),
    /// 直訳、発音付きの単語、文法ポイントを表示する（例: tfb explain "I can not do it."）
    Explain(Args),
    /// 直した箇所と、用途別の言い回し（チャット、メール、対面・通話、GitHub）を表示する（例: tfb fix "I can not do it."）
    Fix(Args),
    /// シェルの補完スクリプトを出力する（例: tfb completions zsh。~/.zshrc に `source <(tfb completions zsh)` と書く）
    Completions {
        /// 補完スクリプトを作るシェル
        shell: CompletionShell,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CompletionShell {
    Zsh,
}

/// 英文を受け取って Azure を呼ぶサブコマンド。
#[derive(Clone, Copy)]
enum Lesson {
    Translate,
    Explain,
    Fix,
}

#[derive(clap::Args)]
struct Args {
    /// 英文（"..." で囲む）。省略すると標準入力から読む
    #[arg(allow_hyphen_values = true)]
    text: Option<String>,
    /// 結果を JSON で出力する
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if is_unquoted_text(&error) => return usage_error(QUOTE_HINT),
        Err(error) => error.exit(),
    };
    let (lesson, args) = match cli.command {
        Command::Translate(args) => (Lesson::Translate, args),
        Command::Explain(args) => (Lesson::Explain, args),
        Command::Fix(args) => (Lesson::Fix, args),
        Command::Completions {
            shell: CompletionShell::Zsh,
        } => {
            clap_complete::generate(
                clap_complete::Shell::Zsh,
                &mut Cli::command(),
                "tfb",
                &mut std::io::stdout(),
            );
            return ExitCode::SUCCESS;
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio ランタイムを作成できませんでした");
    runtime.block_on(run(lesson, args))
}

/// 引用符を付け忘れ、英文が複数の引数に分かれたときのエラーかどうか。
fn is_unquoted_text(error: &clap::Error) -> bool {
    error.kind() == ErrorKind::UnknownArgument
        && matches!(
            error.get(ContextKind::InvalidArg),
            Some(ContextValue::String(arg)) if !arg.starts_with('-')
        )
}

fn usage_error(error: impl Display) -> ExitCode {
    eprintln!("tfb: {error}");
    ExitCode::from(2)
}

fn exit_code(all_ok: bool) -> ExitCode {
    if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

async fn run(lesson: Lesson, args: Args) -> ExitCode {
    let stdin = std::io::stdin();
    let is_terminal = stdin.is_terminal();
    let text = match input::read_text(args.text.as_deref(), stdin.lock(), is_terminal) {
        Ok(text) => text,
        Err(error) => return usage_error(error),
    };
    let http = Http::new(http::TIMEOUT);
    match lesson {
        Lesson::Translate => translate(&http, &text, args.json).await,
        Lesson::Explain => explain(&http, &text, args.json).await,
        Lesson::Fix => fix(&http, &text, args.json).await,
    }
}

async fn translate(http: &Http, text: &str, json: bool) -> ExitCode {
    if let Err(error) = input::check_translate(text) {
        return usage_error(error);
    }
    let config = match config::translator_config(config::env_var) {
        Ok(config) => config,
        Err(error) => return usage_error(error),
    };
    let result = translator::translate(http, &config, text).await;
    if json {
        anstream::print!(
            "{}",
            JsonReport::new()
                .add("translation", result.as_ref())
                .finish()
        );
    } else {
        match &result {
            Ok(translation) => anstream::print!("{}", render::translation_text(translation)),
            Err(error) => eprintln!("翻訳: {error}"),
        }
    }
    exit_code(result.is_ok())
}

fn report_errors(sections: &[(&str, Option<&CallError>)]) {
    for (label, error) in sections {
        if let Some(error) = error {
            eprintln!("{label}: {error}");
        }
    }
}

async fn explain(http: &Http, text: &str, json: bool) -> ExitCode {
    if let Err(error) = input::check_ai(text) {
        return usage_error(error);
    }
    let config = match config::aoai_config(config::env_var) {
        Ok(config) => config,
        Err(error) => return usage_error(error),
    };
    let (reading, grammar) = tokio::join!(
        aoai::run::<Reading>(http, &config, Task::Reading, text),
        aoai::run::<Grammar>(http, &config, Task::Grammar, text),
    );
    if json {
        let report = JsonReport::new()
            .add("reading", reading.as_ref())
            .add("grammar", grammar.as_ref().map(|g| &g.grammar));
        anstream::print!("{}", report.finish());
    } else {
        anstream::print!(
            "{}",
            render::explain_text(reading.as_ref().ok(), grammar.as_ref().ok())
        );
        report_errors(&[
            ("直訳と単語", reading.as_ref().err()),
            ("文法ポイント", grammar.as_ref().err()),
        ]);
    }
    exit_code(reading.is_ok() && grammar.is_ok())
}

async fn fix(http: &Http, text: &str, json: bool) -> ExitCode {
    if let Err(error) = input::check_ai(text) {
        return usage_error(error);
    }
    let config = match config::aoai_config(config::env_var) {
        Ok(config) => config,
        Err(error) => return usage_error(error),
    };
    let (corrections, variants) = tokio::join!(
        aoai::run::<Corrections>(http, &config, Task::Corrections, text),
        aoai::run::<Variants>(http, &config, Task::Variants, text),
    );
    if json {
        let report = JsonReport::new()
            .add("corrections", corrections.as_ref().map(|c| &c.corrections))
            .add("variants", variants.as_ref());
        anstream::print!("{}", report.finish());
    } else {
        anstream::print!(
            "{}",
            render::fix_text(corrections.as_ref().ok(), variants.as_ref().ok())
        );
        report_errors(&[
            ("直した箇所", corrections.as_ref().err()),
            ("言い回し", variants.as_ref().err()),
        ]);
    }
    exit_code(corrections.is_ok() && variants.is_ok())
}
