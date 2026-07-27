---
title: Requirement-line annotations
status: NOT_STARTED
created: 2026-07-27
updated: 2026-07-27
labels: [ready-for-agent]
verification_summary: "Iteration 4: 1/1 PASS (ISC-ANN-7)"
failing_criteria: none
last_phase: UPDATE
---

# PRD: Requirement-line annotations

## Problem Statement

Requirement lines carry no slot for author metadata about a requirement. Writing `- [ ] INV-D1 (blocking): text` aborts `prd-state sync` entirely: `parse_requirements` splits on the first `": "`, yielding `id = "INV-D1 (blocking)"`, which fails the `!id.contains(' ')` guard and raises the hard parse error (`src/prd_md.rs:36-51`). The author's only recourse is to delete the annotation or hand-register the requirement via `req add`, which cannot express it either.

## Solution

Add an optional parenthesised annotation between the ID and its colon:

```
- [ ] (INV|ISC)-<id>[ (<annotation>)]: <text> | Verify: <method>
```

The annotation is restricted free text — one per line, immediately before `": "`, containing neither `)` nor `:`. It is parsed into `ParsedReq`, persisted on `Requirement`, and read by nothing. It is metadata for humans and agents reading the PRD and `state.json`, not a control input.

The `:` ban is load-bearing, not cosmetic. It is what lets `parse_requirements` keep splitting on the first `": "` to find the ID region; the annotation is then peeled off that region by `rsplit_once(" (")` when it ends with `')'`. Because the split happens first, parentheses in the requirement *text* are never examined and stay text.

## User Stories

1. As a PRD author, I want to label a requirement inline without breaking `sync`, so that context travels with the requirement instead of living in prose elsewhere.
2. As an agent, I want the label available in `state.json`, so that I can read it without re-parsing the PRD.
3. As an agent recovering from a parse failure, I want `req add` to express the annotation, so that the fallback does not silently drop what the author wrote.

## Implementation Decisions

- **Inert by design.** `Requirement.annotation` is stored and never branched on. No pre-flight behaviour in `src/decide.rs`, no `SyncReport` counter. Adding either requires a caller that does not yet exist.
- **Parse order** in `parse_requirements`: `split_once(": ")` first (unchanged); then if the ID region ends with `')'`, `rsplit_once(" (")` splits off the annotation; then the existing `!id.contains(' ')` guard runs on the remaining ID. That guard is what rejects `INV-D1 (a) (b)` (leaves `INV-D1 (a)`) and `INV-D1 (blocking) junk`, so the one-annotation rule needs no separate check.
- **`flip_checkboxes` becomes line-oriented.** The current whole-document `String::replace` on the literal `- [ ] {id}:` (`src/prd_md.rs:57-63`) cannot match an annotated line, so a satisfied milestone would be recorded in `state.json` with its checkbox left unflipped — silent divergence between reader and writer, which is the exact agreement the module exists to guarantee. Rewriting it per-line to accept `{id}:` or `{id} (…):` fixes that and closes a latent bug: the unanchored `replace` today also flips the literal string where it appears in prose or a LOG bullet.
- **Annotation is not identity.** Registry lookup stays `r.id == p.id`, so editing an annotation does not read as a removed-then-added milestone and cannot reset a milestone's status.
- **Source-authoritative, like `text`.** `upsert_from_parsed` overwrites the annotation from the parsed source, including clearing it to `None`. Sticky-on-insert would let a stale PRD value outrank `docs/invariant_requirements.md`, breaking the doc-wins precedence in `src/sync.rs:12-43`.
- **Never rendered.** The annotation appears only in the PRD line and `state.json`. `end_iteration` already writes `{id} (unverified)` into the `failing_criteria` frontmatter (`src/end_iteration.rs:143`); rendering an authored annotation alongside it would produce two same-shaped parenthetical suffixes with different meanings in a field the loop skill consumes. Annotation text may also contain the LOG bullet's structural delimiters (`·`, `—`, `;`, `→`, per `docs/adr/0001-log-one-bullet.md`).
- **`req add --annotation <text>`** is optional. `Requirement::new` gains the parameter regardless, because `upsert_from_parsed` calls it directly. The annotation is split only inside `prd_md`; `main.rs` and `req.rs` never parse the grammar (`INV-ARCH2`).
- **Version 0.5.0.** `Requirement` carries `#[serde(deny_unknown_fields)]` (`src/state.rs:129`), so a v0.4.0 binary cannot load a `state.json` containing an `annotation` key. `skip_serializing_if = "Option::is_none"` keeps the key absent until a PRD actually uses an annotation, making the break opt-in per project rather than automatic on upgrade.

