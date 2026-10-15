# Contributing

Start with [architecture](docs/architecture.md), [compatibility](docs/compatibility.md), and the relevant device-profile document. Use descriptive names, small functions and comments that explain contracts or byte-level reasoning.

## Development checks

Run the commands in the README before submitting changes. Regenerate the source inventory after changing code or scripts:

```sh
node scripts/source-inventory.cjs
node scripts/source-inventory.cjs --check
node scripts/check-public-source.cjs
```

Node.js is used for repository tooling, not by the desktop application. PowerShell and shell scripts wrap standard Cargo commands and do not install or replace system tools.

## Code conventions

- Rust: edition 2024, `rustfmt`, `clippy` with warnings denied, typed errors at library boundaries.
- Slint: declarative presentation; hardware and file operations stay in Rust.
- English identifiers, API documentation and contributor-facing explanations.
- Keep user-visible messages separate from protocol invariants; the current UI remains Spanish.
- Add tests for new parsing rules, accepted field changes and rejection paths. Do not add tests that only repeat the implementation.
- Keep physical operations out of tests, startup, import, rendering and source-report commands.

## Device-profile changes

Document request bytes, framing, responses, offsets and the exact evidence that supports a change. Separate static analysis, synthetic software fixtures, tester reports and automatically checked readback. A successful test on one model/type does not validate other variants.

Preserve unknown bytes and identity/service regions. A write must have a complete current-device backup and a fresh verification read. Do not introduce blind retries after ambiguous acknowledgments.

## Public data

Use synthetic fixtures in the repository. Keep live DAT files, raw images, identity records, diagnostic logs and generated plans out of commits and issues. Redact device identifiers and channel data before sharing a diagnostic summary.

Contributions are distributed under the project's GPL-3.0-only license. Preserve third-party notices and technical source attributions. Do not claim a clean-room implementation or vendor certification without evidence.

