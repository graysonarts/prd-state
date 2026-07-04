---
title: State transaction seam + sealed iteration reset
status: ACTIVE
created: 2026-07-04
updated: 2026-07-04
labels: [ready-for-agent]
verification_summary: "Iteration 1: 4/4 PASS (ISC-TX-1, ISC-TX-4, ISC-TX-5, ISC-TX-6)"
failing_criteria: none
last_phase: UPDATE
---

# PRD: State transaction seam + sealed iteration reset

## Problem Statement

Every command that changes `state.json` re-implements the same three steps by
hand: load the State, mutate it, save it back. Nine mutating commands (`sync`,
`phase`, `req add`/`remove`, `verify`, `decide`, `subgoal add`/`remove`,
`end-iteration`) each carry their own copy of that envelope. Two failure modes
follow directly:

1. **A dropped save is a silent no-op.** Nothing forces the final `save`. A
   command that loads, mutates, and forgets to persist compiles cleanly and
   quietly discards the change — the State is the working memory of the whole
   work loop, so a lost write corrupts the loop's memory with no error.
2. **The iteration reset drifts.** `end-iteration` resets the transient fields
   with a hand-written block that silently mirrors the field list in
   `State::new`. The two lists must agree, and nothing enforces it. Adding a new
   transient field to State and forgetting the reset leaks stale data across the
   iteration boundary.

The `Registry` inside State is carefully protected by construction; that
discipline stops at its own edge. The surrounding State is a fully-open struct
whose integrity depends on every caller behaving.

## Solution

Introduce one deep interface that owns the load-modify-save transaction for
`state.json`, and one method that owns the iteration reset — both living next to
the schema they protect.

Commands stop hand-rolling the envelope. Each hands `State::update` a closure
that receives the mutable State and returns the user-facing message; `update`
loads before the closure and saves after it, saving only when the closure
succeeds. A closure that returns an error leaves `state.json` untouched — the
mutation was only ever in memory. The dropped-save failure mode becomes
unrepresentable: the seam saves, not the caller.

The transient reset becomes `State::begin_next_iteration`, and the definition of
"which fields are transient" moves to exactly one place that both `new` and the
reset route through. Adding a transient field no longer requires remembering a
second list.

Nothing else about the commands' observable behavior changes.

## User Stories

1. As a maintainer, I want the load-modify-save of `state.json` to live behind
   one interface, so that I stop copying the same envelope into every command.
2. As a maintainer, I want a command that forgets to persist to be impossible to
   write, so that the work loop never silently loses a state change.
3. As a maintainer, I want a failed mutation to leave `state.json` exactly as it
   was, so that an error mid-command never half-writes the loop's memory.
4. As a maintainer, I want the iteration reset to live in one method next to the
   schema, so that I read the close-out in one place instead of a bare block
   inside `end-iteration`.
5. As a maintainer, I want "which fields are transient" defined exactly once, so
   that adding a field to State cannot leave the reset out of sync.
6. As a maintainer adding a new transient field, I want to update a single list,
   so that the reset stays correct by construction rather than by review.
7. As a reviewer, I want each command's diff to shrink to its actual logic, so
   that I can see what a command does without the load/save boilerplate around
   it.
8. As the `prd_work_loop`/`tdd_work_loop` skill, I want every command's external
   behavior and output unchanged, so that the refactor is invisible to the
   scripts that parse them.
9. As a maintainer, I want `end-iteration`'s PRD write and reflection-bail to
   keep their current ordering inside the new transaction, so that a missing
   reflection still writes nothing at all.
10. As a maintainer, I want `sync`'s external file reads and `verify`'s input
    guard to keep failing before any state load, so that bad input never touches
    `state.json`.
11. As a maintainer, I want the transaction seam covered by a test that persists
    on success and a test that rolls back on failure, so that the guarantee is
    pinned, not assumed.
12. As a maintainer, I want the existing command test modules to pass unedited
    after the refactor, so that their green run is the regression proof that
    behavior held.
13. As a maintainer, I want `State::new` to keep producing exactly today's
    initial State, so that `init` and every load path are unaffected.
14. As a maintainer, I want the other State fields to stay directly writable for
    now, so that this change ships without a large god-struct encapsulation pass
    mixed in.

## Implementation Decisions

- **Scope: `state.json` only.** `State::update` owns the load and save of
  `state.json` and nothing else. `end-iteration` is the only command that also
  writes the PRD `.md`; that write stays a side effect *inside* the closure,
  preserving today's PRD-first, non-atomic ordering. Cross-file atomicity across
  `state.json` and the PRD is explicitly not introduced — it is not a guarantee
  today, and each file is already individually torn-proof via its atomic
  temp-then-rename write.

