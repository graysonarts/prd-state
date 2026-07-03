# Invariant Requirements

`INV-*` rules re-verified every iteration (never satisfied). Authoritative source for `sync`; see [CONTEXT.md](../CONTEXT.md) for registry mechanics.

## Rust (portable — any Rust project)

- [ ] INV-RUST1: No `unwrap()` or `expect()` on production paths; tests and `main()` startup are exempt; any other use carries a `// SAFETY:` or `// INVARIANT:` comment | Verify: Grep
- [ ] INV-RUST2: No `unsafe` block without an adjacent `// SAFETY:` comment stating why it is sound and the invariants the caller must uphold | Verify: Grep unsafe
- [ ] INV-RUST3: `cargo clippy` runs in pedantic mode and is clean — the crate declares `[lints.clippy] pedantic`; no `#[allow(...)]` without a justifying comment | Verify: cargo clippy
- [ ] INV-RUST4: Unit tests live in a `#[cfg(test)]` module at the bottom of the file they test; code ships its tests the same iteration | Verify: Test
- [ ] INV-DOC1: Doc comments are concise and token-dense — no sentence restates what the identifier name, type, visibility, or call site already conveys | Verify: Custom

## prd-state (architecture)

- [ ] INV-ARCH1: Pure decision core, thin I/O shell — policy functions (e.g. `outcome`, the `prd_md` transforms) take and return values with no `fs`/`Command`/env I/O; filesystem and process calls stay in the `run`/caller shell | Verify: Read
- [ ] INV-ARCH2: The PRD grammar — requirement lines, `- [ ]` checkboxes, `## LOG`, frontmatter — is read and written only in `prd_md`; no other module parses or emits it | Verify: Grep
- [ ] INV-ARCH3: `state.json` (de)serialization and the `State` schema live only in the `state` module; other modules go through it and never touch `state.json` directly | Verify: Grep
