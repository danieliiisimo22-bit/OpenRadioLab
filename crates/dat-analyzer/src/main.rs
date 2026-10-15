// SPDX-License-Identifier: GPL-3.0-only
use anyhow::{Context, Result, bail};
use dat_format::{DatDocument, binary_diff};
use std::{env, fs, path::Path};

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
fn print_diff(a: &[u8], b: &[u8], label: &str) {
    let diff = binary_diff(a, b);
    println!("{label}: {} offsets modificados", diff.len());
    for d in diff {
        let display = |v: Option<u8>| v.map(|v| format!("{v:02X}")).unwrap_or_else(|| "--".into());
        println!(
            "0x{:04X}: {} -> {}",
            d.offset,
            display(d.before),
            display(d.after)
        );
        let start = d.offset.saturating_sub(4);
        let end = (d.offset + 5).min(a.len());
        if start < end {
            println!("  A @0x{start:04X}: {}", hex(&a[start..end]));
        }
        let end = (d.offset + 5).min(b.len());
        if start < end {
            println!("  B @0x{start:04X}: {}", hex(&b[start..end]));
        }
    }
}
fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("read-radio") if matches!(args.len(), 3 | 4) => {
            let template = if args.len() == 4 {
                Some(DatDocument::open(Path::new(&args[2]))?)
            } else {
                None
            };
            let path = Path::new(args.last().expect("destino comprobado"));
            let diagnostic = path.with_extension("diagnostic.json");
            let identity = path.with_extension("identity.bin");
            let image = path.with_extension("image.bin");
            for destination in [
                path,
                diagnostic.as_path(),
                identity.as_path(),
                image.as_path(),
            ] {
                if destination.exists() {
                    bail!("El destino ya existe: {}", destination.display());
                }
            }
            let capture = radio_serial::tk2402::capture(&args[1], |count| {
                println!("Lectura: {} / 64 bloques", count / 128)
            });
            dat_format::atomic_write_new(
                &diagnostic,
                &serde_json::to_vec_pretty(&capture.diagnostic)?,
                None,
            )?;
            if !capture.identity.is_empty() {
                dat_format::atomic_write_new(&identity, &capture.identity, None)?;
            }
            if let Some(bytes) = &capture.image {
                dat_format::atomic_write_new(&image, bytes, None)?;
            }
            if let Some(error) = &capture.diagnostic.error {
                bail!("{error}. Diagnóstico: {}", diagnostic.display());
            }
            let received = capture.image.context("Sin imagen completa")?;
            let document = if let Some(template) = &template {
                template.with_radio_image(&received)?
            } else {
                DatDocument::from_radio_image(&received)?
            };
            dat_format::atomic_write_new(
                path,
                document.original_bytes(),
                if args.len() == 4 {
                    Some(Path::new(&args[2]))
                } else {
                    None
                },
            )?;
            println!(
                "Lectura completa: 8192 bytes; salida confirmada. DAT: {}",
                path.display()
            );
        }
        Some("probe-radio") if args.len() == 3 => {
            let path = Path::new(&args[2]);
            if path.exists() {
                bail!("El destino de identificación ya existe");
            }
            let mut session = radio_serial::tk2402::RadioSession::open(&args[1])?;
            let identity = session.identity.clone();
            let exit = session.finish();
            dat_format::atomic_write_new(path, &identity, None)?;
            exit?;
            println!(
                "Identificación real recibida: {} bytes. Modo PC finalizado. Archivo privado: {}",
                identity.len(),
                path.display()
            );
        }
        Some("diff") if args.len() == 3 => {
            let a = fs::read(&args[1])?;
            let b = fs::read(&args[2])?;
            print_diff(&a, &b, "Comparación binaria cruda");
            if let (Ok(a), Ok(b)) = (DatDocument::parse(a), DatDocument::parse(b)) {
                println!("Offsets siguientes relativos a la imagen (sumar 0x41 para DAT)");
                print_diff(
                    a.decoded_image(),
                    b.decoded_image(),
                    "Comparación normalizada",
                );
            }
        }
        Some("ports") if args.len() == 1 => println!(
            "{}",
            serde_json::to_string_pretty(&radio_serial::discover_ports()?)?
        ),
        Some("export-json" | "export-csv") if args.len() == 3 => {
            let d = DatDocument::open(Path::new(&args[1]))?;
            let data = if args[0] == "export-json" {
                radio_model::export_json(d.configuration())?
            } else {
                radio_model::export_csv(d.configuration())?
            };
            dat_format::atomic_write_new(Path::new(&args[2]), &data, d.source())?;
            println!("Exportado {}", args[2]);
        }
        Some("copy") if args.len() == 3 => {
            let d = DatDocument::open(Path::new(&args[1]))?;
            let parent = Path::new(&args[2])
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let backup = d.save_copy(
                Path::new(&args[2]),
                d.configuration(),
                &parent.join("backups"),
            )?;
            println!("Copia idéntica. Respaldo: {}", backup.display());
        }
        Some("export-dat" | "candidate-dat") if args.len() == 4 => {
            let document = DatDocument::open(Path::new(&args[1]))?;
            let configuration = radio_model::import_json(&fs::read(&args[2])?)?;
            let bytes = document.serialize_candidate(&configuration)?;
            let destination = Path::new(&args[3]);
            let parent = destination
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let backup = document.backup_original(&parent.join("backups"))?;
            dat_format::atomic_write_new(destination, &bytes, document.source())?;
            if fs::read(destination)? != bytes {
                bail!("La verificación del archivo candidato falló");
            }
            println!(
                "DAT guardado en archivo nuevo: {}\nSHA-256: {}\nRespaldo: {}\nCampos admitidos comprobados en KPG. No se ha programado ninguna radio.",
                destination.display(),
                dat_format::sha256(&bytes),
                backup.display()
            );
        }
        Some("verify-samples") if args.len() == 4 => verify_samples(&args[1..])?,
        Some("verify-channel-samples") if args.len() == 5 => verify_channel_samples(&args[1..])?,
        Some(path) if args.len() == 1 => {
            let bytes = fs::read(path).with_context(|| format!("No se pudo leer {path}"))?;
            println!(
                "Archivo: {path}\nTamaño: {} bytes\nSHA-256: {}",
                bytes.len(),
                dat_format::sha256(&bytes)
            );
            println!(
                "Cabecera hexadecimal:\n{}",
                hex(&bytes[..bytes.len().min(64)])
            );
            let doc = DatDocument::parse(bytes)?;
            println!(
                "Máscara XOR: {:02X}; byte 0x40: {:02X}",
                doc.mask(),
                doc.original_bytes()[64]
            );
            for f in doc.findings() {
                println!("{}: {} [{}]", f.range, f.interpretation, f.confidence);
            }
            println!("Canales interpretados:");
            for c in &doc.configuration().channels {
                println!(
                    "CH {:02} RX {} TX {} Decode {} Encode {} {:?}; potencia {:?}, scan {:?}; opcional: desconocido",
                    c.number,
                    c.rx.map(|f| f.to_string()).unwrap_or_else(|| "--".into()),
                    c.tx.map(|f| f.to_string()).unwrap_or_else(|| "--".into()),
                    c.decode,
                    c.encode,
                    c.bandwidth,
                    c.power,
                    c.scan_add
                );
            }
            for w in doc.warnings {
                println!("LIMITACIÓN: {w}");
            }
        }
        _ => bail!(
            "Uso: dat-analyzer archivo.dat | diff a.dat b.dat | copy origen.dat destino.dat | export-dat base.dat configuracion.json destino.dat | probe-radio puerto identidad.bin | read-radio puerto [base.dat] nuevo.dat | export-json/csv origen.dat destino | ports | verify-samples base variante-bit0 decode100 | verify-channel-samples base high wide scan-no"
        ),
    }
    Ok(())
}

