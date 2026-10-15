// SPDX-License-Identifier: GPL-3.0-only
//! Native Explorer-style file selection. Dialogs never perform the save itself.
use anyhow::Result;
use std::path::{Path, PathBuf};

pub enum Action<'a> {
    Open,
    Save { kind: &'a str },
}

#[cfg(windows)]
pub fn choose(action: Action<'_>, source: Option<&Path>) -> Result<Option<PathBuf>> {
    use std::{ffi::OsString, mem::size_of, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::UI::{
        Controls::Dialogs::{
            CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER,
            OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
        },
        Input::KeyboardAndMouse::GetActiveWindow,
    };
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let (title, filter, extension, name, saving) = match action {
        Action::Open => (
            "Escoger archivo",
            "Configuración de radio (*.dat;*.json;*.csv)\0*.dat;*.json;*.csv\0Todos los archivos\0*.*\0",
            "",
            String::new(),
            false,
        ),
        Action::Save { kind } => {
            anyhow::ensure!(
                matches!(kind, "dat" | "json" | "csv"),
                "Formato de guardado no admitido"
            );
            let stem = source
                .and_then(Path::file_stem)
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "configuracion".into());
            (
                "Guardar como",
                "",
                kind,
                format!("{stem}-editado.{kind}"),
                true,
            )
        }
    };
    let title = wide(title);
    let filter = if saving {
        wide(&format!(
            "Archivo {} (*.{extension})\0*.{extension}\0",
            extension.to_uppercase()
        ))
    } else {
        wide(filter)
    };
    let extension = wide(extension);
    fn documents() -> Option<PathBuf> {
        use windows_sys::Win32::{
            System::Com::CoTaskMemFree,
            UI::Shell::{FOLDERID_Documents, SHGetKnownFolderPath},
        };
        let mut raw = std::ptr::null_mut();
        // A successful known-folder call returns an owned, NUL-terminated UTF-16 path.
        let result =
            unsafe { SHGetKnownFolderPath(&FOLDERID_Documents, 0, std::ptr::null_mut(), &mut raw) };
        if result < 0 || raw.is_null() {
            return None;
        }
        let mut length = 0;
        unsafe {
            while *raw.add(length) != 0 {
                length += 1;
            }
            let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(raw, length)));
            CoTaskMemFree(raw.cast());
            Some(path)
        }
    }
    let initial_path = source
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(documents);
    let initial = initial_path.as_deref().map(|path| {
        let text = path.to_string_lossy();
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            wide(&format!(r"\\{unc}"))
        } else {
            wide(text.strip_prefix(r"\\?\").unwrap_or(&text))
        }
    });
    let mut buffer = vec![0u16; 32768];
    let filename = wide(&name);
    buffer[..filename.len()].copy_from_slice(&filename);
    // All pointers reference live buffers for the entire synchronous modal call.
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: unsafe { GetActiveWindow() },
        lpstrFilter: filter.as_ptr(),
        nFilterIndex: 1,
        lpstrFile: buffer.as_mut_ptr(),
        nMaxFile: buffer.len() as u32,
        lpstrInitialDir: initial.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
        lpstrTitle: title.as_ptr(),
        lpstrDefExt: extension.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | if saving { 0 } else { OFN_FILEMUSTEXIST },
        ..Default::default()
    };
    // Explorer chooses a path; existing files remain protected by the writer.
    let accepted = unsafe {
        if saving {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if accepted == 0 {
        let code = unsafe { CommDlgExtendedError() };
        anyhow::ensure!(
            code == 0,
            "El selector de archivos falló (Windows 0x{code:04X})"
        );
        return Ok(None);
    }
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    let path = OsString::from_wide(&buffer[..end]);
    Ok(Some(PathBuf::from(path)))
}

#[cfg(not(windows))]
pub fn choose(_action: Action<'_>, _source: Option<&Path>) -> Result<Option<PathBuf>> {
    anyhow::bail!(
        "El selector nativo de esta entrega está disponible en Windows. Use la CLI en otros sistemas."
    )
}
