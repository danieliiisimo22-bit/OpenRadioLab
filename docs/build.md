# Build and release

## Requirements

- Rust 1.99 or newer; the repository's `rust-toolchain.toml` selects stable.
- A native C/C++ linker appropriate for the selected Rust target.
- Node.js 22 or newer only for source reports and release notice tooling.

The default UI backend uses winit with software rendering. Cargo.lock is committed for reproducible dependency resolution. A lockfile does not make platform validation interchangeable.

## Windows

Use Rust with Visual Studio Build Tools/Windows SDK for an MSVC build, or a GNU toolchain with a compatible MinGW linker. The physically exercised baseline used x86-64 GNU; MSVC is the configured hosted CI target and requires its own completed run.

```powershell
rustup component add rustfmt clippy
./scripts/dev.ps1 -Action check
./scripts/dev.ps1 -Action build
./scripts/dev.ps1 -Action run
```

The helper uses the Rust/compiler environment supplied by the contributor. It does not search another project, change global PATH, install drivers or download toolchains.

## Linux

Install Rust, a native compiler and the X11/Wayland/XKB development libraries required by winit on the target distribution. For immutable distributions, use the normal development-container workflow for that system.

```sh
sh scripts/check.sh
cargo build --locked --workspace
cargo run --locked -p radio-app -- --data-dir ./open-radio-lab-data
```

Linux is not physically validated for this profile. The current file-dialog adapter is Windows-only. Device names come from serial discovery; they are not assumed to match Windows names. Consult the official [Slint desktop platform guide](https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/) for native dependencies.

## macOS

Install Rust and Xcode Command Line Tools, then use the same Cargo commands. Build for the architecture of the machine. Native dialogs, device permissions, cable compatibility, application packaging and signing are not validated here.

## Software checks

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
node scripts/source-inventory.cjs
node scripts/source-inventory.cjs --check
node scripts/check-public-source.cjs
```

Optional UI verification, without hardware access:

```sh
cargo run --locked -p radio-app -- --smoke-test --data-dir ./target/ui-check
```

## Release assets

Build the actual target, include the GPL license and corresponding source, and generate dependency notices using `scripts/licenses.cjs`. Keep executables, dependency notices and private runtime data outside the source repository. The public source guard rejects release binaries and device files in the source tree.

Test the portable package on its target OS. Release signing, installers and hosted CI outcomes must be reported separately from a local debug build.

