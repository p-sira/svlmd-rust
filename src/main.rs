/// SVLMD (Sira's Very Large Medical Database) CLI tool
/// 
/// This module implements the command-line interface for managing SVLMD,
/// including initialization, synchronization, and version control features.
mod file_manager;

use anyhow::{bail, Context, Ok, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use dialoguer::Input;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::file_manager::{FileManager, LogseqPage};

/// CLI configuration and command parsing structure
#[derive(Parser)]
#[command(name = "svlmd")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available CLI commands
#[derive(Subcommand, PartialEq)]
enum Commands {
    /// Initialize SVLMD with contributor information
    Init,
    /// Sync database
    Sync {
        /// Sync the version metadata
        #[arg(long, short = 'V')]
        version: bool,
        /// Verbose output mode
        #[arg(long, short = 'v')]
        verbose: bool,
    },
}

/// Initialize SVLMD configuration
/// 
/// Creates or overwrites the .svlmd configuration file with contributor information.
/// Prompts the user for their name and stores it in the configuration.
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

/// Initialize SVLMD system
/// 
/// Sets up the SVLMD environment by:
/// 1. Creating configuration if it doesn't exist
/// 2. Initializing the file manager
/// 3. Creating contributor's Logseq page if it doesn't exist
fn init(root: &PathBuf) -> Result<FileManager> {
    // Initialize the tool if not already initialized
    if !root.join(".svlmd").exists() {
        println!("Config not found. Creating...");
        init_config(root)?;
        println!();
    }

    let file_manager = FileManager::new()?;

    if !file_manager.logseq_page_exists(&file_manager.contributor_name) {
        file_manager.write_logseq_page(&LogseqPage::new(
            &file_manager.contributor_name,
            vec![
                ("icon".into(), "🙂".into()),
                ("exclude-from-graph-view".into(), "true".into()),
                ("tags".into(), "Author".into()),
            ],
            vec![],
        ))?;
    }

    Ok(file_manager)
}

/// Synchronize version information
/// 
/// Updates version tracking by:
/// 1. Reading the current version from version.txt
/// 2. Creating or updating the version page in Logseq
/// 3. Tracking changed pages since the last version
fn sync_version(file_manager: &FileManager, verbose: bool) -> Result<()> {
    let version_path = file_manager.root.join("version.txt");
    if !version_path.exists() {
        bail!("version.txt not found");
    }

    let version = semver::Version::parse(&fs::read_to_string(version_path)?.lines().next().unwrap())
        .context("Failed to parse version")?;

    if verbose {
        println!("Found version: {}", version);
    }

    let version_page = format!("{}.{}.{}", version.major, version.minor, version.patch);
    let changed_pages = file_manager.get_changed_pages()?;

    if verbose {
        changed_pages[0]
            .iter()
            .for_each(|page| println!("+ {}", page));
        changed_pages[1]
            .iter()
            .for_each(|page| println!("* {}", page));
        changed_pages[2]
            .iter()
            .for_each(|page| println!("- {}", page));
    }

    // Create version page if it doesn't exist
    if !file_manager.logseq_page_exists(&version_page) {
        let now = Utc::now().format("%Y-%m-%d").to_string();

        file_manager.write_logseq_page(&LogseqPage::new(
            &version_page,
            vec![
                ("tags".into(), "Version".into()),
                ("released-date".into(), now),
            ],
            vec![
                ("# Summary".into(), 0),
                ("".into(), 0),
                ("# Changed Pages".into(), 0),
            ],
        ))?;
    }

    let mut page = file_manager.read_logseq_page(&version_page)?;
    let full_version_string = format!("## {}", version);
    if !page
        .contents
        .iter()
        .any(|(line, _)| line == &full_version_string)
    {
        page.contents.push((full_version_string, 1));
    }

    // Add changed pages to the subversion
    if !changed_pages.iter().all(|v| v.is_empty()) {
        // Added files
        if !changed_pages[0].is_empty() {
            page.contents.push(("### Added".to_string(), 2));
            for added in &changed_pages[0] {
                page.contents.push((format!("[[{}]]", added), 3));
            }
        }

        // Modified files
        if !changed_pages[1].is_empty() {
            page.contents.push(("### Modified".to_string(), 2));
            for modified in &changed_pages[1] {
                page.contents.push((format!("[[{}]]", modified), 3));
            }
        }

        // Deleted files
        if !changed_pages[2].is_empty() {
            page.contents.push(("### Deleted".to_string(), 2));
            for deleted in &changed_pages[2] {
                page.contents.push((format!("[[{}]]", deleted), 3));
            }
        }
    }

    // Write the updated page
    file_manager.write_logseq_page(&page)?;

    Ok(())
}

/// Handle the sync command
/// 
/// Processes synchronization operations based on provided flags
fn sync_command(file_manager: &FileManager, version: bool, verbose: bool) -> Result<()> {
    if version {
        sync_version(file_manager, verbose)?;
    }
    Ok(())
}

/// Main entry point for the SVLMD CLI tool
fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = file_manager::detect_root()?;

    if cli.command == Commands::Init {
        init_config(&root)?;
        init(&root)?;
        return Ok(());
    }

    let file_manager = init(&root)?;

    // Handle commands
    match cli.command {
        Commands::Init => unreachable!(),
        Commands::Sync { version, verbose } => sync_command(&file_manager, version, verbose),
    }
}
