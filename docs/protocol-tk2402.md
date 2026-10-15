# TK-2402 device profile

The implementation is in `radio-serial::tk2402`. The profile is deliberately limited to observed behavior. No command for RF transmission, tuning or firmware updates is exposed.

## Session entry and identity

1. Open the selected serial port at 9600 baud, 8 data bits, no parity, 2 stop bits, no flow control.
2. Set DTR, clear RTS, reset buffers and send ASCII `PROGRAM`.
3. Receive a byte, switch to 19200 baud and receive the second entry response.
4. Send `0x02` and receive a 40-byte identity.
5. Send a time-derived seed and `0x06`; require an acknowledgment.
6. Set the session XOR mask to `seed ^ wrapping_sum(identity[24..36])`.

The supported family prefix is `P2402\0\xFF\xFF`. Configuration writes additionally require the K type marker. An image's identity region must agree with the initial identification.

All later request and response bytes use the session XOR mask. Timing, response lengths and variants are part of the profile; unsupported variants must not be discovered by sending random commands.

## Full image read

- Read addresses `0x0000` through `0x1F80`, in steps of 128 bytes.
- Request: `R`, address high byte, address low byte, length `0x80`.
- Expected response: `W`, matching address, matching length, then 128 bytes.
- Send `0x06` and require the corresponding acknowledgment before the next block.

The completed image is 8192 bytes. Short/unknown markers are rejected; missing bytes are never replaced with assumed `0xFF` data.

## Restricted configuration write

Normal configuration programming uses `Y`, not the separate W operation found in other protocol paths.

Each frame contains `Y`, address high/low, length, and up to 64 payload bytes. A separate byte contains the wrapping sum of the payload. Frame and sum are XOR-encoded. The radio must acknowledge with `0x06`.

The current implementation uses aligned 64-byte frames and sends only blocks containing supported differences. Allowed changed bytes are active RX/TX, QT values, known power/bandwidth bits and the corroborated CH1 Scan Add fields. All neighboring bytes come from the current live read. Identity, service, calibration and unmapped regions cannot be changed through a write plan.

`RadioSession::apply_plan` requires a complete matching read in that session. The application additionally persists and checks the backup and plan before calling it. An ambiguous acknowledgment stops the operation without an automatic retry.

## Exit and verification

Send masked `E` and require `0x06`. A programming operation then closes the port and opens a new session for an entire image read. Success requires exact equality with the planned image and confirmed exits.

## Evidence and limits

The frame structure was reconstructed from controlled file differences, static inspection and device tests. The Y/byte-sum path was subsequently exercised physically with a CH1 Scan Add change: both affected blocks were acknowledged and a new-session image matched all planned bytes.

Other QT/frequency changes are reported as successful hardware use; that does not establish compatibility with every firmware, radio variant or failure mode. Password behavior, short read responses and interrupted-write recovery remain unvalidated. No vendor binary, firmware or decompiled listing accompanies this profile.

