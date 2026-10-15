// SPDX-License-Identifier: GPL-3.0-only
//! Typed configuration and validated interchange formats.
//!
//! Frequencies serialize as integer Hz. Unknown binary signaling is retained as
//! `Unknown { raw }`; a typed DCS value does not establish its DAT encoding.
//! CSV has one `scan` metadata row and zero to sixteen `channel` rows, so global
//! settings and empty configurations survive interchange. Unknown values are
//! empty cells, except signaling which uses `RAW:XXXX` for a preserved word.
//! The optional signaling cell is a JSON string to distinguish `None` from an
//! explicitly empty string. CSV headers are listed in [`CSV_HEADERS`].

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt, str::FromStr};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;
pub const MIN_FREQUENCY_HZ: u32 = 136_000_000;
pub const MAX_FREQUENCY_HZ: u32 = 174_000_000;

/// Conventional 50-tone domain, in tenths of Hz. This is not a complete
/// capability catalogue for TK-2402 or a verified DAT encoding table.
/// Conventional CTCSS tone catalog; reference: CHIRP's `TONES`:
/// <https://github.com/kk7ds/chirp/blob/master/chirp/chirp_common.py#L27-L37>.
pub const CTCSS_TONES_TENTHS: [u16; 50] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148, 1188,
    1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738, 1773, 1799,
    1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336, 2418, 2503, 2541,
];

/// Conventional 104-code domain; values use Rust's octal representation.
/// Conventional DCS code catalog; reference: CHIRP `DTCS_CODES`:
/// <https://github.com/kk7ds/chirp/blob/master/chirp/chirp_common.py#L45-L57>.
/// This table establishes neither TK-2402 support nor raw DAT representations.
pub const DCS_CODES: [u16; 104] = [
    0o023, 0o025, 0o026, 0o031, 0o032, 0o036, 0o043, 0o047, 0o051, 0o053, 0o054, 0o065, 0o071,
    0o072, 0o073, 0o074, 0o114, 0o115, 0o116, 0o122, 0o125, 0o131, 0o132, 0o134, 0o143, 0o145,
    0o152, 0o155, 0o156, 0o162, 0o165, 0o172, 0o174, 0o205, 0o212, 0o223, 0o225, 0o226, 0o243,
    0o244, 0o245, 0o246, 0o251, 0o252, 0o255, 0o261, 0o263, 0o265, 0o266, 0o271, 0o274, 0o306,
    0o311, 0o315, 0o325, 0o331, 0o332, 0o343, 0o346, 0o351, 0o356, 0o364, 0o365, 0o371, 0o411,
    0o412, 0o413, 0o423, 0o431, 0o432, 0o445, 0o446, 0o452, 0o454, 0o455, 0o462, 0o464, 0o465,
    0o466, 0o503, 0o506, 0o516, 0o523, 0o526, 0o532, 0o546, 0o565, 0o606, 0o612, 0o624, 0o627,
    0o631, 0o632, 0o654, 0o662, 0o664, 0o703, 0o712, 0o723, 0o731, 0o732, 0o734, 0o743, 0o754,
];

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("Frecuencia inválida: {0}; rango admitido 136–174 MHz")]
    Frequency(String),
    #[error("Tono QT inválido: {0}")]
    Ctcss(String),
    #[error("Código DQT inválido: {0}; use un código convencional de tres dígitos octales y N o I")]
    Dcs(String),
    #[error("Señalización inválida: {0}; use Off, QT 94.8, DQT 023N o RAW:XXXX")]
    Signaling(String),
    #[error("Configuración inválida: {0}")]
    Configuration(String),
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
    #[error("CSV inválido: {0}")]
    Csv(#[from] csv::Error),
    #[error("CSV inválido: {0}")]
    CsvFormat(String),
    #[error("Error de escritura: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ModelError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RadioModel {
    Tk2402,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Frequency(u32);

impl Frequency {
    pub fn from_hz(hz: u32) -> Result<Self> {
        if !(MIN_FREQUENCY_HZ..=MAX_FREQUENCY_HZ).contains(&hz) {
            return Err(ModelError::Frequency(format!("{hz} Hz")));
        }
        Ok(Self(hz))
    }

    pub const fn hz(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for Frequency {
    type Error = ModelError;
    fn try_from(value: u32) -> Result<Self> {
        Self::from_hz(value)
    }
}

impl From<Frequency> for u32 {
    fn from(value: Frequency) -> Self {
        value.0
    }
}

impl FromStr for Frequency {
    type Err = ModelError;
    fn from_str(value: &str) -> Result<Self> {
        let units = parse_fixed_decimal(value.trim(), 6)
            .ok_or_else(|| ModelError::Frequency(value.to_owned()))?;
        Self::from_hz(units)
    }
}

impl fmt::Display for Frequency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 1_000_000;
        let fraction = self.0 % 1_000_000;
        if fraction.is_multiple_of(10) {
            write!(f, "{whole}.{:05}", fraction / 10)
        } else {
            write!(f, "{whole}.{fraction:06}")
        }
    }
}

/// A QT frequency in tenths of Hz from the conventional 50-tone domain.
/// Binary export must separately establish the model's permitted tone table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct CtcssTone(u16);

impl CtcssTone {
    pub fn from_tenths(tenths: u16) -> Result<Self> {
        if CTCSS_TONES_TENTHS.binary_search(&tenths).is_err() {
            return Err(ModelError::Ctcss(format!(
                "{}.{:01}",
                tenths / 10,
                tenths % 10
            )));
        }
        Ok(Self(tenths))
    }

    pub const fn tenths(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for CtcssTone {
    type Error = ModelError;
    fn try_from(value: u16) -> Result<Self> {
        Self::from_tenths(value)
    }
}

impl From<CtcssTone> for u16 {
    fn from(value: CtcssTone) -> Self {
        value.0
    }
}

impl FromStr for CtcssTone {
    type Err = ModelError;
    fn from_str(value: &str) -> Result<Self> {
        let tenths = parse_fixed_decimal(value.trim(), 1)
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(|| ModelError::Ctcss(value.to_owned()))?;
        Self::from_tenths(tenths)
    }
}

impl fmt::Display for CtcssTone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:01}", self.0 / 10, self.0 % 10)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DcsValue", into = "DcsValue")]
pub struct DcsCode {
    code: u16,
    inverted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DcsValue {
    code: u16,
    inverted: bool,
}

impl DcsCode {
    /// `code` is the numerical value of an octal code, e.g. `0o023`.
    /// Validates the conventional 104-code set, not a verified DAT mapping.
    pub fn new(code: u16, inverted: bool) -> Result<Self> {
        if DCS_CODES.binary_search(&code).is_err() {
            return Err(ModelError::Dcs(format!("{code}")));
        }
        Ok(Self { code, inverted })
    }

