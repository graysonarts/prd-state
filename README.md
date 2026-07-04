# prd-state

CLI that owns the `state.json` schema for the `prd_work_loop`/`tdd_work_loop` skills. See [CONTEXT.md](CONTEXT.md) for the domain model.

## Install

Download `prd-state-aarch64-apple-darwin.tar.gz` from the [latest release](https://github.com/graysonarts/prd-state/releases/latest), verify it against the `.sha256` sidecar, and extract:

```sh
shasum -a 256 -c prd-state-aarch64-apple-darwin.tar.gz.sha256
tar xzf prd-state-aarch64-apple-darwin.tar.gz
```

Apple Silicon (`aarch64-apple-darwin`) only. The binary is unsigned and not notarized, so a browser download is quarantined by Gatekeeper. Clear it once:

```sh
xattr -d com.apple.quarantine prd-state
```

Binaries installed by `prd-state self-update` are not quarantined — the one-time fix is only for the first browser download.

## Update

```sh
prd-state self-update
```

Replaces the running binary with the latest release. Interactive runs also print a one-line notice to stderr when a newer version is available; the check is silent on piped runs and can be disabled with `PRD_STATE_NO_UPDATE=1`.

## Build from source

```sh
cargo install --path .
```

## Release

Bump `Cargo.toml` (semver), commit, then `git tag vX.Y.Z && git push --tags`. CI builds the macOS binary and publishes the release; the build fails if the tag and `Cargo.toml` version disagree.
