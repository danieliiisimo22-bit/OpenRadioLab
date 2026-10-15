// SPDX-License-Identifier: GPL-3.0-only
use anyhow::{Context, Result, bail};
use dat_format::{DatDocument, atomic_write_new, save_recovery};
use radio_model::{Channel, RadioConfiguration, RadioModel, ScanSettings};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Session {
    pub current: RadioConfiguration,
    saved: RadioConfiguration,
    pub document: Option<DatDocument>,
    pub source: Option<PathBuf>,
    undo: Vec<RadioConfiguration>,
    redo: Vec<RadioConfiguration>,
    pub data_dir: PathBuf,
}

pub fn load_configuration(path: &Path) -> Result<(RadioConfiguration, Option<DatDocument>)> {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("dat") => {
            let doc = DatDocument::open(path)?;
            Ok((doc.configuration().clone(), Some(doc)))
        }
        Some("json") => Ok((radio_model::import_json(&fs::read(path)?)?, None)),
        Some("csv") => Ok((radio_model::import_csv(&fs::read(path)?)?, None)),
        _ => bail!("Extensión no admitida; use .dat, .json o .csv"),
    }
}

impl Session {
    pub fn new(data_dir: PathBuf) -> Self {
        let current = RadioConfiguration {
            schema_version: 1,
            model: RadioModel::Tk2402,
            channels: Vec::new(),
            scan: ScanSettings::default(),
        };
        Self {
            saved: current.clone(),
            current,
            document: None,
            source: None,
            undo: Vec::new(),
            redo: Vec::new(),
            data_dir,
        }
    }
    pub fn dirty(&self) -> bool {
        self.current != self.saved
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn can_dat(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|d| d.can_export_dat(&self.current))
    }
    pub fn load(&mut self, path: &Path, allow_discard: bool) -> Result<()> {
        if self.dirty() && !allow_discard {
            bail!("Hay cambios pendientes. Guarde JSON/CSV o marque permitir reemplazo.");
        }
        let (config, doc) = load_configuration(path)?;
        let canonical = fs::canonicalize(path)?;
        if let Some(doc) = &doc {
            doc.backup_original(&self.data_dir.join("backups"))?;
        }
        save_recovery(&self.data_dir.join("recovery"), &config)?;
        self.current = config.clone();
        self.saved = config;
        self.document = doc;
        self.source = Some(canonical);
        self.undo.clear();
        self.redo.clear();
        tracing::info!(path=%path.display(),"Configuración importada y respaldada");
        Ok(())
    }
    pub fn replace(&mut self, config: RadioConfiguration) -> Result<()> {
        config.validate()?;
        if config == self.current {
            return Ok(());
        }
        save_recovery(&self.data_dir.join("recovery"), &config)
            .context("No se pudo guardar recuperación; el cambio no se aplicó")?;
        self.undo.push(self.current.clone());
        self.redo.clear();
        self.current = config;
        tracing::info!("Cambio de configuración aplicado; recuperación JSON guardada");
        Ok(())
    }
    pub fn upsert(&mut self, channel: Channel) -> Result<()> {
        let mut config = self.current.clone();
        if let Some(old) = config
            .channels
            .iter_mut()
            .find(|c| c.number == channel.number)
        {
            *old = channel;
        } else {
            config.channels.push(channel);
            config.channels.sort_by_key(|c| c.number);
        }
        self.replace(config)
    }
    pub fn remove(&mut self, number: u8) -> Result<()> {
        let mut config = self.current.clone();
        config.channels.retain(|c| c.number != number);
        self.replace(config)
    }
    pub fn undo(&mut self) -> Result<()> {
        if let Some(previous) = self.undo.last() {
            save_recovery(&self.data_dir.join("recovery"), previous)?;
            self.redo.push(self.current.clone());
            self.current = self.undo.pop().expect("historial comprobado");
            tracing::info!("Deshacer aplicado");
        }
        Ok(())
    }
    pub fn redo(&mut self) -> Result<()> {
        if let Some(next) = self.redo.last() {
            save_recovery(&self.data_dir.join("recovery"), next)?;
            self.undo.push(self.current.clone());
            self.current = self.redo.pop().expect("historial comprobado");
            tracing::info!("Rehacer aplicado");
        }
        Ok(())
    }
    pub fn export(&mut self, path: &Path, kind: &str) -> Result<()> {
        if path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
            != Some(kind)
        {
            bail!("Para {kind}, use un nombre nuevo con extensión .{kind}");
        }
        match kind {
            "dat" => {
                let doc = self
                    .document
                    .as_ref()
                    .context("Abra un DAT para guardar una copia")?;
                doc.save_copy(path, &self.current, &self.data_dir.join("backups"))?;
            }
            "json" | "csv" => {
                let data = if kind == "json" {
                    radio_model::export_json(&self.current)?
                } else {
                    radio_model::export_csv(&self.current)?
                };
                atomic_write_new(path, &data, self.source.as_deref())?;
            }
            _ => bail!("Formato no admitido"),
        }
        self.saved = self.current.clone();
        tracing::info!(path=%path.display(),format=kind,"Exportación guardada sin sobrescribir");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use radio_model::{Frequency, SquelchSignaling};
    fn channel() -> Channel {
        Channel {
            number: 1,
            rx: Some(Frequency::from_hz(145_500_000).unwrap()),
            tx: None,
            decode: SquelchSignaling::Unknown { raw: 0xffff },
            encode: SquelchSignaling::Unknown { raw: 0xffff },
            power: None,
            bandwidth: None,
            scan_add: None,
            optional_signaling: None,
        }
    }
    #[test]
    fn history_and_recovery_survive_exports() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::new(dir.path().join("data"));
        s.upsert(channel()).unwrap();
        assert!(s.dirty());
        assert!(s.can_undo());
        s.undo().unwrap();
        assert!(!s.dirty());
        assert!(s.can_redo());
        s.redo().unwrap();
        assert!(s.dirty());
        let p = dir.path().join("saved.json");
        s.export(&p, "json").unwrap();
        assert!(!s.dirty());
        s.remove(1).unwrap();
        assert!(s.load(&p, false).is_err());
        s.load(&p, true).unwrap();
        assert_eq!(s.current.channels.len(), 1);
        assert!(!s.can_undo());
        assert!(
            fs::read_dir(dir.path().join("data/recovery"))
                .unwrap()
                .count()
                >= 4
        );
    }
    #[test]
    fn invalid_change_does_not_mutate_or_create_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Session::new(dir.path().into());
        let mut c = channel();
        c.number = 17;
        assert!(s.upsert(c).is_err());
        assert!(s.current.channels.is_empty());
        assert!(!s.can_undo());
    }
}
