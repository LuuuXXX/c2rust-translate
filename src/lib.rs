pub mod analyzer;
pub mod builder;
pub mod file_scanner;
pub mod git;
pub mod translator;
pub mod util;
pub mod progress;
pub mod logger;
pub mod constants;
pub(crate) mod diff_display;
pub(crate) mod interaction;
pub(crate) mod suggestion;
pub(crate) mod error_handler;

use anyhow::{Context, Result};
use colored::Colorize;

/// 特性的主要翻译工作流
pub fn translate_feature(feature: &str, allow_all: bool, max_fix_attempts: usize, show_full_output: bool) -> Result<()> {
    let msg = format!("Starting translation for feature: {}", feature);
    println!("{}", msg.bright_cyan().bold());
    logger::log_message(&msg);

    // 验证特性名称以防止路径遍历攻击
    util::validate_feature_name(feature)?;

    // 首先查找项目根目录
    let project_root = util::find_project_root()?;
    
    // 步骤 1：检查 rust 目录是否存在（通过适当的 IO 错误处理）
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
        println!("{}", "Rust directory does not exist. Initializing...".yellow());
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
        
        // 提交初始化
        git::git_commit(&format!("Initialize {} rust directory", feature), feature)?;
    }

    // 在主循环之前初始化进度状态
    // 计算总 .rs 文件数和已处理的文件数
    let total_rs_files = file_scanner::count_all_rs_files(&rust_dir)?;
    let initial_empty_count = file_scanner::find_empty_rs_files(&rust_dir)?.len();
    let already_processed = total_rs_files.saturating_sub(initial_empty_count);
    
    let mut progress_state = progress::ProgressState::with_initial_progress(
        total_rs_files,
        already_processed
    );

    // 步骤 1：主循环 - 处理所有空的 .rs 文件
    println!("\n{}", "Step 1: Translate C source files".bright_cyan().bold());
    loop {
        // 步骤 1.1：首先尝试构建
        println!("\n{}", "Building project...".bright_blue().bold());
        match builder::cargo_build(feature, show_full_output) {
            Ok(_) => {
                println!("{}", "✓ Build successful!".bright_green().bold());
            }
            Err(e) => {
                println!("{}", "✗ Initial build failed!".red().bold());
                println!("{}", "This may indicate issues with the project setup or previous translations.".yellow());
                
                // 为启动构建失败提供交互式处理
                let choice = interaction::prompt_user_choice("Initial build failure", false)?;
                
                match choice {
                    interaction::UserChoice::Continue => {
                        println!("│ {}", "Continuing despite build failure. You can fix issues during file processing.".yellow());
                        // 继续工作流
                    }
                    interaction::UserChoice::ManualFix => {
                        println!("│ {}", "Please manually fix the build issues and run the tool again.".yellow());
                        return Err(e).context("Initial build failed and user chose manual fix");
                    }
                    interaction::UserChoice::Exit => {
                        return Err(e).context("Initial build failed and user chose to exit");
                    }
                }
            }
        }

        println!("{}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("{}", "✓ Code analysis updated".bright_green());
            
        git::git_commit(&format!("Update code analysis for {}", feature), feature)?;

        println!("{}", "Running hybrid build tests...".bright_blue());
        match builder::run_hybrid_build(feature) {
            Ok(_) => {
                println!("{}", "✓ Hybrid build tests passed".bright_green());
            }
            Err(e) => {
                println!("{}", "✗ Initial hybrid build tests failed!".red().bold());
                
                // 尝试解析错误并定位文件
                match error_handler::parse_error_for_files(&e.to_string(), feature) {
                    Ok(files) if !files.is_empty() => {
                        // 找到文件，进入修复流程
                        println!("{}", "Attempting to automatically locate and fix files from error...".yellow());
                        error_handler::handle_startup_test_failure_with_files(feature, e, files)?;
                    }
                    Ok(_) => {
                        // 错误消息中未找到文件
                        println!("{}", "Unable to automatically locate files from error.".yellow());
                        println!("{}", "This may indicate issues with the test environment or previous translations.".yellow());
                        
                        let choice = interaction::prompt_user_choice("Initial test failure", false)?;
                        
                        match choice {
                            interaction::UserChoice::Continue => {
                                println!("│ {}", "Continuing despite test failure. You can fix issues during file processing.".yellow());
                                // 继续工作流
                            }
                            interaction::UserChoice::ManualFix | interaction::UserChoice::Exit => {
                                return Err(e).context("Initial tests failed");
                            }
                        }
                    }
                    Err(parse_err) => {
                        // 解析错误消息失败（例如，find_project_root 失败）
                        println!("{}", format!("Error parsing failure message: {}", parse_err).yellow());
                        println!("{}", "Unable to automatically locate files from error.".yellow());
                        println!("{}", "This may indicate issues with the test environment or previous translations.".yellow());
                        
                        let choice = interaction::prompt_user_choice("Initial test failure", false)?;
                        
                        match choice {
                            interaction::UserChoice::Continue => {
                                println!("│ {}", "Continuing despite test failure. You can fix issues during file processing.".yellow());
                                // 继续工作流
                            }
                            interaction::UserChoice::ManualFix | interaction::UserChoice::Exit => {
                                return Err(e).context("Initial tests failed");
                            }
                        }
                    }
                }
            }
        }
        
        // 步骤 1.2：扫描空的 .rs 文件（未处理的文件）
        let empty_rs_files = file_scanner::find_empty_rs_files(&rust_dir)?;
        
        if empty_rs_files.is_empty() {
            let msg = "✓ No empty .rs files found. Translation complete!";
            println!("\n{}", msg.bright_green().bold());
            logger::log_message(msg);
            break;
        }
        
        println!("{}", format!("Found {} empty .rs file(s) to process", 
            empty_rs_files.len()).cyan());

        // 基于 allow_all 标志选择要处理的文件
        let selected_indices: Vec<usize> = if allow_all {
            // 不提示处理所有空文件
            (0..empty_rs_files.len()).collect()
        } else {
            // 提示用户选择文件
            let file_refs: Vec<_> = empty_rs_files.iter().collect();
            file_scanner::prompt_file_selection(&file_refs, &rust_dir)?
        };

        for &idx in selected_indices.iter() {
            let rs_file = &empty_rs_files[idx];
            // 获取当前进度位置（在循环迭代之间保持）
            let current_position = progress_state.get_current_position();
            let total_count = progress_state.get_total_count();
            
            // 获取文件名以供显示
            let file_name = rs_file
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>");
            
            let progress_msg = format!("[{}/{}] Processing {}", current_position, total_count, file_name);
            println!(
                "\n{}",
                progress_msg.bright_magenta().bold()
            );
            logger::log_message(&progress_msg);
            
            process_rs_file(feature, rs_file, file_name, current_position, total_count, max_fix_attempts, show_full_output)?;
            
            // 标记此会话中已处理的文件
            progress_state.mark_processed();
        }
    }

    Ok(())
}

