# Open Radio Lab

Native tools for inspecting, editing and programming supported two-way radios.

**Version:** 0.6.0 · **License:** GPL-3.0-only · **Desktop UI:** Slint · **Core:** Rust

Open Radio Lab focuses on documented device profiles, byte-preserving configuration files and verified device operations. It is a working hardware tool. The current implementation supports one TK-2402K profile; the project name does not imply universal radio support.

[Guía en español](docs/quickstart.es.md) · [Compatibility](docs/compatibility.md) · [Architecture](docs/architecture.md) · [Source languages](docs/source-languages.md)

## Features

- Native desktop editor with 16 channel positions and separate RX/TX and Decode/Encode settings.
- Windows file dialogs for Open and Save As; DAT, JSON and CSV interchange.
- Direct serial read without opening an input file first.
- Configuration writes with a complete live backup, a restricted change plan and full-image verification through a new connection.
- Undo/redo, recovery snapshots and preservation of unknown binary fields.
- A CLI for file inspection, normalized binary differences, exports and serial reads.

## Supported scope

| Area | Current status |
| --- | --- |
| Desktop platform | Windows x86-64 tested; Linux/macOS not validated |
| Device profile | TK-2402 family identification, K-type writes |
| DAT container | KPG-158D V2.20, 8257 bytes, with an 8192-byte image |
| Physically exercised | Identification, full read, Scan Add CH1 write and full-image verification |
| Additional field tests | RX/TX frequencies and QT Encode/Decode writes reported successful by a hardware tester |
| Editable DAT fields | Existing active RX/TX, QT-to-QT, power, bandwidth and Scan Add CH1 |
| Remaining DAT work | None/DQT encoding, adding/removing channels, other Scan Add positions and optional/global settings |

None/DQT values can be represented in JSON/CSV and selected in the UI. That does not imply their binary DAT encoding is implemented. Unsupported edits are rejected. See [compatibility and validation](docs/compatibility.md) for the evidence boundaries.

## Build

Install a Rust toolchain and the native compiler required by your platform. Rust 1.99 or later is the supported toolchain baseline for this release.

```sh
cargo build --locked --workspace
cargo run --locked -p radio-app
```

Windows contributors can use:

```powershell
./scripts/dev.ps1 -Action check
./scripts/dev.ps1 -Action build
./scripts/dev.ps1 -Action run
```

See [build instructions](docs/build.md). Source archives do not contain executables; portable binaries belong in a release asset, with corresponding source and dependency notices.

## Use the application

1. Open **Conexión**, select the radio's serial port, and choose **Leer configuración de la radio**. An input DAT is optional.
2. Edit a channel in **Canales**, then choose **Aplicar canal**.
3. Use **Archivos → Guardar como…** to save a separate file, or **Conexión → Escribir cambios en la radio** to program the supported changes.

Close other programming applications while accessing a port. A write first reads the radio and persists a backup, applies supported differences to the live image, writes only changed blocks, then reconnects and compares all image bytes. An ambiguous response stops the operation without automatic retry or rollback.

The current UI is Spanish. Code, architecture and contribution documentation use English.

## CLI

```sh
cargo run --locked -p dat-analyzer -- profile.dat
cargo run --locked -p dat-analyzer -- diff before.dat after.dat
cargo run --locked -p dat-analyzer -- export-json profile.dat profile.json
cargo run --locked -p dat-analyzer -- export-csv profile.dat profile.csv
cargo run --locked -p dat-analyzer -- ports
cargo run --locked -p dat-analyzer -- read-radio SERIAL_PORT readback.dat
```

Replace `SERIAL_PORT` with the port reported on your system. Listing ports does not open a device. Physical operations require an explicit command or button action.

## Check the source

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
node scripts/source-inventory.cjs --check
node scripts/check-public-source.cjs
```

Software tests run without a connected radio. GitHub Actions is configured for Windows software checks; a workflow file is not evidence of a successful hosted run or a physical device test.

## Language composition

<!-- source-languages:start -->

| Language | Files | Bytes | Lines | Share of source bytes |
| --- | ---: | ---: | ---: | ---: |
| Rust | 12 | 163007 | 4438 | 77.17% |
| Slint | 1 | 36759 | 844 | 17.40% |
| JavaScript | 3 | 9724 | 196 | 4.60% |
| PowerShell | 1 | 1524 | 47 | 0.72% |
| Shell | 1 | 210 | 8 | 0.10% |

<!-- source-languages:end -->

The [generated language report](docs/source-languages.md) shows percentages by UTF-8 bytes of first-party application code, tests and scripts. Its [JSON inventory](docs/source-inventory.json) lists every counted file and hash. Dependencies, binary releases, documentation, manifests and generated files are excluded.

GitHub uses [Linguist](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-repository-languages) for its language bar, so its classification can differ. The repository does not relabel Slint or scripts as Rust.

## Project layout

| Path | Responsibility |
| --- | --- |
| `crates/radio-model` | Validated channel types and JSON/CSV interchange |
| `crates/dat-format` | Binary container parsing and supported, byte-preserving edits |
| `crates/radio-serial` | Port discovery, device profile and restricted write plans |
| `crates/radio-app` | Slint UI, editor state, file dialogs and programming workflow |
| `crates/dat-analyzer` | Command-line inspection and conversion |
| `docs` | Format, protocol, compatibility and contributor documentation |
| `scripts` | Checks, source reports and dependency-notice tooling |

## License and provenance

Project source is licensed under [GPL-3.0-only](LICENSE). See [third-party components](THIRD_PARTY.md) and [provenance](docs/provenance.md). Required dependency notices are retained when packaging binaries.

Radio and vendor names identify technical compatibility. Open Radio Lab is an independent project and is not an official vendor product. No vendor executable, firmware, manual, decompiled listing or private device configuration is included.

