/// Error type for when SVLMD configuration is not found
#[derive(Debug, thiserror::Error)]
#[error("Config not found")]
pub struct ConfigNotFoundError;

use anyhow::{Context, Result};
use git2::{Repository, StatusOptions};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

/// Represents a Logseq page with its metadata and content
///
/// A Logseq page consists of:
/// - A title
/// - Properties (key-value pairs in the page header)
/// - Contents (lines of text with indentation levels)
pub struct LogseqPage {
    /// The title of the page
    pub title: String,
    /// Properties in the page header as key-value pairs
    pub properties: Vec<(String, String)>,
    /// Page contents with indentation levels
    pub contents: Vec<(String, u8)>,
}

impl LogseqPage {
    /// Create a new Logseq page with the given title, properties, and contents
    pub fn new(
        title: &str,
        properties: Vec<(String, String)>,
        contents: Vec<(String, u8)>,
    ) -> Self {
        Self {
            title: title.to_string(),
            properties,
            contents,
        }
    }

    /// Create a Logseq page from plain text content
    ///
    /// Converts plain text content into a structured page by:
    /// - Parsing indentation levels
    /// - Removing bullet points
    /// - Preserving properties
    pub fn from_plain(title: &str, properties: Vec<(String, String)>, contents: &str) -> Self {
        fn count_indentation(line: &str) -> u8 {
            let spaces = line.chars().take_while(|c| *c == ' ').count() as u8;
            spaces / 4
        }
        let contents = contents
            .lines()
            .map(|line| {
                (
                    line.trim().replacen("- ", "", 1).to_string(),
                    count_indentation(line),
                )
            })
            .collect::<Vec<(String, u8)>>();
        Self {
            title: title.to_string(),
            properties,
            contents,
        }
    }

    /// Write the page to the filesystem
    ///
    /// Formats and writes the page with:
    /// - Properties in the header
    /// - Properly indented content
    /// - Bullet points for each line
    pub fn write_page(&self, pages_dir: &Path) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.title_to_path(pages_dir))?;

        self.properties.iter().for_each(|(key, value)| {
            writeln!(file, "{}:: {}", key, value).unwrap();
        });
        writeln!(file).unwrap();

        self.contents.iter().for_each(|(content, indentation)| {
            if content.is_empty() {
                writeln!(file).unwrap();
            } else {
                writeln!(
                    file,
                    "{}- {}",
                    "    ".repeat(*indentation as usize),
                    content
                )
                .unwrap();
            }
        });

        Ok(())
    }

    /// Read a page from the filesystem
    ///
    /// Parses a Logseq page file into a structured format by:
    /// - Extracting properties from the header
    /// - Preserving content with indentation
    pub fn read_page(&self, pages_dir: &Path) -> Result<Self> {
        let file = File::open(self.title_to_path(pages_dir))?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;

        let properties_end = lines
            .iter()
            .position(|line| !line.contains("::"))
            .unwrap_or(0);

        let properties = lines[..properties_end]
            .iter()
            .map(|line| {
                let parts: Vec<&str> = line.split("::").collect();
                (parts[0].to_string(), parts[1].trim().to_string())
            })
            .collect();

        let contents = lines[properties_end..].join("\n");

        Ok(Self::from_plain(&self.title, properties, &contents))
    }

    /// Convert a page title to its filesystem path
    ///
    /// Handles special characters in titles by:
    /// - Replacing forward slashes with triple underscores
    /// - Adding the .md extension
    fn title_to_path(&self, pages_dir: &Path) -> PathBuf {
        pages_dir.join(self.title.replace("/", "___") + ".md")
    }
}

/// Manages file operations and Git integration for SVLMD
#[derive(Debug, Clone)]
pub struct FileManager {
    /// Root directory of the SVLMD project
    pub root: PathBuf,
    /// Name of the current contributor
    pub contributor_name: String,
}

