# Third-party components

The repository contains first-party project source and a Cargo lockfile. Dependency source is downloaded by Cargo; it is not copied into this source tree or counted as project language composition.

| Component | Use | Licensing reference |
| --- | --- | --- |
| Rust standard library and toolchain | Build/runtime support | [Rust licenses](https://github.com/rust-lang/rust/tree/master/LICENSES) |
| Slint | Native declarative UI | [Slint licensing](https://github.com/slint-ui/slint/blob/master/LICENSE.md); GPL-compatible option used by this project |
| serialport | Serial discovery and transport | License recorded in Cargo metadata for the locked package |
| serde, serde_json, csv | Typed configuration exchange | Licenses recorded in Cargo metadata |
| anyhow, thiserror, tracing | Errors and diagnostics | Licenses recorded in Cargo metadata |
| tempfile, sha2, png, Windows bindings | Atomic files, hashes, rendering and native dialogs | Licenses recorded in Cargo metadata |

For a binary release, generate Cargo metadata for its actual target and copy the dependency notices:

```sh
cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-gnu > metadata.json
node scripts/licenses.cjs metadata.json dist/notices
```

The resulting notice inventory includes build/test dependencies as well as runtime dependencies; it must not be described as a list of code embedded in the executable. Include the applicable Rust runtime notices when distributing a binary.

Conventional tone/code catalog references are retained next to their definitions. Protocol and format facts are documented in the device-profile documents. Vendor software and firmware are not redistributed.

