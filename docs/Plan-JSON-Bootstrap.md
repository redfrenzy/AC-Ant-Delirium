# Plan - JSON Bootstrap and Context Injection

## 1. Scope and Objective
Currently, several critical `.json` files within the `plugins/<Project>` directory (such as `task_orchestrator.json`, `connected.json`, etc.) are generated manually. This process needs to be standardized. 

The goal of this plan is to automate the initialization of these files during the `config init` command. Furthermore, to provide essential context to future AI agents, these generated JSON templates must include "inline comments" (implemented as `__comment_<field>` string pairs). 

Finally, we will implement a new command (`config new_task "Task Title"`) to streamline the creation of properly numbered and heavily commented task plan files in the `plan/` directory.

## 2. Standardized File Bootstrapping (`config init`)
When the `config init` command is executed, the engine must now automatically create the following files if they do not already exist:
- `connected.json`
- `context.json`
- `feedback.json`
- `governance_history.json`
- `governance.json`
- `manager_tasks.json`
- `messages.json`
- `submit_queue.json`
- `task_orchestrator.json`

### JSON Commenting Strategy
Since standard JSON does not support `//` or `/* */`, context will be injected using descriptive keys. For example, `task_orchestrator.json` might look like:
```json
{
  "__comment_orchestration": "This object governs the current active plan and the operational mode of the agent.",
  "orchestration": {
    "__comment_active_plan": "The relative path to the currently active plan file.",
    "active_plan": "../plugins/Aesthetic-Red/plan/materials/[0.0]_Sample_Plan.json",
    "__comment_mode": "Valid modes: 'implementation', 'implementation review', 'plan review'.",
    "mode": "implementation",
    "current_step_id": "none",
    "grade": 0.0
  }
}
```

## 3. Sample Plan Generation
During `config init`, the engine should also scaffold the `plan/` folder and generate a sample file (e.g., `plan/[0.0]_Sample_Plan.json`). This file must act as a tutorial for future agents, using `__comment` fields to explain:
- How the `plan_id` dictates the numerical ordering.
- How the `complete_plan` field should provide complete self-contained context in plain text.
- How to structure the `tasks` array (step_id, task_prompt, implemented, reviewed).

## 4. The `config new_task` Command
To eliminate the friction of manually creating and numbering new plans, a new command will be added:
```bash
ac_ant_delirium config new_task "Task Title"
```

### Execution Flow:
1. The engine scans the `plan/` (or `plan/materials/`) folder to determine the highest existing `plan_id` (e.g., if `[1.1]` exists, the next is `[1.2]`).
2. It generates a valid JSON filename: `[1.2]_Task_Title.json`.
3. It creates the file with a pre-filled, commented JSON template guiding the AI on how to populate the complete_plan and tasks.

## 5. Implementation Task List (To-Do)

- [x] **Task 1: Extend `config init` Parser**
  Modify the `config init` logic in `Agent-Runtime_unitcore.rs` to generate the 9 base governance JSON files with hardcoded, heavily-commented default strings.

- [x] **Task 2: Implement Sample Task Generator**
  Expand the `config init` workflow to create the `plan/` directory and write the `[0.0]_Sample_Plan.json` tutorial file.

- [x] **Task 3: Add `config new_task` CLI Parser**
  Update `Main-Logic_unitcore.rs` and the `Cli_Args_struct.rs` to parse `config new_task <title>`. Add the command to the `--help` docs.

- [x] **Task 4: Implement `new_task` Auto-Numbering Logic**
  Write the logic in `Agent-Runtime_unitcore.rs` to read the existing files in the `plan/` directory, extract the highest numerical prefix `[X.X]`, and calculate the next incremental ID.

- [x] **Task 5: Generate the `new_task` Template**
  Implement the file writer that outputs the properly numbered JSON plan file containing structural `__comment` fields to guide AIs during future edits.