    pub const fn code(self) -> u16 {
        self.code
    }

    pub const fn inverted(self) -> bool {
        self.inverted
    }
}

impl TryFrom<DcsValue> for DcsCode {
    type Error = ModelError;
    fn try_from(value: DcsValue) -> Result<Self> {
        Self::new(value.code, value.inverted)
    }
}

impl From<DcsCode> for DcsValue {
    fn from(value: DcsCode) -> Self {
        Self {
            code: value.code,
            inverted: value.inverted,
        }
    }
}

impl FromStr for DcsCode {
    type Err = ModelError;
    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim();
        let value = value
            .strip_prefix('D')
            .or_else(|| value.strip_prefix('d'))
            .unwrap_or(value);
        let bytes = value.as_bytes();
        if bytes.len() != 4 || !bytes[..3].iter().all(|b| (b'0'..=b'7').contains(b)) {
            return Err(ModelError::Dcs(value.to_owned()));
        }
        let inverted = match bytes[3] {
            b'N' | b'n' => false,
            b'I' | b'i' => true,
            _ => return Err(ModelError::Dcs(value.to_owned())),
        };
        let code =
            u16::from_str_radix(&value[..3], 8).map_err(|_| ModelError::Dcs(value.to_owned()))?;
        Self::new(code, inverted)
    }
}

