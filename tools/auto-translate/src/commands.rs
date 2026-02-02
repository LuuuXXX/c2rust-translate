use std::process::{Command, Output};
use std::path::Path;
use crate::error::{AutoTranslateError, Result};

/// Execute a command and return the output
pub fn execute_command(
    program: &str,
    args: &[&str],
    current_dir: Option<&Path>,
    env_vars: &[(&str, &str)],
) -> Result<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    
    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }
    
    for (key, value) in env_vars {
        cmd.env(key, value);
    }
    
    let output = cmd.output()
        .map_err(|e| AutoTranslateError::CommandFailed(
            format!("Failed to execute '{}': {}", program, e)
        ))?;
    
    Ok(output)
}

/// Execute a command and check if it succeeded
pub fn execute_command_checked(
    program: &str,
    args: &[&str],
    current_dir: Option<&Path>,
    env_vars: &[(&str, &str)],
) -> Result<Output> {
    let output = execute_command(program, args, current_dir, env_vars)?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(AutoTranslateError::CommandFailed(
            format!(
                "Command '{}' failed with exit code {:?}\nStdout: {}\nStderr: {}",
                program,
                output.status.code(),
                stdout,
                stderr
            )
        ));
    }
    
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_simple_command() {
        let result = execute_command("echo", &["hello"], None, &[]);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn test_execute_command_with_args() {
        let result = execute_command_checked("echo", &["test"], None, &[]);
        assert!(result.is_ok());
    }
}
