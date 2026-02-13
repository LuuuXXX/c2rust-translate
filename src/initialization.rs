use anyhow::{Context, Result};
use colored::Colorize;
use crate::{analyzer, builder, git, hybrid_build, interaction, util};

/// 检查并初始化 feature 目录
/// 
/// 如果 rust 目录不存在，则初始化并提交
pub fn check_and_initialize_feature(feature: &str) -> Result<()> {
    util::validate_feature_name(feature)?;
    
    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");

    let rust_dir_exists = match std::fs::metadata(&rust_dir) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                anyhow::bail!(
                    "Path exists but is not a directory: {}",
                    rust_dir.display()
                );
            }
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(e).context(format!(
                "Failed to access rust directory at {}",
                rust_dir.display()
            ));
        }
    };

    if !rust_dir_exists {
        println!("{}", "Feature directory does not exist. Initializing...".yellow());
        analyzer::initialize_feature(feature)?;
        
        // 验证 rust 目录已创建并且确实是一个目录
        match std::fs::metadata(&rust_dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    anyhow::bail!(
                        "Initialization created a file instead of a directory: {}",
                        rust_dir.display()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!("Error: Failed to initialize rust directory");
            }
            Err(e) => {
                return Err(e).context(format!(
                    "Failed to verify initialized rust directory at {}",
                    rust_dir.display()
                ));
            }
        }
        
        // 在 .c2rust 目录初始化 git（如果还没有）
        git::git_commit(&format!("Initialize {} feature directory", feature), feature)?;
        
        println!("{}", "✓ Feature directory initialized successfully".bright_green());
    } else {
        println!("{}", "Feature directory exists, continuing...".bright_cyan());
    }
    
    Ok(())
}

/// 门禁验证：Cargo Build
/// 
/// 在 rust 目录下执行 cargo build 验证
pub fn gate_cargo_build(feature: &str, show_full_output: bool) -> Result<()> {
    println!("\n{}", "Step 2.1: Cargo Build Verification".bright_cyan().bold());
    println!("{}", "Building project...".bright_blue().bold());
    
    match builder::cargo_build(feature, show_full_output) {
        Ok(_) => {
            println!("{}", "✓ Build successful!".bright_green().bold());
            Ok(())
        }
        Err(e) => {
            println!("{}", "✗ Initial build failed!".red().bold());
            println!("{}", "This may indicate issues with the project setup or previous translations.".yellow());
            
            // 提供交互式处理
            let choice = interaction::prompt_user_choice("Initial build failure", false)?;
            
            match choice {
                interaction::UserChoice::Continue => {
                    println!("│ {}", "Continuing despite build failure. You can fix issues during file processing.".yellow());
                    Ok(())
                }
                interaction::UserChoice::ManualFix => {
                    Err(e).context("Initial build failed and user chose manual fix")
                }
                interaction::UserChoice::Exit => {
                    Err(e).context("Initial build failed and user chose to exit")
                }
            }
        }
    }
}

