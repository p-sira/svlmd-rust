mod file_manager;

use anyhow::{Context, Ok, Result};
use clap::{Parser, Subcommand};
use dialoguer::Input;
use std::fs::OpenOptions;
use std::path::PathBuf;

use crate::file_manager::FileManager;

#[derive(Parser)]
#[command(name = "svlmd")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, PartialEq)]
enum Commands {
    /// Initialize SVLMD with contributor information
    Init,
    /// Sync file metadata
    Sync,
}

fn init_config(root: &PathBuf) -> Result<()> {
    if root.join(".svlmd").exists() {
        println!(".svlmd already exists. Overwriting...");
    }

    let contributor_name: String = Input::new()
        .with_prompt("Enter your name")
        .interact_text()
        .context("Failed to get contributor name")?;

    let config = serde_json::json!({
        "contributor": contributor_name
    });

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(root.join(".svlmd"))
        .context("Failed to create .svlmd")?;

    serde_json::to_writer_pretty(&file, &config).context("Failed to write to .svlmd")?;
    println!("Initialized config.");

    Ok(())
}

fn init(root: &PathBuf) -> Result<()> {
    // Initialize the tool if not already initialized
    if !root.join(".svlmd").exists() {
        println!("Config not found. Creating...");
        init_config(&root)?;
        println!();
    }

    let file_manager = FileManager::new()?;

    if !file_manager.logseq_page_exists(&file_manager.contributor_name) {
        file_manager.write_logseq_page(
            &file_manager.contributor_name,
            vec![("tags", "Author")],
            "",
        )?;
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = file_manager::detect_root()?;

    if cli.command == Commands::Init {
        init_config(&root)?;
        init(&root)?;
        return Ok(());
    }

    init(&root)?;
    match cli.command {
        Commands::Init => unreachable!(),
        Commands::Sync => todo!(),
    }
}
