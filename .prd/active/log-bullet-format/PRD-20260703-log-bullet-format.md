---
title: prd-state LOG one-bullet format
status: ACTIVE
created: 2026-07-03
updated: 2026-07-03
labels: [ready-for-agent]
verification_summary: "Iteration 1: 1/1 PASS (ISC-LOG-6)"
failing_criteria: none
last_phase: UPDATE
---

# PRD: prd-state LOG one-bullet format

## Problem Statement

`end-iteration` writes a 7-line `### Iteration N` block per iteration (start commit, artifacts, tier, invariants, overall, reflection, remaining). It is fully derived but verbose: the block restates identifiers recoverable from state and git, and grows the PRD tail by ~7 lines × N iterations. The `prd_work_loop` skill's documented LOG format was a single dense bullet; the tool diverged from it when it took ownership of the LOG grammar in the original prd-state PRD.

## Solution

Replace the block with one bullet per iteration, composed from state plus two authored flags. The load-bearing summary moves to a required-on-PASS `--reflection`; test-gate specifics move to a new free-text `--gate`. `append_log` produces a tight bullet list under `## LOG`. Everything dropped from the block (artifacts, tier, invariant roll-up, remaining count) stays recoverable from state or git.

**Bullet grammar**

```
- **{N}** · {date} · [`{start_commit}` · ]{SG} — [{reflection} ]→ {satisfied} satisfied[; {gate}][; FAIL: {reason}]
```

- `{N}` closing iteration, `{date}` today — derived.
- `` `{start_commit}` `` rendered only on PASS; omitted on FAIL and when `start_commit` is none.
- `{SG}` the in-progress subgoal id.
- `{reflection}` from `--reflection` (required on PASS; when `""` the `— … ` segment collapses).
- `{satisfied}` comma-joined passed milestone ids — derived from the outcome.
- `{gate}` from `--gate` when present.
- `; FAIL: {reason}` on FAIL only; `{reason}` = failed + unverified ids (covers failed invariants).

**Worked examples**

PASS (start_commit present):

```text
- **7** · 2026-07-03 · `a1b2c3d` · SG-2 — sum path is the seam → ISC-2, ISC-3 satisfied; RED 1→GREEN 7, cargo test ok
```

FAIL (no commit hash, no reflection):

```text
- **5** · 2026-07-03 · SG-3 → ISC-1 satisfied; RED 2→GREEN 1; FAIL: ISC-2
```

## User Stories

1. As an agent, I want each LOG entry to be one dense bullet, so that the PRD tail stays readable across many iterations.
2. As an agent, I want the load-bearing insight required on a PASS iteration, so that no iteration lands an empty-value LOG line.
3. As an agent, I want a free-text `--gate` slot, so that test-runner specifics (RED→GREEN, build status) appear without the tool inventing them.

## Implementation Decisions

- `log_entry()` in `src/end_iteration.rs` emits the one-bullet string; the `### Iteration N` heading and the Artifacts/Tier/Invariants/Remaining bullets are removed. It takes the gate as a parameter.
- Commit slot uses `start_commit` (HEAD captured at OBSERVE — the parent of the iteration's own commit, which `end-iteration` cannot know because it runs before the skill's auto-commit). Rendered on PASS only; the real iteration commit is found via `git log`.
- `--gate <text>` is a new free-text flag on the `EndIteration` command in `src/main.rs`, threaded into `run()`. Test-runner specifics are not derivable from state.
- `--reflection` becomes required on a PASS iteration: `run()` bails before any state or PRD mutation when the outcome is PASS and reflection is absent, so a missing summary never half-writes. `--reflection ""` is accepted and renders an empty sentence.
- Milestone status is derived from the outcome: `<passed ids> satisfied`; a FAIL iteration appends `; FAIL: <failed and unverified ids>`.
- `append_log` in `src/prd_md.rs` joins with a single newline (tight bullet list) rather than a blank-line-separated block. CommonMark parses a list directly under an ATX heading, so no blank line is required after `## LOG`.