impl fmt::Display for DcsCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:03o}{}",
            self.code,
            if self.inverted { 'I' } else { 'N' }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SquelchSignaling {
    Off,
    Ctcss(CtcssTone),
    Dcs(DcsCode),
    Unknown { raw: u16 },
}

impl FromStr for SquelchSignaling {
    type Err = ModelError;
    fn from_str(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("off") {
            return Ok(Self::Off);
        }
        let mut words = value.split_whitespace();
        if let (Some(kind), Some(tone), None) = (words.next(), words.next(), words.next()) {
            if kind.eq_ignore_ascii_case("QT") {
                return tone.parse().map(Self::Ctcss);
            }
            if kind.eq_ignore_ascii_case("DQT") {
                return tone.parse().map(Self::Dcs);
            }
        }
        if let Some((prefix, digits)) = value.split_once(':')
            && prefix.eq_ignore_ascii_case("RAW")
            && digits.len() == 4
            && digits.bytes().all(|b| b.is_ascii_hexdigit())
        {
            let raw = u16::from_str_radix(digits, 16)
                .map_err(|_| ModelError::Signaling(value.to_owned()))?;
            return Ok(Self::Unknown { raw });
        }
        Err(ModelError::Signaling(value.to_owned()))
    }
}

impl fmt::Display for SquelchSignaling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("Off"),
            Self::Ctcss(tone) => write!(f, "QT {tone}"),
            Self::Dcs(code) => write!(f, "DQT D{code}"),
            Self::Unknown { raw } => write!(f, "RAW:{raw:04X}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerLevel {
    High,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelBandwidth {
    Wide,
    Narrow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ChannelValue", into = "ChannelValue")]
pub struct Channel {
    pub number: u8,
    pub rx: Option<Frequency>,
    pub tx: Option<Frequency>,
    pub decode: SquelchSignaling,
    pub encode: SquelchSignaling,
    pub power: Option<PowerLevel>,
    pub bandwidth: Option<ChannelBandwidth>,
    pub scan_add: Option<bool>,
    pub optional_signaling: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelValue {
    number: u8,
    rx: Option<Frequency>,
    tx: Option<Frequency>,
    decode: SquelchSignaling,
    encode: SquelchSignaling,
    power: Option<PowerLevel>,
    bandwidth: Option<ChannelBandwidth>,
    scan_add: Option<bool>,
    optional_signaling: Option<String>,
}

impl Channel {
    pub fn validate(&self) -> Result<()> {
        if !(1..=16).contains(&self.number) {
            return Err(ModelError::Configuration(format!(
                "canal {} fuera de 1–16",
                self.number
            )));
        }
        Ok(())
    }
}

impl TryFrom<ChannelValue> for Channel {
    type Error = ModelError;
    fn try_from(value: ChannelValue) -> Result<Self> {
        let channel = Self {
            number: value.number,
            rx: value.rx,
            tx: value.tx,
            decode: value.decode,
            encode: value.encode,
            power: value.power,
            bandwidth: value.bandwidth,
            scan_add: value.scan_add,
            optional_signaling: value.optional_signaling,
        };
        channel.validate()?;
        Ok(channel)
    }
}

impl From<Channel> for ChannelValue {
    fn from(value: Channel) -> Self {
        Self {
            number: value.number,
            rx: value.rx,
            tx: value.tx,
            decode: value.decode,
            encode: value.encode,
            power: value.power,
            bandwidth: value.bandwidth,
            scan_add: value.scan_add,
            optional_signaling: value.optional_signaling,
        }
    }
}

/// `None` means unknown, not disabled. Scan fields remain independent of the
/// unverified DAT flag layout. References use the radio's channel numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ScanValue", into = "ScanValue")]
pub struct ScanSettings {
    pub enabled: Option<bool>,
    pub priority_channel: Option<u8>,
    pub revert_channel: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanValue {
    enabled: Option<bool>,
    priority_channel: Option<u8>,
    revert_channel: Option<u8>,
}

impl ScanSettings {
    pub fn validate(&self) -> Result<()> {
        for (name, channel) in [
            ("prioritario", self.priority_channel),
            ("reversión", self.revert_channel),
        ] {
            if let Some(channel) = channel
                && !(1..=16).contains(&channel)
            {
                return Err(ModelError::Configuration(format!(
                    "canal de escaneo {name} fuera de 1–16"
                )));
            }
        }
        Ok(())
    }
}

impl TryFrom<ScanValue> for ScanSettings {
    type Error = ModelError;
    fn try_from(value: ScanValue) -> Result<Self> {
        let scan = Self {
            enabled: value.enabled,
            priority_channel: value.priority_channel,
            revert_channel: value.revert_channel,
        };
        scan.validate()?;
        Ok(scan)
    }
}

impl From<ScanSettings> for ScanValue {
    fn from(value: ScanSettings) -> Self {
        Self {
            enabled: value.enabled,
            priority_channel: value.priority_channel,
            revert_channel: value.revert_channel,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ConfigurationValue", into = "ConfigurationValue")]
pub struct RadioConfiguration {
    pub schema_version: u32,
    pub model: RadioModel,
    pub channels: Vec<Channel>,
    pub scan: ScanSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigurationValue {
    schema_version: u32,
    model: RadioModel,
    channels: Vec<Channel>,
    scan: ScanSettings,
}

impl Default for RadioConfiguration {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            model: RadioModel::Tk2402,
            channels: Vec::new(),
            scan: ScanSettings::default(),
        }
    }
}

impl RadioConfiguration {
    /// Checks technical value domains only. This is not permission to transmit
    /// or a claim of compatibility with an unverified binary/protocol encoding.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(ModelError::Configuration(format!(
                "versión {} no admitida; se espera {}",
                self.schema_version, CURRENT_SCHEMA_VERSION
            )));
        }
        if self.channels.len() > 16 {
            return Err(ModelError::Configuration("máximo 16 canales".to_owned()));
        }
        let mut numbers = HashSet::new();
        for channel in &self.channels {
            channel.validate()?;
            if !numbers.insert(channel.number) {
                return Err(ModelError::Configuration(format!(
                    "canal {} duplicado",
                    channel.number
                )));
            }
        }
        self.scan.validate()?;
        for (name, channel) in [
            ("prioritario", self.scan.priority_channel),
            ("reversión", self.scan.revert_channel),
        ] {
            if let Some(channel) = channel {
                if !(1..=16).contains(&channel) {
                    return Err(ModelError::Configuration(format!(
                        "canal de escaneo {name} fuera de 1–16"
                    )));
                }
                if !numbers.contains(&channel) {
                    return Err(ModelError::Configuration(format!(
                        "canal de escaneo {name} {channel} no definido"
                    )));
                }
            }
        }
        Ok(())
    }
}

impl TryFrom<ConfigurationValue> for RadioConfiguration {
    type Error = ModelError;
    fn try_from(value: ConfigurationValue) -> Result<Self> {
        let configuration = Self {
            schema_version: value.schema_version,
            model: value.model,
            channels: value.channels,
            scan: value.scan,
        };
        configuration.validate()?;
        Ok(configuration)
    }
}

impl From<RadioConfiguration> for ConfigurationValue {
    fn from(value: RadioConfiguration) -> Self {
        Self {
            schema_version: value.schema_version,
            model: value.model,
            channels: value.channels,
            scan: value.scan,
        }
    }
}

pub fn import_json(bytes: &[u8]) -> Result<RadioConfiguration> {
    let configuration: RadioConfiguration = serde_json::from_slice(bytes)?;
    configuration.validate()?;
    Ok(configuration)
}

pub fn export_json(configuration: &RadioConfiguration) -> Result<Vec<u8>> {
    configuration.validate()?;
    Ok(serde_json::to_vec_pretty(configuration)?)
}

/// UTF-8 CSV, decimal MHz, QT/DQT Kenwood names. A `scan` row is required even
/// when every scan option is unknown. Header order may vary on import.
pub const CSV_HEADERS: [&str; 15] = [
    "schema_version",
    "model",
    "record_type",
    "number",
    "rx_mhz",
    "tx_mhz",
    "decode",
    "encode",
    "power",
    "bandwidth",
    "scan_add",
    "optional_signaling_json",
    "scan_enabled",
    "scan_priority_channel",
    "scan_revert_channel",
];

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CsvRow {
    schema_version: String,
    model: String,
    record_type: String,
    number: String,
    rx_mhz: String,
    tx_mhz: String,
    decode: String,
    encode: String,
    power: String,
    bandwidth: String,
    scan_add: String,
    optional_signaling_json: String,
    scan_enabled: String,
    scan_priority_channel: String,
    scan_revert_channel: String,
}

impl CsvRow {
    fn new(record_type: &str) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION.to_string(),
            model: "Tk2402".to_owned(),
            record_type: record_type.to_owned(),
            ..Self::default()
        }
    }

    fn has_channel_fields(&self) -> bool {
        [
            &self.number,
            &self.rx_mhz,
            &self.tx_mhz,
            &self.decode,
            &self.encode,
            &self.power,
            &self.bandwidth,
            &self.scan_add,
            &self.optional_signaling_json,
        ]
        .iter()
        .any(|value| !value.is_empty())
    }

    fn has_scan_fields(&self) -> bool {
        [
            &self.scan_enabled,
            &self.scan_priority_channel,
            &self.scan_revert_channel,
        ]
        .iter()
        .any(|value| !value.is_empty())
    }
}

