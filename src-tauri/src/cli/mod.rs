mod output;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::runtime;

#[derive(Parser)]
#[command(
    name = "openquota-cli",
    version,
    about = "OpenQuota usage and quotas from the terminal.",
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show usage and quota state for enabled providers.
    Usage {
        /// Emit machine-readable JSON instead of a summary.
        #[arg(long)]
        json: bool,
        /// Print cached snapshots without refreshing.
        #[arg(long)]
        cached: bool,
        /// Refresh and show only this provider id (see `providers`).
        #[arg(long)]
        provider: Option<String>,
    },
    /// List known providers with their enabled and detected state.
    Providers {
        /// Emit machine-readable JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

/// Entry point for the `openquota-cli` binary. Returns the process exit code: 0 success,
/// 1 runtime failure (including provider refresh errors), 2 invalid arguments.
pub fn run_cli(arguments: impl IntoIterator<Item = String>) -> i32 {
    // No file log in the CLI; keep internal library chatter out of stdout/stderr so JSON
    // consumers only ever see command output and explicit errors.
    crate::logging::set_level(crate::models::LogLevel::Error);
    let cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() { 2 } else { 0 };
        }
    };
    let data_dir = resolve_data_dir();
    let core = match runtime::build_core(&data_dir) {
        Ok(core) => core,
        Err(error) => {
            eprintln!("error: {error}");
            return 1;
        }
    };
    match cli.command {
        Command::Providers { json } => {
            let settings = core.settings.get();
            let text = if json {
                output::providers_json(core.registry.catalog(), &settings)
            } else {
                output::providers_table(core.registry.catalog(), &settings)
            };
            println!("{text}");
            0
        }
        Command::Usage {
            json,
            cached,
            provider,
        } => run_usage(core, json, cached, provider),
    }
}

fn run_usage(core: runtime::Core, json: bool, cached: bool, provider: Option<String>) -> i32 {
    if let Some(provider_id) = &provider {
        if !core
            .settings
            .enabled_provider_ids()
            .iter()
            .any(|id| id == provider_id)
        {
            eprintln!("error: provider {provider_id} is not enabled.");
            return 1;
        }
    }
    if !cached {
        let service = core.service.clone();
        let settings = core.settings.clone();
        let requested = provider.clone();
        tauri::async_runtime::block_on(async move {
            match requested {
                Some(provider_id) => {
                    service.refresh(&provider_id, true).await;
                }
                None => {
                    let ids = settings.enabled_provider_ids();
                    service.refresh_all_with_progress(&ids, true, |_| {}).await;
                }
            }
        });
    }
    let state = core.service.state();
    let target_ids = provider
        .map(|id| vec![id])
        .unwrap_or_else(|| core.settings.enabled_provider_ids());
    let text = if json {
        output::usage_json(&state)
    } else {
        output::usage_table(&state, &target_ids)
    };
    println!("{text}");
    if target_ids.iter().any(|id| {
        state
            .providers
            .get(id)
            .is_some_and(|view| view.error.is_some())
    }) {
        1
    } else {
        0
    }
}

/// OPENQUOTA_DATA_DIR wins so tests and scripts can point at a scratch directory. The default
/// mirrors the Tauri app data directory for the GUI identifier, so both share one database.
fn resolve_data_dir() -> PathBuf {
    if let Some(directory) = std::env::var_os("OPENQUOTA_DATA_DIR") {
        if !directory.is_empty() {
            return PathBuf::from(directory);
        }
    }
    default_data_dir().expect("could not resolve the OpenQuota data directory")
}

fn default_data_dir() -> Option<PathBuf> {
    const IDENTIFIER: &str = "io.github.deviffyy.openquota";
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join(IDENTIFIER))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(IDENTIFIER)
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("share"))
            })?;
        Some(base.join(IDENTIFIER))
    }
}
