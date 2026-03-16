use crate::util;
use anyhow::{Context, Result};
use std::process::Command;

/// Commit changes with a message.
/// Only stages the `.c2rust/` directory and the specific feature directory to avoid
/// committing unrelated local modifications.
pub fn git_commit(message: &str, feature: &str) -> Result<()> {
    let project_root = util::find_project_root()?;
    let c2rust_dir = project_root.join(".c2rust");

    // Only add the specific feature directory (not all features) to prevent
    // accidentally committing unrelated local changes.
    // The path is relative to the .c2rust directory (.c2rust/<feature>/rust/).
    let feature_rust_path = format!("{}/rust/", feature);
    let add_output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(["add", ".", &feature_rust_path])
        .output()
        .context("Failed to git add")?;

    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        anyhow::bail!("git add failed: {}", stderr);
    }

    // Commit from the .c2rust directory
    let commit_output = Command::new("git")
        .current_dir(&c2rust_dir)
        .args(["commit", "-m", message])
        .output()
        .context("Failed to git commit")?;

    if !commit_output.status.success() {
        let stdout = String::from_utf8_lossy(&commit_output.stdout);
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        let combined_output = format!("{}{}", stdout, stderr);
        let exit_code = commit_output.status.code();

        // Nothing to commit is not an error (git exits with code 1 in this case)
        let is_nothing_to_commit =
            exit_code == Some(1) && combined_output.contains("nothing to commit");

        if !is_nothing_to_commit {
            anyhow::bail!(
                "git commit failed with exit code {:?}: {}",
                exit_code,
                combined_output
            );
        }
    }

    Ok(())
}

/// Run garbage collection on the `.c2rust` repository to compact history objects
/// and reduce `.git` size.
///
/// All reachable commits and refs are preserved. Reflog-based recovery of unreachable
/// commits (e.g. via `HEAD@{n}`) is retained within the reflog expiry window set by
/// [`git_expire_reflog`] (90 days by default); objects outside that window may be pruned.
/// Recommended to be called periodically (e.g. every N files) or at the end of a
/// feature translation.
///
/// When `aggressive` is `true`, passes `--aggressive --prune=now` for stronger delta
/// recompression and immediate pruning of unreachable objects. Use `aggressive = false`
/// for cheap periodic runs (Git's default 2-week prune grace period applies) and
/// `aggressive = true` for the final end-of-feature cleanup.
///
/// All errors (including a missing git binary) are printed as warnings and never abort
/// the main workflow.
pub fn git_gc(aggressive: bool) {
    let project_root = match util::find_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Warning: git gc skipped, could not find project root: {}", e);
            return;
        }
    };
    let c2rust_dir = project_root.join(".c2rust");

    let mut args = vec!["gc"];
    if aggressive {
        args.push("--aggressive");
        args.push("--prune=now");
    }

    match Command::new("git")
        .current_dir(&c2rust_dir)
        .args(&args)
        .output()
    {
        Err(e) => {
            // Could not spawn git (e.g. not installed); warn and continue.
            eprintln!("Warning: failed to run git gc: {}", e);
        }
        Ok(gc_output) if !gc_output.status.success() => {
            let stderr = String::from_utf8_lossy(&gc_output.stderr);
            eprintln!("Warning: git gc failed: {}", stderr);
        }
        Ok(_) => {}
    }
}

/// Expire old reflog entries in the `.c2rust` repository to allow GC to reclaim
/// objects that are only referenced by stale reflog entries.
///
/// Uses `--expire=90.days.ago` and `--expire-unreachable=90.days.ago` to retain
/// recent reflog history for both reachable and unreachable commits, preserving
/// the ability to recover commits via `HEAD@{n}` or detached-HEAD recovery for
/// the past 90 days. (Git's default for unreachable entries is typically 30 days,
/// so the explicit flag is required to match the intended 90-day retention.)
/// Call this before [`git_gc`] so that GC can prune a larger set of unreachable
/// objects older than the retention window.
///
/// All errors are printed as warnings and never abort the main workflow.
pub fn git_expire_reflog() {
    let project_root = match util::find_project_root() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "Warning: git reflog expire skipped, could not find project root: {}",
                e
            );
            return;
        }
    };
    let c2rust_dir = project_root.join(".c2rust");

    match Command::new("git")
        .current_dir(&c2rust_dir)
        .args(["reflog", "expire", "--expire=90.days.ago", "--expire-unreachable=90.days.ago", "--all"])
        .output()
    {
        Err(e) => {
            eprintln!("Warning: failed to run git reflog expire: {}", e);
        }
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Warning: git reflog expire failed: {}", stderr);
        }
        Ok(_) => {}
    }
}
