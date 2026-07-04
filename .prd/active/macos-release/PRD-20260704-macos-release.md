---
title: macOS release build + self-update
status: DRAFT
created: 2026-07-04
updated: 2026-07-04
labels: [ready-for-agent]
verification_summary: "Iteration 1: 1/1 PASS (ISC-REL-5)"
failing_criteria: none
last_phase: UPDATE
---

# PRD: macOS release build + self-update

## Problem Statement

`prd-state` has no distribution path. There are no git tags, no CI, and the only
way to get the binary is `cargo install --path .` from a checkout. A user on an
Apple Silicon Mac cannot download a built binary, and an installed binary has no
way to learn that a newer version exists or to update itself.

## Solution

Pushing a version tag builds the macOS (Apple Silicon) binary in GitHub Actions
and publishes it to a GitHub Release as a downloadable, checksummed archive. The
binary updates itself: `prd-state self-update` replaces the running binary with
the latest release, and interactive runs print a one-line notice to stderr when
a newer version is available. Because `prd-state` is invoked programmatically by
the `prd_work_loop`/`tdd_work_loop` skills, the background check never runs on
non-interactive (piped) runs, never auto-replaces the binary, and never touches
stdout.

## User Stories

1. As a user on an Apple Silicon Mac, I want to download a prebuilt `prd-state`
   binary from a GitHub Release, so that I don't have to build it from source.
2. As a user, I want each release archive to ship a SHA-256 checksum, so that I
   can verify the download.
3. As a maintainer, I want the release built automatically when I push a version
   tag, so that publishing is one `git push --tags`.
4. As a maintainer, I want the release to fail if the tag and `Cargo.toml`
   version disagree, so that I never publish a binary that reports a different
   version than its tag.
5. As a user, I want `prd-state self-update` to replace my binary with the latest
   release, so that upgrading is one command.
6. As an interactive user, I want a short notice when a newer version exists, so
   that I know to run `self-update`.
7. As the `prd_work_loop`/`tdd_work_loop` skill, I want the update check to stay
   silent and off the wire on non-interactive runs, so that it never adds latency
   or corrupts the structured output I parse.
8. As a user behind an update check, I want a network or API failure to be
   invisible, so that a flaky connection never breaks the command I actually ran.
9. As a maintainer, I want the update check throttled to at most once a day, so
   that interactive runs don't hit the GitHub API on every invocation.
10. As a CI/automation operator, I want an env var to disable the check outright,
    so that non-interactive contexts can opt out explicitly.

## Implementation Decisions

- **Target:** Apple Silicon only — `aarch64-apple-darwin`. Built natively on a
  `macos-14` runner; no cross-compilation.
- **Distribution + autoupdate:** the `self_update` crate against the GitHub
  Releases backend for `graysonarts/prd-state`. It is blocking/synchronous,
  fitting the existing no-async CLI (no Tokio). Features `archive-tar` and
  `compression-flate2` for the `.tar.gz` asset. `self_update::get_target()`
  returns `aarch64-apple-darwin`, matching the asset name; the archived binary is
  `prd-state`, matching `bin_path_in_archive`'s default.
- **Release workflow** (`.github/workflows/release.yml`): triggers on tags
  `v*.*.*`, runs on `macos-14`, holds `permissions: contents: write`. Steps:
  checkout → version guard → `cargo build --release` → tar the release binary as
  `prd-state-aarch64-apple-darwin.tar.gz` → write a `.sha256` sidecar →
  `gh release create "$TAG" --generate-notes` uploading both assets. No
  third-party actions; `gh` and the toolchain are preinstalled.
- **Version guard:** a step compares the pushed tag (with `v` stripped) to the
  `Cargo.toml` `version` and exits non-zero on mismatch, before the build.
- **`self-update` command:** a new `Cmd::SelfUpdate` in `src/main.rs` backed by
  `src/update.rs`. It builds a `self_update` GitHub updater with the current
  version taken from the compiled crate (`cargo_crate_version!()` /
  `CARGO_PKG_VERSION`), downloads the latest asset, and replaces the running
  binary in place, showing download progress on a TTY. It reports the new version
  or that the binary is already current.
- **Background notify:** `src/update.rs` exposes one pure decision function,
  `update_decision(disabled, is_tty, last_check, now) -> Decision` (the single
  test seam), collapsing all gates. A side-effectful wrapper reads
  `PRD_STATE_NO_UPDATE`, `std::io::stdout().is_terminal()`, and the cache file,
  then acts:
  - disabled, or not a TTY, or last check `< 24h` ago → skip;
  - otherwise fetch the latest release, update the cache timestamp, and if the
    latest version is greater than the running version, `eprintln` one line to
    **stderr**: `prd-state X.Y.Z available — run: prd-state self-update`.
- **Throttle cache:** `~/.cache/prd-state/last_update_check` stores the last check
  time. Missing `HOME` or an unreadable/unwritable cache degrades to "skip", never
  an error.
- **Wiring:** `main` calls the check once after the command runs, for every
  command **except** `self-update`. Any failure in the check is swallowed and does
  not affect the command's exit status.
- **Signing:** the binary is unsigned and not notarized. `self_update` writes the
  replacement file itself, so it carries no `com.apple.quarantine` xattr and runs
  without a Gatekeeper prompt. Only a first browser download is quarantined; the
  one-time fix (`xattr -d com.apple.quarantine prd-state`) is documented.

