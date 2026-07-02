---
title: prd-state CLI
status: COMPLETE
created: 2026-07-02
updated: 2026-07-02
labels: [ready-for-agent]
verification_summary: "Iteration 10: 4/4 PASS (ISC-E2, ISC-E4, ISC-E5, ISC-E8)"
failing_criteria: none
last_phase: UPDATE
---

# PRD: prd-state CLI

## Problem Statement

Agents running the `prd_work_loop` skill hand-edit `state.json` with ad-hoc jq/Edit calls every phase of every iteration. This churns tokens and mutates the schema: 4 active PRDs in the yoodli repo show 3 schema variants (`requirements` vs `requirements_registry`, ad-hoc `_last_verify_results`). The skill also devotes an entire "MECHANICAL RULE" paragraph to LOG-append ordering because agents botch the Edit anchor.

## Solution

`prd-state`: a Rust CLI whose phase-verb commands mirror the work loop's state transitions. The tool owns the schema (strict deserialization rejects unknown keys), derives everything derivable (pre-flight checklists, resume phase, pass/fail, stall logic), and performs the mechanical PRD writes (checkbox flips, LOG append, frontmatter). Agents issue one command per transition instead of composing JSON edits.

## User Stories

1. As an agent running prd_work_loop, I want `init` to create a canonical `state.json` next to a PRD, so that every loop starts from a valid schema.
2. As an agent, I want `status` to report iteration, phase, next subgoal, and pending milestones, so that resumption needs no JSON reading.
3. As an agent, I want `status` to compute the resume phase from the skill's resume table (including on-disk artifact checks), so that I obey it instead of re-deriving it.
4. As an agent, I want `status --json` for machine-readable output, so that structured consumers need no text parsing.
5. As an agent, I want `phase <PHASE>` to set the current phase, so that phase tracking is one command.
6. As an agent, I want entering OBSERVE to capture `start_commit` from git automatically, so that I never run and paste `git rev-parse` myself.
7. As an agent, I want `sync` to parse the invariant doc and register every `INV-*` item, so that new invariants apply to in-progress PRDs without PRD edits.
8. As an agent, I want `sync` to parse the PRD's ISC checkbox lines and diff them against the registry (add new, mark removed, preserve satisfied), so that ORIENT's registry sync is one command.
9. As an agent, I want `sync` to report what changed, so that I can flag `prd_changed` reasoning without diffing files myself.
10. As an agent, I want `req add` / `req remove` for manual registry edits, so that I have a fallback when parsing fails or a requirement is out-of-band.
11. As an agent, I want `subgoal add` with tier, artifacts, milestones, and description, so that decomposition lands in state in one command per subgoal.
12. As an agent, I want `subgoal remove`, so that split/merge composes from add + remove.
13. As an agent, I want `decide <SG-ID>` to mark the subgoal in-progress, set `current_action` from it, and derive the pre-flight checklist as all invariants plus the subgoal's milestones, so that no invariant can be forgotten.
14. As an agent, I want `decide` to print the pre-flight checklist, so that DECIDE's visible-output requirement is satisfied by the tool.
15. As an agent, I want `verify <ID> <PASS|FAIL> <evidence>` to append to `verify_results`, so that mid-VERIFY resumption works without JSON edits.
16. As an agent, I want `end-iteration` to mark passing milestones satisfied in the registry, so that UPDATE's bookkeeping is mechanical.
17. As an agent, I want `end-iteration` to flip PRD checkboxes `- [ ]` → `- [x]` for milestones that passed, so that the PRD stays authoritative without Edit-tool anchoring.
18. As an agent, I want `end-iteration` to mark the subgoal complete when all its milestones passed, so that subgoal status never drifts from verify results.
19. As an agent, I want `end-iteration` to append the LOG entry (composed from state; reflection passed as a flag) at the true end of the LOG, so that entries can never land in reverse order.
20. As an agent, I want `end-iteration` to update PRD frontmatter (`verification_summary`, `failing_criteria`, `last_phase`, `updated`), so that frontmatter edits stop being manual.
21. As an agent, I want `end-iteration` to derive overall pass/fail from `verify_results`, increment or reset `stall_count`, and warn at 3 stalls, so that stall handling is not my judgment call.
22. As an agent, I want `end-iteration` to increment the iteration, clear transient fields, and null the phase, so that between-iterations state is always well-formed.
23. As an agent, I want `end-iteration` to set PRD status COMPLETE when all milestones are satisfied, so that completion detection is mechanical.
24. As an agent, I want every command to reject a `state.json` with unknown or misshapen fields loudly, so that schema drift dies at first contact instead of propagating.
25. As an agent, I want `-C <dir>` to target the PRD directory from anywhere, so that commands work regardless of cwd.
26. As a developer, I want the binary installed once via cargo to serve all my worktrees, so that there is no per-repo version skew.
27. As a developer, I want atomic state writes (temp file + rename), so that a crash cannot leave a torn `state.json`.
28. As a developer, I want the skill doc to eventually shrink to a command list, so that the token cost of every iteration drops.

