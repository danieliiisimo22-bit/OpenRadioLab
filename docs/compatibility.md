# Compatibility and validation

## Implemented profile

The current profile targets TK-2402 family identification and K-type configuration writes. The desktop application is tested on Windows x86-64. Other radio variants and operating systems require separate validation.

| Capability | Evidence | Limits |
| --- | --- | --- |
| DAT parsing and preservation | Synthetic tests and controlled binary comparisons | One V2.20 container profile |
| Supported DAT edits | Parser round trips and reopening in the vendor application | Existing active channels and mapped fields |
| Serial identification | Physical 40-byte identity and confirmed PC-mode exit | Current profile |
| Full serial read | 64 acknowledged blocks, 8192-byte image and parsed channels | Long W responses |
| Scan Add CH1 write | Two changed bytes, two acknowledged Y blocks, new-session full readback identical to the plan | One K-type unit |
| RX/TX and QT Encode/Decode writes | Successful physical use reported by a hardware tester | Raw before/after reports not independently audited for these additional changes |
| Power and bandwidth | Binary mapping, typed validation and file round trips | Separate physical change tests remain desirable |
| None/DQT | Typed JSON/CSV model and UI catalog | Binary DAT edits rejected |
| Other platforms/models | Source architecture only | No claim of validated operation |

## Software checks

The functional baseline contains 34 automated tests across model, format, transport, editor and CLI code. Tests use synthetic fixtures and do not access a radio. See the build/CI commands for reproducible checks.

A configured CI workflow does not establish a successful hosted run until it executes in the target repository. Likewise, software tests do not replace a physical protocol test.

## Known limitations

- Adding/removing channels, disabling RX/TX and optional/global settings lack complete binary mappings.
- Scan Add's global bitmap is corroborated only for channel 1.
- Short/unknown block responses are rejected instead of being filled with assumed values.
- A prior vendor-produced image differed from raw readback at three opaque non-channel offsets. Their meaning remains unresolved; raw bytes are preserved.
- Password-protected variants, interrupted-write recovery and other firmware/type combinations are not validated.
- Frequency-range validation is a technical constraint, not permission to transmit.

Keep unit identifiers, live configurations, images and detailed reports outside the public repository. Issue reports can describe the model/type, operation and redacted outcome.

