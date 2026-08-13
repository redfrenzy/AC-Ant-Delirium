# Global Transition Task Delivery

## Objective

Implement Ant-Delirium support for reusable transition tasks that appear between JSON task files in a plugin plan stream.

This is an Ant-Delirium engine feature, not an Aesthetic-Red task file. Aesthetic-Red may provide project-local transition task data at `plugins/Aesthetic-Red/Global/transition_task.json`, but the loader, final-plan-boundary detection, step-id projection, delivery behavior, and cleanup safety logic belong in the Ant-Delirium Rust codebase.

The operational problem is that agents can leave background commands, dev servers, PTY sessions, log streams, file locks, or validation processes running when a plan file completes and the next plan file begins. Those leftovers can degrade IDE performance, hold ports, consume CPU/GPU, or corrupt the assumptions of the next task.

## Current System

The current task advancement logic is in:

- `AC-Ant-Delirium/src/Agent-Runtime_unitcore.rs`
- `AC-Ant-Delirium/src/Main-Logic_unitcore.rs`
- `AC-Ant-Delirium/src/structs/Agent_Runtime_struct.rs`

The current state file is project-local:

- `plugins/Aesthetic-Red/task_orchestrator.json`

The current task files are project-local:

- `plugins/Aesthetic-Red/plan/**/*.json`

The current terminal manifest is written to:

- `AC_Ant-Delirium_props/Logs/artifacts/to-do_list_manifest.json`

## Current Algorithm

`auto_advance_orchestrator` reads the active plan and mode from `task_orchestrator.json`.

For the active plan, it searches the task list in file order for the first pending task:

- `plan review`: first task where `task_reviewed == false`
- `implementation review`: first task where `implemented == true && review_status == "none"`
- `implementation`: first task where `implemented == false`

If a pending task exists, the orchestrator updates `current_step_id` to that task and stays in the same plan.

If no pending task exists, the plan is treated as complete. The engine then recursively scans the plugin `plan` directory, sorts plan files by the numeric version inside filename brackets, and selects the first incomplete plan for the current mode.

Important gap: the current engine does not have a transition-task stage. It does not compute the final task id of the completed plan for continuation numbering. It simply sees that no pending task remains, then selects the next incomplete plan.

## Required Feature

When a plan file reaches completion, Ant-Delirium should insert a transition-task phase before selecting the next plan.

The transition tasks should be loaded from:

```text
plugins/<project>/Global/transition_task.json
```

For Aesthetic-Red, that path is:

```text
plugins/Aesthetic-Red/Global/transition_task.json
```

The transition file should be reusable across all plan files in the project. It should store local transition ids such as `1.0`, `1.1`, `1.2`, or another stable internal sequence. Those source ids are not written into the completed plan file.

At delivery time, the engine projects transition tasks as a continuation of the completed plan's task ids:

- Plan final task `1.10` -> transition tasks appear as `1.11`, `1.12`, `1.13`
- Plan final task `1.15` -> transition tasks appear as `1.16`, `1.17`, `1.18`

## Transparency And Provenance

Transition tasks can use the same displayed task object shape as ordinary tasks so agents process them through the normal implementation flow.

The system must still keep truthful internal provenance:

- source file: `plugins/<project>/Global/transition_task.json`
- original transition task id
- projected display step id
- completed source plan id
- completed source plan path
- cleanup evidence

The display can be seamless. The audit trail must not be false.

## Final-Task Detection Algorithm

The transition phase needs a helper that computes the final task id for a plan.

Required behavior:

1. Load the completed `NumberedPlan`.
2. Ignore empty task lists. Empty plans do not produce transition tasks.
3. Parse every task `step_id` into numeric segments split on `.`.
4. Compare segment vectors numerically, not lexically.
5. Select the maximum numeric step id.
6. Increment the last numeric segment to project transition task ids.
7. Preserve the completed plan's prefix segments.

Examples:

```text
1.9  < 1.10
1.10 -> first transition id 1.11
1.15 -> first transition id 1.16
1.2.9 -> first transition id 1.2.10
```

If a task id cannot be parsed into numeric segments, the engine should reject transition injection for that plan with an explicit diagnostic. It should not guess.

## Transition Phase State

Ant-Delirium should represent transition work as a distinct orchestrator phase or sub-state, not by mutating the completed plan file.

The state should be enough to answer:

- Which plan just completed?
- Which projected transition task is active?
- Which source transition task id does it map to?
- Which transition tasks have been completed?
- Is the next plan selection blocked until transition cleanup passes?

Suggested fields can live in `task_orchestrator.json` or a companion runtime state file, depending on the existing architecture:

```json
{
  "transition": {
    "active": true,
    "source_plan": "../plugins/Aesthetic-Red/plan/widgets/[1.36]_Plan___TextBlock_Widget.json",
    "source_plan_id": "1.36",
    "final_step_id": "1.36.9",
    "transition_source": "../plugins/Aesthetic-Red/Global/transition_task.json",
    "current_transition_id": "1.0",
    "current_projected_step_id": "1.36.10"
  }
}
```

## Transition Task Fixture Shape

The fixture should use the ordinary task fields where possible:

```json
{
  "transition_tasks": [
    {
      "transition_task_id": "1.0",
      "task_prompt": "Enumerate task-owned background processes and sessions.",
      "task_reviewed": true,
      "review_explanation": "Global transition policy task.",
      "implemented": false,
      "implementation_explanation": "",
      "implementation_context": "Only inspect processes registered to the completed task session.",
      "review_status": "none",
      "grade_explanation": ""
    }
  ]
}
```

The display adapter should convert that source object into the normal current-task object with the projected `step_id`.

The source tasks are not copied into every numbered plan JSON. After the final numbered task completes, the runtime projects these shared tasks as sequential continuation steps and delivers each full `task_prompt` through the normal terminal and `to-do_list_manifest.json` surfaces. The agent should experience an uninterrupted task queue without needing to know that an internal transition phase exists.

Every transition `task_prompt` must be self-contained. `implementation_context` is supplementary metadata and must not contain a safety rule, ownership restriction, validation requirement, or stop condition that is absent from `task_prompt`, because not every terminal presentation includes `implementation_context`.

## Default Transition Tasks

The default transition task set should use four projected continuation tasks:

1. Inventory only resources personally started by the current agent during the current task, using direct current-session or project-local ownership evidence. This task is audit-only and treats an empty inventory as success.
2. Gracefully stop ownership-proven background commands, process groups, PTYs, development servers, watchers, log streams, and listeners; wait for a bounded timeout; escalate only an ownership-proven resource that remains active and cannot affect another agent or user process.
3. Release ownership-proven locks, PID or session files, sockets, temporary scripts, and temporary directories only after confirming that no live owned process requires them.
4. Recheck only the known inventory, persist compact cleanup evidence, account for every owned resource, preserve uncertain resources, and continue automatically unless safe progress requires elevated privileges or touching an uncertain or shared resource.

No-op cleanup must complete normally and immediately. Uncertain resources that do not block progress must be recorded and left untouched rather than pausing the queue. Whole-system process scans, broad process-name termination, broad port matching, `sudo`, and cleanup of another agent's resources are prohibited.

## Process Ownership Rules

Cleanup must be ownership-based.

Ant-Delirium should maintain a process/session registry with:

- command
- cwd
- pid or process group
- launch surface
- owner plan id
- owner task id
- start timestamp
- associated PTY/session id, if any
- known ports, if any

Unknown processes must be reported, not killed.

Cleanup order:

1. Send graceful termination to registered owned processes.
2. Wait for a configured timeout.
3. Force terminate only registered owned processes that remain alive.
4. Report unknown processes and blocked resources without terminating them.

## Implementation Surfaces

Likely Rust implementation surfaces:

- `AC-Ant-Delirium/src/Agent-Runtime_unitcore.rs`
- `AC-Ant-Delirium/src/Agent-Runtime_unit.rs`
- `AC-Ant-Delirium/src/Main-Logic_unitcore.rs`
- `AC-Ant-Delirium/src/structs/Agent_Runtime_struct.rs`
- `AC-Ant-Delirium/src/tests/Agent-Runtime_test.rs`

The current `auto_advance_orchestrator` hook point is after the active plan has no pending task and before the next plan scan starts.

The current `write_todo_manifest` path must be updated so that, during transition phase, it emits the projected transition task instead of `null` or the next plan task.

The cargo warning path in `Main-Logic_unitcore.rs` should count transition tasks as pending when a transition phase is active.

## Acceptance Criteria

The feature is complete when:

1. `plugins/<project>/Global/transition_task.json` is loaded and validated.
2. Completion of any non-empty plan activates transition phase before next-plan selection.
3. Transition tasks are projected with continuation step ids based on the completed plan's final step id.
4. Transition tasks are delivered through the same current-task manifest/API shape as ordinary tasks.
5. Provenance remains available internally and in governance history.
6. The completed plan JSON is not mutated to include transition tasks.
7. `transition_task.json` is not mutated with projected step ids.
8. Owned process/session cleanup is implemented safely.
9. Unknown processes are reported but not killed.
10. The next plan is selected only after transition tasks are complete.

## Test Plan

Add Rust tests for:

- loading transition task fixtures
- malformed fixture diagnostics
- missing `Global` directory behavior
- duplicate transition task ids
- final task id detection for `1.9`, `1.10`, `1.15`, and `1.2.9`
- transition id projection without mutating source JSON
- active transition state serialization
- current-task manifest output during transition phase
- `hierarchy.task` behavior during transition phase
- next-plan selection after transition completion
- process registry ownership
- graceful process cleanup
- timeout escalation for owned processes
- unknown process reporting
- lock/session cleanup ownership checks

## Handover Task List

These tasks are the implementation handoff for the Ant-Delirium agent that will build this feature. They are not Aesthetic-Red plan tasks. They should become Ant-Delirium Rust work items, or be converted into the appropriate Ant-Delirium task format if that project later adds its own task queue.

### 1. Audit The Current Task Boundary

Find every Rust path that currently decides whether a plan still has pending tasks or should advance to the next plan.

Required anchors:

- `AC-Ant-Delirium/src/Agent-Runtime_unitcore.rs`
- `AC-Ant-Delirium/src/Main-Logic_unitcore.rs`
- `AC-Ant-Delirium/src/Agent-Runtime_unit.rs`
- `AC-Ant-Delirium/src/structs/Agent_Runtime_struct.rs`
- `AC-Ant-Delirium/src/tests/Agent-Runtime_test.rs`

Acceptance:

- Identify the exact `auto_advance_orchestrator` hook point.
- Identify the current todo-manifest writer path.
- Identify the current cargo warning path.
- Record current mode behavior for `plan review`, `implementation`, and `implementation review`.

### 2. Add Transition Runtime State

Add typed Rust state for an active transition phase.

Required state:

- transition active flag
- source plan path
- source plan id
- source final step id
- transition source path
- current transition source id
- current projected step id
- completed transition source ids

Acceptance:

- State serializes and deserializes through the existing orchestrator or a clearly named runtime companion file.
- Existing orchestrator files without transition state continue to load.
- Transition state never mutates the completed plan JSON.

### 3. Implement Final-Step Detection

Add a helper that computes the final task id of a completed non-empty plan.

Algorithm:

- parse each `step_id` into numeric segments split on `.`
- compare segment vectors numerically
- reject malformed ids with an explicit diagnostic
- select the maximum id
- increment only the last numeric segment for projected transition ids

Acceptance:

- `1.9 < 1.10`
- `1.10` projects first transition as `1.11`
- `1.15` projects first transition as `1.16`
- `1.2.9` projects first transition as `1.2.10`