impl FileManager {
    /// Create a new FileManager instance
    ///
    /// Initializes by:
    /// - Finding the project root
    /// - Reading configuration
    /// - Loading contributor information
    pub fn new() -> Result<Self, ConfigNotFoundError> {
        let root = detect_root().map_err(|_| ConfigNotFoundError)?;
        let config_path = root.join(".svlmd");

        if config_path.exists() {
            let file = File::open(config_path).map_err(|_| ConfigNotFoundError)?;
            let reader = BufReader::new(file);
            let config: serde_json::Value =
                serde_json::from_reader(reader).map_err(|_| ConfigNotFoundError)?;
            let contributor_name = config["contributor"].as_str().unwrap_or("").to_string();
            Ok(Self {
                root,
                contributor_name,
            })
        } else {
            Err(ConfigNotFoundError)
        }
    }

    /// Check if a Logseq page exists
    pub fn logseq_page_exists(&self, title: &str) -> bool {
        let page_path = self
            .root
            .join("pages")
            .join(title.replace("/", "___") + ".md");
        page_path.exists()
    }

    /// Write a Logseq page to the filesystem
    pub fn write_logseq_page(&self, page: &LogseqPage) -> Result<()> {
        page.write_page(&self.root.join("pages"))
    }

    /// Read a Logseq page from the filesystem
    pub fn read_logseq_page(&self, title: &str) -> Result<LogseqPage> {
        let page = LogseqPage::new(title, vec![], vec![]);
        page.read_page(&self.root.join("pages"))
    }

    /// Get lists of changed pages from Git status
    ///
    /// Returns an array of three vectors containing:
    /// - [0]: New pages
    /// - [1]: Modified pages
    /// - [2]: Deleted pages
    pub fn get_changed_pages(&self) -> Result<[Vec<String>; 3]> {
        let repo = Repository::open(&self.root).context("Failed to open git repository")?;

        let mut status_opts = StatusOptions::new();
        status_opts
            .include_untracked(true)
            .include_ignored(false)
            .include_unmodified(false)
            .show(git2::StatusShow::Index);

        let statuses = repo
            .statuses(Some(&mut status_opts))
            .context("Failed to get git status")?;

        // Sort pages into new, modified, and deleted
        let mut new_pages = Vec::new();
        let mut modified_pages = Vec::new();
        let mut deleted_pages = Vec::new();

        for entry in statuses.iter() {
            let status = entry.status();
            if let Some(path) = entry.path() {
                if path.starts_with("pages/") && path.ends_with(".md") {
                    let filename = path
                        .strip_prefix("pages/")
                        .unwrap()
                        .strip_suffix(".md")
                        .unwrap();
                    let page_name = filename.replace("___", "/");
                    if status.is_wt_new() || status.is_index_new() {
                        new_pages.push(page_name);
                    } else if status.is_wt_modified()
                        || status.is_wt_renamed()
                        || status.is_index_modified()
                        || status.is_index_renamed()
                    {
                        modified_pages.push(page_name);
                    } else if status.is_wt_deleted() || status.is_index_deleted() {
                        deleted_pages.push(page_name);
                    }
                }
            }
        }

        Ok([new_pages, modified_pages, deleted_pages])
    }
}

/// Returns the path to the current executable
pub fn get_executable_path() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| anyhow::anyhow!("Failed to get executable path: {}", e))
}

/// Detects the root directory of the project
///
/// Searches for the .svlmd configuration file to determine
/// the root directory of the SVLMD project
pub fn detect_root() -> Result<PathBuf> {
    let exe_path = get_executable_path()?;
    fn fail() -> anyhow::Error {
        anyhow::anyhow!("Failed to get executable directory")
    }
    // Get the directory containing the executable
    let exe_dir = exe_path.parent().ok_or_else(fail)?;

    // Check if we're in debug/release mode (in target directory)
    if exe_dir.ends_with("debug") || exe_dir.ends_with("release") {
        // Go up three levels: debug/release -> target -> project_root
        return Ok(exe_dir
            .parent() // target
            .ok_or_else(fail)?
            .parent() // svlmd
            .ok_or_else(fail)?
            .parent() // root
            .ok_or_else(fail)?
            .to_path_buf());
    }

    // If we're running the installed binary, check current directory
    if exe_dir.join("pages/").exists() {
        return Ok(exe_dir.to_path_buf());
    }

    anyhow::bail!("Failed to detect root directory. Please run svlmd from project root or installation directory.")
}
