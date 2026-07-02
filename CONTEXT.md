# prd-state

CLI that owns the `state.json` schema for the `prd_work_loop` skill, replacing hand-rolled jq edits that caused schema drift.

## Language

**State**:
The `state.json` companion file next to a PRD; working memory for one PRD's work loop. Never committed.
_Avoid_: status json, state file variants (`requirements_registry`, `_last_verify_results` — drift, not schema)

**PRD**:
The markdown file that is the single source of truth for goal, milestones, and acceptance criteria. Located via the `prd_path` field in State.

**Iteration**:
One full OBSERVE→ORIENT→DECIDE→ACT→VERIFY→UPDATE pass. One skill invocation = one iteration.

**Phase**:
The current position within an iteration (`current_phase`). `null` means between iterations.

**Milestone**:
An `ISC-*` requirement from the PRD; verified once, then `satisfied`. PRD checkboxes are authoritative.
_Avoid_: task, criterion

**Invariant**:
An `INV-*` requirement from `docs/invariant_requirements.md`; never satisfied, re-verified every iteration. The doc, not the PRD, is authoritative.

**Requirement registry**:
The `requirements` array in State: all known invariants and milestones with status. Synced from PRD + invariant doc by `sync`.

**Subgoal**:
A unit of iteration work: one or more artifacts plus the milestones they satisfy, sized by Tier. Re-evaluated every ORIENT.

**Tier**:
Subgoal complexity class — `trivial` (batch up to 4 declaration-only files), `standard` (impl + test pair), `complex` (one file, alone).

**Pre-flight**:
The binding checklist for one iteration: all invariants plus the active subgoal's milestones. Derived by `decide`, never hand-assembled.

**Evidence**:
A cited proof for one verify result — quoted line, search result, or code path. A bare checkmark is not Evidence.

**Stall**:
A failed VERIFY. `stall_count` reaching 3 halts the loop for user guidance. Derived from verify results; there is no `verify_status` field.