### 4. Add Transition Fixture Loading

Load project-local transition tasks from:

```text
plugins/<project>/Global/transition_task.json
```

For Aesthetic-Red, the fixture path is:

```text
plugins/Aesthetic-Red/Global/transition_task.json
```

Acceptance:

- Missing `Global` directory is handled explicitly.
- Missing transition file is handled explicitly.
- Empty transition task list is handled explicitly.
- Duplicate source transition ids fail validation.
- Malformed task fields fail validation.

### 5. Define The Visible Task Projection

Implement an adapter that converts source transition tasks into ordinary terminal/API task objects.

Visible behavior:

- the displayed task has a normal `step_id`
- the displayed task has the normal task fields
- the displayed task uses the same current-task manifest shape as ordinary tasks
- no visible label, banner, category, or queue status tells the terminal agent that this is a transition phase

Internal behavior:

- preserve source path
- preserve source transition id
- preserve projected step id
- preserve source plan id and source final step id
- write provenance to debug/audit/history surfaces, not to the visible task object

Acceptance:

- The terminal-facing task object is indistinguishable from an ordinary task object by field shape and instructions.
- Audit/debug state can still prove the task came from `Global/transition_task.json`.

### 6. Intercept Plan Advancement

Change `auto_advance_orchestrator` so completed plans enter transition phase before the next normal plan is selected.

Required flow:

1. Active plan has no pending ordinary task.
2. Completed plan is non-empty.
3. Transition fixture exists and has enabled tasks.
4. Transition phase activates.
5. First transition task is projected from the final plan step id.
6. Next normal plan selection is blocked until transition tasks are complete.

Acceptance:

- Existing ordinary in-plan advancement remains unchanged.
- Existing next-plan selection runs only after transition completion.
- Force-next-plan behavior is defined and tested.

### 7. Update Todo Manifest Delivery

Update the current manifest writer so transition phase emits the projected transition task as `current_task`.

Acceptance:

- `to-do_list_manifest.json` shows a normal `current_task`.
- `copy_paste_command` uses the projected step id.
- `complete_plan` remains the completed plan context or an explicitly chosen transition-safe context.
- `queue_status` remains compatible with ordinary opaque queue behavior.
- The terminal-facing task does not reveal transition mechanics.

### 8. Update Cargo Warning Delivery

Update the cargo warning/task discovery path so transition phase reports pending work naturally.

Acceptance:

- Cargo warning indicates pending tasks using the same style as ordinary plan warnings.
- Pending count includes projected transition tasks when transition phase is active.
- Warning text does not expose transition internals.
- Debug logs or governance history still retain transition provenance.

### 9. Implement Process Ownership Registry

Add a Rust registry for task-owned background processes and sessions.

Track:

- command
- cwd
- pid
- process group when available
- launch surface
- owner plan id
- owner task id
- start timestamp
- PTY/session id when available
- known ports when available
- lock/session files created by the task

Acceptance:

- Registry can add, update, query, and remove owned process records.
- Registry survives long enough for transition cleanup.
- Unknown processes are never treated as owned by inference alone.

### 10. Implement Safe Cleanup Actions

Implement cleanup commands that act only on owned registry entries.

Cleanup order:

1. close owned log streams
2. close owned PTY/session handles
3. gracefully terminate owned process groups
4. wait for timeout
5. force terminate only still-running owned processes
6. remove owned lock/session files only when ownership is proven
7. report remaining handles

Acceptance:

- Unknown processes are reported, not killed.
- Unowned ports are reported, not closed.
- Unowned lock files are reported, not removed.
- Cleanup evidence names each owned process and action taken.

### 11. Author Default Transition Tasks

Create the initial project-local transition task fixture for Aesthetic-Red:

```text
plugins/Aesthetic-Red/Global/transition_task.json
```

Default source tasks:

1. enumerate owned active processes and sessions
2. close owned PTY/log sessions
3. gracefully stop owned background commands
4. release owned locks and session files
5. verify no owned ports remain open
6. summarize cleanup evidence
7. assert no owned background work remains

Acceptance:

- Source ids are stable and local to the transition fixture.
- Source ids are not projected ids.
- The fixture does not contain Aesthetic-Red feature work.
- The fixture contains only reusable lifecycle cleanup work.

### 12. Persist Completion Evidence

When a projected transition task is marked complete, write evidence to the same governance history surfaces used by ordinary tasks, plus internal provenance.

Acceptance:

- Visible completion works through the normal projected `step_id`.
- Internal record maps projected id back to source transition id.
- Completed plan file is not modified.
- Transition fixture is not modified with projected ids.

### 13. Resume Normal Plan Selection

After all projected transition tasks are complete, clear transition state and resume the existing next-plan selection algorithm.

Acceptance:

- The next normal plan activates exactly once.
- `current_step_id` becomes the first pending task of the next plan.
- Completed transition state cannot be replayed accidentally on restart.
- If no next plan exists, the existing phase-complete behavior remains intact.

### 14. Add Unit Tests

Add Rust unit tests for:

- final-step detection
- malformed step ids
- transition fixture loading
- duplicate transition ids
- projection from `1.10` to `1.11`
- projection from `1.15` to `1.16`
- projection from `1.2.9` to `1.2.10`
- transition state serialization
- transition state restart recovery

### 15. Add Integration Tests

Add Rust integration tests for:

- active plan completes and transition phase activates
- transition task appears through ordinary manifest shape
- transition completion blocks next-plan selection until all transition tasks complete
- next plan activates after transition completion
- cargo warning reports projected transition task naturally
- hierarchy task API accepts projected transition ids

### 16. Add Cleanup Safety Tests

Add Rust tests or controlled integration tests for:

- owned process graceful termination
- owned process timeout escalation
- owned PTY/session cleanup
- owned lock cleanup
- owned port cleanup
- unknown process reporting
- unrelated process survival
- cleanup evidence formatting

### 17. Manual End-To-End Validation

Run a manual workflow with two temporary plans.

Scenario A:

1. Create a plan ending at `1.10`.
2. Complete all ordinary tasks.
3. Confirm the next delivered task appears as `1.11`.
4. Complete all projected transition tasks.
5. Confirm the next normal plan activates.

Scenario B:

1. Create a plan ending at `1.15`.
2. Complete all ordinary tasks.
3. Confirm the next delivered task appears as `1.16`.

Scenario C:

1. Launch a controlled owned background process through the registry.
2. Complete the plan.
3. Confirm transition cleanup stops the owned process.
4. Confirm an unrelated process is not terminated.

## Terminal Presentation Contract

The terminal-facing projected transition task must look like a normal task continuation.

Allowed in terminal/API current-task display:

- projected `step_id`
- normal `task_prompt`
- normal task status fields
- normal copy-paste command
- ordinary opaque queue wording

Not allowed in terminal/API current-task display:

- `transition`
- `global transition`
- `handover`
- `cleanup phase`
- `source_transition_id`
- `projected_from`
- `Global/transition_task.json`
- any explanation that the previous plan ended

Required internally:

- source transition id
- source transition path
- completed source plan id
- completed source final step id
- projected display id
- cleanup evidence
- audit/debug provenance

This split is intentional: the terminal agent receives ordinary required work, while the engine keeps truthful provenance for operators and debugging.

## Manual Validation Scenario

1. Create a small test plan ending at `1.10`.
2. Mark all normal tasks complete.
3. Verify the next delivered task is projected as `1.11` from `Global/transition_task.json`.
4. Complete transition tasks.
5. Verify the next normal plan activates.
6. Repeat with a plan ending at `1.15` and confirm first transition id is `1.16`.
7. Start a controlled owned background process and confirm transition cleanup stops it.
8. Confirm an unrelated process is reported but not terminated.