- **The transaction interface.** A new associated function on `State`, placed
  beside `load`/`save`/`new`. The signature encodes the contract precisely:

  ```rust
  pub fn update(
      dir: &Path,
      f: impl FnOnce(&mut State) -> Result<String>,
  ) -> Result<String> {
      let mut st = load(dir)?;
      let msg = f(&mut st)?;   // closure Err ⇒ return early, no save
      save(dir, &st)?;
      Ok(msg)
  }
  ```

  - Concrete `Result<String>`, not a generic `<T>`. Every caller returns the
    command's message string; a generic return is an abstraction no caller needs.
  - The closure receives `&mut State` only. `end-iteration` needs `dir` for its
    PRD write and *captures* it from the enclosing scope rather than receiving it
    as a parameter, keeping the interface off the filesystem. `update`'s own
    `&Path` argument and the closure's captured `&Path` are two shared borrows —
    they coexist without conflict.
  - The message is built inside the closure (e.g. `verify` computes its result
    count after the push), so no caller reads State after `update` returns.

- **Sealed iteration reset, by construction.** `State` derives `Default`
  (`Registry` already does; every field is default-able). The transient set is
  defined once, and `new` and the reset both route through it:

  ```rust
  pub fn new(prd_filename: &str) -> Self {
      State { prd_path: prd_filename.to_string(), ..Default::default() }
  }
  pub fn begin_next_iteration(&mut self) {
      self.iteration += 1;
      self.clear_transients();
  }
  fn clear_transients(&mut self) {            // the ONE transient list
      self.current_phase = None;
      self.start_commit = None;
      self.current_action = None;
      self.pre_flight_checklist.clear();
      self.verify_results.clear();
  }
  ```

  The transient fields are `current_phase`, `start_commit`, `current_action`,
  `pre_flight_checklist`, `verify_results`. `iteration` increments (not cleared);
  `stall_count` is set by the outcome policy (`next_stall_count`), so it is *not*
  part of the reset; `prd_path`, `subgoals`, `requirements` persist. `new` via
  `..Default::default()` is behaviorally identical to today's full literal.

- **Closure boundary rule.** The closure contains exactly what touches
  `&mut State`; everything else brackets the call.
  - Pure-input guards run *before* `update` (fail fast, no load): `verify`'s
    empty-evidence check, `subgoal`'s empty-artifacts check.
  - External file reads run *before* `update` and pass data in: `sync` reads and
    parses the invariant doc and PRD and runs its "doc wins" dedup outside, so its
    closure is just `st.requirements.upsert_from_parsed(&parsed)`.
  - State-dependent guards run at the top of the closure: `req`/`subgoal` dup-id,
    `decide` milestone-registered.
  - Side-effect writes tied to the mutation run inside the closure: `end-iteration`
    PRD read/transform/save, `phase`'s OBSERVE git-HEAD capture.

- **`end-iteration` shape.** Its closure computes `outcome(&st)`, applies the
  reflection-bail short-circuit *before any write*, runs the `mark_satisfied` loop
  *before* the `pending_milestones()` COMPLETE count (order preserved), updates
  subgoal status and `stall_count`, performs the PRD write, calls
  `begin_next_iteration` in place of the deleted manual reset block, and returns
  the assembled message. `update` then saves `state.json`.

- **Bounded scope.** The remaining `State` fields stay `pub`; callers still write
  `st.subgoals.push(...)`, `st.current_action = ...`, etc. directly inside their
  closures. Full god-struct privatization (accessors/mutators for every field) is
  a separate, larger deepening and is not part of this change.

- **Non-participants.** `init` (creates `state.json`, does not load-modify) and
  `status` (read-only, no save) are not `update` sites and are unchanged.

## Testing Decisions

- **Good test:** exercises external behavior through an interface, not internal
  wiring. The command test modules already do this — they call the public
  `run`/`add` and reload `state.json` to assert the result — which is why they
  serve as the regression net without edits.

- **Three new unit tests** (`#[cfg(test)]` in `src/state.rs`):
  1. `update` persists on `Ok` — mutate inside the closure, then a fresh `load`
     shows the change.
  2. `update` rolls back on `Err` — a closure that returns `Err` leaves
     `state.json` byte-for-byte as before. This encodes the dropped-save failure
     mode as an executable guarantee.
  3. `begin_next_iteration` — increments `iteration`, clears the five transient
     fields, and preserves `prd_path`, `subgoals`, `requirements`, `stall_count`.

- **Regression net:** the nine command test modules (`verify.rs`, `phase.rs`,
  `req.rs`, `subgoal.rs`, `decide.rs`, `sync.rs`, and `end_iteration.rs`) MUST
  pass unedited after each site is refactored. Their zero-edit green run is the
  evidence that behavior held. `end_iteration.rs`'s
  `all_pass_satisfies_completes_resets_and_clears` and
  `pass_without_reflection_errors_and_writes_nothing` specifically prove the reset
  and the reflection-bail survive the move into the closure.