## Implementation Decisions

- Single Rust crate; binary named `prd-state`; installed with `cargo install --path .`; deps: clap (derive), serde/serde_json, anyhow. No workspace.
- The tool owns the canonical state schema: the prd_work_loop SKILL.md State Format plus a `prd_path` field, minus `verify_status` (derived from `verify_results`, never stored). Deserialization is strict (`deny_unknown_fields`); unknown keys are an error, not a warning.
- Commands are phase verbs mirroring skill transitions: `init`, `status`, `phase`, `sync`, `req add|remove`, `subgoal add|remove`, `decide`, `verify`, `end-iteration`. No generic get/set — agents never touch raw JSON.
- Derivation over instruction: pre-flight checklist (`decide`), resume phase (`status`), overall pass/fail and stall handling (`end-iteration`) are computed by the tool from state, not asserted by the agent.
- File discovery: `-C <prd-dir>` flag defaulting to cwd locates `state.json`; the PRD is found via the `prd_path` state field set at `init`; the invariant doc is `docs/invariant_requirements.md` under `git rev-parse --show-toplevel`. No config file.
- Markdown coupling: `sync` parses the canonical line format `- [ ] (INV|ISC)-<id>: <text> | Verify: <method>`; `end-iteration` flips checkboxes, appends LOG entries, and updates YAML frontmatter. Parse failure is a hard error with a message directing the agent to the manual `req` fallback.
- All state writes are atomic: write temp file in the same directory, then rename.
- No migration code for existing drifted state files; they are fixed by hand once, out of scope here.
- `subgoal split`/`merge` are not commands; they compose from `add` + `remove` (YAGNI).
- Domain vocabulary per `CONTEXT.md` at repo root (State, Iteration, Phase, Milestone, Invariant, Requirement registry, Subgoal, Tier, Pre-flight, Evidence, Stall).

## Testing Decisions

- Primary test surface is per-module unit tests in `#[cfg(test)]` blocks in the module files, standard Rust style. No separate `tests/` e2e layer.
- Markdown operations (ISC/INV line parsing, checkbox flip, frontmatter update, LOG append) are pure `&str -> String` functions tested without any filesystem.
- State load/save and command functions are tested against `tempfile` directories holding real fixture files; commands needing git (`start_commit`, invariant-doc discovery) run against a `git init` tempdir. No mocks — the tool is file manipulation; mocking the filesystem would test nothing.
- Tests assert external behavior only: file contents after the command and the command's output text, never internal representations.
- TDD ordering per repo rules: tests written with or immediately after each module.

## Immutable Success Criteria

### Core

- [x] ISC-C1: `init <prd-path>` creates a canonical state.json with `prd_path`, iteration 0, null phase | Verify: Test: unit test asserts file contents
- [x] ISC-C2: Loading a state.json with unknown keys fails with a clear error | Verify: Test: fixture with `requirements_registry` key rejected
- [x] ISC-C3: All state writes are atomic (temp + rename in same dir) | Verify: Code: write path uses tempfile + persist/rename
- [x] ISC-C4: `-C <dir>` targets the PRD directory; default is cwd | Verify: Test: command run against tempdir via -C

### Status

- [x] ISC-S1: `status` prints iteration, phase, next incomplete subgoal, pending milestone count | Verify: Test: fixture state → expected text
- [x] ISC-S2: `status` computes resume phase per the skill resume table, including DECIDE→ACT (pre-flight present) and ACT→VERIFY (artifact exists on disk) shortcuts | Verify: Test: one case per table row
- [x] ISC-S3: `status --json` emits the computed summary as JSON | Verify: Test: output parses, fields match

### Phase & registry

