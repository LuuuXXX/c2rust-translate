use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use crate::error::{AutoTranslateError, Result};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub clean: Option<BuildCommand>,
    pub build: Option<BuildCommand>,
    pub test: Option<BuildCommand>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BuildCommand {
    pub dir: Option<String>,
    pub command: String,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|e| AutoTranslateError::ConfigError(
                format!("Failed to read config file: {}", e)
            ))?;
        
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Find the .c2rust/config.toml file starting from the current directory
    pub fn find_and_load(start_dir: &Path) -> Result<(Self, PathBuf)> {
        let mut current = start_dir;
        
        loop {
            let config_path = current.join(".c2rust").join("config.toml");
            if config_path.exists() {
                let config = Self::load_from_file(&config_path)?;
                return Ok((config, current.to_path_buf()));
            }
            
            match current.parent() {
                Some(parent) => current = parent,
                None => return Err(AutoTranslateError::ProjectRootNotFound),
            }
        }
    }

    pub fn get_clean_command(&self) -> Option<(&str, Option<&str>)> {
        self.clean.as_ref().map(|c| (c.command.as_str(), c.dir.as_deref()))
    }

    pub fn get_build_command(&self) -> Option<(&str, Option<&str>)> {
        self.build.as_ref().map(|c| (c.command.as_str(), c.dir.as_deref()))
    }

    pub fn get_test_command(&self) -> Option<(&str, Option<&str>)> {
        self.test.as_ref().map(|c| (c.command.as_str(), c.dir.as_deref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::fs::File;

    #[test]
    fn test_load_config() {
        let temp_dir = std::env::temp_dir();
        let config_file = temp_dir.join("test_config.toml");
        
        let mut file = File::create(&config_file).unwrap();
        writeln!(file, r#"
[build]
dir = "src"
command = "cargo build"

[test]
command = "cargo test"
        "#).unwrap();
        
        let config = Config::load_from_file(&config_file).unwrap();
        assert!(config.build.is_some());
        assert!(config.test.is_some());
        
        std::fs::remove_file(config_file).ok();
    }
}