## Testing Decisions

- Pure-function unit tests (no filesystem) for `log_entry` grammar, PASS vs FAIL rendering, gate segment, and the `append_log` tight join and ordering.
- `run()` tests against `tempfile` PRD fixtures for the reflection-required guard (asserting nothing is written on the error path) and the end-to-end bullet write.
- TDD ordering per repo rules: tests written with or immediately before each change.

## Immutable Success Criteria

### Grammar

- [ ] ISC-LOG-1: end-iteration writes each LOG entry as one `- **N** · date · SG — reflection → status` bullet and no longer adds the `### Iteration N` heading or the Artifacts/Tier/Invariants/Remaining bullets | Verify: Test: fixture run adds a single `- **` line and no `### Iteration` heading
- [ ] ISC-LOG-2: On a PASS iteration the bullet renders the start_commit in backticks, omitting it when start_commit is none; on a FAIL iteration the commit hash is omitted | Verify: Test: pass-with-commit, pass-without-commit, and fail fixtures
- [ ] ISC-LOG-3: The bullet renders passed milestones as `<ids> satisfied` and on FAIL appends `; FAIL: <failed and unverified ids>` | Verify: Test: all-pass and mixed-fail fixtures
- [ ] ISC-LOG-4: end-iteration --gate text renders `; text` after the milestone status, and no gate segment when the flag is absent | Verify: Test: run with and without --gate

### Contract

- [ ] ISC-LOG-5: end-iteration on a PASS iteration errors when --reflection is absent, writing neither state nor PRD; --reflection with an empty string is accepted and collapses the sentence segment | Verify: Test: PASS without reflection errors and leaves files unchanged, PASS with empty string writes an empty sentence
- [x] ISC-LOG-6: append_log appends the new bullet after the last existing entry using a single newline so entries form a tight list | Verify: Test: two existing bullets produce a contiguous third at the tail

### Docs

- [ ] ISC-LOG-7: An ADR under docs/adr/ records the block-to-one-bullet change, the start_commit-not-iteration-commit caveat, and the authored-prose trade-off | Verify: Read: ADR states the decision and the caveat
- [ ] ISC-LOG-8: CONTEXT.md notes that the LOG grammar is one bullet per iteration | Verify: Grep: CONTEXT.md contains the LOG-grammar line

## Out of Scope

- Re-syncing the `prd_work_loop` / `tdd_work_loop` SKILL.md LOG section to the new grammar — cross-repo (lives in `~/.claude/skills/`), not committable from this repo; follow-up, matching the original PRD's precedent.
- Rewriting the existing block-format entries in this repo's prior PRD — history is immutable.
- Structured parsing or test-runner integration for `--gate` — free-text only (YAGNI).

## Further Notes

- Dogfood: early iterations of this PRD close out in the current block format; once ISC-LOG-1 lands, remaining iterations log as one bullet — the same self-hosting pattern as the original PRD (Iteration 9 built `end-iteration`, Iteration 10 used it).
- ISC-LOG-5 changes the loop's own contract: after it lands, every PASS `end-iteration` MUST pass `--reflection`. The skill's UPDATE step (out-of-scope re-sync) depends on this.
- `docs/invariant_requirements.md` defines 8 invariants (INV-RUST1-4, INV-DOC1, INV-ARCH1-3); `sync` loads them into every pre-flight from iteration 2 on. Iteration 1 (SG-1) closed before the doc existed, so it ran on ISC milestones only.

## LOG

### Iteration 1 — 2026-07-03
- **Start commit:** `6013cd6`
- **Artifacts:** `src/prd_md.rs` (tier: standard)
- **Milestones addressed:** ISC-LOG-6
- **Invariants verified:** none registered
- **Overall:** PASS
- **Reflection:** prd-state on PATH stays block-format until 'cargo install --path .'; rebuild after SG-2 lands or later iterations won't dogfood one-bullet output.
- **Remaining:** 7 milestones pending