## Testing Decisions

- **Good test:** exercises external behavior, not internals. The load-bearing
  logic here is the gate decision — a pure function of four inputs — so it is
  tested directly with no filesystem, network, or TTY.
- **Unit tests** (`#[cfg(test)]` in `src/update.rs`) cover `update_decision`:
  disabled → skip; non-tty → skip; last check `< 24h` → skip; enabled + tty +
  stale (or never checked) → check. Prior art: the pure `log_entry` grammar tests
  in `src/end_iteration.rs`.
- **Manual / integration** (not unit-tested; recorded as evidence by pushing a
  tag): the release workflow producing the tarball + checksum on a real tag, and
  `self-update` replacing the binary against a newer published release. These
  cross the GitHub API and filesystem-replacement boundaries that unit tests
  should not.

## Immutable Success Criteria

### Release CI

- [ ] ISC-REL-1: `.github/workflows/release.yml` triggers on `v*.*.*` tag pushes, runs on a macOS Apple-Silicon runner, and creates a GitHub Release for the tag | Verify: Read: workflow `on.push.tags` is `v*.*.*`, runner is `macos-14`, a step runs `gh release create`
- [ ] ISC-REL-2: the release uploads `prd-state-aarch64-apple-darwin.tar.gz` (the release binary) and a matching `.sha256` checksum sidecar as assets | Verify: Read: workflow tars `target/release/prd-state`, writes a sha256 file, and passes both to `gh release create`
- [ ] ISC-REL-3: the workflow fails, before building, when the pushed tag (v stripped) does not equal the `Cargo.toml` version | Verify: Read: a guard step compares tag to Cargo version and exits non-zero on mismatch

### Self-update

- [ ] ISC-REL-4: `prd-state self-update` uses the compiled crate version as "current", downloads the latest `aarch64-apple-darwin` GitHub release asset, replaces the running binary, and reports the new version or that it is already current | Verify: Read: updater built with `cargo_crate_version!()` and GitHub backend for `graysonarts/prd-state`; Manual: self-update against a newer release replaces the binary

### Background notify

- [x] ISC-REL-5: `update_decision(disabled, is_tty, last_check, now)` is a pure function returning check-or-skip, unit-tested for disabled→skip, non-tty→skip, last-check<24h→skip, and enabled+tty+stale→check | Verify: Test: the four decision cases
- [ ] ISC-REL-6: the check runs only after a command and never for `self-update`; any failure in the check (no HOME, cache error, network/API error) leaves the command's exit status unchanged | Verify: Read: `main` invokes the check at the end for every command except `SelfUpdate`, and the check returns without surfacing errors
- [ ] ISC-REL-7: when a newer version exists, the notice is a single line written to stderr that names the new version and the `self-update` command | Verify: Read: the notice uses `eprintln!` and includes the version and `self-update`

### Docs

- [ ] ISC-REL-8: an ADR under `docs/adr/` records the distribution choices — self_update + GitHub Releases, Apple-Silicon-only, notify-interactive-only, and unsigned/no-notarization — with their trade-offs | Verify: Read: ADR 0002 states these decisions and why
- [ ] ISC-REL-9: user-facing docs note the one-time first-download Gatekeeper quarantine fix | Verify: Grep: README or CONTEXT.md contains `xattr -d com.apple.quarantine`

## Out of Scope

- Intel (`x86_64`) or universal binaries; Linux and Windows targets.
- Code signing and notarization (Apple Developer account; revisit only for wide
  distribution).
- Auto-applying updates without an explicit `self-update` (never replaces the
  binary during a normal run).
- Homebrew tap or other package managers.
- A Rust integration test that drives the real GitHub API or replaces the binary
  — verified manually via a real tag.

## Further Notes

- **Version flow:** bump `Cargo.toml` (per CLAUDE.md semver rule) → commit → `git
  tag vX.Y.Z` → `git push --tags`. The CI guard enforces tag/Cargo agreement.
- **Dependency weight:** `self_update` pulls in `reqwest` + TLS — meaningful for a
  currently-tiny CLI, but unavoidable for GitHub-Releases autoupdate; hand-rolling
  HTTP + extract is more code (YAGNI). On macOS, reqwest's default TLS uses Secure
  Transport (no OpenSSL); switch to the crate's `rustls` feature only if a build
  issue appears.
- **Runner minutes:** `macos-14` minutes are billable on private repos, free on
  public. Confirm `graysonarts/prd-state` visibility.
- **First release:** there is no existing tag; the first `v*.*.*` push both
  creates the release and is the baseline `self-update` compares against.
- **Issue tracker:** `to-prd` publishes to the project issue tracker with the
  `ready-for-agent` label. No tracker vocabulary was provided this session, so the
  label lives in frontmatter; creating a GitHub issue is a separate, outward-facing
  action pending user approval.

## LOG
- **1** · 2026-07-04 · `d598271` · SG-DECISION — src/update.rs carries a module-level #[allow(dead_code)] (transient TDD window). SG-UPDATE-IO MUST delete it when the check wrapper calls update_decision, else it silently hides real dead code. update_decision uses Unix-epoch-second u64s — the I/O shell must convert SystemTime->u64 at the boundary. → ISC-REL-5 satisfied; RED 3->GREEN 5, cargo test ok, clippy clean
