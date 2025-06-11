#[derive(Debug, thiserror::Error)]
#[error("Config not found")]
pub struct ConfigNotFoundError;

use anyhow::{Result};
use std::{
    fs::File,
    fs::OpenOptions,
    io::{BufReader, Write},
    path::PathBuf,
};

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
        let page_path = self.root.join("pages").join(title.replace("/", "___") + ".md");
        page_path.exists()
    }

    pub fn write_logseq_page(
        &self,
        title: &str,
        properties: Vec<(&str, &str)>,
        contents: &str,
    ) -> Result<()> {
        write_logseq_page(self.root.clone(), title, properties, contents)
    }
}

/// Write or create a logseq page
pub fn write_logseq_page(
    root: PathBuf,
    title: &str,
    properties: Vec<(&str, &str)>,
    contents: &str,
) -> Result<()> {
    let page_path = root
        .join("pages")
        .join(title.to_string().replace("/", "___") + ".md");

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(page_path)?;

    for (key, value) in properties {
        writeln!(file, "{}:: {}", key, value)?;
    }
    writeln!(file)?;

    writeln!(file, "{}", contents)?;

    Ok(())
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
