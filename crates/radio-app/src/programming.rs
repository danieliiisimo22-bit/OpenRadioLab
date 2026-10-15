// SPDX-License-Identifier: GPL-3.0-only
use anyhow::{Context, Result, bail};
use dat_format::{DatDocument, atomic_write_new};
use radio_model::RadioConfiguration;
use radio_serial::tk2402::{RadioSession, plan_write};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct ProgramResult {
    pub path: PathBuf,
    pub changed_bytes: usize,
    pub blocks: usize,
}

/// Runs only after the user presses the physical write button.
/// Persists the complete live backup before the first Y byte can be sent.
pub fn program(
    port: &str,
    desired: &RadioConfiguration,
    directory: &Path,
    mut progress: impl FnMut(&str, usize, usize),
) -> Result<ProgramResult> {
    fs::create_dir_all(directory)?;
    let tick = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let prefix = directory.join(format!("write-{tick}"));
    let report_path = prefix.with_extension("report.json");
    let mut report = serde_json::json!({"port":port,"stage":"opening","write_acknowledged":0,"verified":false,"error":null});
    let result: Result<ProgramResult> = (|| {
        desired.validate()?;
        let mut radio = RadioSession::open(port)?;
        let operation: Result<(Vec<u8>, usize, usize)> = (|| {
            report["stage"] = "backup".into();
            let live = radio.read_image(|count| progress("Respaldando radio", count, 8192))?;
            let document = DatDocument::from_radio_image(&live)?;
            let backup = prefix.with_extension("before.dat");
            atomic_write_new(&backup, document.original_bytes(), None)?;
            atomic_write_new(&prefix.with_extension("before.bin"), &live, None)?;
            atomic_write_new(
                &prefix.with_extension("identity.bin"),
                &radio.identity,
                None,
            )?;
            report["backup"] = backup.display().to_string().into();
            let candidate = DatDocument::parse(document.serialize_candidate(desired)?)?;
            let plan = plan_write(&live, candidate.decoded_image())?;
            report["changed_bytes"] = plan.changed_bytes.into();
            report["blocks"] = serde_json::to_value(plan.blocks())?;
            atomic_write_new(
                &prefix.with_extension("plan.json"),
                &serde_json::to_vec_pretty(&plan)?,
                None,
            )?;
            atomic_write_new(
                &prefix.with_extension("planned.dat"),
                candidate.original_bytes(),
                None,
            )?;
            // Persist and re-read the backup; failure prevents any memory write.
            if fs::read(&backup)? != document.original_bytes() {
                bail!("No se pudo comprobar el respaldo previo");
            }
            report["stage"] = "writing".into();
            radio.apply_plan(&plan, |done, total| {
                report["write_acknowledged"] = done.into();
                progress("Escribiendo cambios", done, total);
            })?;
            Ok((
                plan.target().to_vec(),
                plan.changed_bytes,
                plan.blocks().len(),
            ))
        })();
        let exit = radio.finish();
        report["write_session_exit_confirmed"] = exit.is_ok().into();
        report["write_session_events"] = serde_json::to_value(&radio.trace)?;
        drop(radio);
        let (target, changed_bytes, blocks) = match (operation, exit) {
            (Ok(value), Ok(())) => value,
            (Err(error), Ok(())) => return Err(error),
            (operation, Err(exit)) => bail!(
                "{}; salida PC no confirmada: {exit}",
                operation
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "Bloques enviados".into())
            ),
        };
        report["stage"] = "verification".into();
        std::thread::sleep(Duration::from_millis(500));
        let mut verify =
            RadioSession::open(port).context("No se pudo abrir la sesión de verificación")?;
        let actual = verify.read_image(|count| progress("Verificando radio", count, 8192));
        let exit = verify.finish();
        report["verification_exit_confirmed"] = exit.is_ok().into();
        report["verification_events"] = serde_json::to_value(&verify.trace)?;
        let actual = actual?;
        let verified_document = DatDocument::from_radio_image(&actual)?;
        let path = prefix.with_extension("after.dat");
        atomic_write_new(&path, verified_document.original_bytes(), None)?;
        atomic_write_new(&prefix.with_extension("after.bin"), &actual, None)?;
        let differences = dat_format::binary_diff(&target, &actual);
        report["differences"] = serde_json::to_value(&differences)?;
        exit?;
        if !differences.is_empty() {
            bail!(
                "La relectura tiene {} diferencias respecto al plan. No se reintentó ni se restauró automáticamente. Respaldo: {}",
                differences.len(),
                prefix.with_extension("before.dat").display()
            );
        }
        report["verified"] = true.into();
        report["stage"] = "complete".into();
        Ok(ProgramResult {
            path,
            changed_bytes,
            blocks,
        })
    })();
    if let Err(error) = &result {
        report["error"] = format!("{error:#}").into();
    }
    atomic_write_new(&report_path, &serde_json::to_vec_pretty(&report)?, None)
        .context("No se pudo guardar el informe de escritura")?;
    result.with_context(|| format!("Informe: {}", report_path.display()))
}