fn verify_samples(paths: &[String]) -> Result<()> {
    let docs = paths
        .iter()
        .map(|p| DatDocument::open(Path::new(p)))
        .collect::<Result<Vec<_>, _>>()?;
    for (i, d) in docs.iter().enumerate() {
        let regenerated = d.serialize_lossless(d.configuration())?;
        if regenerated != d.original_bytes() {
            bail!("Round-trip falló muestra {i}");
        }
        if d.configuration().channels.len() != 6 {
            bail!("Se esperaban seis canales en muestra {i}");
        }
    }
    let narrow = binary_diff(docs[0].decoded_image(), docs[1].decoded_image());
    if narrow.len() != 1
        || narrow[0].offset != 0x12d1
        || narrow[0].before != Some(0xcc)
        || narrow[0].after != Some(0xcd)
    {
        bail!("Diferencia bit 0 Narrow/Wide no coincide");
    }
    let qt = binary_diff(docs[0].decoded_image(), docs[2].decoded_image());
    if qt.len() != 1
        || qt[0].offset != 0x12cc
        || qt[0].before != Some(0xb4)
        || qt[0].after != Some(0xe8)
    {
        bail!("Diferencia QT no coincide");
    }
    let dir = std::env::temp_dir().join(format!("kenwood-verify-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    for (i, d) in docs.iter().enumerate() {
        let dest = dir.join(format!("sample-{i}.dat"));
        d.save_copy(&dest, d.configuration(), &dir.join("backups"))?;
        if fs::read(&dest)? != d.original_bytes() {
            bail!("Copia atómica falló");
        }
    }
    println!(
        "PASS: tres round-trips byte por byte, copia atómica con backups, bit0 de ancho y QT Decode 948/1000. La dirección del ancho se corrigió a 0 Narrow/1 Wide con nuevas muestras. Integridad/edición DAT aún sin verificar. Copias privadas en {}",
        dir.display()
    );
    Ok(())
}

fn verify_channel_samples(paths: &[String]) -> Result<()> {
    use radio_model::{ChannelBandwidth, PowerLevel};
    let documents = paths
        .iter()
        .map(|path| DatDocument::open(Path::new(path)))
        .collect::<Result<Vec<_>, _>>()?;
    let expected_differences = [
        vec![(0x12d1, 0xcc, 0xec)],
        vec![(0x12d1, 0xcc, 0xcd)],
        vec![(0x1000, 0xc0, 0xc1), (0x12d1, 0xcc, 0xdc)],
    ];
    for (index, document) in documents.iter().enumerate() {
        if document.serialize_lossless(document.configuration())? != document.original_bytes() {
            bail!("Round-trip falló muestra {index}");
        }
        if document.configuration().channels.len() != 6 {
            bail!("Se esperaban seis canales en muestra {index}");
        }
        for channel in &document.configuration().channels {
            let modified = channel.number == 1;
            let expected_power = if index == 1 && modified {
                PowerLevel::High
            } else {
                PowerLevel::Low
            };
            let expected_width = if index == 2 && modified {
                ChannelBandwidth::Wide
            } else {
                ChannelBandwidth::Narrow
            };
            if channel.power != Some(expected_power)
                || channel.bandwidth != Some(expected_width)
                || channel.scan_add != Some(!(index == 3 && modified))
            {
                bail!(
                    "Flags inesperados, muestra {index}, canal {}",
                    channel.number
                );
            }
        }
        if index > 0 {
            let observed = binary_diff(documents[0].decoded_image(), document.decoded_image());
            let observed = observed
                .iter()
                .map(|difference| (difference.offset, difference.before, difference.after))
                .collect::<Vec<_>>();
            let expected = expected_differences[index - 1]
                .iter()
                .map(|&(offset, before, after)| (offset, Some(before), Some(after)))
                .collect::<Vec<_>>();
            if observed != expected {
                bail!("Diferencias normalizadas no coinciden, muestra {index}: {observed:?}");
            }
        }
        println!(
            "Muestra {index}: SHA-256 {}, XOR {:02X}",
            dat_format::sha256(document.original_bytes()),
            document.mask()
        );
    }
    println!(
        "PASS: cuatro round-trips byte por byte y cambios controlados Low/High, Narrow/Wide, Scan Add Yes/No. Tabla global Scan corroborada para CH1; exportación de campos admitidos comprobada por reapertura KPG."
    );
    Ok(())
}