/// 通过翻译工作流处理单个 .rs 文件
fn process_rs_file(feature: &str, rs_file: &std::path::Path, file_name: &str, current_position: usize, total_count: usize, max_fix_attempts: usize, show_full_output: bool) -> Result<()> {
    use constants::MAX_TRANSLATION_ATTEMPTS;
    
    for attempt_number in 1..=MAX_TRANSLATION_ATTEMPTS {
        let is_last_attempt = attempt_number == MAX_TRANSLATION_ATTEMPTS;
        
        print_attempt_header(attempt_number, rs_file);
        
        // 为重试尝试添加消息  
        if attempt_number > 1 {
            println!("│ {}", "Starting fresh translation (previous translation will be overwritten)...".bright_cyan());
        }
        
        let (file_type, _name) = extract_and_validate_file_info(rs_file)?;
        check_c_file_exists(rs_file)?;
        
        let format_progress = |operation: &str| {
            format!("[{}/{}] Processing {} - {}", current_position, total_count, file_name, operation)
        };
        
        // 将 C 翻译为 Rust
        translate_file(feature, file_type, rs_file, &format_progress, show_full_output)?;
        
        // 构建并修复错误
        let build_successful = build_and_fix_loop(
            feature, 
            file_type, 
            rs_file, 
            file_name, 
            &format_progress,
            is_last_attempt,
            attempt_number,
            max_fix_attempts,
            show_full_output
        )?;
        
        if build_successful {
            complete_file_processing(feature, file_name, file_type, rs_file, &format_progress)?;
            return Ok(());
        }
    }
    
    anyhow::bail!("Unexpected: all retry attempts completed without resolution")
}