pub fn import_csv(bytes: &[u8]) -> Result<RadioConfiguration> {
    let mut reader = csv::ReaderBuilder::new().from_reader(bytes);
    let headers = reader.headers()?;
    if headers.len() != CSV_HEADERS.len()
        || CSV_HEADERS
            .iter()
            .any(|expected| headers.iter().filter(|h| h == expected).count() != 1)
    {
        return Err(ModelError::CsvFormat(format!(
            "cabeceras esperadas: {}",
            CSV_HEADERS.join(",")
        )));
    }
    let mut configuration = RadioConfiguration::default();
    let mut scan_seen = false;
    for row in reader.deserialize::<CsvRow>() {
        let row = row?;
        if row.schema_version != CURRENT_SCHEMA_VERSION.to_string() || row.model != "Tk2402" {
            return Err(ModelError::CsvFormat(
                "versión o modelo no admitido".to_owned(),
            ));
        }
        match row.record_type.as_str() {
            "scan" => {
                if scan_seen || row.has_channel_fields() {
                    return Err(ModelError::CsvFormat(
                        "una única fila scan sin campos de canal es obligatoria".to_owned(),
                    ));
                }
                scan_seen = true;
                configuration.scan = ScanSettings {
                    enabled: parse_optional(&row.scan_enabled, "scan_enabled")?,
                    priority_channel: parse_optional(
                        &row.scan_priority_channel,
                        "scan_priority_channel",
                    )?,
                    revert_channel: parse_optional(
                        &row.scan_revert_channel,
                        "scan_revert_channel",
                    )?,
                };
            }
            "channel" => {
                if row.has_scan_fields() {
                    return Err(ModelError::CsvFormat(
                        "los parámetros globales pertenecen a la fila scan".to_owned(),
                    ));
                }
                let power = match row.power.as_str() {
                    "" => None,
                    "High" => Some(PowerLevel::High),
                    "Low" => Some(PowerLevel::Low),
                    _ => {
                        return Err(ModelError::CsvFormat(format!(
                            "potencia inválida: {}",
                            row.power
                        )));
                    }
                };
                let bandwidth = match row.bandwidth.as_str() {
                    "" => None,
                    "Wide" => Some(ChannelBandwidth::Wide),
                    "Narrow" => Some(ChannelBandwidth::Narrow),
                    _ => {
                        return Err(ModelError::CsvFormat(format!(
                            "ancho inválido: {}",
                            row.bandwidth
                        )));
                    }
                };
                let optional_signaling = if row.optional_signaling_json.is_empty() {
                    None
                } else {
                    Some(serde_json::from_str::<String>(
                        &row.optional_signaling_json,
                    )?)
                };
                configuration.channels.push(Channel {
                    number: row.number.parse().map_err(|_| {
                        ModelError::CsvFormat(format!("número de canal inválido: {}", row.number))
                    })?,
                    rx: parse_optional(&row.rx_mhz, "rx_mhz")?,
                    tx: parse_optional(&row.tx_mhz, "tx_mhz")?,
                    decode: row.decode.parse()?,
                    encode: row.encode.parse()?,
                    power,
                    bandwidth,
                    scan_add: parse_optional(&row.scan_add, "scan_add")?,
                    optional_signaling,
                });
            }
            _ => {
                return Err(ModelError::CsvFormat(format!(
                    "tipo de fila inválido: {}",
                    row.record_type
                )));
            }
        }
    }
    if !scan_seen {
        return Err(ModelError::CsvFormat("falta la fila scan".to_owned()));
    }
    configuration.validate()?;
    Ok(configuration)
}

