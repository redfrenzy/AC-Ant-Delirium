# Plan - Task Orchestrator & Review Mode

## 1. Engine Analysis (AC-Ant-Delirium)
Currently, the `ac_ant_delirium` engine requires a dedicated orchestration loop for task tracking and evaluation. To enforce rigorous governance and learning, we must implement a dual-mode workflow (Implementation vs. Review) and a robust feedback mechanism for architectural blockers.

## 2. To-Do List Manifest Integration
Just as the engine currently generates `shadow_context.sarif.json` and `violations_manifest.json`, the orchestrator will now produce a specialized manifest in the artifacts directory:
**Path:** `Aesthetic-Red/AC_Ant-Delirium_props/Logs/artifacts/to-do_list_manifest.json` (or strictly `to-do_list_manifest` as a raw JSON file).

### Manifest Structure
This JSON manifest will contain:
- `complete_plan`: A string containing the overall plan and context related to the current phase.
- `current_task`: The current specific task assigned to the agent.
- `total_tasks`: A numeric counter for the total number of available tasks.
- `explanation`: A static instructional text:
  *"Tasks are evaluated based on their resolution; the goal isn't simply to tick a checkbox but to solve the problem. Tasks will be reviewed, and you will receive a score from 0.0 to 10.0. Use the command `--api hierarchy.task.[1.0].implemented = true`, for example, to notify the system that the task is complete; use the command `--api hierarchy.task.[1.0]` to view your grade, score, and general implementation details."*

## 3. Command API and Task Status Checks
The engine will expose a robust API for the agent to report progress and request information.

- **Status Check:** `--api hierarchy.task.[1.0]`
  Displays the current grade, score, and general implementation details for a specific task.
- **Completion Signal:** `--api hierarchy.task.[1.0].implemented = true`
  Used by the implementation agent to notify the system that the work is ready for review.

### Orchestration Modes
The API and manifest generation will dynamically adapt based on the `mode` defined in `task_orchestrator.json`.

1. **`implementation` Mode:**
   - Provides general instructions in the manifest on how to implement the task and access the API.
   - Emphasizes that the goal is actual implementation and problem-solving, not just checking a box.
   - The implementation agent cannot self-grade.
2. **`implementation review` Mode:**
   - The agent operates as a reviewer.
   - It evaluates the implemented work against the plan and assigns a final score (0.0 to 10.0), as previously established.
3. **`plan review` Mode:**
   - The agent analyzes whether the global plan aligns with the current codebase.
   - It determines if tasks are still necessary, or if they have already been resolved through other means (preventing redundant work due to outdated context).

## 4. Problem Reporting Questionnaire System
If the agent fails to complete the task or encounters a blocker, it must explain the difficulties using the problem API:
**Command:** `--api hierarchy.task.[X.X].problems`

### Root Categories
The questionnaire will present the following base possibilities:
- `governance_error`: Conflicts with strict code governance rules.
- `architecture_error`: Structural roadblocks or mismatches with expected patterns.
- `need_for_libraries`: Missing dependencies or tools required to solve the issue.
- `create_category`: A fallback where the agent suggests creating a new category to advise on how to handle an unforeseen difficulty.

### Dynamic Resolution Feedback
If an agent flags a specific category (e.g., `--api hierarchy.task.[1.0].problems.architecture_error`), the terminal will dynamically respond by:
1. Displaying known examples of how to resolve this specific architectural issue.
2. Alternatively, requesting the AI to write a detailed text block explaining the exact error encountered.

### Solution Expansion and Persistence
All problem reports, generated texts, and new categories will be serialized and saved in JSON format within the `plugins/` folder. This creates a growing knowledge base that expands our scope of solutions as the project develops.

## 5. Suggested API & Orchestrator Improvements (Not Yet Considered)
To fully capitalize on this JSON-based task reading system, we should consider the following future enhancements:
1. **Dependency Graphs:** Allow tasks to formally block one another in the JSON plan. The API could automatically reject `--api hierarchy.task.[2.0].implemented=true` if `[1.0]` is incomplete or scored below the minimum threshold.
2. **Auto-Regression Trigger:** If a task receives a score below the minimum threshold during Review Mode, the orchestrator should automatically revert the task state and inject the reviewer's feedback directly into the `to-do_list_manifest.json` `complete_plan` string for the implementation agent's next turn.
3. **Problem Knowledge Retrieval:** A dedicated query command (e.g., `--api hierarchy.problems.search <keyword>`) allowing the agent to query the saved JSON problem database for past solutions before resorting to creating a new problem category.

## 6. Implementation Task List (To-Do)
- [x] **Task 1: Manifest Generation Implementation**
  - Update the orchestrator loop to write `to-do_list_manifest.json` locally.
  - Populate fields: `complete_plan`, `current_task`, `total_tasks`.
  - Inject the static `explanation` string instructing agents on how to use the API and the grading rules.
- [x] **Task 2: Task Status & Completion API**
  - Expose `--api hierarchy.task.[X.X]` to view grade and implementation details.
  - Implement `--api hierarchy.task.[X.X].implemented = true` to allow agents to mark tasks as ready for review.
- [x] **Task 3: Review Mode Constraints**
  - Implement a restriction logic in the orchestrator reading the JSON configuration.
  - Ensure that an implementation agent cannot self-grade tasks, explicitly requiring `mode: review` in the payload to invoke grading features.
- [x] **Task 4: Questionnaire Routing & API Integration**
  - Build the parser for `--api hierarchy.task.[X.X].problems.<category>`.
  - Support the base categories: `governance_error`, `architecture_error`, `need_for_libraries`, and `create_category`.
- [x] **Task 5: Terminal Feedback & Problem Resolution**
  - Display dynamic feedback (examples or request for more text) in the terminal when a problem category is invoked.
  - Build the JSON serializer to persist the problem reports into `plugins/` (e.g., `plugins/Aesthetic-Red/problems/`).
- [x] **Task 6: Validation & Agent Testing**
  - Write test cases or manually validate the API commands via the CLI to ensure proper state mutations and JSON persistence without crashing the build guard.
- [ ] **Task 7: Mode-Specific Manifest Instructions**
  - Update `write_todo_manifest` to parse the `mode` field from `task_orchestrator.json`.
  - Dynamically alter the `explanation` string in `to-do_list_manifest.json` based on whether the mode is `implementation`, `implementation review`, or `plan review`.
- [ ] **Task 8: Plan Review Mechanics**
  - Define criteria or an API command for agents operating in `plan review` mode to mark a plan as "outdated" or "resolved externally" without executing standard implementation code.
  - Implement a mechanism to safely skip or archive tasks that are no longer necessary.