/// 打印当前尝试的标题
fn print_attempt_header(attempt_number: usize, rs_file: &std::path::Path) {
    if attempt_number > 1 {
        let retry_number = attempt_number - 1;
        let max_retries = constants::MAX_TRANSLATION_ATTEMPTS - 1;
        println!("\n{}", format!("┌─ Retry attempt {}/{}: {}", retry_number, max_retries, rs_file.display()).bright_yellow().bold());
    } else {
        println!("\n{}", format!("┌─ Processing file: {}", rs_file.display()).bright_white().bold());
    }
}

/// 提取文件类型和名称，打印信息
fn extract_and_validate_file_info(rs_file: &std::path::Path) -> Result<(&'static str, &str)> {
    let file_stem = rs_file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid filename")?;

    let (file_type, name) = file_scanner::extract_file_type(file_stem)
        .ok_or_else(|| anyhow::anyhow!("Unknown file prefix: {}", file_stem))?;

    println!("│ {} {}", "File type:".cyan(), file_type.bright_yellow());
    println!("│ {} {}", "Name:".cyan(), name.bright_yellow());
    
    Ok((file_type, name))
}

/// 检查对应的 C 文件是否存在
fn check_c_file_exists(rs_file: &std::path::Path) -> Result<()> {
    use std::fs;
    
    let c_file = rs_file.with_extension("c");
    match fs::metadata(&c_file) {
        Ok(_) => {
            println!("│ {} {}", "C source:".cyan(), c_file.display().to_string().bright_yellow());
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("Corresponding C file not found for Rust file: {}", rs_file.display());
        }
        Err(err) => {
            Err(err).context(format!("Failed to access corresponding C file for Rust file: {}", rs_file.display()))
        }
    }
}

/// 将 C 文件翻译为 Rust
fn translate_file<F>(feature: &str, file_type: &str, rs_file: &std::path::Path, format_progress: &F, show_full_output: bool) -> Result<()> 
where
    F: Fn(&str) -> String
{
    use std::fs;
    
    let c_file = rs_file.with_extension("c");
    
    println!("│");
    println!("│ {}", format_progress("Translation").bright_magenta().bold());
    println!("│ {}", format!("Translating {} to Rust...", file_type).bright_blue().bold());
    translator::translate_c_to_rust(feature, file_type, &c_file, rs_file, show_full_output)?;

    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Translation failed: output file is empty");
    }
    println!("│ {}", format!("✓ Translation complete ({} bytes)", metadata.len()).bright_green());
    
    Ok(())
}

/// 在循环中构建并修复错误
fn build_and_fix_loop<F>(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    file_name: &str,
    format_progress: &F,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    show_full_output: bool,
) -> Result<bool>
where
    F: Fn(&str) -> String
{
    
    for attempt in 1..=max_fix_attempts {
        println!("│");
        println!("│ {}", format_progress("Build").bright_magenta().bold());
        println!("│ {}", format!("Building Rust project (attempt {}/{})", attempt, max_fix_attempts).bright_blue().bold());
        
        match builder::cargo_build(feature, show_full_output) {
            Ok(_) => {
                println!("│ {}", "✓ Build successful!".bright_green().bold());
                return Ok(true);
            }
            Err(build_error) => {
                if attempt == max_fix_attempts {
                    return handle_max_fix_attempts_reached(
                        build_error,
                        file_name,
                        rs_file,
                        is_last_attempt,
                        attempt_number,
                        max_fix_attempts,
                        feature,
                        file_type,
                    );
                } else {
                    // Show full fixed code for visibility, respect user's error preview preference
                    apply_error_fix(feature, file_type, rs_file, &build_error, format_progress, show_full_output)?;
                }
            }
        }
        
        println!("{}", "Updating code analysis...".bright_blue());
        analyzer::update_code_analysis(feature)?;
        println!("{}", "✓ Code analysis updated".bright_green());
    }
    
    Ok(false)
}

