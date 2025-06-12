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
use std::path::Path;

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
fn init_config(root: &Path) -> Result<()> {
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
fn init(root: &Path) -> Result<FileManager> {
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

    let version = semver::Version::parse(fs::read_to_string(version_path)?.lines().next().unwrap())
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
    let full_version_string = format!("## [[{}]]", version);

    // Find the "Changed Pages" section
    let changed_pages_index = page
        .contents
        .iter()
        .position(|(line, _)| line == "# Changed Pages")
        .unwrap_or(0);

    // Find the latest version entry after "Changed Pages"
    let latest_version_index = page.contents[changed_pages_index..]
        .iter()
        .position(|(line, indent)| line.starts_with("## [[") && *indent == 1)
        .map(|pos| pos + changed_pages_index);

    let mut new_entries = vec![(full_version_string.clone(), 1)];
    let mut existing_changes = Vec::new();

    if let Some(idx) = latest_version_index {
        let next_version_index = page.contents[idx + 1..]
            .iter()
            .position(|(line, indent)| line.starts_with("## [[") && *indent == 1)
            .map(|pos| pos + idx + 1)
            .unwrap_or(page.contents.len());

        // Check if the latest version matches current version
        if page.contents[idx].0 == full_version_string {
            // Accumulate changes from the existing version
            for (line, indent) in &page.contents[idx..next_version_index] {
                if line.starts_with("### ") || line.starts_with("[[") {
                    existing_changes.push((line.clone(), *indent));
                }
            }
            // Remove existing version entry as we'll merge it with new changes
            page.contents.drain(idx..next_version_index);
        }
    }

    // Merge existing changes with new changes
    let mut all_changes = Vec::new();

    // Process new changes
    if !changed_pages.iter().all(|v| v.is_empty()) {
        // Added files
        let mut added = existing_changes
            .iter()
            .filter(|(line, indent)| *indent == 3 && line.starts_with("[["))
            .filter(|(line, _)| {
                let section_start = existing_changes
                    .iter()
                    .position(|(l, i)| *i == 2 && l == "### Added");
                let section_end = existing_changes
                    .iter()
                    .position(|(l, i)| *i == 2 && l == "### Modified")
                    .or_else(|| {
                        existing_changes
                            .iter()
                            .position(|(l, i)| *i == 2 && l == "### Deleted")
                    });
                section_start
                    .and_then(|start| section_end.map(|end| (start, end)))
                    .map_or(false, |(start, end)| {
                        existing_changes[start..end].iter().any(|(l, _)| l == line)
                    })
            })
            .map(|(line, _)| line.clone())
            .collect::<Vec<_>>();

        added.extend(changed_pages[0].iter().map(|p| format!("[[{}]]", p)));
        added.sort();
        added.dedup();

        if !added.is_empty() {
            all_changes.push(("### Added".to_string(), 2));
            all_changes.extend(added.into_iter().map(|page| (page, 3)));
        }

        // Modified files
        let mut modified = existing_changes
            .iter()
            .filter(|(line, indent)| *indent == 3 && line.starts_with("[["))
            .filter(|(line, _)| {
                let section_start = existing_changes
                    .iter()
                    .position(|(l, i)| *i == 2 && l == "### Modified");
                let section_end = existing_changes
                    .iter()
                    .position(|(l, i)| *i == 2 && l == "### Deleted")
                    .or_else(|| existing_changes.iter().position(|(_, i)| *i == 1));
                section_start
                    .and_then(|start| section_end.map(|end| (start, end)))
                    .map_or(false, |(start, end)| {
                        existing_changes[start..end].iter().any(|(l, _)| l == line)
                    })
            })
            .map(|(line, _)| line.clone())
            .collect::<Vec<_>>();

        modified.extend(changed_pages[1].iter().map(|p| format!("[[{}]]", p)));
        modified.sort();
        modified.dedup();

        if !modified.is_empty() {
            all_changes.push(("### Modified".to_string(), 2));
            all_changes.extend(modified.into_iter().map(|page| (page, 3)));
        }

        // Deleted files
        let mut deleted = existing_changes
            .iter()
            .filter(|(line, indent)| *indent == 3 && line.starts_with("[["))
            .filter(|(line, _)| {
                let section_start = existing_changes
                    .iter()
                    .position(|(l, i)| *i == 2 && l == "### Deleted");
                let section_end = existing_changes
                    .iter()
                    .position(|(_, i)| *i == 1)
                    .unwrap_or(existing_changes.len());
                section_start.map_or(false, |start| {
                    existing_changes[start..section_end]
                        .iter()
                        .any(|(l, _)| l == line)
                })
            })
            .map(|(line, _)| line.clone())
            .collect::<Vec<_>>();

        deleted.extend(changed_pages[2].iter().map(|p| format!("[[{}]]", p)));
        deleted.sort();
        deleted.dedup();

        if !deleted.is_empty() {
            all_changes.push(("### Deleted".to_string(), 2));
            all_changes.extend(deleted.into_iter().map(|page| (page, 3)));
        }
    }

    // Add all accumulated changes to new entries
    new_entries.extend(all_changes);

    // Insert all new entries after the "Changed Pages" section
    let insert_position = changed_pages_index + 1;
    for (entry, indent) in new_entries.into_iter().rev() {
        page.contents.insert(insert_position, (entry, indent));
    }

    // Write the updated page
    file_manager.write_logseq_page(&page)?;

    // Modify the Version page
    file_manager.write_logseq_page(&LogseqPage {
        title: "Version".into(),
        properties: vec![
            ("icon".into(), "🏷️".into()),
            ("exclude-from-graph-view".into(), "true".into()),
        ],
        contents: vec![],
    })?;

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
