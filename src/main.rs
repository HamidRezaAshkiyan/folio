mod core;
mod gui;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use core::config::FolioConfig;
use core::io::{ExportFormat, ImportFormat, export_document, import_from_file};
use core::model::{Signifier, parse_rapid_input};
use core::review::{generate_monthly_review, generate_weekly_review};
use core::vault::Vault;
use std::fs;
use std::path::PathBuf;

/// Folio — Minimalistic cross-platform Bullet Journal (TUI + GUI) backed by Obsidian Markdown.
#[derive(Parser, Debug)]
#[command(name = "folio", version = "1.4.0", about)]
struct Cli {
    /// Launch the Ratatui TUI (default when no subcommand or --gui is passed)
    #[arg(long)]
    tui: bool,

    /// Launch the embedded local Web GUI
    #[arg(long)]
    gui: bool,

    /// Port for --gui local server
    #[arg(long, default_value_t = 3420)]
    port: u16,

    /// Do not auto-open the browser when running --gui
    #[arg(long)]
    no_open: bool,

    /// Path to Obsidian-compatible Bullet Journal vault
    #[arg(long, global = true)]
    vault: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Rapid-log a single entry directly from the command line
    Log {
        /// Entry text, optionally prefixed with `. `, `o `, `= `, or `- `
        #[arg(allow_hyphen_values = true)]
        text: Vec<String>,
        /// Target custom space name instead of today's Daily Log
        #[arg(short, long)]
        space: Option<String>,
    },
    /// Generate and print a Weekly or Monthly Review
    Review {
        /// Review period: `weekly` or `monthly`
        #[arg(default_value = "weekly")]
        period: String,
    },
    /// Import notes from Markdown, Obsidian, Apple Notes, Notion, CSV, or JSON
    Import {
        /// Path to source file
        file: PathBuf,
        /// Format override: auto, md, txt, csv, json
        #[arg(short, long, default_value = "auto")]
        format: String,
        /// Optional target Space name (defaults to today's Daily Log)
        #[arg(short, long)]
        space: Option<String>,
    },
    /// Export today's log or a specific Space to Markdown, JSON, or CSV
    Export {
        /// Output format: md, json, csv
        #[arg(short, long, default_value = "md")]
        format: String,
        /// Optional Space name (defaults to today's Daily Log)
        #[arg(short, long)]
        space: Option<String>,
        /// Optional output file path (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Create a new custom Space
    Space {
        /// Name of the space (e.g. "Studio & Work")
        name: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = FolioConfig::load();
    let vault_path = cfg.resolve_vault_path(cli.vault);
    let vault = Vault::open_or_init(vault_path, &cfg.default_spaces)?;

    if let Some(cmd) = cli.command {
        return handle_subcommand(&vault, cfg.notebook_page, cmd);
    }

    if cli.gui {
        gui::run_gui(vault, cfg.notebook_page, cli.port, !cli.no_open)
    } else {
        // Default to TUI (`folio` or `folio --tui`)
        tui::run_tui(vault, cfg.notebook_page)
    }
}

fn handle_subcommand(vault: &Vault, default_page: u32, cmd: Commands) -> Result<()> {
    let today = Vault::today_date();
    match cmd {
        Commands::Log { text, space } => {
            let joined = text.join(" ");
            let (sig, clean) = parse_rapid_input(&joined, Signifier::Action);
            let mut doc = if let Some(sp) = space {
                vault.create_space(&sp)?
            } else {
                vault.open_or_create_daily(today, default_page)?
            };
            let time = Vault::now_hhmm();
            doc.add_entry(time.clone(), sig, clean.clone());
            vault.save_document(&doc)?;
            println!(
                "Logged: {} [{}] {} [paper:no] -> {}",
                sig.symbol(),
                time,
                clean,
                doc.rel_path
            );
        }
        Commands::Review { period } => {
            let (doc, sum) = match period.to_ascii_lowercase().as_str() {
                "monthly" | "month" | "m" => generate_monthly_review(vault, today)?,
                _ => generate_weekly_review(vault, today)?,
            };
            println!("Generated {} ({})", doc.title, sum.rel_path);
            println!(
                "Days logged: {} | Done: {} | Open: {} | Events: {} | Emotions: {} | Notes: {} | Paper pending: {}",
                sum.days_logged,
                sum.actions_done,
                sum.actions_open,
                sum.events_count,
                sum.emotions_count,
                sum.notes_count,
                sum.paper_pending
            );
        }
        Commands::Import {
            file,
            format,
            space,
        } => {
            let mut doc = if let Some(sp) = space {
                vault.create_space(&sp)?
            } else {
                vault.open_or_create_daily(today, default_page)?
            };
            let fmt = ImportFormat::parse(&format);
            let count = import_from_file(&file, fmt, &mut doc)?;
            vault.save_document(&doc)?;
            println!(
                "Imported {count} entries from {} into {}",
                file.display(),
                doc.rel_path
            );
        }
        Commands::Export {
            format,
            space,
            output,
        } => {
            let doc = if let Some(sp) = space {
                vault.create_space(&sp)?
            } else {
                vault.open_or_create_daily(today, default_page)?
            };
            let fmt = ExportFormat::parse(&format)?;
            let content = export_document(&doc, fmt)?;
            if let Some(out_path) = output {
                fs::write(&out_path, content)?;
                println!("Exported {} to {}", doc.rel_path, out_path.display());
            } else {
                print!("{content}");
            }
        }
        Commands::Space { name } => {
            let joined = name.join(" ");
            let doc = vault.create_space(&joined)?;
            println!("Space ready: {} ({})", doc.title, doc.rel_path);
        }
    }
    Ok(())
}
