use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "c2rust-config")]
#[command(version = VERSION)]
#[command(about = "Configuration management tool for c2rust-translate", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage configuration settings
    Config {
        /// Create configuration if it doesn't exist
        #[arg(long)]
        make: bool,
        
        /// Feature name to configure
        #[arg(long)]
        feature: Option<String>,
        
        /// Set a configuration key to a value
        #[arg(long, requires = "value")]
        set: Option<String>,
        
        /// Value to set (used with --set)
        value: Option<String>,
        
        /// List/get a configuration value
        #[arg(long)]
        list: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    
    let result = match cli.command {
        Some(Commands::Config { make, feature, set, value, list }) => {
            handle_config(make, feature, set, value, list)
        }
        None => {
            // No subcommand provided, just show help
            Ok(())
        }
    };
    
    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}

fn handle_config(
    make: bool,
    feature: Option<String>,
    set_key: Option<String>,
    set_value: Option<String>,
    list_key: Option<String>,
) -> Result<()> {
    let config_path = PathBuf::from("config.toml");
    
    // If make flag is set, ensure config file exists
    if make && !config_path.exists() {
        fs::write(&config_path, "# c2rust configuration\n")
            .context("Failed to create config.toml")?;
    }
    
    let feature_name = feature.as_deref().unwrap_or("default");
    
    // Load or create config
    let mut config_content = if config_path.exists() {
        fs::read_to_string(&config_path)
            .context("Failed to read config.toml")?
    } else {
        String::new()
    };
    
    // Handle set operation
    if let (Some(key), Some(value)) = (set_key, set_value) {
        set_config_value(&mut config_content, feature_name, &key, &value)?;
        fs::write(&config_path, &config_content)
            .context("Failed to write config.toml")?;
    }
    
    // Handle list operation
    if let Some(key) = list_key {
        let value = get_config_value(&config_content, feature_name, &key)?;
        print!("{}", value);
    }
    
    Ok(())
}

fn set_config_value(
    content: &mut String,
    feature: &str,
    key: &str,
    value: &str,
) -> Result<()> {
    // Simple TOML-like configuration handling
    // Format: [feature.section]
    // key = "value"
    
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 2 {
        anyhow::bail!("Key must be in format 'section.key'");
    }
    
    let section = parts[0];
    let key_name = parts[1];
    let section_header = format!("[{}.{}]", feature, section);
    let key_line = format!("{} = \"{}\"", key_name, value);
    
    // Check if section exists
    if let Some(section_start) = content.find(&section_header) {
        // Section exists, update or add key
        let after_section = &content[section_start..];
        if let Some(next_bracket) = after_section[1..].find('[') {
            // There's another section after this one
            let section_end = section_start + 1 + next_bracket;
            let section_content = &content[section_start..section_end];
            
            if let Some(key_start) = section_content.find(&format!("{} = ", key_name)) {
                // Key exists, replace it
                let key_line_start = section_start + key_start;
                let key_line_end = content[key_line_start..]
                    .find('\n')
                    .map(|i| key_line_start + i)
                    .unwrap_or(section_end);
                
                content.replace_range(key_line_start..key_line_end, &key_line);
            } else {
                // Key doesn't exist, add it before next section
                let insert_pos = section_end;
                content.insert_str(insert_pos, &format!("{}\n", key_line));
            }
        } else {
            // This is the last section
            if let Some(key_start) = after_section.find(&format!("{} = ", key_name)) {
                // Key exists, replace it
                let key_line_start = section_start + key_start;
                let key_line_end = content[key_line_start..]
                    .find('\n')
                    .map(|i| key_line_start + i)
                    .unwrap_or(content.len());
                
                content.replace_range(key_line_start..key_line_end, &key_line);
            } else {
                // Key doesn't exist, add it at the end
                if !content.ends_with('\n') {
                    content.push('\n');
                }
                content.push_str(&format!("{}\n", key_line));
            }
        }
    } else {
        // Section doesn't exist, create it
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("{}\n{}\n", section_header, key_line));
    }
    
    Ok(())
}

fn get_config_value(
    content: &str,
    feature: &str,
    key: &str,
) -> Result<String> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 2 {
        anyhow::bail!("Key must be in format 'section.key'");
    }
    
    let section = parts[0];
    let key_name = parts[1];
    let section_header = format!("[{}.{}]", feature, section);
    
    // Find the section
    if let Some(section_start) = content.find(&section_header) {
        let after_section = &content[section_start..];
        
        // Find the section end (next [ or end of file)
        let section_end = after_section[1..]
            .find('[')
            .map(|i| section_start + 1 + i)
            .unwrap_or(content.len());
        
        let section_content = &content[section_start..section_end];
        
        // Find the key
        let key_pattern = format!("{} = ", key_name);
        if let Some(key_start) = section_content.find(&key_pattern) {
            let value_start = section_start + key_start + key_pattern.len();
            let value_end = content[value_start..]
                .find('\n')
                .map(|i| value_start + i)
                .unwrap_or(content.len());
            
            let value = content[value_start..value_end].trim();
            // Remove quotes if present
            let value = value.trim_matches('"').trim_matches('\'');
            return Ok(value.to_string());
        }
    }
    
    anyhow::bail!("Key '{}' not found in feature '{}'", key, feature)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_set_and_get_config_value() {
        let mut content = String::new();
        
        // Set a value
        set_config_value(&mut content, "default", "build.target", "my_target").unwrap();
        
        // Get the value
        let value = get_config_value(&content, "default", "build.target").unwrap();
        assert_eq!(value, "my_target");
    }
    
    #[test]
    fn test_update_existing_value() {
        let mut content = String::new();
        
        // Set initial value
        set_config_value(&mut content, "default", "build.target", "target1").unwrap();
        
        // Update the value
        set_config_value(&mut content, "default", "build.target", "target2").unwrap();
        
        // Get the updated value
        let value = get_config_value(&content, "default", "build.target").unwrap();
        assert_eq!(value, "target2");
    }
}
