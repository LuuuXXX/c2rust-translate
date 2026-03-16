use crate::util;
use anyhow::{Context, Result};
use std::process::Command;

/// 运行 code_analyse 并传递额外参数；失败时包含 feature、stdout 和 stderr。
fn run_code_analyse(feature: &str, extra_args: &[&str]) -> Result<()> {
    let project_root = util::find_project_root()?;

    let mut args = vec!["--update", "--feature", feature];
    args.extend_from_slice(extra_args);

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(&args)
        .output()
        .with_context(|| {
            format!(
                "Failed to execute code_analyse --update --feature {} {}",
                feature,
                extra_args.join(" ")
            )
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "code_analyse --update --feature {} {} failed:\nstdout: {}\nstderr: {}",
            feature,
            extra_args.join(" "),
            stdout,
            stderr
        );
    }

    Ok(())
}

/// 为功能初始化代码分析
pub fn initialize_feature(feature: &str) -> Result<()> {
    println!("Running code_analyse --init --feature {}", feature);

    let project_root = util::find_project_root()?;

    let output = Command::new("code_analyse")
        .current_dir(&project_root)
        .args(["--init", "--feature", feature])
        .output()
        .with_context(|| format!("Failed to execute code_analyse --init --feature {}", feature))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "code_analyse --init --feature {} failed:\nstdout: {}\nstderr: {}",
            feature,
            stdout,
            stderr
        );
    }

    Ok(())
}

/// 为功能更新代码分析
pub fn update_code_analysis(feature: &str) -> Result<()> {
    run_code_analyse(feature, &[])
}

/// 在所有文件构建和测试通过后，更新代码分析并标记构建成功
pub fn update_code_analysis_build_success(feature: &str) -> Result<()> {
    run_code_analyse(feature, &["--build-success"])
}
