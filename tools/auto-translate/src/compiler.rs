use std::path::Path;
use crate::error::{AutoTranslateError, Result};
use crate::commands::execute_command;

const MAX_COMPILE_ATTEMPTS: usize = 5;

/// Attempt to compile the Rust code and return errors if any
pub fn check_compilation(_feature_root: &Path, build_dir: &Path) -> Result<Option<String>> {
    // This would run the build command from the config
    // For now, we just run a simple cargo check
    let output = execute_command(
        "cargo",
        &["check"],
        Some(build_dir),
        &[]
    )?;
    
    if output.status.success() {
        Ok(None)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Ok(Some(stderr))
    }
}

/// Extract compilation errors from compiler output
pub fn extract_errors(compiler_output: &str) -> Vec<String> {
    compiler_output
        .lines()
        .filter(|line| line.contains("error") || line.contains("warning"))
        .map(|s| s.to_string())
        .collect()
}

/// Run the compilation and fixing loop
pub fn compile_and_fix_loop<F>(
    feature_root: &Path,
    build_dir: &Path,
    mut fix_fn: F,
) -> Result<()>
where
    F: FnMut(&str) -> Result<bool>,
{
    for attempt in 1..=MAX_COMPILE_ATTEMPTS {
        println!("Compilation attempt {}/{}", attempt, MAX_COMPILE_ATTEMPTS);
        
        match check_compilation(feature_root, build_dir)? {
            None => {
                println!("Compilation successful!");
                return Ok(());
            }
            Some(errors) => {
                println!("Compilation errors found:");
                println!("{}", errors);
                
                if attempt == MAX_COMPILE_ATTEMPTS {
                    return Err(AutoTranslateError::CompilationFailed(attempt));
                }
                
                // Try to fix the errors
                let fixed = fix_fn(&errors)?;
                if !fixed {
                    println!("Unable to fix errors automatically. Manual intervention required.");
                    return Err(AutoTranslateError::CompilationFailed(attempt));
                }
            }
        }
    }
    
    Err(AutoTranslateError::CompilationFailed(MAX_COMPILE_ATTEMPTS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_errors() {
        let output = r#"
error[E0425]: cannot find value `x` in this scope
  --> src/main.rs:2:13
   |
2  |     println!("{}", x);
   |                    ^ not found in this scope

warning: unused variable: `y`
  --> src/main.rs:3:9
   |
3  |     let y = 5;
   |         ^ help: if this is intentional, prefix it with an underscore: `_y`
        "#;
        
        let errors = extract_errors(output);
        assert!(errors.len() >= 2);
        assert!(errors.iter().any(|e| e.contains("error")));
        assert!(errors.iter().any(|e| e.contains("warning")));
    }
}
