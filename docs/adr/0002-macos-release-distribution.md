# 0002 — macOS binary distribution and self-update

`prd-state` had no distribution path: no tags, no CI, `cargo install --path .` only. It needs a downloadable binary that can update itself, invoked programmatically by the work-loop skills.

**Decision — `self_update` + GitHub Releases.** A pushed `v*.*.*` tag builds on `macos-14` and publishes a checksummed `.tar.gz` to a GitHub Release; `self_update` (GitHub backend, `graysonarts/prd-state`) downloads and replaces the binary. It is blocking, matching the no-async CLI. *Trade-off:* pulls in `reqwest` + TLS — heavy for a tiny CLI, but hand-rolling HTTP fetch + archive extraction is more code for the same result (YAGNI). On macOS reqwest uses Secure Transport, no OpenSSL.

**Decision — Apple Silicon only (`aarch64-apple-darwin`).** Built natively; `self_update::get_target()` matches the asset name. *Trade-off:* Intel Macs, universal binaries, Linux, and Windows are unserved — acceptable while the audience is the maintainer's Apple-Silicon machines. Revisit by adding runners, not by rearchitecting.

**Decision — notify on interactive runs only.** The background check `eprintln`s one line to **stderr** when a newer version exists, gated to TTY + at-most-daily + not disabled by `PRD_STATE_NO_UPDATE`; it never auto-replaces the binary. *Why:* the skills parse structured stdout, so the check must never touch stdout, add latency to piped runs, or update without an explicit `self-update`. Any check failure is swallowed — a flaky network never breaks the command that ran.

**Decision — unsigned, not notarized.** No Apple Developer account. *Trade-off:* a browser-downloaded binary carries `com.apple.quarantine` and needs a one-time `xattr -d` (documented in the README). `self_update` writes the replacement itself, so updated binaries carry no quarantine xattr and run without a Gatekeeper prompt. Signing is deferred until wide distribution justifies the account cost.
