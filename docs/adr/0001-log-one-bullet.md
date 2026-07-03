# 0001 — LOG entries are one dense bullet, not a per-iteration block

`end-iteration` wrote a 7-line `### Iteration N` block per iteration (start commit, artifacts, tier, invariant roll-up, overall, reflection, remaining) — fully derived, yet it grew the PRD tail ~7 lines × N and restated identifiers recoverable from state and git.

**Decision.** `log_entry` emits one bullet: `- **N** · date · commit · SG — reflection → ids satisfied[; gate][; FAIL: reason]` (full grammar in the [PRD](../../.prd/active/log-bullet-format/PRD-20260703-log-bullet-format.md)). Artifacts, tier, invariant roll-up, and remaining count are dropped — all recoverable from `state.json` and git. Per user-directed PRD.

**start_commit caveat.** The commit slot renders `start_commit`, the HEAD captured at OBSERVE — the *parent* of the iteration's own commit. `end-iteration` runs before the skill's auto-commit, so it cannot know the real iteration hash; that is recovered later via `git log`. Rendered on PASS only.

**Authored-prose trade-off.** The summary is no longer derived. `--reflection` becomes required on a PASS iteration (end-iteration bails before any write if absent), and `--gate` carries free-text test-runner specifics. Two authored fields the tool cannot invent, in exchange for dropping five it was inventing.
