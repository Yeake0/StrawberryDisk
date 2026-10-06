# Contributing to StrawberryDisk

Thank you for contributing to StrawberryDisk. This guide defines the contribution
workflow; repository instructions remain authoritative for implementation
details.

## Before making a change

1. Read [`AGENTS.md`](AGENTS.md) and the nearest subtree instructions.
2. Keep Core, Platform, Tauri, CLI, and Vue responsibilities separate.
3. Prefer validated TOML when extending ordinary cleanup coverage.
4. Explain ownership, failure behavior, and tests before introducing a
   capability that crosses three or more domains.

## Development requirements

- Use Node.js 24 and the pnpm version declared in `package.json`.
- Use the Rust toolchain declared by the repository.
- Keep user-facing text synchronized across every supported locale resource.
- Use generic deterministic paths in tests.
- Never commit credentials, personal paths, private file names, raw scan
  results, build outputs, or local dependency directories.
- Preserve dry-run, protected-path, preflight, confirmation, and verification
  boundaries for every destructive flow.

## Validation

Run the full local validation before submitting a change:

```sh
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml -p strawberrydisk-core
```

Cross-platform changes require the applicable checks on macOS and Windows. If a
platform is unavailable, identify that unvalidated scope in the pull request.
Performance changes should describe a reproducible workload and before-and-after
result without committing raw machine reports or private datasets.

Windows service switches change automatic startup, not the current running state.
Only eligible third-party services are writable; protected or uncertain entries
remain read-only. Disabling preserves the delayed-start setting in the
administrator-owned `StrawberryDiskStartupRestoreV1` service registry value. Unknown
or malformed backup versions fail before mutation; successful re-enabling removes
the backup. Startup helper protocol v4 rejects older requests before execution.
For service-control changes, run the ignored `windows::startup::service_control`
tests in a disposable elevated Windows VM. The existing-service test requires an
explicit `STRAWBERRYDISK_TEST_SERVICE_NAMES` allowlist (semicolon-separated); choose
noncritical third-party services and verify that configuration and runtime state
are restored afterward.

On macOS, validate that closing hides the window, Dock reopening restores it,
and Quit exits immediately, including during a scan, using the bundled app.

## Pull requests

- Keep each pull request focused on one coherent purpose.
- Explain observable behavior, safety impact, and validation evidence.
- Add tests for safety boundaries, protocol changes, persistence, and
  regressions.
- Update contributor-facing README guidance in the same change as its behavior.
- Do not hide failures with allow attributes, global machine settings, or
  undocumented fallbacks.

Security vulnerabilities must follow [`SECURITY.md`](SECURITY.md) rather than a
public issue.
