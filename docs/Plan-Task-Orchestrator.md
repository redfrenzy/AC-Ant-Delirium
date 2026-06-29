# Plan - Task Orchestrator & Review Mode

## 1. Engine Analysis (AC-Ant-Delirium)
Currently, the `ac_ant_delirium` engine **does not support** this specific orchestration mode. 
The `ManagerTask` struct inside `src/structs/Agent_Runtime_struct.rs` only has primitive fields (`id`, `title`, `body`, `status`, `priority`). It lacks any state for a "review mode", scoring (0.0 to 10.0), or fine-grained checkbox states (`implemented=true`, `reviewed=true`). 

**Conclusion:** Major changes are required in the core engine (`AC-Ant-Delirium`) to support transversal task modes (Implementation vs Review). This aligns perfectly with the exception listed in Phase 8 of your "Ritual de Criacao de Rule" document: *"Quando editar engine: Somente se precisar novo comportamento transversal"*. Task orchestration and scoring is a new transversal behavior.

## 2. Proposed JSON Structures

### A. Numbered Plan JSON (e.g., `[1.3]_Zero_Cost_UI.json`)
This replaces the old monolithic `.md` checklists.
```json
{
  "plan_id": "1.3",
  "title": "Zero Cost Simple UI Mode",
  "objective": "Convert UI rendering to zero cost by disabling PBR passes.",
  "tasks": [
    {
      "step_id": "1.3.1",
      "description": "Fix FinalColor contract in aero.wgsl",
      "implemented": false,
      "reviewed": false
    },
    {
      "step_id": "1.3.2",
      "description": "Lock cost contract: Interface does not call direct light.",
      "implemented": false,
      "reviewed": false
    }
  ]
}
```

### B. Task Orchestrator JSON (`task_orchestrator.json`)
This replaces/extends `manager_tasks.json` to handle the iterative handshake.
```json
{
  "orchestration": {
    "active_plan": "plugins/Aesthetic-Red/plan/[1.3]_Zero_Cost_UI.json",
    "current_step_id": "1.3.1",
    "mode": "review", 
    "grade": 0.0
  }
}
```

## 3. Implementation Task List (AC-Ant-Delirium)

- [ ] **Task 1: Update Runtime Structs** 
  - Edit `AC-Ant-Delirium/src/structs/Agent_Runtime_struct.rs`. Add `mode` (String: "implementation" | "review") and `grade` (f64) to the task payload. Add definitions for the new Numbered Plan JSON schema.
- [ ] **Task 2: Implement Orchestrator Parsing**
  - Implement parsing logic to read `task_orchestrator.json` alongside `manager_tasks.json`.
- [ ] **Task 3: Terminal Output Logic for Review Mode**
  - Edit the `--next-action` and `--whoami` CLI outputs (likely in `src/Agent-Runtime_unitcore.rs` or `Main-Logic_unitcore.rs`).
  - Inject the explicit prompt: *"The goal is to solve the task, not just check the box as done; solve the task below, and the reviewer will assign an implementation grade."* ONLY when `mode == "review"`.
- [ ] **Task 4: Add Grading API to Submit Command**
  - Add logic in `Cli_Args_struct.rs` to allow the reviewer agent to assign a grade (e.g., `--submit-grade 8.5`) when evaluating the implementation.
- [ ] **Task 5: Update JSON Checkboxes on Grade**
  - When a grade >= passing threshold is assigned, the engine should automatically flip the hidden `reviewed=true` flag in the target `[X.Y]_plan.json` file.
- [ ] **Task 6: TOML Configuration (Ritual Phase 2)**
  - Ensure the passing grade threshold (e.g., `min_passing_grade = 7.0`) is defined in `default_hierarchy.toml` under a new `hierarchy.orchestration` key.
- [ ] **Task 7: Unit Tests (Ritual Phase 9)**
  - Create tests simulating an agent calling `--next-action` in `review` mode and verifying the explicit instructions are printed, and that `implemented` and `reviewed` toggle correctly.
- [ ] **Task 8: Build and Validate (Ritual Phase 10)**
  - Recompile `delirium_cli` and install it into `plugins/Aesthetic-Red/ac_ant_delirium`. Test the workflow inside the Aesthetic-Red workspace.
