use std::path::Path;
use crate::error::{AutoTranslateError, Result};
use crate::commands::execute_command;

/// Check if a required tool is available in PATH
pub fn check_tool_exists(tool_name: &str) -> Result<()> {
    let result = execute_command("which", &[tool_name], None, &[]);
    
    match result {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(AutoTranslateError::ToolNotFound(tool_name.to_string())),
    }
}

/// Check all required tools for the translation process
pub fn check_required_tools() -> Result<()> {
    let required_tools = vec![
        "c2rust",
        "c2rust-config",
        "code-analyse",
        "git",
    ];
    
    let mut missing_tools = Vec::new();
    
    for tool in required_tools {
        if check_tool_exists(tool).is_err() {
            missing_tools.push(tool.to_string());
        }
    }
    
    if !missing_tools.is_empty() {
        return Err(AutoTranslateError::ToolNotFound(
            format!("Missing required tools: {}", missing_tools.join(", "))
        ));
    }
    
    Ok(())
}

/// Find the project root (directory containing .c2rust)
pub fn find_project_root(start_dir: &Path) -> Result<std::path::PathBuf> {
    let mut current = start_dir;
    
    loop {
        let c2rust_dir = current.join(".c2rust");
        if c2rust_dir.exists() && c2rust_dir.is_dir() {
            return Ok(current.to_path_buf());
        }
        
        match current.parent() {
            Some(parent) => current = parent,
            None => return Err(AutoTranslateError::ProjectRootNotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_tool_exists() {
        // Test with a tool that should exist
        let result = check_tool_exists("echo");
        assert!(result.is_ok());
        
        // Test with a tool that shouldn't exist
        let result = check_tool_exists("nonexistent-tool-xyz123");
        assert!(result.is_err());
    }
}