## Testing Decisions

- Pure-function unit tests in `src/prd_md.rs` for the grammar, the text-parens case, the malformed forms, and the flip round-trip; per `INV-RUST4` they stay in-file.
- `src/state.rs` tests cover the serde shape both directions and the upsert overwrite/clear path; `src/req.rs` covers the CLI fallback.
- The never-rendered decision gets a real assertion, not a comment: a `decide` pre-flight test asserts the annotation text is absent from the printed rows.
- TDD ordering per repo rules: tests written with or immediately before each change.

## Immutable Success Criteria

### Grammar

- [x] ISC-ANN-1: parse_requirements accepts one optional parenthesised annotation immediately before the colon on both INV- and ISC- lines, exposing it on ParsedReq, and yields None when absent | Verify: Test: annotated INV line, annotated ISC line, and unannotated line
- [x] ISC-ANN-2: Parentheses in the requirement text are left in the text and never captured as the annotation | Verify: Test: line whose text contains parenthesised prose parses with annotation None and text intact
- [x] ISC-ANN-3: Two annotations on one line and an unclosed annotation are hard parse errors carrying the line number and the `req add` fallback hint | Verify: Test: `INV-D1 (a) (b)` and `INV-D1 (blocking: text` both error

### Reader/writer agreement

- [x] ISC-ANN-4: flip_checkboxes flips an annotated requirement line and leaves the annotation byte-identical, and does not flip a `- [ ] <id>:` literal occurring outside a requirement line | Verify: Test: annotated flip preserves annotation, prose/LOG occurrence untouched, parse-flip round trip includes an annotated requirement

### State

- [x] ISC-ANN-5: Requirement carries an optional annotation that is omitted from state.json when absent and loads as absent from a state.json lacking the key | Verify: Test: serialize omits key when None, load of annotation-free JSON succeeds
- [x] ISC-ANN-6: upsert_from_parsed takes the annotation from the parsed source, including clearing it, and an annotation change leaves the requirement's status untouched | Verify: Test: overwrite, clear, and status-preserved-on-annotation-change fixtures
- [x] ISC-ANN-7: req add accepts an optional annotation and stores it, defaulting to absent | Verify: Test: add with and without the flag

### Containment

- [ ] ISC-ANN-8: No command renders the annotation; the decide pre-flight rows contain the requirement id and text only | Verify: Test: pre-flight output for an annotated invariant excludes the annotation text

### Docs

- [ ] ISC-ANN-9: The parse_requirements docstring states the full line grammar including the optional annotation and its restrictions, and CONTEXT.md defines Annotation as inert non-identity metadata | Verify: Read: docstring states the grammar; Grep: CONTEXT.md contains the Annotation entry
- [ ] ISC-ANN-10: Cargo.toml is 0.5.0 | Verify: Grep: Cargo.toml version line

## Out of Scope

- Any behaviour keyed on annotation content — `(blocking)` changes no control flow. Deferred until a caller exists.
- A `SyncReport` counter for annotation changes; an annotation-only edit reports as unchanged, exactly as a text-only edit does today.
- Rendering the annotation in `status`, `decide`, the LOG bullet, or frontmatter.
- A closed annotation vocabulary. `prd_md` owns the grammar, not the taxonomy (`INV-ARCH2`).
- Relaxing `deny_unknown_fields` for forward compatibility with older binaries.
- Re-syncing `~/.claude/skills/prd_work_loop/SKILL.md` to document the annotation — cross-repo, not committable here.

