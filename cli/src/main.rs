mod aoai;
mod config;
mod http;
mod input;
mod render;
mod tasks;
mod translator;

use clap::{Parser, Subcommand};
use http::{CallError, Http};
use render::JsonReport;
use std::fmt::Display;
use std::io::IsTerminal;
use std::process::ExitCode;
use tasks::{Corrections, Grammar, Reading, Task, Variants};

#[derive(Parser)]
#[command(
    name = "tfb",
    version,
    about = "英文を翻訳・解説・添削する（Azure Translator / Azure OpenAI）"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Azure Translator で日本語に翻訳する
    Translate(Args),
    /// 直訳と単語、文法ポイントを表示する
    Explain(Args),
    /// 直した箇所と、用途別の言い回し（チャット、メール、対面・通話、GitHub）を表示する
    Fix(Args),
}

#[derive(clap::Args)]
struct Args {
    /// 英文。省略すると標準入力から読む
    text: Vec<String>,
    /// 結果を JSON で出力する
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio ランタイムを作成できませんでした");
    runtime.block_on(run(cli))
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

async fn run(cli: Cli) -> ExitCode {
    let (Command::Translate(args) | Command::Explain(args) | Command::Fix(args)) = &cli.command;
    let stdin = std::io::stdin();
    let is_terminal = stdin.is_terminal();
    let text = match input::read_text(&args.text, stdin.lock(), is_terminal) {
        Ok(text) => text,
        Err(error) => return usage_error(error),
    };
    let http = Http::new(http::TIMEOUT);
    match &cli.command {
        Command::Translate(args) => translate(&http, &text, args.json).await,
        Command::Explain(args) => explain(&http, &text, args.json).await,
        Command::Fix(args) => fix(&http, &text, args.json).await,
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
