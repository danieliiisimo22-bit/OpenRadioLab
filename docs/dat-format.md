# DAT container profile

This document describes the supported KPG-158D V2.20 / TK2402 file profile. Vendor/model identifiers are interoperability facts, not project branding.

## Container layout

| File offset | Size | Meaning |
| --- | ---: | --- |
| `0x0000` | 7 | ASCII `KPG158D` |
| `0x000A` | 5 | ASCII `V2.20` |
| `0x0010` | 6 | ASCII `TK2402` |
| `0x0020` | 1 | Opaque profile byte `0x44`; meaning not established |
| `0x0040` | 1 | XOR seed |
| `0x0041` | 8192 | Encoded image |

Total size: 8257 bytes. Remaining header bytes are preserved. The image XOR mask is `file[0x40] ^ 0xFF`. Image offsets in this document are relative to the decoded 8192-byte payload, so `file_offset = 0x41 + image_offset`.

Direct radio imports use the known container header and encode every image byte from the received payload. They do not require or copy channel data from a template file.

## Channel records

Sixteen 32-byte records begin at image offset `0x12C0`. An all-`0xFF` record is empty.

| Record-relative offset | Size | Interpretation |
| --- | ---: | --- |
| `+0` | 1 | Channel number |
| `+2` | 4 | RX frequency, packed little-endian BCD, 10 Hz units |
| `+6` | 4 | TX frequency, same representation |
| `+10`, `+11` | 1 each | RX/TX signaling mode; QT mode observed as `0x02` |
| `+12` | 2 | QT Decode, little-endian tenths of Hz |
| `+14` | 2 | QT Encode, same representation |
| `+17`, bit 0 | 1 bit | Narrow=0, Wide=1 |
| `+17`, bit 4 | 1 bit | Scan Add Yes=0, No=1 |
| `+17`, bit 5 | 1 bit | Low=0, High=1 |

Scan Add CH1 also updates bit 0 at image offset `0x1000`, with the same inverted Yes/No polarity. Its record/table coherence is checked before accepting the edit. Mapping other positions remains incomplete.

## Serialization contract

The writer validates channel identity/order, active frequencies, exact 10 Hz precision and supported field changes. It preserves all other bytes and reparses the result to verify the typed configuration. Unsupported changes return an error rather than producing a partial interpretation.

None/DQT, optional signaling, channel creation/deletion and additional global settings do not have a complete supported binary mapping. JSON/CSV can represent typed values without implying DAT support.

SHA-256 identifies backup contents. It is not presented as a device checksum. The transport's Y-frame byte sum is a separate protocol integrity check, described in [the device protocol](protocol-tk2402.md).

