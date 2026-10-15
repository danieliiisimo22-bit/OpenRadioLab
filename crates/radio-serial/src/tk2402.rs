// SPDX-License-Identifier: GPL-3.0-only
//! TK-2402 configuration transport reconstructed from KPG-158D V2.20.
//! Protected configuration writes; no tuning, firmware or transmission commands.
use crate::SerialError;
use serde::Serialize;
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::{
    io::{Read, Write},
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct RadioSession {
    port: Box<dyn SerialPort>,
    mask: u8,
    pub identity: Vec<u8>,
    pub trace: Vec<String>,
    finished: bool,
    last_image: Option<Vec<u8>>,
}

pub const IMAGE_SIZE: usize = 8192;
pub const BLOCK_SIZE: usize = 128;
pub const WRITE_BLOCK_SIZE: usize = 64;

#[derive(Debug, Clone, Serialize)]
pub struct WritePlan {
    before: Vec<u8>,
    after: Vec<u8>,
    blocks: Vec<u16>,
    pub changed_bytes: usize,
}

impl WritePlan {
    pub fn blocks(&self) -> &[u16] {
        &self.blocks
    }
    pub fn target(&self) -> &[u8] {
        &self.after
    }
}

/// Only offsets mapped and round-tripped through KPG may change.
/// Entire 64-byte frames retain neighboring live bytes unchanged.
pub fn plan_write(before: &[u8], after: &[u8]) -> Result<WritePlan, SerialError> {
    if before.len() != IMAGE_SIZE || after.len() != IMAGE_SIZE {
        return Err(SerialError::Protocol(
            "Plan de escritura: se requieren dos imágenes completas".into(),
        ));
    }
    if before[0x70..0x78] != [b'P', b'2', b'4', b'0', b'2', 0, 255, 255]
        || before[0x70..0x78] != after[0x70..0x78]
    {
        return Err(SerialError::Protocol(
            "Plan de escritura: identidad incompatible".into(),
        ));
    }
    let mut blocks = Vec::new();
    let mut changed_bytes = 0;
    for (offset, (&a, &b)) in before.iter().zip(after).enumerate() {
        if a == b {
            continue;
        }
        let allowed = if offset == 0x1000 {
            a ^ b == 1
        } else if (0x12C0..0x14C0).contains(&offset) {
            let field = (offset - 0x12C0) % 32;
            let flag_mask = if offset < 0x12E0 { 0x31 } else { 0x21 };
            matches!(field,2..=9|12..=15) || (field == 17 && (a ^ b) & !flag_mask == 0)
        } else {
            false
        };
        if !allowed {
            return Err(SerialError::Protocol(format!(
                "Cambio fuera de campos admitidos: 0x{offset:04X}"
            )));
        }
        let address = (offset / WRITE_BLOCK_SIZE * WRITE_BLOCK_SIZE) as u16;
        if blocks.last() != Some(&address) {
            blocks.push(address);
        }
        changed_bytes += 1;
    }
    Ok(WritePlan {
        before: before.to_vec(),
        after: after.to_vec(),
        blocks,
        changed_bytes,
    })
}

fn exchange_write<T: Read + Write + ?Sized>(
    port: &mut T,
    mask: u8,
    address: u16,
    data: &[u8],
    trace: &mut Vec<String>,
) -> Result<(), SerialError> {
    if data.len() != WRITE_BLOCK_SIZE || usize::from(address) % WRITE_BLOCK_SIZE != 0 {
        return Err(SerialError::Protocol("Bloque de escritura inválido".into()));
    }
    let mut frame = vec![
        b'Y',
        (address >> 8) as u8,
        address as u8,
        WRITE_BLOCK_SIZE as u8,
    ];
    frame.extend_from_slice(data);
    let checksum = data.iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
    trace.push(format!(
        "TX Y 0x{address:04X}, 64 bytes, suma {checksum:02X}"
    ));
    port.write_all(&frame.iter().map(|b| b ^ mask).collect::<Vec<_>>())?;
    port.flush()?;
    port.write_all(&[checksum ^ mask])?;
    port.flush()?;
    let mut ack = [0];
    port.read_exact(&mut ack).map_err(|e| {
        SerialError::Protocol(format!("Escritura 0x{address:04X}, ACK no confirmado: {e}"))
    })?;
    if ack[0] ^ mask != 6 {
        return Err(SerialError::Protocol(format!(
            "Escritura 0x{address:04X}, ACK inesperado: {:02X}",
            ack[0] ^ mask
        )));
    }
    trace.push(format!("Escritura 0x{address:04X} / ACK 06 confirmada"));
    Ok(())
}

/// Validate the 40-byte identity against the supported TK-2402 family profile.
pub fn validate_identity(identity: &[u8]) -> Result<(), SerialError> {
    if identity.len() != 40 || identity[..8] != [b'P', b'2', b'4', b'0', b'2', 0, 255, 255] {
        return Err(SerialError::Protocol(
            "Identidad distinta del perfil TK-2402 admitido; lectura detenida".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ReadDiagnostic {
    pub port: String,
    pub identity_hex: String,
    pub events: Vec<String>,
    pub received_bytes: usize,
    pub exit_confirmed: bool,
    pub error: Option<String>,
}

pub struct ReadCapture {
    pub identity: Vec<u8>,
    pub image: Option<Vec<u8>>,
    pub diagnostic: ReadDiagnostic,
}

/// Always returns a diagnostic, including when opening the OS port fails.
/// Attempts E exactly once after a completed identification, even on read errors.
pub fn capture(port: &str, mut progress: impl FnMut(usize)) -> ReadCapture {
    let mut diagnostic = ReadDiagnostic {
        port: port.into(),
        identity_hex: String::new(),
        events: vec![],
        received_bytes: 0,
        exit_confirmed: false,
        error: None,
    };
    let mut session = match RadioSession::open(port) {
        Ok(session) => session,
        Err(error) => {
            diagnostic.error = Some(error.to_string());
            return ReadCapture {
                identity: vec![],
                image: None,
                diagnostic,
            };
        }
    };
    let identity = session.identity.clone();
    diagnostic.identity_hex = hex(&identity);
    let result = session.read_image(|count| {
        diagnostic.received_bytes = count;
        progress(count);
    });
    let exit = session.finish();
    diagnostic.exit_confirmed = exit.is_ok();
    diagnostic.error = match (&result, &exit) {
        (Err(read), Err(exit)) => Some(format!("{read}. Salida no confirmada: {exit}")),
        (Err(read), _) => Some(read.to_string()),
        (_, Err(exit)) => Some(format!("Salida no confirmada: {exit}")),
        _ => None,
    };
    diagnostic.events = session.trace;
    ReadCapture {
        identity,
        image: result.ok(),
        diagnostic,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn exchange_block<T: Read + Write + ?Sized>(
    port: &mut T,
    mask: u8,
    address: u16,
    trace: &mut Vec<String>,
) -> Result<[u8; BLOCK_SIZE], SerialError> {
    let command = [b'R', (address >> 8) as u8, address as u8, BLOCK_SIZE as u8];
    let encoded = command.map(|b| b ^ mask);
    trace.push(format!("TX {}", hex(&command)));
    port.write_all(&encoded)?;
    port.flush()?;
    let mut header = [0; 4];
    port.read_exact(&mut header)
        .map_err(|e| SerialError::Protocol(format!("Bloque 0x{address:04X}, cabecera: {e}")))?;
    for b in &mut header {
        *b ^= mask;
    }
    trace.push(format!("RX {}", hex(&header)));
    if header != [b'W', command[1], command[2], command[3]] {
        return Err(SerialError::Protocol(format!(
            "Bloque 0x{address:04X}, respuesta no admitida: {}. No se completa con datos inventados",
            hex(&header)
        )));
    }
    let mut data = [0; BLOCK_SIZE];
    port.read_exact(&mut data)
        .map_err(|e| SerialError::Protocol(format!("Bloque 0x{address:04X}, datos: {e}")))?;
    for b in &mut data {
        *b ^= mask;
    }
    trace.push(format!("RX datos {}", hex(&data)));
    port.write_all(&[6 ^ mask])?;
    port.flush()?;
    let mut ack = [0];
    port.read_exact(&mut ack)
        .map_err(|e| SerialError::Protocol(format!("Bloque 0x{address:04X}, ACK: {e}")))?;
    if ack[0] ^ mask != 6 {
        return Err(SerialError::Protocol(format!(
            "Bloque 0x{address:04X}, ACK inesperado: {:02X}",
            ack[0] ^ mask
        )));
    }
    trace.push(format!("Bloque 0x{address:04X} confirmado"));
    Ok(data)
}

impl RadioSession {
    /// Enters programming mode, reads identification, exchanges the session key.
    /// This operation is physical and must only be called by an explicit action.
    pub fn open(port_name: &str) -> Result<Self, SerialError> {
        let mut port = serialport::new(port_name, 9600)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::Two)
            .flow_control(FlowControl::None)
            .timeout(Duration::from_millis(1000))
            .open()?;
        port.write_data_terminal_ready(true)?;
        port.write_request_to_send(false)?;
        port.clear(serialport::ClearBuffer::All)?;
        sleep(Duration::from_millis(100));
        port.write_all(b"PROGRAM")?;
        port.flush()?;
        sleep(Duration::from_millis(25));
        let mut response = [0];
        port.read_exact(&mut response)
            .map_err(|e| SerialError::Protocol(format!("Entrada PROGRAM: {e}")))?;
        port.set_baud_rate(19200)?;
        sleep(Duration::from_millis(250));
        port.read_exact(&mut response)
            .map_err(|e| SerialError::Protocol(format!("Cambio a 19200: {e}")))?;
        port.write_all(&[2])?;
        let mut identity = vec![0; 40];
        port.read_exact(&mut identity)
            .map_err(|e| SerialError::Protocol(format!("Identificación de 40 bytes: {e}")))?;
        // KPG sends a time-derived byte and then ACK; it does not wait between them.
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // KPG uses minute + second; the exchanged byte determines the XOR key.
        let seed = ((seconds / 60) % 60 + seconds % 60) as u8;
        let sum = identity[24..36]
            .iter()
            .fold(0u8, |sum, value| sum.wrapping_add(*value));
        port.write_all(&[seed])?;
        port.write_all(&[6])?;
        port.read_exact(&mut response)?;
        if response != [6] {
            return Err(SerialError::Protocol(format!(
                "ACK de identificación inesperado: {:02X}",
                response[0]
            )));
        }
        Ok(Self {
            port,
            mask: seed ^ sum,
            identity,
            trace: vec![format!(
                "PROGRAM 9600 8N2 → 19200; identificación y clave confirmadas; máscara {mask:02X}",
                mask = seed ^ sum
            )],
            finished: false,
            last_image: None,
        })
    }
    fn send(&mut self, bytes: &[u8]) -> Result<(), SerialError> {
        let encoded = bytes.iter().map(|b| b ^ self.mask).collect::<Vec<_>>();
        self.port.write_all(&encoded)?;
        self.port.flush()?;
        Ok(())
    }
    fn receive(&mut self, bytes: &mut [u8]) -> Result<(), SerialError> {
        self.port.read_exact(bytes)?;
        for byte in bytes {
            *byte ^= self.mask;
        }
        Ok(())
    }
    /// Ends PC mode. No memory data is sent to the radio.
    pub fn finish(&mut self) -> Result<(), SerialError> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        self.port.clear(serialport::ClearBuffer::Input)?;
        self.send(b"E")?;
        let mut ack = [0];
        self.receive(&mut ack)?;
        if ack != [6] {
            return Err(SerialError::Protocol("ACK de salida inesperado".into()));
        }
        self.trace.push("Salida E / ACK 06 confirmada".into());
        Ok(())
    }

    /// Reads only 0x0000..0x1FFF, using the R command. Never sends memory payloads.
    pub fn read_image(&mut self, mut progress: impl FnMut(usize)) -> Result<Vec<u8>, SerialError> {
        if self.finished {
            return Err(SerialError::Protocol("Sesión finalizada".into()));
        }
        validate_identity(&self.identity)?;
        let mut image = Vec::with_capacity(IMAGE_SIZE);
        for address in (0..IMAGE_SIZE).step_by(BLOCK_SIZE) {
            let block =
                exchange_block(&mut *self.port, self.mask, address as u16, &mut self.trace)?;
            image.extend_from_slice(&block);
            progress(image.len());
        }
        if image[0x70..0x78] != self.identity[..8] {
            return Err(SerialError::Protocol(
                "La identidad de la imagen no coincide con la identificación inicial".into(),
            ));
        }
        self.last_image = Some(image.clone());
        Ok(image)
    }

    /// Requires a fresh full read on this same session and an immutable validated plan.
    pub fn apply_plan(
        &mut self,
        plan: &WritePlan,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<(), SerialError> {
        if self.finished || self.last_image.as_deref() != Some(plan.before.as_slice()) {
            return Err(SerialError::Protocol(
                "Escritura sin una lectura previa coincidente en esta sesión".into(),
            ));
        }
        validate_identity(&self.identity)?;
        if self.identity[20] != b'K' {
            return Err(SerialError::Protocol(
                "Escritura limitada al tipo K admitido".into(),
            ));
        }
        self.last_image = None;
        for (i, &address) in plan.blocks.iter().enumerate() {
            let start = usize::from(address);
            exchange_write(
                &mut *self.port,
                self.mask,
                address,
                &plan.after[start..start + WRITE_BLOCK_SIZE],
                &mut self.trace,
            )?;
            progress(i + 1, plan.blocks.len());
        }
        self.last_image = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    struct Transcript {
        rx: Cursor<Vec<u8>>,
        tx: Vec<u8>,
    }
    impl Read for Transcript {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            self.rx.read(b)
        }
    }
    impl Write for Transcript {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.tx.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn response(header: [u8; 4], ack: u8, mask: u8) -> Transcript {
        let mut rx = header.to_vec();
        rx.extend((0..128).map(|v| v as u8));
        rx.push(ack);
        Transcript {
            rx: Cursor::new(rx.iter().map(|b| b ^ mask).collect()),
            tx: vec![],
        }
    }
    #[test]
    fn block_exchange_checks_address_length_mask_and_ack() {
        let mut wire = response([b'W', 0x12, 0x80, 0x80], 6, 0xA7);
        let block = exchange_block(&mut wire, 0xA7, 0x1280, &mut vec![]).unwrap();
        assert_eq!(block[127], 127);
        assert_eq!(wire.tx, [b'R', 0x12, 0x80, 0x80, 6].map(|b| b ^ 0xA7));
    }
    #[test]
    fn invalid_header_stops_before_ack() {
        for h in [[b'W', 0, 1, 128], [b'W', 0, 0, 127], [b'Z', 0, 0, 128]] {
            let mut wire = response(h, 6, 0);
            assert!(exchange_block(&mut wire, 0, 0, &mut vec![]).is_err());
            assert_eq!(wire.tx, vec![b'R', 0, 0, 128]);
        }
    }
    #[test]
    fn missing_bytes_and_bad_ack_fail() {
        let mut wire = response([b'W', 0, 0, 128], 0x15, 0);
        assert!(exchange_block(&mut wire, 0, 0, &mut vec![]).is_err());
        wire.rx = Cursor::new(vec![b'W', 0, 0, 128, 1]);
        wire.tx.clear();
        assert!(exchange_block(&mut wire, 0, 0, &mut vec![]).is_err());
        assert_eq!(wire.tx.len(), 4);
    }
    #[test]
    fn other_models_are_rejected() {
        let mut identity = vec![255; 40];
        identity[..6].copy_from_slice(b"P2402\0");
        assert!(validate_identity(&identity).is_ok());
        identity[1] = b'3';
        assert!(validate_identity(&identity).is_err());
        assert!(validate_identity(&[]).is_err());
    }
    #[test]
    fn write_frame_uses_y_payload_checksum_and_xor() {
        let mut wire = Transcript {
            rx: Cursor::new(vec![6 ^ 0xA7]),
            tx: vec![],
        };
        let data = (0..64).map(|v| v as u8).collect::<Vec<_>>();
        exchange_write(&mut wire, 0xA7, 0x12C0, &data, &mut vec![]).unwrap();
        let mut expected = vec![b'Y', 0x12, 0xC0, 64];
        expected.extend(&data);
        expected.push(0xE0); // sum(0..63) = 2016, modulo 256 = 224.
        assert_eq!(
            wire.tx,
            expected.iter().map(|b| b ^ 0xA7).collect::<Vec<_>>()
        );
    }
    #[test]
    fn write_plan_preserves_neighbors_and_rejects_unmapped_regions() {
        let mut before = vec![255; IMAGE_SIZE];
        before[0x70..0x76].copy_from_slice(b"P2402\0");
        let mut after = before.clone();
        after[0x1000] ^= 1;
        after[0x12D1] ^= 0x10;
        let plan = plan_write(&before, &after).unwrap();
        assert_eq!(plan.blocks(), [0x1000, 0x12C0]);
        assert_eq!(plan.changed_bytes, 2);
        for offset in [
            0, 0x10, 0x60, 0x70, 0x800, 0x1001, 0x12CA, 0x12CB, 0x12D0, 0x14C0, 0x1FFF,
        ] {
            let mut bad = after.clone();
            bad[offset] ^= 1;
            assert!(plan_write(&before, &bad).is_err(), "{offset:04X}");
        }
        after[0x12D1] ^= 0x80;
        assert!(plan_write(&before, &after).is_err());
    }
    #[test]
    fn rejected_write_ack_is_not_retried() {
        let mut wire = Transcript {
            rx: Cursor::new(vec![0x15]),
            tx: vec![],
        };
        assert!(exchange_write(&mut wire, 0, 0x1000, &[0; 64], &mut vec![]).is_err());
        assert_eq!(wire.tx.len(), 69);
    }
}