## Further Notes

- Dogfood: this PRD's own requirement lines are unannotated until ISC-ANN-1 lands, after which later iterations may annotate them. Annotating any line in this repo pins it to prd-state >= 0.5.0 (see the version decision).
- The design was settled in a grilling session covering nine branches: inert-vs-behavioural, grammar restrictions, the flip rewrite, upsert semantics, the CLI fallback, rendering, parse order, documentation home, and the forward-compat break. Q8 of that session deliberately chose no ADR; the rationale lives in this PRD and in the `parse_requirements` docstring.

## LOG

### Iteration 1 — 2026-07-27
- **Start commit:** `931300a`
- **Artifacts:** `src/prd_md.rs` (tier: standard)
- **Milestones addressed:** ISC-ANN-1, ISC-ANN-2, ISC-ANN-3
- **Invariants verified:** all PASS
- **Overall:** PASS
- **Reflection:** Parse order does the work: peeling the annotation off the ID region after split_once(": ") means the existing !id.contains(space) guard rejects both malformed forms for free — no separate one-annotation check. Only an empty annotation needed a new guard. Adding a field to ParsedReq forced 6 test-literal edits in state.rs, a reminder that public struct literals leak across module lines even when the schema stays put.
- **Remaining:** 7 milestones pending

### Iteration 2 — 2026-07-27
- **Start commit:** `931300a`
- **Artifacts:** `src/prd_md.rs` (tier: standard)
- **Milestones addressed:** ISC-ANN-4
- **Invariants verified:** all PASS
- **Overall:** PASS
- **Reflection:** split_inclusive(char) is the right primitive for line-oriented markdown rewrites: it keeps every terminator, so the untouched bytes are trivially byte-exact and no trailing-newline or CRLF special case is needed — lines() would have forced a rejoin decision. Also: matching an id needs a right boundary, not just a prefix. Writing the ISC-A1-vs-ISC-A10 test surfaced that the old whole-document replace was already prefix-safe only by luck of the colon.
- **Remaining:** 6 milestones pending

### Iteration 3 — 2026-07-27
- **Start commit:** `931300a`
- **Artifacts:** `src/state.rs` (tier: standard)
- **Milestones addressed:** ISC-ANN-5, ISC-ANN-6
- **Invariants verified:** all PASS
- **Overall:** PASS
- **Reflection:** Splitting SG-3 by file boundary paid off immediately: state.rs owns the schema, so keeping Registry::add at its old arity and threading the annotation only through Requirement::new + upsert left req.rs/main.rs untouched — the CLI arity change is now isolated in SG-3b. Also: skip_serializing_if on the new field is what keeps deny_unknown_fields from being an unconditional break; the absent-key load test and the omitted-key serialize test are two halves of one guarantee and belong in one test fn.
- **Remaining:** 4 milestones pending

### Iteration 4 — 2026-07-27
- **Start commit:** `931300a`
- **Artifacts:** `src/req.rs`, `src/main.rs` (tier: standard)
- **Milestones addressed:** ISC-ANN-7
- **Invariants verified:** all PASS
- **Overall:** PASS
- **Reflection:** The dogfood step caught what the unit test could not: the prd-state on PATH is still 0.4.0, so a --annotation dogfood run silently exercised the old binary and died with 'unexpected argument'. Any in-repo dogfood of an unreleased flag must go through cargo run, not the PATH binary — and this loop's own --gate flag hits the same wall. Second: widening Registry::add by one param cost 10 test call-site edits across 4 files; perl handled the bulk but missed the reg.add receiver spelling and one non-literal arg, so compiler errors, not the regex, were the real completion check. Test gate: 89 tests GREEN, clippy pedantic clean.
- **Remaining:** 3 milestones pending
