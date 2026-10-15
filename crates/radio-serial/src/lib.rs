// SPDX-License-Identifier: GPL-3.0-only
//! Port discovery, explicit TK-2402 reads and protected configuration writes.
pub mod tk2402;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortDescriptor {
    pub name: String,
    pub adapter: String,
    pub usb_vid: Option<u16>,
    pub usb_pid: Option<u16>,
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Parity {
    None,
    Odd,
    Even,
}

/// Explicit user-supplied settings; deliberately no default baud or framing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SerialSettings {
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: Parity,
    pub timeout_ms: u64,
}

#[derive(Debug, Error)]
pub enum SerialError {
    #[error("Error de transporte: {0}")]
    Io(#[from] std::io::Error),
    #[error("Protocolo: {0}")]
    Protocol(String),
    #[error("Error del puerto serie: {0}")]
    Discovery(#[from] serialport::Error),
    #[error("Parámetros seriales inválidos: {0}")]
    Invalid(&'static str),
}

impl SerialSettings {
    pub fn validate(&self) -> Result<(), SerialError> {
        if self.baud_rate == 0 {
            return Err(SerialError::Invalid("baud debe ser positivo"));
        }
        if !(5..=8).contains(&self.data_bits) {
            return Err(SerialError::Invalid("bits de datos: 5 a 8"));
        }
        if !matches!(self.stop_bits, 1 | 2) {
            return Err(SerialError::Invalid("bits de parada: 1 o 2"));
        }
        if self.timeout_ms == 0 || self.timeout_ms > 60_000 {
            return Err(SerialError::Invalid("timeout: 1 a 60000 ms"));
        }
        Ok(())
    }
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

pub fn discover_ports() -> Result<Vec<PortDescriptor>, SerialError> {
    let mut ports = serialport::available_ports()?
        .into_iter()
        .map(|p| {
            let mut result = PortDescriptor {
                name: p.port_name,
                adapter: String::new(),
                usb_vid: None,
                usb_pid: None,
                serial_number: None,
            };
            match p.port_type {
                serialport::SerialPortType::UsbPort(u) => {
                    result.adapter = format!(
                        "{} {}",
                        u.manufacturer.unwrap_or_default(),
                        u.product.unwrap_or_default()
                    )
                    .trim()
                    .into();
                    result.usb_vid = Some(u.vid);
                    result.usb_pid = Some(u.pid);
                    result.serial_number = u.serial_number;
                }
                serialport::SerialPortType::BluetoothPort => result.adapter = "Bluetooth".into(),
                serialport::SerialPortType::PciPort => result.adapter = "PCI".into(),
                serialport::SerialPortType::Unknown => {
                    result.adapter = "Tipo no identificado".into()
                }
            }
            result
        })
        .collect::<Vec<_>>();
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    tracing::info!(count = ports.len(), "Puertos enumerados; ninguno abierto");
    Ok(ports)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_require_explicit_valid_framing() {
        let mut s = SerialSettings {
            baud_rate: 1,
            data_bits: 8,
            stop_bits: 1,
            parity: Parity::None,
            timeout_ms: 500,
        };
        assert!(s.validate().is_ok());
        s.baud_rate = 0;
        assert!(s.validate().is_err());
        s.baud_rate = 1;
        s.data_bits = 9;
        assert!(s.validate().is_err());
    }
}