- [x] ISC-P1: `phase <PHASE>` sets current_phase; invalid phase names rejected | Verify: Test: valid and invalid inputs
- [x] ISC-P2: `phase OBSERVE` captures `start_commit` from git HEAD | Verify: Test: git-init tempdir, commit, assert short hash stored
- [x] ISC-R1: `sync` registers every INV-* from the invariant doc, deduplicating existing entries | Verify: Test: doc fixture with 3 INVs, one pre-registered
- [x] ISC-R2: `sync` adds new PRD ISC milestones as active, marks missing ones removed, preserves satisfied status | Verify: Test: PRD fixture diff scenario
- [x] ISC-R3: `sync` reports added/removed/unchanged counts | Verify: Test: output text assertion
- [x] ISC-R4: `sync` on an unparseable requirement line fails with an error naming the line and suggesting `req add` | Verify: Test: malformed fixture
- [x] ISC-R5: `req add <ID> <type> <text>` and `req remove <ID>` edit the registry; remove marks milestones removed rather than deleting | Verify: Test: registry contents after each

### Subgoals & decide

- [x] ISC-G1: `subgoal add` writes id, tier, artifacts, milestones, description, status pending | Verify: Test: state contents
- [x] ISC-G2: `subgoal remove <ID>` deletes the subgoal | Verify: Test: state contents
- [x] ISC-G3: `decide <SG-ID>` marks the subgoal in_progress, sets current_action from it, and writes a pre-flight containing every registered invariant plus exactly the subgoal's milestones | Verify: Test: derived checklist contents
- [x] ISC-G4: `decide` prints the pre-flight checklist | Verify: Test: output includes each item id

### Verify & end-iteration

- [x] ISC-V1: `verify <ID> <PASS|FAIL> <evidence>` appends to verify_results; empty evidence rejected | Verify: Test: append + rejection cases
- [x] ISC-E1: `end-iteration` marks milestones with PASS results satisfied in the registry | Verify: Test: registry after run
- [x] ISC-E2: `end-iteration` flips PRD checkboxes to `- [x]` for passed milestones only | Verify: Test: PRD fixture before/after
- [x] ISC-E3: `end-iteration` marks the subgoal complete iff all its milestones passed | Verify: Test: pass and mixed-fail scenarios
- [x] ISC-E4: `end-iteration` appends a LOG entry after the last existing entry, composed from state, with `--reflection` text included | Verify: Test: PRD with 2 existing entries → new entry is third
- [x] ISC-E5: `end-iteration` updates frontmatter verification_summary, failing_criteria, last_phase, updated | Verify: Test: frontmatter after run
- [x] ISC-E6: `end-iteration` on any FAIL increments stall_count; on all-PASS resets it; at stall_count 3 the output warns to stop and ask the user | Verify: Test: three scenarios
- [x] ISC-E7: `end-iteration` increments iteration, clears current_action/pre_flight_checklist/verify_results/start_commit, sets phase null | Verify: Test: state after run
- [x] ISC-E8: `end-iteration` sets PRD frontmatter status COMPLETE when every milestone is satisfied | Verify: Test: final-iteration fixture

## Out of Scope

- Rewriting the prd_work_loop SKILL.md to use the tool (follow-up).
- Fixing the 4 drifted state.json files in the yoodli repo (one-time manual jq).
- Syncing skill copies across worktrees.
- Migration/alias code for drifted schemas.
- `subgoal split`/`merge` commands.
- Any config file or invariant-doc path override.

## Further Notes

- This PRD includes an ISC section beyond the standard template so `prd_work_loop` can execute it directly — the tool dogfoods its own workflow.
- The yoodli repo's invariant doc (`docs/invariant_requirements.md`) does not exist in this repo; tests use fixture docs. INV-* items for this repo's own development are not defined — the work loop here runs on ISC milestones only.
- Schema reference: prd_work_loop SKILL.md "State Format" section is the canonical shape; this tool is its enforcement.

## LOG

### Iteration 1 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `Cargo.toml`, `src/main.rs`, `src/state.rs` (tier: standard)
- **Milestones addressed:** ISC-C1, ISC-C2, ISC-C3, ISC-C4
- **Invariants verified:** none registered (no docs/invariant_requirements.md in this repo)
- **Overall:** PASS
- **Reflection:** Strict-schema test used the real drift key (`requirements_registry`) observed in yoodli state files — fixtures from observed failures beat invented ones. `load` is dead code until SG-2 consumes it; left the warning rather than suppress a lint. Repo has no git; `start_commit` capture (ISC-P2) will need `git init` before SG-3.
- **Remaining:** 23 milestones pending

### Iteration 2 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/status.rs` (+ `src/state.rs` Display impl, `src/main.rs` wiring) (tier: standard)
- **Milestones addressed:** ISC-S1, ISC-S2, ISC-S3
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** First draft rendered the resume phase via a Debug-format uppercase hack; user rejected it — enums own their presentation, so `Display` on `Phase` replaced the hack and the resume-table match collapsed (`Some(p) => p` for repeatable phases). Dogfood smoke against this PRD's own live state fired the ACT→VERIFY shortcut on real data — stronger evidence than fixtures alone.
- **Remaining:** 20 milestones pending