/// 处理达到最大修复尝试次数的情况
/// 如果处理应继续而不重试翻译，则返回 Ok(true)；如果应重试翻译，则返回 Ok(false)
fn handle_max_fix_attempts_reached(
    build_error: anyhow::Error,
    file_name: &str,
    rs_file: &std::path::Path,
    is_last_attempt: bool,
    attempt_number: usize,
    max_fix_attempts: usize,
    feature: &str,
    file_type: &str,
) -> Result<bool> {
    use constants::MAX_TRANSLATION_ATTEMPTS;
    
    println!("│");
    println!("│ {}", "⚠ Maximum fix attempts reached!".red().bold());
    println!("│ {}", format!("File {} still has build errors after {} fix attempts.", file_name, max_fix_attempts).yellow());
    
    // 显示代码比较和构建错误
    let c_file = rs_file.with_extension("c");
    
    // 显示文件位置
    interaction::display_file_paths(Some(&c_file), rs_file);
    
    // 使用差异显示进行更好的比较
    let error_message = format!("✗ Build Error:\n{}", build_error);
    if let Err(e) = diff_display::display_code_comparison(
        &c_file,
        rs_file,
        &error_message,
        diff_display::ResultType::BuildFail,
    ) {
        // 如果比较失败则回退到旧显示
        println!("│ {}", format!("Failed to display comparison: {}", e).yellow());
        println!("│ {}", "═══ C Source Code (Full) ═══".bright_cyan().bold());
        translator::display_code(&c_file, "─ C Source ─", usize::MAX, true);
        
        println!("│ {}", "═══ Rust Code (Full) ═══".bright_cyan().bold());
        translator::display_code(rs_file, "─ Rust Code ─", usize::MAX, true);
        
        println!("│ {}", "═══ Build Error ═══".bright_red().bold());
        println!("│ {}", build_error);
    }
    
    // 使用新提示获取用户选择
    let choice = interaction::prompt_compile_failure_choice()?;
    
    match choice {
        interaction::FailureChoice::AddSuggestion => {
            println!("│");
            println!("│ {}", "You chose: Add fix suggestion for AI to modify".bright_cyan());
            
            // 在提示新建议之前清除旧建议
            suggestion::clear_suggestions()?;
            
            // 从用户获取必需的建议
            let suggestion_text = interaction::prompt_suggestion(true)?
                .ok_or_else(|| anyhow::anyhow!(
                    "Suggestion is required for compilation failure but none was provided. \
                     This may indicate an issue with the prompt_suggestion function when require_input=true."
                ))?;
            
            // 将建议保存到 suggestions.txt
            suggestion::append_suggestion(&suggestion_text)?;
            
            // 如果我们仍然可以重试翻译，则执行
            if !is_last_attempt {
                let remaining_retries = MAX_TRANSLATION_ATTEMPTS - attempt_number;
                println!("│ {}", format!("Retrying translation from scratch... ({} retries remaining)", remaining_retries).bright_cyan());
                println!("│ {}", "Note: The translator will overwrite the existing file content.".bright_blue());
                println!("│ {}", "✓ Retry scheduled".bright_green());
                Ok(false) // 发出重试信号
            } else {
                // 没有更多翻译重试，但我们可以再次尝试修复
                println!("│ {}", "No translation retries remaining, attempting fix with new suggestion...".bright_yellow());
                
                // 应用带有建议的修复
                let format_progress = |op: &str| format!("Fix with suggestion - {}", op);
                apply_error_fix(feature, file_type, rs_file, &build_error, &format_progress, true)?;
                
                // 再试一次构建
                println!("│");
                println!("│ {}", "Building with applied fix...".bright_blue().bold());
                match builder::cargo_build(feature, true) {
                    Ok(_) => {
                        println!("│ {}", "✓ Build successful after applying suggestion!".bright_green().bold());
                        
                        println!("{}", "Updating code analysis...".bright_blue());
                        analyzer::update_code_analysis(feature)?;
                        println!("{}", "✓ Code analysis updated".bright_green());
                        
                        Ok(true)
                    }
                    Err(e) => {
                        println!("│ {}", "✗ Build still failing after fix attempt".red());
                        Err(e).context(format!(
                            "Build failed after fix with suggestion for file {}",
                            rs_file.display()
                        ))
                    }
                }
            }
        }
        interaction::FailureChoice::ManualFix => {
            println!("│");
            println!("│ {}", "You chose: Manual fix".bright_cyan());
            
            // 尝试打开 vim
            match interaction::open_in_vim(rs_file) {
                Ok(_) => {
                    // Vim 编辑后，重复尝试构建并允许用户
                    // 决定是重试还是退出，使用循环来避免递归
                    loop {
                        println!("│");
                        println!("│ {}", "Vim editing completed. Attempting to build...".bright_blue());
                        
                        // 手动编辑后尝试构建
                        match builder::cargo_build(feature, true) {
                            Ok(_) => {
                                println!("│ {}", "✓ Build successful after manual fix!".bright_green().bold());
                                
                                println!("{}", "Updating code analysis...".bright_blue());
                                analyzer::update_code_analysis(feature)?;
                                println!("{}", "✓ Code analysis updated".bright_green());
                                
                                return Ok(true);
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
                                        match interaction::open_in_vim(rs_file) {
                                            Ok(_) => {
                                                // 在额外的手动修复后，循环将重试构建
                                                continue;
                                            }
                                            Err(open_err) => {
                                                println!("│ {}", format!("Failed to reopen vim: {}", open_err).red());
                                                println!("│ {}", "Cannot continue manual fix flow; exiting.".yellow());
                                                return Err(open_err).context(format!(
                                                    "Build still failing and could not reopen vim for file {}",
                                                    rs_file.display()
                                                ));
                                            }
                                        }
                                    }
                                    interaction::UserChoice::Exit => {
                                        return Err(e).context(format!(
                                            "Build failed after manual fix for file {}",
                                            rs_file.display()
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("│ {}", format!("Failed to open vim: {}", e).red());
                    println!("│ {}", "Falling back to exit.".yellow());
                    Err(e).context(format!(
                        "Build failed (original error: {}) and could not open vim for file {}",
                        build_error,
                        rs_file.display()
                    ))
                }
            }
        }
        interaction::FailureChoice::Exit => {
            println!("│");
            println!("│ {}", "You chose: Exit".yellow());
            println!("│ {}", "Exiting due to build failures.".yellow());
            Err(build_error).context(format!(
                "Build failed after {} fix attempts for file {}. User chose to exit.",
                max_fix_attempts,
                rs_file.display()
            ))
        }
    }
}

/// 对文件应用错误修复
pub(crate) fn apply_error_fix<F>(
    feature: &str,
    file_type: &str,
    rs_file: &std::path::Path,
    build_error: &anyhow::Error,
    format_progress: &F,
    show_full_output: bool,
) -> Result<()>
where
    F: Fn(&str) -> String
{
    use std::fs;
    
    println!("│ {}", "⚠ Build failed, attempting to fix errors...".yellow().bold());
    println!("│");
    println!("│ {}", format_progress("Fix").bright_magenta().bold());
    // 始终显示完整的修复代码，但尊重用户对错误预览的偏好
    translator::fix_translation_error(
        feature, 
        file_type, 
        rs_file, 
        &build_error.to_string(), 
        show_full_output,  // 用户对错误预览的偏好
        true,              // 始终显示完整的修复代码
    )?;

    let metadata = fs::metadata(rs_file)?;
    if metadata.len() == 0 {
        anyhow::bail!("Fix failed: output file is empty");
    }
    println!("│ {}", "✓ Fix applied".bright_green());
    
    Ok(())
}

/// 完成文件处理（提交、分析、混合构建）
fn complete_file_processing<F>(
    feature: &str, 
    file_name: &str, 
    file_type: &str,
    rs_file: &std::path::Path,
    format_progress: &F
) -> Result<()>
where
    F: Fn(&str) -> String
{
    // 在提交之前首先运行混合构建测试
    println!("│");
    println!("│ {}", format_progress("Hybrid Build Tests").bright_magenta().bold());
    println!("│ {}", "Running hybrid build tests...".bright_blue());
    
    // 预检查（与 run_hybrid_build_interactive 中相同）
    let project_root = util::find_project_root()?;
    let config_path = project_root.join(".c2rust/config.toml");
    
    if !config_path.exists() {
        eprintln!("{}", format!("Error: Config file not found at {}", config_path.display()).red());
        anyhow::bail!("Config file not found, cannot run hybrid build tests");
    }

    // 继续之前检查 c2rust-config 是否可用
    let check_output = std::process::Command::new("c2rust-config")
        .arg("--help")
        .output();
    
    match check_output {
        Ok(output) => {
            if !output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "{}",
                    format!(
                        "Error: c2rust-config --version failed.\nstdout:\n{}\nstderr:\n{}",
                        stdout, stderr
                    )
                    .red()
                );
                anyhow::bail!(
                    "c2rust-config is present but failed to run successfully, cannot run hybrid build tests"
                );
            }
        }
        Err(_) => {
            eprintln!("{}", "Error: c2rust-config not found".red());
            anyhow::bail!("c2rust-config not found, cannot run hybrid build tests");
        }
    }
    
    // 使用自定义处理运行测试以检测成功/失败
    builder::c2rust_clean(feature)?;
    
    match builder::c2rust_build(feature) {
        Ok(_) => {
            println!("│ {}", "✓ Build successful".bright_green().bold());
        }
        Err(build_error) => {
            println!("│ {}", "✗ Build failed".red().bold());
            // Enter interactive build failure handling
            builder::handle_build_failure_interactive(feature, file_type, rs_file, build_error)?;
        }
    }
    
    let test_result = builder::c2rust_test(feature);
    
    match test_result {
        Ok(_) => {
            println!("│ {}", "✓ Hybrid build tests passed".bright_green().bold());
            
            // 如果不在自动接受模式下，显示代码比较和成功提示
            if !interaction::is_auto_accept_mode() {
                let c_file = rs_file.with_extension("c");
                
                // 显示文件位置
                interaction::display_file_paths(Some(&c_file), rs_file);
                
                // 使用差异显示进行更好的比较
                let success_message = "✓ All tests passed";
                if let Err(e) = diff_display::display_code_comparison(
                    &c_file,
                    rs_file,
                    success_message,
                    diff_display::ResultType::TestPass,
                ) {
                    // 如果比较失败则回退到简单消息
                    println!("│ {}", format!("Failed to display comparison: {}", e).yellow());
                    println!("│ {}", success_message.bright_green().bold());
                }
                
                // 获取用户选择
                let choice = interaction::prompt_compile_success_choice()?;
                
                match choice {
                    interaction::CompileSuccessChoice::Accept => {
                        println!("│ {}", "You chose: Accept this code".bright_cyan());
                        // 继续提交
                    }
                    interaction::CompileSuccessChoice::AutoAccept => {
                        println!("│ {}", "You chose: Auto-accept all subsequent translations".bright_cyan());
                        interaction::enable_auto_accept_mode();
                        // 继续提交
                    }
                    interaction::CompileSuccessChoice::ManualFix => {
                        println!("│ {}", "You chose: Manual fix".bright_cyan());
                        
                        // 打开 vim 进行手动编辑
                        match interaction::open_in_vim(rs_file) {
                            Ok(_) => {
                                // 编辑后，重新构建并再次测试
                                println!("│ {}", "Rebuilding and retesting after manual changes...".bright_blue());
                                builder::c2rust_build(feature)?;
                                builder::c2rust_test(feature)?;
                                println!("│ {}", "✓ Tests still pass after manual changes".bright_green());
                                // 继续提交
                            }
                            Err(e) => {
                                return Err(e).context("Failed to open vim for manual editing");
                            }
                        }
                    }
                    interaction::CompileSuccessChoice::Exit => {
                        println!("│ {}", "You chose: Exit".yellow());
                        anyhow::bail!("User chose to exit after successful tests");
                    }
                }
            } else {
                println!("│ {}", "Auto-accept mode: automatically accepting translation".bright_green());
            }
        }
        Err(test_error) => {
            // 测试失败 - 使用交互式处理器
            builder::handle_test_failure_interactive(feature, file_type, rs_file, test_error)?;
        }
    }
    
    // 提交更改
    println!("│");
    println!("│ {}", format_progress("Commit").bright_magenta().bold());
    println!("│ {}", "Committing changes...".bright_blue());
    git::git_commit(&format!("Translate {} from C to Rust (feature: {})", file_name, feature), feature)?;
    println!("│ {}", "✓ Changes committed".bright_green());

    // 更新代码分析
    println!("│");
    println!("│ {}", format_progress("Update Analysis").bright_magenta().bold());
    println!("│ {}", "Updating code analysis...".bright_blue());
    analyzer::update_code_analysis(feature)?;
    println!("│ {}", "✓ Code analysis updated".bright_green());

    // 提交分析
    println!("│");
    println!("│ {}", format_progress("Commit Analysis").bright_magenta().bold());
    git::git_commit(&format!("Update code analysis for {}", feature), feature)?;
    
    println!("{}", "└─ File processing complete".bright_white().bold());
    
    Ok(())
}