pub fn export_csv(configuration: &RadioConfiguration) -> Result<Vec<u8>> {
    configuration.validate()?;
    let mut writer = csv::Writer::from_writer(Vec::new());
    let mut scan = CsvRow::new("scan");
    scan.scan_enabled = display_optional(configuration.scan.enabled);
    scan.scan_priority_channel = display_optional(configuration.scan.priority_channel);
    scan.scan_revert_channel = display_optional(configuration.scan.revert_channel);
    writer.serialize(scan)?;
    for channel in &configuration.channels {
        let mut row = CsvRow::new("channel");
        row.number = channel.number.to_string();
        row.rx_mhz = display_optional(channel.rx);
        row.tx_mhz = display_optional(channel.tx);
        row.decode = channel.decode.to_string();
        row.encode = channel.encode.to_string();
        row.power = match channel.power {
            None => "",
            Some(PowerLevel::High) => "High",
            Some(PowerLevel::Low) => "Low",
        }
        .to_owned();
        row.bandwidth = match channel.bandwidth {
            None => "",
            Some(ChannelBandwidth::Wide) => "Wide",
            Some(ChannelBandwidth::Narrow) => "Narrow",
        }
        .to_owned();
        row.scan_add = display_optional(channel.scan_add);
        row.optional_signaling_json = channel
            .optional_signaling
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?
            .unwrap_or_default();
        writer.serialize(row)?;
    }
    writer.flush()?;
    writer
        .into_inner()
        .map_err(|error| ModelError::Io(error.into_error()))
}

