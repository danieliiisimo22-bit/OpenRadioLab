// SPDX-License-Identifier: GPL-3.0-only
//! Conservative KPG-158D V2.20 container reader. Unknown bytes stay in the document.
use radio_model::{
    Channel, ChannelBandwidth, CtcssTone, Frequency, PowerLevel, RadioConfiguration, RadioModel,
    ScanSettings, SquelchSignaling,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const FILE_SIZE: usize = 8257;
pub const HEADER_SIZE: usize = 64;
pub const IMAGE_START: usize = 65;
pub const IMAGE_SIZE: usize = 8192;
pub const CHANNEL_BASE: usize = 0x12c0;
pub const CHANNEL_STRIDE: usize = 32;

#[derive(Debug, Error)]
pub enum DatError {
    #[error("Tamaño DAT no admitido: {actual} bytes; perfil V2.20: 8257")]
    Size { actual: usize },
    #[error("Cabecera incompatible: {0}")]
    Header(&'static str),
    #[error("Registro {channel}, offset 0x{offset:04X}: {reason}")]
    Record {
        channel: usize,
        offset: usize,
        reason: String,
    },
    #[error("Cambio DAT no admitido: {0}")]
    UnsupportedEdit(String),
    #[error("Se impide sobrescribir el DAT original: {0}")]
    Original(PathBuf),
    #[error("Ya existe el destino; elija un nombre nuevo: {0}")]
    Exists(PathBuf),
    #[error("Error de archivo: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuración inválida: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub range: String,
    pub interpretation: String,
    pub confidence: String,
}

#[derive(Debug, Clone)]
pub struct DatDocument {
    original: Vec<u8>,
    image: Vec<u8>,
    configuration: RadioConfiguration,
    source: Option<PathBuf>,
    pub warnings: Vec<String>,
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn bcd_frequency(
    bytes: &[u8],
    number: usize,
    offset: usize,
) -> Result<Option<Frequency>, DatError> {
    if bytes.iter().all(|&b| b == 0xff) {
        return Ok(None);
    }
    let mut value = 0u32;
    for &b in bytes.iter().rev() {
        if (b >> 4) > 9 || (b & 15) > 9 {
            return Err(DatError::Record {
                channel: number,
                offset,
                reason: "frecuencia BCD inválida".into(),
            });
        }
        value = value * 100 + u32::from(b >> 4) * 10 + u32::from(b & 15);
    }
    Frequency::from_hz(value * 10)
        .map(Some)
        .map_err(|e| DatError::Record {
            channel: number,
            offset,
            reason: e.to_string(),
        })
}

fn signaling(mode: u8, bytes: &[u8]) -> SquelchSignaling {
    let raw = u16::from_le_bytes([bytes[0], bytes[1]]);
    // Only mode 0x02 and QT words occur in the supplied samples.
    if mode == 2
        && let Ok(tone) = CtcssTone::from_tenths(raw)
    {
        return SquelchSignaling::Ctcss(tone);
    }
    SquelchSignaling::Unknown { raw }
}

fn write_candidate_frequency(
    image: &mut [u8],
    offset: usize,
    before: Option<Frequency>,
    after: Option<Frequency>,
) -> Result<(), DatError> {
    if before == after {
        return Ok(());
    }
    let (Some(_), Some(frequency)) = (before, after) else {
        return Err(DatError::UnsupportedEdit(
            "activar/desactivar RX o TX requiere una muestra controlada".into(),
        ));
    };
    if !frequency.hz().is_multiple_of(10) {
        return Err(DatError::UnsupportedEdit(
            "la frecuencia DAT debe ser múltiplo exacto de 10 Hz".into(),
        ));
    }
    let mut units = frequency.hz() / 10;
    for byte in &mut image[offset..offset + 4] {
        let digits = units % 100;
        *byte = ((digits / 10) as u8) << 4 | (digits % 10) as u8;
        units /= 100;
    }
    Ok(())
}

fn write_candidate_qt(
    image: &mut [u8],
    offset: usize,
    before: &SquelchSignaling,
    after: &SquelchSignaling,
) -> Result<(), DatError> {
    if before == after {
        return Ok(());
    }
    let (SquelchSignaling::Ctcss(_), SquelchSignaling::Ctcss(tone)) = (before, after) else {
        return Err(DatError::UnsupportedEdit(
            "solo se admiten cambios QT a QT; Off, DQT y RAW no tienen codificación confirmada"
                .into(),
        ));
    };
    image[offset..offset + 2].copy_from_slice(&tone.tenths().to_le_bytes());
    Ok(())
}

impl DatDocument {
    /// Container profile observed in KPG-158D V2.20 for the tested TK-2402K.
    /// No channel or EEPROM data is stored in this header.
    pub fn from_radio_image(image: &[u8]) -> Result<Self, DatError> {
        if image.len() != IMAGE_SIZE
            || image[0x70..0x78] != [b'P', b'2', b'4', b'0', b'2', 0, 255, 255]
        {
            return Err(DatError::Validation(
                "Imagen incompleta o distinta del perfil TK-2402 admitido".into(),
            ));
        }
        let mut bytes = vec![255; IMAGE_START];
        bytes[..7].copy_from_slice(b"KPG158D");
        bytes[10..15].copy_from_slice(b"V2.20");
        bytes[16..22].copy_from_slice(b"TK2402");
        bytes[0x20] = 0x44;
        bytes[HEADER_SIZE] = 0xB3;
        bytes.extend(image.iter().map(|b| b ^ (0xB3 ^ 255)));
        Self::parse(bytes)
    }
    /// Wrap a complete radio image using an existing, compatible KPG container.
    /// The template contributes only its header and seed, never EEPROM bytes.
    pub fn with_radio_image(&self, image: &[u8]) -> Result<Self, DatError> {
        if image.len() != IMAGE_SIZE {
            return Err(DatError::Validation(
                "Lectura incompleta: se requieren 8192 bytes".into(),
            ));
        }
        if image[0x70..0x78] != self.image[0x70..0x78] {
            return Err(DatError::Validation(
                "El tipo de radio no coincide con el DAT abierto".into(),
            ));
        }
        let mut bytes = self.original[..IMAGE_START].to_vec();
        bytes.extend(image.iter().map(|b| b ^ self.mask()));
        Self::parse(bytes)
    }
    pub fn parse(bytes: Vec<u8>) -> Result<Self, DatError> {
        if bytes.len() != FILE_SIZE {
            return Err(DatError::Size {
                actual: bytes.len(),
            });
        }
        if &bytes[..7] != b"KPG158D" {
            return Err(DatError::Header("identificador KPG158D ausente"));
        }
        if &bytes[10..15] != b"V2.20" {
            return Err(DatError::Header("versión diferente de V2.20"));
        }
        if &bytes[16..22] != b"TK2402" {
            return Err(DatError::Header("modelo diferente de TK2402"));
        }
        let mask = bytes[HEADER_SIZE] ^ 0xff;
        let image = bytes[IMAGE_START..]
            .iter()
            .map(|b| b ^ mask)
            .collect::<Vec<_>>();
        let mut channels = Vec::new();
        for index in 0..16 {
            let base = CHANNEL_BASE + index * CHANNEL_STRIDE;
            let record = &image[base..base + CHANNEL_STRIDE];
            if record.iter().all(|&v| v == 0xff) {
                continue;
            }
            let n = index + 1;
            if usize::from(record[0]) != n {
                return Err(DatError::Record {
                    channel: n,
                    offset: IMAGE_START + base,
                    reason: format!(
                        "identificador de canal inesperado {:02X}; estructura no compatible",
                        record[0]
                    ),
                });
            }
            channels.push(Channel {
                number: n as u8,
                rx: bcd_frequency(&record[2..6], n, IMAGE_START + base + 2)?,
                tx: bcd_frequency(&record[6..10], n, IMAGE_START + base + 6)?,
                decode: signaling(record[10], &record[12..14]),
                encode: signaling(record[11], &record[14..16]),
                power: Some(if record[17] & 0x20 == 0 {
                    PowerLevel::Low
                } else {
                    PowerLevel::High
                }),
                bandwidth: Some(if record[17] & 1 == 0 {
                    ChannelBandwidth::Narrow
                } else {
                    ChannelBandwidth::Wide
                }),
                scan_add: Some(record[17] & 0x10 == 0),
                optional_signaling: None,
            });
        }
        let configuration = RadioConfiguration {
            schema_version: 1,
            model: RadioModel::Tk2402,
            channels,
            scan: ScanSettings::default(),
        };
        configuration
            .validate()
            .map_err(|e| DatError::Validation(e.to_string()))?;
        Ok(Self {original:bytes,image,configuration,source:None,warnings:vec![
            "Contenedor V2.20 identificado. Cabecera, semilla XOR y campos no editados se conservan.".into(),
            "Los campos RX/TX activos, QT, potencia y ancho admiten edición de copia DAT.".into(),
            "Optional Signaling y DCS se conservan sin reinterpretar su codificación binaria.".into(),
            "Scan Add modifica el registro y la tabla global; solo se admite editar el canal 1.".into(),
            "La cabecera dice TK2402 y byte 0x20=0x44; no confirma sufijo K ni su autorización RF.".into(),
        ]})
    }
    pub fn open(path: &Path) -> Result<Self, DatError> {
        let mut doc = Self::parse(fs::read(path)?)?;
        doc.source = Some(fs::canonicalize(path)?);
        Ok(doc)
    }
    pub fn configuration(&self) -> &RadioConfiguration {
        &self.configuration
    }
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }
    pub fn decoded_image(&self) -> &[u8] {
        &self.image
    }
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }
    pub fn mask(&self) -> u8 {
        self.original[HEADER_SIZE] ^ 0xff
    }
    pub fn can_export_dat(&self, configuration: &RadioConfiguration) -> bool {
        self.serialize_lossless(configuration).is_ok()
    }
    pub fn serialize_lossless(
        &self,
        configuration: &RadioConfiguration,
    ) -> Result<Vec<u8>, DatError> {
        configuration
            .validate()
            .map_err(|e| DatError::Validation(e.to_string()))?;
        if &self.configuration == configuration {
            return Ok(self.original.clone());
        }
        self.serialize_candidate(configuration)
    }
    /// File-only serialization for fields verified by KPG reopening.
    ///
    /// This does not establish radio-write support.
    /// The source header, XOR seed and every unsupported byte are preserved.
    pub fn serialize_candidate(
        &self,
        configuration: &RadioConfiguration,
    ) -> Result<Vec<u8>, DatError> {
        configuration
            .validate()
            .map_err(|error| DatError::Validation(error.to_string()))?;
        if configuration.schema_version != self.configuration.schema_version
            || configuration.model != self.configuration.model
            || configuration.scan != self.configuration.scan
        {
            return Err(DatError::UnsupportedEdit(
                "modelo, versión y ajustes globales deben permanecer iguales".into(),
            ));
        }
        if configuration.channels.len() != self.configuration.channels.len() {
            return Err(DatError::UnsupportedEdit(
                "agregar o eliminar canales requiere una muestra controlada".into(),
            ));
        }
        let mut image = self.image.clone();
        for (before, after) in self
            .configuration
            .channels
            .iter()
            .zip(&configuration.channels)
        {
            if before.number != after.number {
                return Err(DatError::UnsupportedEdit(
                    "los números y el orden de los canales deben permanecer iguales".into(),
                ));
            }
            if before.optional_signaling != after.optional_signaling {
                return Err(DatError::UnsupportedEdit(
                    "Optional Signaling no tiene codificación confirmada".into(),
                ));
            }
            let base = CHANNEL_BASE + usize::from(before.number - 1) * CHANNEL_STRIDE;
            write_candidate_frequency(&mut image, base + 2, before.rx, after.rx)?;
            write_candidate_frequency(&mut image, base + 6, before.tx, after.tx)?;
            write_candidate_qt(&mut image, base + 12, &before.decode, &after.decode)?;
            write_candidate_qt(&mut image, base + 14, &before.encode, &after.encode)?;
            if before.power != after.power {
                let high = match after.power {
                    Some(PowerLevel::High) => true,
                    Some(PowerLevel::Low) => false,
                    None => return Err(DatError::UnsupportedEdit("potencia desconocida".into())),
                };
                image[base + 17] = (image[base + 17] & !0x20) | (u8::from(high) << 5);
            }
            if before.bandwidth != after.bandwidth {
                let wide = match after.bandwidth {
                    Some(ChannelBandwidth::Wide) => true,
                    Some(ChannelBandwidth::Narrow) => false,
                    None => return Err(DatError::UnsupportedEdit("ancho desconocido".into())),
                };
                image[base + 17] = (image[base + 17] & !1) | u8::from(wide);
            }
            if before.scan_add != after.scan_add {
                if before.number != 1 {
                    return Err(DatError::UnsupportedEdit(
                        "Scan Add solo tiene tabla global corroborada para el canal 1".into(),
                    ));
                }
                let (Some(old_add), Some(new_add)) = (before.scan_add, after.scan_add) else {
                    return Err(DatError::UnsupportedEdit("Scan Add desconocido".into()));
                };
                if (self.image[0x1000] & 1 == 0) != old_add {
                    return Err(DatError::UnsupportedEdit(
                        "Scan Add del canal 1 contradice la tabla global original".into(),
                    ));
                }
                image[0x1000] = (image[0x1000] & !1) | u8::from(!new_add);
                image[base + 17] = (image[base + 17] & !0x10) | (u8::from(!new_add) << 4);
            }
        }
        let mut bytes = self.original.clone();
        for (encoded, decoded) in bytes[IMAGE_START..].iter_mut().zip(image) {
            *encoded = decoded ^ self.mask();
        }
        let reparsed = Self::parse(bytes.clone())?;
        if reparsed.configuration() != configuration {
            return Err(DatError::Validation(
                "el candidato DAT no reproduce exactamente la configuración solicitada".into(),
            ));
        }
        Ok(bytes)
    }
    pub fn backup_original(&self, directory: &Path) -> Result<PathBuf, DatError> {
        fs::create_dir_all(directory)?;
        let name = format!("original-{}.dat", sha256(&self.original));
        let dest = directory.join(name);
        if dest.exists() {
            if fs::read(&dest)? != self.original {
                return Err(DatError::Validation(
                    "respaldo existente no coincide con su hash".into(),
                ));
            }
        } else {
            atomic_write_new(&dest, &self.original, self.source())?;
        }
        Ok(dest)
    }
    pub fn save_copy(
        &self,
        path: &Path,
        configuration: &RadioConfiguration,
        backup_dir: &Path,
    ) -> Result<PathBuf, DatError> {
        let bytes = self.serialize_lossless(configuration)?;
        let backup = self.backup_original(backup_dir)?;
        atomic_write_new(path, &bytes, self.source())?;
        if fs::read(path)? != bytes {
            return Err(DatError::Validation("verificación de copia falló".into()));
        }
        tracing::info!(path=%path.display(),hash=%sha256(&bytes),"Copia DAT íntegra guardada");
        Ok(backup)
    }
    pub fn findings(&self) -> Vec<Finding> {
        [
            (
                "0x0000..0x003F",
                "Cabecera: KPG158D / V2.20 / TK2402; byte mercado 0x20 opaco",
                "observado",
            ),
            (
                "0x0040",
                "Semilla XOR; mask = byte XOR FF; conservada al editar",
                "reapertura KPG y guardado con otra semilla e imagen idéntica",
            ),
            (
                "0x0041..0x2040",
                "Imagen decodificable de 8192 bytes; no validada como memoria grabable",
                "derivado",
            ),
            (
                "0x1301..0x1500",
                "Hasta 16 posibles registros de 32 bytes; 6 ocupados en muestras",
                "estructura inferida",
            ),
            (
                "registro +2..+9",
                "RX/TX BCD little endian en unidades de 10 Hz",
                "inferencia",
            ),
            (
                "registro +12..+13",
                "QT Decode tenths Hz little endian, 948 y 1000",
                "confirmado por diferencia controlada",
            ),
            (
                "registro +14..+15",
                "QT Encode en décimas Hz little endian; reapertura KPG corroborada",
                "inferencia",
            ),
            (
                "registro +17 bit 0",
                "0 Narrow, 1 Wide; dirección corregida con captura certificada",
                "confirmado por nueva diferencia controlada",
            ),
            (
                "registro +17 bit 5",
                "0 Low, 1 High",
                "confirmado por diferencia controlada",
            ),
            (
                "registro +17 bit 4",
                "0 Scan Add Yes, 1 No",
                "confirmado por diferencia controlada",
            ),
            (
                "imagen 0x1000 / DAT 0x1041",
                "C0->C1 al excluir canal 1 de scan; tabla global candidata",
                "observado, semántica completa pendiente",
            ),
            (
                "resto del archivo",
                "Señalización opcional / DCS / tablas / calibración: desconocidos preservados",
                "sin determinar",
            ),
        ]
        .into_iter()
        .map(|(r, i, c)| Finding {
            range: r.into(),
            interpretation: i.into(),
            confidence: c.into(),
        })
        .collect()
    }
}

/// Publish with no-clobber semantics. Temp file lives on the destination volume.
pub fn atomic_write_new(
    path: &Path,
    bytes: &[u8],
    protected: Option<&Path>,
) -> Result<(), DatError> {
    if path.exists() {
        if let Some(original) = protected
            && fs::canonicalize(path)? == original
        {
            return Err(DatError::Original(path.into()));
        }
        return Err(DatError::Exists(path.into()));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(DatError::Validation(
            "el directorio de destino no existe".into(),
        ));
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .map_err(|e| DatError::Io(e.error))?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn save_recovery(
    directory: &Path,
    configuration: &RadioConfiguration,
) -> Result<PathBuf, DatError> {
    fs::create_dir_all(directory)?;
    let bytes =
        radio_model::export_json(configuration).map_err(|e| DatError::Validation(e.to_string()))?;
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| DatError::Validation(e.to_string()))?
        .as_nanos();
    let path = directory.join(format!("recovery-{tick}-{}.json", std::process::id()));
    atomic_write_new(&path, &bytes, None)?;
    Ok(path)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ByteDifference {
    pub offset: usize,
    pub before: Option<u8>,
    pub after: Option<u8>,
}
pub fn binary_diff(a: &[u8], b: &[u8]) -> Vec<ByteDifference> {
    (0..a.len().max(b.len()))
        .filter_map(|offset| {
            let before = a.get(offset).copied();
            let after = b.get(offset).copied();
            (before != after).then_some(ByteDifference {
                offset,
                before,
                after,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn synthetic() -> Vec<u8> {
        let mut b = vec![0xff; FILE_SIZE];
        b[..7].copy_from_slice(b"KPG158D");
        b[10..15].copy_from_slice(b"V2.20");
        b[16..22].copy_from_slice(b"TK2402");
        b[64] = 0xff;
        b
    }
    #[test]
    fn radio_image_replaces_all_payload_bytes_and_preserves_container() {
        let mut source = synthetic();
        source[64] = 0xB3;
        let mask = source[64] ^ 255;
        for b in &mut source[65..] {
            *b ^= mask;
        }
        let template = DatDocument::parse(source.clone()).unwrap();
        let mut image = template.decoded_image().to_vec();
        image[0x1800] = 0x42;
        let read = template.with_radio_image(&image).unwrap();
        assert_eq!(read.decoded_image(), image);
        assert_eq!(&read.original_bytes()[..65], &source[..65]);
        assert!(template.with_radio_image(&image[..8191]).is_err());
        image[0x70] = 0;
        assert!(template.with_radio_image(&image).is_err());
    }
    #[test]
    fn direct_radio_import_needs_no_dat_and_preserves_full_image() {
        let mut image = vec![255; IMAGE_SIZE];
        image[0x70..0x76].copy_from_slice(b"P2402\0");
        image[0x1800] = 0x42;
        let document = DatDocument::from_radio_image(&image).unwrap();
        assert_eq!(document.decoded_image(), image);
        assert_eq!(document.original_bytes().len(), FILE_SIZE);
        assert!(DatDocument::from_radio_image(&image[..8191]).is_err());
        image[0x71] = b'3';
        assert!(DatDocument::from_radio_image(&image).is_err());
    }
    #[test]
    fn truncated_and_unknown_headers_are_rejected() {
        for len in [0, 1, 64, 65, 8192, 8256, 8258] {
            assert!(matches!(
                DatDocument::parse(vec![0; len]),
                Err(DatError::Size { .. })
            ));
        }
        let mut b = synthetic();
        b[0] = 0;
        assert!(matches!(DatDocument::parse(b), Err(DatError::Header(_))));
        let mut b = synthetic();
        b[17] = b'X';
        assert!(DatDocument::parse(b).is_err());
        let mut b = synthetic();
        b[14] = b'1';
        assert!(DatDocument::parse(b).is_err());
    }
    #[test]
    fn opaque_bytes_are_preserved_and_edits_rejected() {
        let mut b = synthetic();
        b[8000] = 0x73;
        b[100] = 0x82;
        let d = DatDocument::parse(b.clone()).unwrap();
        assert_eq!(d.serialize_lossless(d.configuration()).unwrap(), b);
        let mut c = d.configuration().clone();
        c.scan.enabled = Some(true);
        assert!(matches!(
            d.serialize_lossless(&c),
            Err(DatError::UnsupportedEdit(_))
        ));
    }
    #[test]
    fn atomic_copy_never_clobbers() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("copy.dat");
        atomic_write_new(&p, b"one", None).unwrap();
        assert!(atomic_write_new(&p, b"two", None).is_err());
        assert_eq!(fs::read(p).unwrap(), b"one");
    }
    #[test]
    fn original_is_protected_and_backup_recoverable() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("source.dat");
        let bytes = synthetic();
        fs::write(&p, &bytes).unwrap();
        let d = DatDocument::open(&p).unwrap();
        let backups = dir.path().join("backups");
        assert!(matches!(
            d.save_copy(&p, d.configuration(), &backups),
            Err(DatError::Original(_))
        ));
        let copy = dir.path().join("copy.dat");
        let backup = d.save_copy(&copy, d.configuration(), &backups).unwrap();
        assert_eq!(fs::read(copy).unwrap(), bytes);
        assert_eq!(fs::read(backup).unwrap(), bytes);
        assert_eq!(fs::read(p).unwrap(), bytes);
    }
    #[test]
    fn diff_includes_trailing_bytes() {
        let d = binary_diff(&[1, 2], &[1, 3, 4]);
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].before, None);
        assert_eq!(d[1].after, Some(4));
    }
    #[test]
    fn wrong_record_and_bad_bcd_rejected() {
        let mut b = synthetic();
        b[IMAGE_START + CHANNEL_BASE] = 5;
        assert!(DatDocument::parse(b).is_err());
        let mut b = synthetic();
        let base = IMAGE_START + CHANNEL_BASE;
        b[base] = 1;
        b[base + 2] = 0xab;
        assert!(DatDocument::parse(b).is_err());
    }
    #[test]
    fn confirmed_flags_decode_independently_with_xor_and_preserve_bytes() {
        // Entirely synthetic profile: no personal DAT or RF configuration is embedded.
        for flags in [0xcc, 0xcd, 0xdc, 0xdd, 0xec, 0xed, 0xfc, 0xfd] {
            let mut bytes = synthetic();
            let base = IMAGE_START + CHANNEL_BASE;
            bytes[base] = 1;
            bytes[base + 2..base + 6].copy_from_slice(&[0x00, 0x00, 0x55, 0x14]);
            bytes[base + 6..base + 10].copy_from_slice(&[0x00, 0x00, 0x55, 0x14]);
            bytes[base + 10..base + 12].copy_from_slice(&[2, 2]);
            bytes[base + 12..base + 16].copy_from_slice(&[0xb4, 3, 0x9e, 2]);
            bytes[base + 17] = flags;
            let mask = 0x53;
            bytes[HEADER_SIZE] = mask ^ 0xff;
            for byte in &mut bytes[IMAGE_START..] {
                *byte ^= mask;
            }
            let document = DatDocument::parse(bytes.clone()).unwrap();
            let channel = &document.configuration().channels[0];
            assert_eq!(
                channel.power,
                Some(if flags & 0x20 == 0 {
                    PowerLevel::Low
                } else {
                    PowerLevel::High
                })
            );
            assert_eq!(
                channel.bandwidth,
                Some(if flags & 1 == 0 {
                    ChannelBandwidth::Narrow
                } else {
                    ChannelBandwidth::Wide
                })
            );
            assert_eq!(channel.scan_add, Some(flags & 0x10 == 0));
            assert_eq!(channel.optional_signaling, None);
            assert_eq!(
                document
                    .serialize_lossless(document.configuration())
                    .unwrap(),
                bytes
            );
            let mut edited = document.configuration().clone();
            edited.channels[0].scan_add = Some(!channel.scan_add.unwrap());
            if flags & 0x10 != 0 {
                let encoded = document.serialize_lossless(&edited).unwrap();
                let parsed = DatDocument::parse(encoded).unwrap();
                assert_eq!(parsed.configuration(), &edited);
                assert_eq!(parsed.decoded_image()[0x1000] & 1, 0);
            } else {
                assert!(matches!(
                    document.serialize_lossless(&edited),
                    Err(DatError::UnsupportedEdit(_))
                ));
            }
        }
    }
    fn populated_candidate_fixture() -> Vec<u8> {
        let mut bytes = synthetic();
        bytes[IMAGE_START + 0x1000] = 0xc0;
        for number in 1..=2 {
            let base = IMAGE_START + CHANNEL_BASE + (number - 1) * CHANNEL_STRIDE;
            bytes[base] = number as u8;
            bytes[base + 2..base + 6].copy_from_slice(&[0, 0, 0x55, 0x14]);
            bytes[base + 6..base + 10].copy_from_slice(&[0, 0, 0x55, 0x14]);
            bytes[base + 10..base + 12].copy_from_slice(&[2, 2]);
            bytes[base + 12..base + 16].copy_from_slice(&[0xb4, 3, 0x9e, 2]);
            bytes[base + 17] = 0xcc;
        }
        bytes[HEADER_SIZE] = 0xac;
        for byte in &mut bytes[IMAGE_START..] {
            *byte ^= 0x53;
        }
        bytes
    }
    #[test]
    fn verified_edits_preserve_opaque_bytes_and_export_exactly() {
        let document = DatDocument::parse(populated_candidate_fixture()).unwrap();
        let mut configuration = document.configuration().clone();
        let channel = &mut configuration.channels[0];
        channel.rx = Some(Frequency::from_hz(146_520_000).unwrap());
        channel.tx = Some(Frequency::from_hz(145_600_000).unwrap());
        channel.decode = SquelchSignaling::Ctcss(CtcssTone::from_tenths(1000).unwrap());
        channel.encode = SquelchSignaling::Ctcss(CtcssTone::from_tenths(1000).unwrap());
        channel.power = Some(PowerLevel::High);
        channel.bandwidth = Some(ChannelBandwidth::Wide);
        channel.scan_add = Some(false);
        let candidate = document.serialize_candidate(&configuration).unwrap();
        assert_eq!(
            &candidate[..IMAGE_START],
            &document.original_bytes()[..IMAGE_START]
        );
        let parsed = DatDocument::parse(candidate).unwrap();
        assert_eq!(parsed.configuration(), &configuration);
        let differences = binary_diff(document.decoded_image(), parsed.decoded_image());
        assert_eq!(
            differences
                .iter()
                .map(|difference| difference.offset)
                .collect::<Vec<_>>(),
            vec![
                0x1000,
                CHANNEL_BASE + 3,
                CHANNEL_BASE + 4,
                CHANNEL_BASE + 8,
                CHANNEL_BASE + 12,
                CHANNEL_BASE + 14,
                CHANNEL_BASE + 15,
                CHANNEL_BASE + 17
            ]
        );
        assert_eq!(parsed.decoded_image()[CHANNEL_BASE + 17], 0xfd);
        assert_eq!(parsed.decoded_image()[0x1000], 0xc1);
        assert!(document.can_export_dat(&configuration));
        assert_eq!(
            document.serialize_lossless(&configuration).unwrap(),
            parsed.original_bytes()
        );
        assert_eq!(
            document
                .serialize_candidate(document.configuration())
                .unwrap(),
            document.original_bytes()
        );
    }
    #[test]
    fn candidate_rejects_unsupported_semantics_and_incoherent_scan_table() {
        let document = DatDocument::parse(populated_candidate_fixture()).unwrap();
        for scenario in 0..10 {
            let mut configuration = document.configuration().clone();
            match scenario {
                0 => {
                    configuration.channels.pop();
                }
                1 => configuration.channels[0].decode = SquelchSignaling::Off,
                2 => configuration.channels[0].optional_signaling = Some("None".into()),
                3 => configuration.channels[0].rx = Some(Frequency::from_hz(145_500_001).unwrap()),
                4 => configuration.channels[0].tx = None,
                5 => configuration.channels[0].power = None,
                6 => configuration.channels[0].bandwidth = None,
                7 => configuration.channels[1].scan_add = Some(false),
                8 => configuration.scan.enabled = Some(true),
                9 => configuration.channels.swap(0, 1),
                _ => unreachable!(),
            }
            assert!(
                document.serialize_candidate(&configuration).is_err(),
                "scenario {scenario}"
            );
        }
        let mut incoherent = populated_candidate_fixture();
        incoherent[IMAGE_START + 0x1000] ^= 1;
        let document = DatDocument::parse(incoherent).unwrap();
        let mut configuration = document.configuration().clone();
        configuration.channels[0].scan_add = Some(false);
        assert!(matches!(
            document.serialize_candidate(&configuration),
            Err(DatError::UnsupportedEdit(_))
        ));
    }
}
