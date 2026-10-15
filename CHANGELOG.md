# Changelog

## 0.6.0

- Adopt the independent Open Radio Lab name and standard source layout.
- Remove personal paths, local toolchain assumptions, conversation history and preferred-port hardcoding.
- Preserve a selected serial port when refreshing the available list.
- Document the supported device profile, binary format and evidence boundaries.
- Add reproducible source-language reports, public-source checks and Windows software CI.
- Retain the existing backup, restricted-write and full-readback behavior.

## Functional baseline

The initial supported profile provides a 16-channel editor, DAT/JSON/CSV, native Windows file selection, direct serial identification/read and verified configuration writes. Full read and Scan Add CH1 programming have a checked before/after result. Additional RX/TX and QT Encode/Decode field changes have been reported successful by a hardware tester.

