use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
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
    Doctor,
    Init {
        shell: Shell,
    },
    Search {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        cwd: Option<String>,
        #[arg(long, hide = true)]
        shell_integration: bool,
    },
    Import {
        shell: Shell,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        include_ignored: bool,
    },
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
    let cli = Cli::parse();
    let hook_command = matches!(
        &cli.command,
        Command::RecordStart(_) | Command::RecordFinish(_)
    );
    match run(cli) {
        Ok(exit_code) => exit_code,
        Err(error) => {
            if hook_command {
                if let Ok(paths) = AppPaths::discover() {
                    let _ = fmn::doctor::append_diagnostic(&paths, &error.to_string());
                }
            } else {
                eprintln!("fmn: {error}");
            }
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> fmn::Result<ExitCode> {
    if let Command::Session = cli.command {
        println!("{}", uuid::Uuid::new_v4());
        return Ok(ExitCode::SUCCESS);
    }
    if let Command::Init { shell } = cli.command {
        print!("{}", fmn::shell::init_script(shell));
        return Ok(ExitCode::SUCCESS);
    }

    let paths = AppPaths::discover()?;
    paths.ensure_data_dir()?;
    let config = Config::load(&paths.config_file)?;
    let mut store = Store::open(&paths.database)?;

    match cli.command {
        Command::Doctor => {
            let report = fmn::doctor::inspect(&paths, &store);
            for check in &report.checks {
                let status = match check.status {
                    fmn::doctor::Status::Ok => "ok",
                    fmn::doctor::Status::Warning => "warn",
                    fmn::doctor::Status::Error => "error",
                };
                println!("[{status}] {}: {}", check.name, check.detail);
            }
            if !report.is_healthy() {
                return Ok(ExitCode::from(2));
            }
        }
        Command::Init { .. } => unreachable!(),
        Command::Session => unreachable!(),
        Command::Search {
            query,
            cwd,
            shell_integration,
        } => {
            let cwd = cwd.unwrap_or_else(|| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            let selection = if shell_integration {
                fmn::tui::run_for_shell(&mut store, query, cwd)?
            } else {
                fmn::tui::run(&mut store, query, cwd)?
            };
            match selection {
                Some(command) => {
                    if shell_integration {
                        eprint!("{command}");
                    } else {
                        print!("{command}");
                    }
                }
                None => return Ok(ExitCode::from(1)),
            }
        }
        Command::Import {
            shell,
            path,
            dry_run,
            include_ignored,
        } => {
            let path = path
                .or_else(|| fmn::import::default_history_path(shell))
                .ok_or(fmn::Error::HomeDirectoryUnavailable)?;
            let contents = std::fs::read_to_string(&path)?;
            let parsed = fmn::import::parse(shell, &contents);
            let filter = PrivacyFilter::new(&config.history)?;
            let mut ignored = 0;
            let mut duplicates = 0;
            let mut imported = 0;
            let fallback = now_millis();
            for (index, record) in parsed.records.iter().enumerate() {
                if !include_ignored && !filter.should_record(&record.command) {
                    ignored += 1;
                    continue;
                }
                if store.has_import_key(&record.import_key)? {
                    duplicates += 1;
                    continue;
                }
                if !dry_run
                    && store.import_record(record, shell, fallback.saturating_add(index as i64))?
                {
                    imported += 1;
                }
            }
            let would_import = parsed.records.len() - ignored - duplicates;
            if dry_run {
                println!(
                    "parsed={} would_import={} duplicates={} ignored={} invalid={}",
                    parsed.records.len(),
                    would_import,
                    duplicates,
                    ignored,
                    parsed.warnings.len()
                );
            } else {
                println!(
                    "parsed={} imported={} duplicates={} ignored={} invalid={}",
                    parsed.records.len(),
                    imported,
                    duplicates,
                    ignored,
                    parsed.warnings.len()
                );
            }
            for warning in parsed.warnings {
                eprintln!("fmn import: warning: {warning}");
            }
        }
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
    Ok(ExitCode::SUCCESS)
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
