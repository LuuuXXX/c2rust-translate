//! 用于解析和处理测试失败的错误处理工具

use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::{builder, file_scanner, interaction, suggestion, translator, util};

/// 解析错误消息以提取 Rust 文件路径
/// 返回在错误消息中找到的文件路径列表
/// 过滤为仅包含项目内的文件
pub(crate) fn parse_error_for_files(error_msg: &str, feature: &str) -> Result<Vec<PathBuf>> {
    // 验证特性名称以防止路径遍历
    util::validate_feature_name(feature)?;
    
    lazy_static::lazy_static! {
        static ref ERROR_PATH_RE: regex::Regex = 
            regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?")
                .expect("Failed to compile error path regex");
    }
    
    let project_root = util::find_project_root()?;
    let feature_path = project_root.join(".c2rust").join(feature);
    let rust_dir = feature_path.join("rust");
    
    let mut file_paths = HashSet::new();
    
    for cap in ERROR_PATH_RE.captures_iter(error_msg) {
        if let Some(path_match) = cap.get(1) {
            let path_str = path_match.as_str();
            let path = PathBuf::from(path_str);
            
            // 尝试原样路径和相对于 rust_dir 的路径
            let candidates = vec![
                path.clone(),
                rust_dir.join(&path),
            ];
            
            for candidate in candidates {
                // 检查文件是否存在且在我们的项目内
                if candidate.exists() && candidate.is_file() {
                    // 确保文件在 rust 目录内
                    if let Ok(canonical) = candidate.canonicalize() {
                        if let Ok(rust_canonical) = rust_dir.canonicalize() {
                            if canonical.starts_with(&rust_canonical) {
                                file_paths.insert(canonical);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    
    // 转换为 Vec 并排序以保持一致的顺序
    let mut result: Vec<PathBuf> = file_paths.into_iter().collect();
    result.sort();
    
    Ok(result)
}

/// 当可以定位文件时处理启动测试失败
#[allow(unused_assignments)]
pub(crate) fn handle_startup_test_failure_with_files(
    feature: &str,
    test_error: anyhow::Error,
    mut files: Vec<PathBuf>,
) -> Result<()> {
    let mut current_error = test_error;
    
    // 使用循环迭代处理文件，避免深度递归
    'outer: loop {
        if files.is_empty() {
            // 没有要处理的文件，返回当前错误
            return Err(current_error).context("No files found to fix");
        }
        
        println!("│");
        println!("│ {}", format!("Found {} file(s) in error message:", files.len()).bright_cyan());
        for (idx, file) in files.iter().enumerate() {
            println!("│   {}. {}", idx + 1, file.display());
        }
        
        // 处理错误中找到的每个文件
        for (idx, file) in files.iter().enumerate() {
            println!("│");
            let file_display_name = file.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            println!("│ {}", format!("═══ Processing file {}/{}: {} ═══", 
                idx + 1, files.len(), file_display_name).bright_cyan().bold());
            
            // 从文件主干提取文件类型（var_ 或 fun_）
            let file_stem = file.file_stem()
            .and_then(|s| s.to_str())
            .context("Invalid file stem")?;
            
        let (file_type, _) = file_scanner::extract_file_type(file_stem)
            .context(format!("Could not extract file type from filename: {}", file_display_name))?;
        
        // 显示 C 和 Rust 代码
        let c_file = file.with_extension("c");
        
        if c_file.exists() {
            interaction::display_file_paths(Some(&c_file), file);
            
            println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
            translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);
        } else {
            interaction::display_file_paths(None, file);
        }
        
        println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
        translator::display_code(file, "─ Rust Code ─", usize::MAX, true);
        
        println!("│ {}", "═══ Test Error ═══".bright_red().bold());
        println!("│ {}", current_error);
        
        // 提供与 handle_max_fix_attempts_reached 相同的选择
        let choice = interaction::prompt_user_choice("Initial test failure", false)?;
        
        match choice {
            interaction::UserChoice::Continue => {
                println!("│");
                println!("│ {}", "You chose: Continue trying with a new suggestion".bright_cyan());
                
                // 在提示新建议之前清除旧建议
                suggestion::clear_suggestions()?;
                
                // 从用户获取可选建议
                if let Some(suggestion_text) = interaction::prompt_suggestion(false)? {
                    // 将建议保存到 suggestions.txt
                    suggestion::append_suggestion(&suggestion_text)?;
                }
                
                // 应用带有建议的修复
                let format_progress = |op: &str| format!("Fix startup test failure - {}", op);
                crate::apply_error_fix(feature, file_type, file, &current_error, &format_progress, true)?;
                
                // 再次尝试构建和测试
                println!("│");
                println!("│ {}", "Rebuilding and retesting...".bright_blue().bold());
                match builder::cargo_build(feature, true) {
                    Ok(_) => {
                        println!("│ {}", "✓ Build successful!".bright_green().bold());
                        
                        // 现在尝试完整的混合构建测试
                        match builder::run_hybrid_build(feature) {
                            Ok(_) => {
                                println!("│ {}", "✓ Hybrid build tests passed!".bright_green().bold());
                                
                                println!("{}", "Updating code analysis...".bright_blue());
                                crate::analyzer::update_code_analysis(feature)?;
                                println!("{}", "✓ Code analysis updated".bright_green());
                                
                                // 混合构建现在通过；停止进一步的错误处理
                                return Ok(());
                            }
                            Err(e) => {
                                println!("│ {}", "✗ Hybrid build tests still failing".red());
                                
                                // 尝试解析新错误并查看是否有更多文件
                                match parse_error_for_files(&e.to_string(), feature) {
                                    Ok(new_files) if !new_files.is_empty() => {
                                        println!("│ {}", "Found additional files in new error, will process them...".yellow());
                                        // 更新文件和错误以进行下一次迭代
                                        files = new_files;
                                        current_error = e;
                                        continue 'outer; // 重新开始外部循环以处理新文件
                                    }
                                    _ => {
                                        // 没有更多文件需要处理，返回错误
                                        return Err(e).context("Hybrid build tests failed after fix attempt");
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Build still failing after fix attempt".red());
                        return Err(e).context(format!(
                            "Build failed after fix for file {}",
                            file.display()
                        ));
                    }
                }
            }
            interaction::UserChoice::ManualFix => {
                println!("│");
                println!("│ {}", "You chose: Manual fix".bright_cyan());
                
                // 尝试打开 vim
                match interaction::open_in_vim(file) {
                    Ok(_) => {
                        // Vim 编辑后，重复尝试构建和测试
                        loop {
                            println!("│");
                            println!("│ {}", "Vim editing completed. Rebuilding and retesting...".bright_blue());
                            
                            // 手动编辑后尝试构建
                            match builder::cargo_build(feature, true) {
                                Ok(_) => {
                                    println!("│ {}", "✓ Build successful!".bright_green().bold());
                                    
                                    // 现在尝试完整的混合构建测试
                                    match builder::run_hybrid_build(feature) {
                                        Ok(_) => {
                                            println!("│ {}", "✓ Hybrid build tests passed after manual fix!".bright_green().bold());
                                            // 所有测试都通过；成功退出处理器
                                            return Ok(());
                                        }
                                        Err(e) => {
                                            println!("│ {}", "✗ Hybrid build tests still failing".red());
                                            
                                            // 询问用户是否想再试一次
                                            println!("│");
                                            println!("│ {}", "Tests still have errors. What would you like to do?".yellow());
                                            let retry_choice = interaction::prompt_user_choice("Tests still failing", false)?;
                                            
                                            match retry_choice {
                                                interaction::UserChoice::Continue => {
                                                    // 只需使用现有更改重试构建
                                                    continue;
                                                }
                                                interaction::UserChoice::ManualFix => {
                                                    println!("│ {}", "Reopening file in Vim for additional manual fixes...".bright_blue());
                                                    match interaction::open_in_vim(file) {
                                                        Ok(_) => {
                                                            // 循环将重试构建
                                                            continue;
                                                        }
                                                        Err(open_err) => {
                                                            println!("│ {}", format!("Failed to reopen vim: {}", open_err).red());
                                                            return Err(open_err).context(format!(
                                                                "Tests still failing and could not reopen vim for file {}",
                                                                file.display()
                                                            ));
                                                        }
                                                    }
                                                }
                                                interaction::UserChoice::Exit => {
                                                    return Err(e).context(format!(
                                                        "Tests failed after manual fix for file {}",
                                                        file.display()
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("│ {}", "✗ Build still failing after manual fix".red());
                                    
                                    // 询问用户是否想再试一次
                                    println!("│");
                                    println!("│ {}", "Build still has errors. What would you like to do?".yellow());
                                    let retry_choice = interaction::prompt_user_choice("Build still failing", false)?;
                                    
                                    match retry_choice {
                                        interaction::UserChoice::Continue => {
                                            // 继续：只需使用现有更改重试构建
                                            continue;
                                        }
                                        interaction::UserChoice::ManualFix => {
                                            println!("│ {}", "Reopening file in Vim for additional manual fixes...".bright_blue());
                                            match interaction::open_in_vim(file) {
                                                Ok(_) => {
                                                    // 在额外的手动修复后，循环将重试构建
                                                    continue;
                                                }
                                                Err(open_err) => {
                                                    println!("│ {}", format!("Failed to reopen vim: {}", open_err).red());
                                                    return Err(open_err).context(format!(
                                                        "Build still failing and could not reopen vim for file {}",
                                                        file.display()
                                                    ));
                                                }
                                            }
                                        }
                                        interaction::UserChoice::Exit => {
                                            return Err(e).context(format!(
                                                "Build failed after manual fix for file {}",
                                                file.display()
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        println!("│ {}", format!("Failed to open vim: {}", e).red());
                        return Err(e).context(format!(
                            "Initial test failed and could not open vim for file {}",
                            file.display()
                        ));
                    }
                }
            }
            interaction::UserChoice::Exit => {
                println!("│");
                println!("│ {}", "You chose: Exit".yellow());
                return Err(current_error).context("User chose to exit during startup test failure handling");
            }
        }
        } // 内部 for 循环结束
        
        // 如果我们已经处理完所有文件而没有错误或提前返回，则完成
        println!("│");
        println!("│ {}", "✓ All files processed successfully".bright_green().bold());
        return Ok(());
    } // 外部循环结束
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_error_pattern_extraction() {
        // 测试我们可以从错误消息中提取文件路径
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`

error[E0425]: cannot find value `y` in this scope
  --> src/fun_helper.rs:20:9
   |
20 |         y
   |         ^ not found in this scope";
        
        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re.captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"src/var_test.rs".to_string()));
        assert!(matches.contains(&"src/fun_helper.rs".to_string()));
    }
    
    #[test]
    fn test_parse_error_pattern_warnings() {
        // 测试我们也可以从警告中提取文件路径
        let error_msg = "warning: unused variable: `x`
  --> src/var_counter.rs:5:9
   |
5  |     let x = 42;
   |         ^ help: if this is intentional, prefix it with an underscore: `_x`";
        
        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re.captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "src/var_counter.rs");
    }
    
    #[test]
    fn test_parse_error_multiple_files_same_error() {
        // 测试单个错误中的多个文件引用
        let error_msg = "error[E0308]: mismatched types
  --> src/var_main.rs:15:10
   |
15 |     foo(x);
   |          ^ expected `String`, found `i32`
   |
note: expected signature from here
  --> src/fun_foo.rs:3:1
   |
3  | fn foo(s: String) { }
   | ^^^^^^^^^^^^^^^^^";
        
        let re = regex::Regex::new(r"(?:-->|at)\s+([^\s:]+\.rs)(?::\d+:\d+)?").unwrap();
        let matches: Vec<String> = re.captures_iter(error_msg)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();
        
        // 应该找到两个文件
        assert!(matches.len() >= 2);
        assert!(matches.contains(&"src/var_main.rs".to_string()));
        assert!(matches.contains(&"src/fun_foo.rs".to_string()));
    }
    
    #[test]
    #[serial_test::serial]
    fn test_parse_error_for_files_with_real_directory() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;
        
        // 创建临时目录结构
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();
        
        // 将当前目录设置为临时目录，以便 find_project_root 可以工作
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();
        
        // 创建特性目录结构
        let feature = "test_feature";
        let c2rust_dir = project_root.join(".c2rust");
        fs::create_dir_all(&c2rust_dir).unwrap();
        
        let feature_dir = c2rust_dir.join(feature);
        let rust_dir = feature_dir.join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        
        // 创建测试文件
        let test_file1 = src_dir.join("var_test.rs");
        fs::write(&test_file1, "// test content").unwrap();
        
        let test_file2 = src_dir.join("fun_helper.rs");
        fs::write(&test_file2, "// helper content").unwrap();
        
        // 创建应被过滤掉的 rust 目录外的文件
        let outside_file = c2rust_dir.join("outside.rs");
        fs::write(&outside_file, "// outside").unwrap();
        
        // 测试包含多个文件的错误消息
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    |     ^^^^^^ expected `i32`, found `&str`

error[E0425]: cannot find value `y` in this scope
  --> src/fun_helper.rs:20:9
   |
20 |         y
   |         ^ not found in this scope
   
note: some note about outside file
  --> ../../outside.rs:1:1";
        
        let result = parse_error_for_files(error_msg, feature).unwrap();
        
        // 恢复原始目录
        env::set_current_dir(&original_dir).unwrap();
        
        // 应该准确找到 2 个文件（不包括 outside.rs）
        assert_eq!(result.len(), 2);
        
        // 检查两个文件都存在且是规范化的
        let canonical_file1 = test_file1.canonicalize().unwrap();
        let canonical_file2 = test_file2.canonicalize().unwrap();
        
        assert!(result.contains(&canonical_file1), "Should contain var_test.rs");
        assert!(result.contains(&canonical_file2), "Should contain fun_helper.rs");
        
        // 验证文件已排序
        assert!(result[0] < result[1], "Files should be sorted");
    }
    
    #[test]
    #[serial_test::serial]
    fn test_parse_error_for_files_deduplication() {
        use std::env;
        use std::fs;
        use tempfile::tempdir;
        
        let temp_dir = tempdir().unwrap();
        let project_root = temp_dir.path();
        
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(project_root).unwrap();
        
        fs::create_dir(project_root.join(".git")).unwrap();
        
        let feature = "test_feature";
        let rust_dir = project_root.join(".c2rust").join(feature).join("rust");
        let src_dir = rust_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        
        let test_file = src_dir.join("var_test.rs");
        fs::write(&test_file, "// test").unwrap();
        
        // 多次提及同一文件的错误消息
        let error_msg = "error[E0308]: mismatched types
   --> src/var_test.rs:10:5
    |
10  |     let x: i32 = \"hello\";
    
error[E0308]: another error
   --> src/var_test.rs:15:5
    
note: note about same file
   --> src/var_test.rs:20:1";
        
        let result = parse_error_for_files(error_msg, feature).unwrap();
        
        env::set_current_dir(&original_dir).unwrap();
        
        // 尽管多次提及，但应该只有 1 个文件
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("var_test.rs"));
    }
    
    #[test]
    fn test_parse_error_for_files_validates_feature_name() {
        // 测试无效的特性名称被拒绝
        let error_msg = "error: --> src/test.rs:1:1";
        
        // 带有路径遍历的特性名称应该失败
        let result = parse_error_for_files(error_msg, "../bad");
        assert!(result.is_err(), "Should reject feature name with ..");
        
        let result = parse_error_for_files(error_msg, "good/bad");
        assert!(result.is_err(), "Should reject feature name with /");
    }
}