- **Prior art:** the existing `save_load_roundtrip_leaves_no_tmp` and Registry
  tests in `src/state.rs` (interface-level, TempDir-backed), and the `run()`
  end-to-end tests in `src/end_iteration.rs`.

- **`new` parity** needs no new test — `init_creates_canonical_state` in
  `src/state.rs` already pins `new`'s field values through `load`, covering the
  switch to `..Default::default()`.

## Immutable Success Criteria

### Transaction seam

- [x] ISC-TX-1: `State::update(dir, f)` loads `state.json`, runs the closure, and saves only when the closure returns `Ok`; a closure that returns `Err` leaves `state.json` unchanged | Verify: Test: update persists a mutation on Ok; update leaves the file unchanged on Err
- [ ] ISC-TX-2: all nine mutating commands (`sync`, `phase`, `req add`/`remove`, `verify`, `decide`, `subgoal add`/`remove`, `end-iteration`) perform their `state.json` read-modify-write through `State::update`, and no command calls `state::save` directly for a mutation | Verify: Grep: `state::save` appears only inside `State::update` and `init`; Read: each command wraps its mutation in `State::update`
- [ ] ISC-TX-3: `State::update` returns the message its closure produced, and every command's stdout/stderr and exit behavior is unchanged from before the refactor | Verify: Test: the nine command test modules pass unedited

### Sealed reset

- [x] ISC-TX-4: `begin_next_iteration` increments `iteration` and clears the transient fields (`current_phase`, `start_commit`, `current_action`, `pre_flight_checklist`, `verify_results`) while preserving `prd_path`, `subgoals`, `requirements`, and `stall_count` | Verify: Test: begin_next_iteration clears transients and preserves the rest
- [x] ISC-TX-5: the transient field list is defined in exactly one place; `State` derives `Default`, `new` constructs via `..Default::default()`, and `begin_next_iteration` calls the single `clear_transients` | Verify: Read: one `clear_transients`, `new` via Default, no second transient list
- [x] ISC-TX-6: `end-iteration` uses `begin_next_iteration` for its reset (the manual six-line block is deleted), the reset runs inside the `update` closure, and a PASS iteration with no reflection still writes neither `state.json` nor the PRD | Verify: Read: closure calls `begin_next_iteration`, no manual reset block remains; Test: `pass_without_reflection_errors_and_writes_nothing` passes

### Boundary discipline

- [ ] ISC-TX-7: pure-input guards and external file reads stay outside `update` — `verify`'s empty-evidence guard and `sync`'s doc/PRD parse+dedup run before any `state.json` load | Verify: Read: `verify` bails before `update`; `sync` parses and dedups before `update`, closure only calls `upsert_from_parsed`

## Out of Scope

- **Cross-file atomicity** across `state.json` and the PRD `.md`. `end-iteration`
  keeps today's PRD-first, non-atomic ordering; no two-file commit protocol.
- **A generic `<T>` return** on `State::update`. Concrete `Result<String>` until a
  caller needs otherwise.
- **God-struct privatization.** The non-`Registry` State fields stay `pub`; no
  accessor/mutator methods in this change. Full encapsulation is a future
  candidate.
- **`init` and `status`.** Not load-modify-save sites; untouched.
- **The other architecture-review candidates** — reuniting the LOG grammar in
  `prd_md` (candidate 3), collapsing the shallow command wrappers (candidate 4),
  and the shared `atomic_write`/`capture` helpers (candidate 5). Separate PRDs.

## Further Notes

- **Ordering invariants preserved inside the `end-iteration` closure:**
  reflection-bail before any write; `mark_satisfied` loop before the
  `pending_milestones()` COMPLETE count. The refactor relocates these into the
  closure without reordering them.
- **Candidate 4 rides on this.** Once `State::update` exists, the shallow
  `req`/`subgoal`/`verify` wrappers can collapse into dispatch closures — but that
  is a follow-up PRD, not this one.
- **ADR candidate.** The state.json-only scope (rejecting cross-file atomicity)
  and the closure-transaction shape are hard-to-reverse decisions made against
  real alternatives — worth an ADR under `docs/adr/` when this lands.
- **Issue tracker.** No tracker vocabulary was provided this session, so
  `ready-for-agent` lives in frontmatter, matching the macos-release PRD
  precedent. Creating a GitHub issue on `graysonarts/prd-state` is a separate,
  outward-facing action pending user approval.

## LOG
- **1** · 2026-07-04 · `8313993` · SG-1 — Migrating a command to State::update removes its last non-test state:: use, so the test-only `state` alias then trips clippy unused_import — gate it with #[cfg(test)] use crate::state;. Every SG-2/SG-3 command migration will hit this. → ISC-TX-1, ISC-TX-4, ISC-TX-5, ISC-TX-6 satisfied; RED 3->GREEN 78, cargo test + clippy (bin & all-targets) clean