/// 门禁验证：代码分析同步
pub fn gate_code_analysis(feature: &str) -> Result<()> {
    println!("\n{}", "Step 2.2: Code Analysis Sync".bright_cyan().bold());
    println!("{}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("{}", "✓ Code analysis updated".bright_green());
    Ok(())
}

/// 门禁验证：混合构建清除
pub fn gate_hybrid_clean(feature: &str) -> Result<bool> {
    println!("\n{}", "Step 2.3: Hybrid Build Clean".bright_cyan().bold());
    
    match hybrid_build::execute_hybrid_build_command(feature, hybrid_build::HybridCommandType::Clean) {
        Ok(_) => {
            println!("{}", "✓ Hybrid clean successful".bright_green());
            Ok(true)
        }
        Err(e) => {
            println!("{}", "✗ Hybrid clean failed!".red().bold());
            
            // 提供交互式处理
            let choice = interaction::prompt_user_choice("Hybrid clean failure", false)?;
            
            match choice {
                interaction::UserChoice::Continue => Ok(false),
                interaction::UserChoice::ManualFix | interaction::UserChoice::Exit => {
                    Err(e).context("Hybrid clean failed")
                }
            }
        }
    }
}

/// 门禁验证：混合构建构建
pub fn gate_hybrid_build(feature: &str) -> Result<bool> {
    println!("\n{}", "Step 2.4: Hybrid Build".bright_cyan().bold());
    
    match hybrid_build::execute_hybrid_build_command(feature, hybrid_build::HybridCommandType::Build) {
        Ok(_) => {
            println!("{}", "✓ Hybrid build successful".bright_green());
            Ok(true)
        }
        Err(e) => {
            println!("{}", "✗ Hybrid build failed!".red().bold());
            
            // 提供交互式处理
            let choice = interaction::prompt_user_choice("Hybrid build failure", false)?;
            
            match choice {
                interaction::UserChoice::Continue => Ok(false),
                interaction::UserChoice::ManualFix | interaction::UserChoice::Exit => {
                    Err(e).context("Hybrid build failed")
                }
            }
        }
    }
}

/// 门禁验证：混合构建测试
pub fn gate_hybrid_test(feature: &str) -> Result<bool> {
    println!("\n{}", "Step 2.5: Hybrid Build Test".bright_cyan().bold());
    
    match hybrid_build::execute_hybrid_build_command(feature, hybrid_build::HybridCommandType::Test) {
        Ok(_) => {
            println!("{}", "✓ Hybrid test successful".bright_green());
            Ok(true)
        }
        Err(e) => {
            println!("{}", "✗ Hybrid test failed!".red().bold());
            
            // 提供交互式处理
            let choice = interaction::prompt_user_choice("Hybrid test failure", false)?;
            
            match choice {
                interaction::UserChoice::Continue => Ok(false),
                interaction::UserChoice::ManualFix | interaction::UserChoice::Exit => {
                    Err(e).context("Hybrid test failed")
                }
            }
        }
    }
}

/// 运行完整的门禁验证流程
/// 
/// 包括：
/// 1. Cargo Build
/// 2. 代码分析同步
/// 3. 混合构建清除
/// 4. 混合构建构建
/// 5. 混合构建测试
/// 6. 如果全部通过，提交到 git
pub fn run_gate_verification(feature: &str, show_full_output: bool) -> Result<()> {
    println!("\n{}", "═══ Gate Verification (Post-Initialization) ═══".bright_magenta().bold());
    
    // 2.1 Cargo Build
    gate_cargo_build(feature, show_full_output)?;
    
    // 2.2 代码分析同步
    gate_code_analysis(feature)?;
    
    // 2.3 混合构建清除
    let hybrid_clean_ok = gate_hybrid_clean(feature)?;
    if !hybrid_clean_ok {
        println!("{}", "Hybrid clean gate reported a user-accepted failure, stopping gate verification before commit.".yellow());
        return Ok(());
    }
    
    // 2.4 混合构建构建
    let hybrid_build_ok = gate_hybrid_build(feature)?;
    if !hybrid_build_ok {
        println!("{}", "Hybrid build gate reported a user-accepted failure, stopping gate verification before commit.".yellow());
        return Ok(());
    }
    
    // 2.5 混合构建测试
    let hybrid_test_ok = gate_hybrid_test(feature)?;
    if !hybrid_test_ok {
        println!("{}", "Hybrid test gate reported a user-accepted failure, stopping gate verification before commit.".yellow());
        return Ok(());
    }
    
    // 2.6 门禁通过提交
    println!("\n{}", "Step 2.6: Gate Verification Passed - Committing".bright_cyan().bold());
    git::git_commit(&format!("Gate verification passed for {}", feature), feature)?;
    println!("{}", "✓ Gate verification complete and committed".bright_green().bold());
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 确认 `check_and_initialize_feature` 的签名保持为 `fn(&str) -> Result<()>`
    #[test]
    fn check_and_initialize_feature_has_expected_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str) -> Result<()>,
        {
            // 不调用实际逻辑，只验证类型兼容性
            let _ = f;
        }

        assert_signature(check_and_initialize_feature);
    }

    /// 确认 `gate_hybrid_test` 的签名保持为 `fn(&str) -> Result<bool>`
    #[test]
    fn gate_hybrid_test_has_expected_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str) -> Result<bool>,
        {
            let _ = f;
        }

        assert_signature(gate_hybrid_test);
    }

    /// 确认 `run_gate_verification` 的签名保持为 `fn(&str, bool) -> Result<()>`
    #[test]
    fn run_gate_verification_has_expected_signature() {
        fn assert_signature<F>(f: F)
        where
            F: Fn(&str, bool) -> Result<()>,
        {
            let _ = f;
        }

        assert_signature(run_gate_verification);
    }
}
