import re

file_path = "src/Agent-Runtime_unitcore.rs"

with open(file_path, "r") as f:
    content = f.read()

# Add auto_advance_orchestrator function
new_func = """
fn auto_advance_orchestrator(plugin: &PluginContext, orchestrator: &mut TaskOrchestrator, current_plan: &NumberedPlan) -> io::Result<bool> {
    let mode = orchestrator.orchestration.mode.as_str();
    
    // Check if current plan is complete for the current mode
    let is_complete = match mode {
        "plan review" => current_plan.tasks.iter().all(|t| t.task_reviewed),
        "implementation review" => current_plan.tasks.iter().all(|t| t.reviewed || !t.implemented),
        _ => current_plan.tasks.iter().all(|t| t.implemented),
    };
    
    if !is_complete {
        return Ok(false);
    }
    
    // It's complete. We need to find the next plan.
    let mut all_plans = Vec::new();
    let plan_root = plugin.plugin_dir.join("plan");
    
    // Simple recursive search for .json files
    fn visit_dirs(dir: &Path, cb: &mut dyn FnMut(&Path)) -> io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dirs(&path, cb)?;
                } else if let Some(ext) = path.extension() {
                    if ext == "json" {
                        cb(&path);
                    }
                }
            }
        }
        Ok(())
    }
    
    visit_dirs(&plan_root, &mut |p| {
        all_plans.push(p.to_path_buf());
    }).unwrap_or(());
    
    // Sort them alphabetically which naturally handles [1.1], [1.2] etc if formatted correctly
    all_plans.sort();
    
    for plan_path_buf in all_plans {
        if let Ok(plan) = read_numbered_plan(&plan_path_buf) {
            let plan_is_complete = match mode {
                "plan review" => plan.tasks.iter().all(|t| t.task_reviewed),
                "implementation review" => plan.tasks.iter().all(|t| t.reviewed || !t.implemented),
                _ => plan.tasks.iter().all(|t| t.implemented),
            };
            if !plan_is_complete {
                // Found next plan!
                if let Ok(rel_path) = plan_path_buf.strip_prefix(&plugin.workspace_path) {
                    let new_active_plan = format!("../{}", rel_path.display());
                    orchestrator.orchestration.active_plan = new_active_plan;
                    // Reset current_step_id to the first pending task
                    if let Some(first_pending) = plan.tasks.iter().find(|t| {
                        match mode {
                            "plan review" => !t.task_reviewed,
                            "implementation review" => t.implemented && !t.reviewed,
                            _ => !t.implemented,
                        }
                    }) {
                        orchestrator.orchestration.current_step_id = first_pending.step_id.clone();
                    }
                    write_task_orchestrator(&plugin.task_orchestrator_path(), orchestrator)?;
                    return Ok(true);
                }
            }
        }
    }
    
    Ok(false)
}
"""

content = content.replace("pub(super) fn write_todo_manifest", new_func + "\n\npub(super) fn write_todo_manifest")

# Now inject the call to auto_advance_orchestrator in handle_task_api
# inside `.implemented`
hook_impl = """            write_numbered_plan(&plan_path, &plan)?;
            let mut auto_adv = String::new();
            if auto_advance_orchestrator(plugin, &mut orchestrator, &plan).unwrap_or(false) {
                auto_adv = format!("\\nAuto-advanced to next plan: {}", orchestrator.orchestration.active_plan);
            }
            return Ok(format!("Task {} marked as implemented with explanation.{}", step_id_with_brackets, auto_adv));"""

content = content.replace("""            write_numbered_plan(&plan_path, &plan)?;
            return Ok(format!("Task {} marked as implemented with explanation.", step_id_with_brackets));""", hook_impl)

# inside `.task_reviewed`
hook_rev = """            write_numbered_plan(&plan_path, &plan)?;
            let mut auto_adv = String::new();
            if auto_advance_orchestrator(plugin, &mut orchestrator, &plan).unwrap_or(false) {
                auto_adv = format!("\\nAuto-advanced to next plan: {}", orchestrator.orchestration.active_plan);
            }
            return Ok(format!("Task {} marked as task_reviewed.{}", step_id_with_brackets, auto_adv));"""

content = content.replace("""            write_numbered_plan(&plan_path, &plan)?;
            return Ok(format!("Task {} marked as task_reviewed.", step_id_with_brackets));""", hook_rev)

# inside `.grade`
hook_grade = """                write_numbered_plan(&plan_path, &plan)?;
                
                let mut auto_adv = String::new();
                if auto_advance_orchestrator(plugin, &mut orchestrator, &plan).unwrap_or(false) {
                    auto_adv = format!("\\nAuto-advanced to next plan: {}", orchestrator.orchestration.active_plan);
                }
                
                return Ok(format!("Task {} graded with score {}.{}", step_id_with_brackets, score, auto_adv));"""

content = content.replace("""                write_numbered_plan(&plan_path, &plan)?;
                
                return Ok(format!("Task {} graded with score {}.", step_id_with_brackets, score));""", hook_grade)

with open(file_path, "w") as f:
    f.write(content)