fn parse_optional<T: FromStr>(value: &str, field: &str) -> Result<Option<T>>
where
    T::Err: fmt::Display,
{
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse()
        .map(Some)
        .map_err(|error| ModelError::CsvFormat(format!("{field}: {error}")))
}

fn display_optional<T: fmt::Display>(value: Option<T>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

/// Decimal parser with checked integer arithmetic; no binary floating point.
fn parse_fixed_decimal(value: &str, places: u32) -> Option<u32> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > places as usize
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
    {
        return None;
    }
    let whole: u32 = whole.parse().ok()?;
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().ok()?
    };
    whole
        .checked_mul(10u32.pow(places))?
        .checked_add(fraction_value.checked_mul(10u32.pow(places - fraction.len() as u32))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RadioConfiguration {
        RadioConfiguration {
            channels: vec![
                Channel {
                    number: 1,
                    rx: Some("145.01251".parse().unwrap()),
                    tx: Some("145.012501".parse().unwrap()),
                    decode: "QT 94.8".parse().unwrap(),
                    encode: "DQT 023I".parse().unwrap(),
                    power: None,
                    bandwidth: Some(ChannelBandwidth::Narrow),
                    scan_add: Some(false),
                    optional_signaling: Some("DTMF, \"A\"\nunknown".to_owned()),
                },
                Channel {
                    number: 16,
                    rx: None,
                    tx: None,
                    decode: SquelchSignaling::Unknown { raw: 0xF948 },
                    encode: SquelchSignaling::Off,
                    power: Some(PowerLevel::Low),
                    bandwidth: None,
                    scan_add: None,
                    optional_signaling: Some(String::new()),
                },
            ],
            scan: ScanSettings {
                enabled: Some(false),
                priority_channel: Some(16),
                revert_channel: Some(1),
            },
            ..RadioConfiguration::default()
        }
    }

    #[test]
    fn exact_frequency_decimal_and_bounds() {
        for (input, hz, shown) in [
            ("136", 136_000_000, "136.00000"),
            ("145.01251", 145_012_510, "145.01251"),
            ("145.012501", 145_012_501, "145.012501"),
            ("174.000000", 174_000_000, "174.00000"),
        ] {
            let frequency: Frequency = input.parse().unwrap();
            assert_eq!(frequency.hz(), hz);
            assert_eq!(frequency.to_string(), shown);
            assert_eq!(
                frequency.to_string().parse::<Frequency>().unwrap(),
                frequency
            );
        }
        for invalid in [
            "135.999999",
            "174.000001",
            "145.0000001",
            "145.",
            ".145",
            "1e2",
            "145,1",
            "145.1.2",
            "-145",
            "NaN",
            "4294967295",
        ] {
            assert!(invalid.parse::<Frequency>().is_err(), "{invalid}");
        }
        assert!(serde_json::from_str::<Frequency>("135000000").is_err());
        assert!(serde_json::from_str::<Frequency>("145012501.5").is_err());
        assert!(serde_json::from_str::<Frequency>("\"145.01250\"").is_err());
    }

    #[test]
    fn signaling_exact_formats_and_validation() {
        for value in [
            "Off",
            "QT 94.8",
            "QT 100.0",
            "DQT D023N",
            "DQT D754I",
            "RAW:FFFF",
            "RAW:0000",
        ] {
            let signaling: SquelchSignaling = value.parse().unwrap();
            assert_eq!(signaling.to_string(), value);
        }
        assert_eq!(
            "DQT 023N".parse::<SquelchSignaling>().unwrap(),
            SquelchSignaling::Dcs(DcsCode::new(0o23, false).unwrap())
        );
        for invalid in [
            "",
            "QT 0",
            "QT 94.9",
            "QT 94.81",
            "QT 300",
            "QT 94.8 extra",
            "DQT 089N",
            "DQT 000N",
            "DQT 777I",
            "DQT 23N",
            "DQT 023",
            "DQT 023R",
            "RAW:FFFFF",
            "RAW:ZZZZ",
        ] {
            assert!(invalid.parse::<SquelchSignaling>().is_err(), "{invalid}");
        }
        assert!(serde_json::from_str::<CtcssTone>("0").is_err());
        assert!(serde_json::from_str::<DcsCode>(r#"{"code":512,"inverted":false}"#).is_err());
        assert!(serde_json::from_str::<SquelchSignaling>(r#"{"Ctcss":0}"#).is_err());
    }

    #[test]
    fn conventional_tone_and_code_domains_are_complete_unique_and_roundtrip() {
        assert!(CTCSS_TONES_TENTHS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(DCS_CODES.windows(2).all(|pair| pair[0] < pair[1]));
        for tenths in CTCSS_TONES_TENTHS {
            let tone = CtcssTone::from_tenths(tenths).unwrap();
            assert_eq!(tone.to_string().parse::<CtcssTone>().unwrap(), tone);
            assert_eq!(
                serde_json::from_str::<CtcssTone>(&serde_json::to_string(&tone).unwrap()).unwrap(),
                tone
            );
        }
        for code in DCS_CODES {
            for inverted in [false, true] {
                let dcs = DcsCode::new(code, inverted).unwrap();
                assert_eq!(dcs.to_string().parse::<DcsCode>().unwrap(), dcs);
                assert_eq!(
                    serde_json::from_str::<DcsCode>(&serde_json::to_string(&dcs).unwrap()).unwrap(),
                    dcs
                );
            }
        }
    }

    #[test]
    fn configuration_revalidates_on_json_deserialization() {
        let valid = sample();
        let mut value = serde_json::to_value(&valid).unwrap();
        value["channels"][0]["number"] = 0.into();
        assert!(serde_json::from_value::<RadioConfiguration>(value.clone()).is_err());
        value["channels"][0]["number"] = 16.into();
        assert!(serde_json::from_value::<RadioConfiguration>(value.clone()).is_err());
        value["channels"][0]["number"] = 1.into();
        value["channels"][0]["rx"] = 175_000_000.into();
        assert!(serde_json::from_value::<RadioConfiguration>(value).is_err());
        for invalid in [
            b"{".as_slice(),
            br#"{"schema_version":1,"model":"Unknown","channels":[],"scan":{}}"#,
        ] {
            assert!(import_json(invalid).is_err());
        }
        let mut value = serde_json::to_value(&valid).unwrap();
        value["schema_version"] = 2.into();
        assert!(serde_json::from_value::<RadioConfiguration>(value).is_err());
        let mut channel = serde_json::to_value(&valid.channels[0]).unwrap();
        channel["number"] = 17.into();
        assert!(serde_json::from_value::<Channel>(channel).is_err());
        assert!(serde_json::from_str::<ScanSettings>(r#"{"priority_channel":17}"#).is_err());
        let mut value = serde_json::to_value(&valid).unwrap();
        value["future_setting"] = true.into();
        assert!(serde_json::from_value::<RadioConfiguration>(value).is_err());
        assert!(
            serde_json::from_str::<SquelchSignaling>(
                r#"{"Unknown":{"raw":65535,"discarded":true}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn checks_channel_count_scan_references_and_exports() {
        let mut configuration = sample();
        configuration.channels[0].number = 17;
        assert!(configuration.validate().is_err());
        assert!(export_json(&configuration).is_err());
        assert!(export_csv(&configuration).is_err());
        configuration = sample();
        configuration.scan.priority_channel = Some(2);
        assert!(configuration.validate().is_err());
        configuration.scan.priority_channel = Some(0);
        assert!(configuration.validate().is_err());
        configuration = sample();
        configuration.channels = vec![configuration.channels[0].clone(); 17];
        assert!(configuration.validate().is_err());
    }

    #[test]
    fn json_and_csv_preserve_all_known_unknown_and_global_values() {
        for configuration in [sample(), RadioConfiguration::default()] {
            let json = export_json(&configuration).unwrap();
            assert_eq!(import_json(&json).unwrap(), configuration);
            let csv = export_csv(&configuration).unwrap();
            assert_eq!(import_csv(&csv).unwrap(), configuration);
            assert_eq!(
                import_json(&export_json(&import_csv(&csv).unwrap()).unwrap()).unwrap(),
                configuration
            );
        }
    }

    #[test]
    fn csv_rejects_corruption_extra_headers_and_missing_metadata() {
        assert!(import_csv(b"number,rx_mhz\n1,145.0\n").is_err());
        let configuration = sample();
        let csv = String::from_utf8(export_csv(&configuration).unwrap()).unwrap();
        for (old, new) in [
            ("145.01251", "174.00001"),
            ("QT 94.8", "QT 94.81"),
            ("DQT D023I", "DQT D089I"),
            ("channel,1,", "channel,0,"),
            ("scan,", "other,"),
        ] {
            assert!(csv.contains(old), "La muestra CSV debe contener {old}");
            assert!(
                import_csv(csv.replace(old, new).as_bytes()).is_err(),
                "{new}"
            );
        }
        let headers_only = csv.lines().next().unwrap().as_bytes();
        assert!(import_csv(headers_only).is_err());
        let duplicate_headers = csv.replacen("scan_revert_channel", "scan_enabled", 1);
        assert!(import_csv(duplicate_headers.as_bytes()).is_err());
    }

    #[test]
    fn csv_accepts_reordered_headers_and_rejects_misplaced_global_settings() {
        let configuration = sample();
        let bytes = export_csv(&configuration).unwrap();
        let mut reader = csv::Reader::from_reader(bytes.as_slice());
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer
            .write_record(reader.headers().unwrap().iter().rev())
            .unwrap();
        for record in reader.records() {
            writer.write_record(record.unwrap().iter().rev()).unwrap();
        }
        let reordered = writer.into_inner().unwrap();
        assert_eq!(import_csv(&reordered).unwrap(), configuration);

        let mut reader = csv::Reader::from_reader(bytes.as_slice());
        let mut writer = csv::Writer::from_writer(Vec::new());
        for row in reader.deserialize::<CsvRow>() {
            let mut row = row.unwrap();
            if row.record_type == "channel" {
                row.scan_enabled = "true".to_owned();
            }
            writer.serialize(row).unwrap();
        }
        assert!(import_csv(&writer.into_inner().unwrap()).is_err());
    }
}
