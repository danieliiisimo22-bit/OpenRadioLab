# Provenance and evidence policy

Open Radio Lab contains the project's Rust implementation, Slint presentation code, synthetic software tests and contributor tooling. It preserves third-party component references and licensing information.

The current interoperability profile was developed from controlled DAT comparisons, protocol reconstruction, static inspection of vendor software and physical read/write tests. This is not a claim of clean-room development, vendor certification or support for undocumented variants.

## Evidence categories

- **Software test:** synthetic input, expected output and rejection paths, without hardware.
- **Controlled file comparison:** a field change correlated with specific container bytes.
- **Static protocol reconstruction:** a command/response interpretation; not proof of execution.
- **Hardware report:** a tester reports an observed result; the raw evidence may remain private.
- **Verified readback:** a saved live backup, a recorded change plan, acknowledged writes and a complete new-session read compared automatically with that plan.

Documentation must state which category supports a claim. Additional tester-confirmed RX/TX and QT Encode/Decode changes are recorded as hardware reports; they are not described as independently audited readback captures.

## Public boundary

The public tree excludes personal paths, conversation history, unit identities, live channel configurations, raw images, diagnostic logs, vendor executables, manuals and decompiled output. Technical model identifiers, byte layouts, protocol limitations and required license attributions remain because they are necessary to understand compatibility and provenance.

The `Lab` name describes an engineering project with explicit support boundaries. It does not describe simulated radio functionality: physical operations use the real selected device.

