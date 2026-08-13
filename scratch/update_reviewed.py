import re

file_path = "AC-Ant-Delirium/src/Agent-Runtime_unitcore.rs"
with open(file_path, "r") as f:
    content = f.read()

# 1. DEFAULT_SAMPLE_PLAN_JSON
content = content.replace(
    '"description": "Example task to demonstrate the JSON schema.",\n      "implemented": false,',
    '"description": "Example task to demonstrate the JSON schema.",\n      "task_reviewed": false,\n      "implemented": false,'
)

# 2. read_numbered_plan
content = content.replace(
    'description: extract_string_field(&obj, "description").unwrap_or_default(),\n        implemented: extract_bool_field(&obj, "implemented").unwrap_or(false),',
    'description: extract_string_field(&obj, "description").unwrap_or_default(),\n        task_reviewed: extract_bool_field(&obj, "task_reviewed").unwrap_or(false),\n        implemented: extract_bool_field(&obj, "implemented").unwrap_or(false),'
)

# 3. write_numbered_plan format
content = content.replace(
    '"    {{\\n      \\"step_id\\": \\"{}\\",\\n      \\"description\\": \\"{}\\",\\n      \\"implemented\\": {},\\n      \\"reviewed\\": {},\\n      \\"explanation\\": \\"{}\\"\\n    }}{}\\n",\n            json_escape(&task.step_id),\n            json_escape(&task.description),\n            if task.implemented { "true" } else { "false" },',
    '"    {{\\n      \\"step_id\\": \\"{}\\",\\n      \\"description\\": \\"{}\\",\\n      \\"task_reviewed\\": {},\\n      \\"implemented\\": {},\\n      \\"reviewed\\": {},\\n      \\"explanation\\": \\"{}\\"\\n    }}{}\\n",\n            json_escape(&task.step_id),\n            json_escape(&task.description),\n            if task.task_reviewed { "true" } else { "false" },\n            if task.implemented { "true" } else { "false" },'
)

# 4. handle_task_api info block
content = content.replace(
    'out.push_str(&format!("Implemented: {}\\n", task.implemented));\n        out.push_str(&format!("Reviewed: {}\\n", task.reviewed));',
    'out.push_str(&format!("Task Reviewed: {}\\n", task.task_reviewed));\n        out.push_str(&format!("Implemented: {}\\n", task.implemented));\n        out.push_str(&format!("Reviewed: {}\\n", task.reviewed));'
)

# 5. handle_task_api .grade check
content = content.replace(
    'if orchestrator.orchestration.mode != "review" {',
    'if orchestrator.orchestration.mode != "implementation review" {'
)
content = content.replace(
    "You must be in 'review' mode to assign a score.",
    "You must be in 'implementation review' mode to assign a score."
)

# 6. handle_task_api .task_reviewed addition
task_reviewed_handler = """
    if remainder.starts_with(".task_reviewed") {
        let val_part = remainder.split('=').nth(1).unwrap_or("").trim();
        if val_part == "true" || val_part == "\\"true\\"" || val_part == "'true'" {
            plan.tasks[task_idx].task_reviewed = true;
            write_numbered_plan(&plan_path, &plan)?;
            return Ok(format!("Task {} marked as task_reviewed.", step_id_with_brackets));
        } else if val_part == "false" || val_part == "\\"false\\"" || val_part == "'false'" {
            plan.tasks[task_idx].task_reviewed = false;
            write_numbered_plan(&plan_path, &plan)?;
            return Ok(format!("Task {} marked as NOT task_reviewed.", step_id_with_brackets));
        } else {
            return Ok("Usage: --api \\"hierarchy.task.[X.X].task_reviewed=true\\"".to_string());
        }
    }
"""
content = content.replace(
    'if remainder.starts_with(".grade") {',
    task_reviewed_handler + '\n    if remainder.starts_with(".grade") {'
)

# 7. write_todo_manifest
content = content.replace(
    '"{\\n    \\"step_id\\": \\"{}\\",\\n    \\"description\\": \\"{}\\",\\n    \\"implemented\\": {},\\n    \\"reviewed\\": {},\\n    \\"explanation\\": \\"{}\\"\\n  }",\n            json_escape(&t.step_id),\n            json_escape(&t.description),\n            if t.implemented { "true" } else { "false" },',
    '"{\\n    \\"step_id\\": \\"{}\\",\\n    \\"description\\": \\"{}\\",\\n    \\"task_reviewed\\": {},\\n    \\"implemented\\": {},\\n    \\"reviewed\\": {},\\n    \\"explanation\\": \\"{}\\"\\n  }",\n            json_escape(&t.step_id),\n            json_escape(&t.description),\n            if t.task_reviewed { "true" } else { "false" },\n            if t.implemented { "true" } else { "false" },'
)

# 8. materialize_new_task
content = content.replace(
    '"description": "Initial task.",\n      "implemented": false,',
    '"description": "Initial task.",\n      "task_reviewed": false,\n      "implemented": false,'
)

with open(file_path, "w") as f:
    f.write(content)
print("Updated Rust engine with task_reviewed.")
