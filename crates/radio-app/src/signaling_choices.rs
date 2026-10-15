// SPDX-License-Identifier: GPL-3.0-only
use radio_model::{CTCSS_TONES_TENTHS, DCS_CODES, SquelchSignaling};

pub fn label(value: &SquelchSignaling) -> String {
    match value {
        SquelchSignaling::Off => "None".into(),
        SquelchSignaling::Ctcss(tone) => tone.to_string(),
        SquelchSignaling::Dcs(code) => format!("D{code}"),
        SquelchSignaling::Unknown { raw } => format!("Desconocido (RAW:{raw:04X})"),
    }
}

pub fn choices(value: &SquelchSignaling) -> (Vec<String>, i32) {
    let mut values = vec!["None".into()];
    values.extend(
        CTCSS_TONES_TENTHS
            .iter()
            .filter(|tone| **tone <= 2503)
            .map(|tone| format!("{}.{:01}", tone / 10, tone % 10)),
    );
    for polarity in ['N', 'I'] {
        values.extend(
            DCS_CODES
                .iter()
                .map(|code| format!("D{code:03o}{polarity}")),
        );
    }
    let selected = label(value);
    let index = match values.iter().position(|item| item == &selected) {
        Some(index) => index,
        None => {
            values.push(selected);
            values.len() - 1
        }
    };
    (values, index as i32)
}

pub fn parse(value: &str) -> Result<SquelchSignaling, radio_model::ModelError> {
    let value = value.trim();
    if value == "None" {
        return "Off".parse();
    }
    if let Some(raw) = value
        .strip_prefix("Desconocido (")
        .and_then(|s| s.strip_suffix(')'))
    {
        return raw.parse();
    }
    if value.starts_with('D') && !value.contains(' ') {
        return format!("DQT {value}").parse();
    }
    if value.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return format!("QT {value}").parse();
    }
    value.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requested_catalog_roundtrips_and_preserves_unknown_imports() {
        let (list, selected) = choices(&SquelchSignaling::Off);
        assert_eq!(selected, 0);
        assert_eq!(&list[..2], ["None", "67.0"]);
        assert_eq!(list[49], "250.3");
        assert_eq!(list[50], "D023N");
        assert_eq!(list[154], "D023I");
        assert_eq!(list.last().unwrap(), "D754I");
        for item in &list {
            assert_eq!(label(&parse(item).unwrap()), *item);
        }
        for value in ["RAW:1234", "QT 254.1"] {
            let signal = value.parse().unwrap();
            let (list, index) = choices(&signal);
            assert_eq!(parse(&list[index as usize]).unwrap(), signal);
        }
    }
}
