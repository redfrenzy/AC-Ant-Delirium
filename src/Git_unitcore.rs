use ac_ant_delirium::structs::PluginContext;
use std::io;

/// Commit the current governance state to the internal pure-Rust Git repository.
pub fn commit_governance_state(plugin: &PluginContext, task_id: &str) -> io::Result<()> {
    let state_root = plugin.state_root();
    let repo_path = state_root.root();

    // This uses gix to initialize or open the repository.
    let _repo = match gix::discover(repo_path) {
        Ok(repo) => repo,
        Err(_) => {
            // Initialize if not found
            match gix::init(repo_path) {
                Ok(repo) => repo,
                Err(e) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to init gix repo: {}", e),
                    ))
                }
            }
        }
    };

    // Note: A full gix 'add' and 'commit' implementation requires interacting with gix::index
    // For now, if the API is too complex, we fallback to a native standard command
    // but the goal is to make it 100% gix.

    // As a temporary functional bridge, we shell out to git if gix logic is incomplete,
    // though the architecture plan calls for full gix.
    let _ = std::process::Command::new("git")
        .current_dir(repo_path)
        .arg("add")
        .arg(".")
        .output();

    let _ = std::process::Command::new("git")
        .current_dir(repo_path)
        .arg("commit")
        .arg("-m")
        .arg(format!(
            "Governance Checkpoint: Completed Task [{}]",
            task_id
        ))
        .output();

    Ok(())
}

/// Copies the core engine files into the plugin's project_state directory to create a standalone version.
/// Returns a string describing the diff-stat of what was modified.
pub fn capture_project_snapshot(plugin: &PluginContext) -> io::Result<String> {
    let state_dir = plugin.state_root().project_state_dir();

    // Create project_state directory if it doesn't exist
    let _ = std::fs::create_dir_all(&state_dir);

    // Use rsync to efficiently sync the codebase, excluding governance/meta folders
    let _ = std::process::Command::new("rsync")
        .arg("-a")
        .arg("--delete")
        .arg("--exclude=.git")
        .arg("--exclude=target")
        .arg("--exclude=plugins")
        .arg("--exclude=AC-Ant-Delirium")
        .arg("--exclude=AC_Ant-Delirium_props")
        .arg(format!("{}/", plugin.workspace_path.display()))
        .arg(format!("{}/", state_dir.display()))
        .output();

    // Generate a quick stat of what the agent actually changed in the engine repo to save in the JSON context.
    let diff_output = std::process::Command::new("git")
        .current_dir(&plugin.workspace_path)
        .arg("diff")
        .arg("HEAD")
        .arg("--stat")
        .output()?;

    let stat = String::from_utf8_lossy(&diff_output.stdout).to_string();
    let stat = if stat.trim().is_empty() {
        "No engine files were modified.".to_string()
    } else {
        format!("Files modified:\n{}", stat.trim())
    };

    Ok(stat)
}

pub fn handle_git_orchestrator_api(plugin: &PluginContext, query: &str) -> io::Result<String> {
    let state_root = plugin.state_root();
    let repo_path = state_root.root();

    // Only available in 'git orchestrator' mode
    if query == "hierarchy.git.log" {
        // Implement gix log
        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .arg("log")
            .arg("--oneline")
            .arg("-n")
            .arg("10")
            .output()?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    if query.starts_with("hierarchy.git.rollback=") {
        let commit_hash = query
            .strip_prefix("hierarchy.git.rollback=")
            .unwrap_or("")
            .trim();
        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .arg("reset")
            .arg("--hard")
            .arg(commit_hash)
            .output()?;
        return Ok(format!(
            "Rollback to {}: {}",
            commit_hash,
            String::from_utf8_lossy(&output.stdout)
        ));
    }

    if query == "hierarchy.git.status" {
        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .arg("status")
            .output()?;
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    Ok("Unknown git orchestrator command.".to_string())
}

pub fn handle_context_api(plugin: &PluginContext, query: &str) -> io::Result<String> {
    let state_root = plugin.state_root();
    let repo_path = state_root.root();

    // query is like "hierarchy.context.1.14.0"
    let task_id = query
        .strip_prefix("hierarchy.context.")
        .unwrap_or("")
        .trim();
    if task_id.is_empty() {
        return Ok("Error: No task ID provided.".to_string());
    }

    // Find the commit in the plugin repository associated with this task
    let log_output = std::process::Command::new("git")
        .current_dir(repo_path)
        .arg("log")
        .arg("--grep")
        .arg(format!("\\[{}\\]", task_id))
        .arg("--format=%H")
        .arg("-1")
        .output()?;

    let commit_hash = String::from_utf8_lossy(&log_output.stdout)
        .trim()
        .to_string();
    if commit_hash.is_empty() {
        return Ok(format!("No snapshot found for task {}. This task may not have been implemented under the new Standalone Snapshot architecture.", task_id));
    }

    // Get the diff of the project_state folder for that commit
    let diff_output = std::process::Command::new("git")
        .current_dir(repo_path)
        .arg("show")
        .arg(commit_hash)
        .arg("--")
        .arg("project_state/")
        .output()?;

    let diff_str = String::from_utf8_lossy(&diff_output.stdout).to_string();

    // Clean up the diff string so it doesn't say "project_state/" everywhere
    let cleaned_diff = diff_str
        .replace("a/project_state/", "a/")
        .replace("b/project_state/", "b/");

    Ok(format!(
        "Context Snapshot for Task [{}]:\n\n{}",
        task_id, cleaned_diff
    ))
}
