#[derive(Debug, thiserror::Error)]
#[error("Config not found")]
pub struct ConfigNotFoundError;

use anyhow::{Context, Result};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};
use git2::{Repository, StatusOptions};

pub struct LogseqPage {
    pub title: String,
    pub properties: Vec<(String, String)>,
    pub contents: Vec<(String, u8)>, // Vec of content and indentation level
}

impl LogseqPage {
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

    pub fn from_plain(
        title: &str,
        properties: Vec<(String, String)>,
        contents: &str,
    ) -> Self {
        fn count_indentation(line: &str) -> u8 {
            let spaces = line.chars().take_while(|c| *c == ' ').count() as u8;
            spaces / 4
        }
        let contents = contents
            .lines()
            .map(|line| (line.trim().replacen("- ", "", 1).to_string(), count_indentation(line)))
            .collect::<Vec<(String, u8)>>();
        Self {
            title: title.to_string(),
            properties,
            contents,
        }
    }

    pub fn write_page(&self, pages_dir: &PathBuf) -> Result<()> {
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
            writeln!(file, "{}- {}", "    ".repeat(*indentation as usize), content).unwrap();
        });

        Ok(())
    }

    pub fn read_page(&self, pages_dir: &PathBuf) -> Result<Self> {
        let file = File::open(self.title_to_path(pages_dir))?;
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
        
        let properties_end = lines.iter()
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

        Ok(Self::from_plain(
            &self.title,
            properties,
            &contents,
        ))
    }

    fn title_to_path(&self, pages_dir: &PathBuf) -> PathBuf {
        pages_dir.join(self.title.replace("/", "___") + ".md")
    }
}

#[derive(Debug, Clone)]
pub struct FileManager {
    pub root: PathBuf,
    pub contributor_name: String,
}

impl FileManager {
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

    pub fn logseq_page_exists(&self, title: &str) -> bool {
        let page_path = self
            .root
            .join("pages")
            .join(title.replace("/", "___") + ".md");
        page_path.exists()
    }

    pub fn write_logseq_page(&self, page: &LogseqPage) -> Result<()> {
        page.write_page(&self.root.join("pages"))
    }

    pub fn read_logseq_page(&self, title: &str) -> Result<LogseqPage> {
        let page = LogseqPage::new(title, vec![], vec![]);
        page.read_page(&self.root.join("pages"))
    }
    
    pub fn get_changed_pages(&self) -> Result<[Vec<String>; 3]> {
        let repo = Repository::open(&self.root)
            .context("Failed to open git repository")?;
        let mut changed_pages = [Vec::new(), Vec::new(), Vec::new()];
        
        let mut status_opts = StatusOptions::new();
        status_opts
            .include_untracked(true)
            .include_ignored(false)
            .include_unmodified(false)
            .show(git2::StatusShow::IndexAndWorkdir);
            
        let statuses = repo.statuses(Some(&mut status_opts))
            .context("Failed to get git status")?;
            
        // Sort pages into new, modified, and deleted
        let mut new_pages = Vec::new();
        let mut modified_pages = Vec::new();
        let mut deleted_pages = Vec::new();

        for entry in statuses.iter() {
            let status = entry.status();
            if let Some(path) = entry.path() {
                if path.starts_with("pages/") && path.ends_with(".md") {
                    if let Some(filename) = PathBuf::from(path).file_stem() {
                        let page_name = filename.to_string_lossy().replace("___", "/");
                        if status.is_wt_new() {
                            new_pages.push(page_name);
                        } else if status.is_wt_modified() || status.is_wt_renamed() {
                            modified_pages.push(page_name); 
                        } else if status.is_wt_deleted() {
                            deleted_pages.push(page_name);
                        }
                    }
                }
            }
        }

        changed_pages[0].extend(new_pages);
        changed_pages[1].extend(modified_pages);
        changed_pages[2].extend(deleted_pages);
        Ok(changed_pages)
    }
}

/// Returns the path to the current executable
pub fn get_executable_path() -> Result<PathBuf> {
    std::env::current_exe().map_err(|e| anyhow::anyhow!("Failed to get executable path: {}", e))
}

/// Detects the root directory of the project
pub fn detect_root() -> Result<PathBuf> {
    let exe_path = get_executable_path()?;
    fn fail() -> anyhow::Error {
        anyhow::anyhow!("Failed to get executable directory")
    }
    // Get the directory containing the executable
    let exe_dir = exe_path.parent().ok_or_else(|| fail())?;

    // Check if we're in debug/release mode (in target directory)
    if exe_dir.ends_with("debug") || exe_dir.ends_with("release") {
        // Go up three levels: debug/release -> target -> project_root
        return Ok(exe_dir
            .parent() // target
            .ok_or_else(|| fail())?
            .parent() // svlmd
            .ok_or_else(|| fail())?
            .parent() // root
            .ok_or_else(|| fail())?
            .to_path_buf());
    }

    // If we're running the installed binary, check current directory
    if exe_dir.join("pages/").exists() {
        return Ok(exe_dir.to_path_buf());
    }

    anyhow::bail!("Failed to detect root directory. Please run svlmd from project root or installation directory.")
}