### Iteration 3 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/phase.rs` (+ `Phase` ValueEnum derive, `src/main.rs` wiring) (tier: standard)
- **Milestones addressed:** ISC-P1, ISC-P2
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** clap's `ValueEnum` on the existing `Phase` enum gave ISC-P1 rejection for free — validation lives on the type, not in a parser. Git-absent behavior chosen graceful (`start_commit: none`) rather than an error, since this very repo has no git yet; the message states the reason so the agent isn't silently missing a commit.
- **Remaining:** 18 milestones pending

### Iteration 4 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/req.rs` (+ `ReqType` ValueEnum derive, `src/main.rs` wiring) (tier: standard)
- **Milestones addressed:** ISC-R5
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** Spec gap surfaced: ISC-R5 says milestone removal marks `removed`, but invariants carry no status field — chose delete-outright for invariant removal and documented it in a code comment. Duplicate `req add` rejected rather than upserted; sync (SG-5) will own reconciliation, so manual add stays strict.
- **Remaining:** 17 milestones pending

### Iteration 5 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/sync.rs` (+ `src/main.rs` wiring) (tier: complex)
- **Milestones addressed:** ISC-R1, ISC-R2, ISC-R3, ISC-R4
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** Hand-rolled string parsing (strip_prefix + split_once) covered the canonical line format without a regex dependency — the format's rigidity is what makes the no-dep parse safe. Best evidence this iteration was free: running `sync` against this PRD's own live registry returned `27 unchanged`, simultaneously validating the parser against real data and proving the hand-maintained registry consistent. Registry maintenance is now the tool's job for the remainder of this loop.
- **Remaining:** 13 milestones pending

### Iteration 6 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/subgoal.rs` (+ `Tier` ValueEnum derive, `src/main.rs` wiring) (tier: standard)
- **Milestones addressed:** ISC-G1, ISC-G2
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** Routine iteration; one guard beyond spec (empty artifacts rejected) since a subgoal without artifacts breaks the ACT→VERIFY resume shortcut, which requires at least one artifact path to check.
- **Remaining:** 11 milestones pending

### Iteration 7 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/decide.rs` (+ `src/main.rs` wiring) (tier: standard)
- **Milestones addressed:** ISC-G3, ISC-G4
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** `decide` bails on a subgoal milestone absent from the registry rather than emitting a text-only checklist item — checklist ids must resolve to registry entries or `end-iteration` (SG-9) can't mark them satisfied. Dogfood caught a stale-binary trap: `cargo test` green but `./target/debug/prd-state` lacked the new subcommand until `cargo build`. Live `decide SG-7` reproduced the jq-written state exactly — the command now replaces manual DECIDE edits for the rest of this loop.
- **Remaining:** 9 milestones pending

### Iteration 8 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/verify.rs` (+ `VerifyStatus` ValueEnum derive, `src/main.rs` wiring) (tier: standard)
- **Milestones addressed:** ISC-V1
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** Self-referential dogfood: the new `verify` command recorded its own milestone's evidence, exercising both the rejection path (whitespace evidence → exit 1) and the append path on live state in one step. From here every phase transition except end-iteration runs through the tool.
- **Remaining:** 8 milestones pending

### Iteration 9 — 2026-07-02
- **Start commit:** none (repo not yet git-initialized)
- **Artifacts:** `src/end_iteration.rs` (+ `src/main.rs` wiring) (tier: complex)
- **Milestones addressed:** ISC-E1, ISC-E3, ISC-E6, ISC-E7
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** Spec interpretation locked in: a milestone with no verify result counts as overall FAIL (named in output) and stays active — absent evidence is a fail, matching the skill's no-checkmark rule. This iteration's own close-out ran through the new `end-iteration`; only PRD markdown writes (SG-10) remain manual.
- **Remaining:** 4 milestones pending

### Iteration 10 — 2026-07-02
- **Start commit:** none
- **Artifacts:** `src/prd_md.rs` (tier: complex)
- **Milestones addressed:** ISC-E2, ISC-E4, ISC-E5, ISC-E8
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** Tool closed its own final iteration: checkbox flips, frontmatter, LOG append, and the COMPLETE flip all ran through end-iteration rather than manual edits — the dogfood loop is closed. today() shells out to date +%F instead of a chrono dep; frontmatter update is strict (unknown key = error) to match the schema philosophy.
- **Remaining:** 0 milestones pending
