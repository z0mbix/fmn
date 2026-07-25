use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand};
use fmn::config::Config;
use fmn::model::Shell;
use fmn::paths::AppPaths;
use fmn::privacy::PrivacyFilter;
use fmn::storage::Store;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    History {
        #[command(subcommand)]
        command: HistoryCommand,
    },
    #[command(hide = true, name = "_session")]
    Session,
    #[command(hide = true, name = "_record-start")]
    RecordStart(RecordStartArgs),
    #[command(hide = true, name = "_record-finish")]
    RecordFinish(RecordFinishArgs),
}

#[derive(Debug, Subcommand)]
enum HistoryCommand {
    List {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    Delete {
        entry_id: i64,
    },
    Forget {
        command_id: i64,
    },
    Clear {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Args)]
struct RecordStartArgs {
    #[arg(long)]
    shell: Shell,
    #[arg(long)]
    session: String,
    #[arg(long)]
    cwd: String,
    #[arg(long)]
    started_at: Option<i64>,
}

#[derive(Debug, Args)]
struct RecordFinishArgs {
    #[arg(long)]
    session: String,
    #[arg(long)]
    exit_code: i32,
    #[arg(long)]
    finished_at: Option<i64>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fmn: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> fmn::Result<()> {
    if let Command::Session = cli.command {
        println!("{}", uuid::Uuid::new_v4());
        return Ok(());
    }

    let paths = AppPaths::discover()?;
    paths.ensure_data_dir()?;
    let config = Config::load(&paths.config_file)?;
    let mut store = Store::open(&paths.database)?;

    match cli.command {
        Command::Session => unreachable!(),
        Command::RecordStart(args) => {
            let mut command = String::new();
            std::io::stdin().read_to_string(&mut command)?;
            let filter = PrivacyFilter::new(&config.history)?;
            if filter.should_record(&command) {
                store.record_start(
                    &command,
                    args.started_at.unwrap_or_else(now_millis),
                    &args.cwd,
                    args.shell,
                    &args.session,
                )?;
            }
        }
        Command::RecordFinish(args) => {
            store.record_finish(
                &args.session,
                args.finished_at.unwrap_or_else(now_millis),
                args.exit_code,
            )?;
        }
        Command::History { command } => run_history(command, &mut store)?,
    }
    Ok(())
}

fn run_history(command: HistoryCommand, store: &mut Store) -> fmn::Result<()> {
    match command {
        HistoryCommand::List { limit, json } => {
            let entries = store.entries(limit)?;
            if json {
                let values: Vec<_> = entries
                    .into_iter()
                    .map(|entry| {
                        serde_json::json!({
                            "id": entry.id,
                            "command_id": entry.command_id,
                            "command": entry.command,
                            "started_at": entry.started_at,
                            "finished_at": entry.finished_at,
                            "duration_ms": entry.duration_ms,
                            "exit_code": entry.exit_code,
                            "cwd": entry.cwd,
                            "shell": entry.shell,
                            "session_id": entry.session_id,
                            "source": entry.source,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&values).expect("JSON value")
                );
            } else {
                for entry in entries {
                    println!("{}\t{}\t{}", entry.id, entry.started_at, entry.command);
                }
            }
        }
        HistoryCommand::Delete { entry_id } => store.delete_entry(entry_id)?,
        HistoryCommand::Forget { command_id } => store.forget_command(command_id)?,
        HistoryCommand::Clear { yes } => {
            if !yes {
                if !std::io::stdin().is_terminal() {
                    return Err(std::io::Error::other(
                        "history clear requires --yes when stdin is not a terminal",
                    )
                    .into());
                }
                eprint!("Type clear to permanently remove all fmn history: ");
                std::io::stderr().flush()?;
                let mut confirmation = String::new();
                std::io::stdin().read_line(&mut confirmation)?;
                if confirmation.trim() != "clear" {
                    return Ok(());
                }
            }
            store.clear()?;
        }
    }
    Ok(())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
