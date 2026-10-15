// SPDX-License-Identifier: GPL-3.0-only
//! Native UI composition and callback binding. Hardware work runs off the UI thread.
mod file_dialog;
mod programming;
mod signaling_choices;
mod state;
use anyhow::{Context, Result, bail};
use radio_model::{Channel, ChannelBandwidth, Frequency, PowerLevel};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use state::Session;
use std::{
    cell::{Cell, RefCell},
    env, fs,
    io::BufWriter,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
slint::include_modules!();

fn text<T: ToString>(value: T) -> SharedString {
    value.to_string().into()
}
fn frequency(value: Option<Frequency>) -> SharedString {
    value.map(text).unwrap_or_else(|| "—".into())
}
fn unknown<T: ToString>(value: Option<T>) -> SharedString {
    value.map(text).unwrap_or_else(|| "Desconocido".into())
}

fn refresh(ui: &MainWindow, s: &Session) {
    let rows = (1..=16)
        .map(|n| {
            if let Some(c) = s.current.channels.iter().find(|c| c.number == n) {
                ChannelRow {
                    number: text(format!("{n:02}")),
                    rx: frequency(c.rx),
                    tx: frequency(c.tx),
                    decode: text(signaling_choices::label(&c.decode)),
                    encode: text(signaling_choices::label(&c.encode)),
                    power: c
                        .power
                        .map(|p| {
                            text(match p {
                                PowerLevel::High => "High",
                                PowerLevel::Low => "Low",
                            })
                        })
                        .unwrap_or_else(|| "Desconocida".into()),
                    bandwidth: c
                        .bandwidth
                        .map(|b| {
                            text(match b {
                                ChannelBandwidth::Wide => "Wide",
                                ChannelBandwidth::Narrow => "Narrow",
                            })
                        })
                        .unwrap_or_else(|| "Desconocido".into()),
                    scan: c
                        .scan_add
                        .map(|v| text(if v { "Sí" } else { "No" }))
                        .unwrap_or_else(|| "Desconocido".into()),
                    optional: unknown(c.optional_signaling.as_deref()),
                    present: true,
                }
            } else {
                ChannelRow {
                    number: text(format!("{n:02}")),
                    rx: "Vacío".into(),
                    tx: "—".into(),
                    decode: "—".into(),
                    encode: "—".into(),
                    power: "—".into(),
                    bandwidth: "—".into(),
                    scan: "—".into(),
                    optional: "—".into(),
                    present: false,
                }
            }
        })
        .collect::<Vec<_>>();
    ui.set_channels(ModelRc::new(VecModel::from(rows)));
    ui.set_dirty(s.dirty());
    ui.set_can_undo(s.can_undo());
    ui.set_can_redo(s.can_redo());
    ui.set_can_dat(s.can_dat());
    ui.set_source_path(text(
        s.source
            .as_ref()
            .map(|p| {
                p.display()
                    .to_string()
                    .trim_start_matches(r"\\?\")
                    .to_owned()
            })
            .unwrap_or_default(),
    ));
    ui.set_source_title(text(
        s.source
            .as_ref()
            .map(|p| {
                p.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.display().to_string())
            })
            .unwrap_or_else(|| "Sin archivo · modelo independiente".into()),
    ));
}

fn fill_editor(ui: &MainWindow, s: &Session, index: i32) {
    let n = (index + 1) as u8;
    ui.set_selected(index);
    let e = if let Some(c) = s.current.channels.iter().find(|c| c.number == n) {
        ChannelEditor {
            rx: c.rx.map(text).unwrap_or_default(),
            tx: c.tx.map(text).unwrap_or_default(),
            decode: text(signaling_choices::label(&c.decode)),
            encode: text(signaling_choices::label(&c.encode)),
            power: match c.power {
                None => 0,
                Some(PowerLevel::High) => 1,
                Some(PowerLevel::Low) => 2,
            },
            bandwidth: match c.bandwidth {
                None => 0,
                Some(ChannelBandwidth::Wide) => 1,
                Some(ChannelBandwidth::Narrow) => 2,
            },
            scan: match c.scan_add {
                None => 0,
                Some(true) => 1,
                Some(false) => 2,
            },
            optional: c.optional_signaling.clone().unwrap_or_default().into(),
            optional_known: c.optional_signaling.is_some(),
        }
    } else {
        ChannelEditor {
            decode: "None".into(),
            encode: "None".into(),
            ..Default::default()
        }
    };
    let (decode, decode_index) = signaling_choices::choices(
        &signaling_choices::parse(&e.decode).expect("Señalización tipada"),
    );
    let (encode, encode_index) = signaling_choices::choices(
        &signaling_choices::parse(&e.encode).expect("Señalización tipada"),
    );
    ui.set_decode_options(ModelRc::new(VecModel::from(
        decode
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    ui.set_encode_options(ModelRc::new(VecModel::from(
        encode
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    ui.set_decode_index(decode_index);
    ui.set_encode_index(encode_index);
    ui.set_editor(e);
    ui.set_form_dirty(false);
}

fn parse_editor(number: u8, e: &ChannelEditor) -> Result<Channel> {
    let parse_frequency = |s: &SharedString| -> Result<Option<Frequency>> {
        if s.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(s.as_str().parse()?))
        }
    };
    let c = Channel {
        number,
        rx: parse_frequency(&e.rx)?,
        tx: parse_frequency(&e.tx)?,
        decode: signaling_choices::parse(&e.decode).context("QT/DQT Decode")?,
        encode: signaling_choices::parse(&e.encode).context("QT/DQT Encode")?,
        power: match e.power {
            0 => None,
            1 => Some(PowerLevel::High),
            2 => Some(PowerLevel::Low),
            _ => bail!("Potencia inválida"),
        },
        bandwidth: match e.bandwidth {
            0 => None,
            1 => Some(ChannelBandwidth::Wide),
            2 => Some(ChannelBandwidth::Narrow),
            _ => bail!("Ancho inválido"),
        },
        scan_add: match e.scan {
            0 => None,
            1 => Some(true),
            2 => Some(false),
            _ => bail!("Scan Add inválido"),
        },
        optional_signaling: e.optional_known.then(|| e.optional.to_string()),
    };
    c.validate()?;
    Ok(c)
}

fn finish(ui: &MainWindow, s: &Session, result: Result<String>) {
    match result {
        Ok(message) => {
            ui.set_status(text(message));
            ui.set_error(false);
        }
        Err(e) => {
            tracing::error!(error=%e,"Operación rechazada");
            ui.set_status(text(format!("{e:#}")));
            ui.set_error(true);
        }
    }
    refresh(ui, s);
}

fn connect(ui: &MainWindow, session: &Rc<RefCell<Session>>) {
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_choose_file(move || {
        let Some(ui) = weak.upgrade() else { return };
        let source = state.borrow().source.clone();
        match file_dialog::choose(file_dialog::Action::Open, source.as_deref()) {
            Ok(Some(path)) => {
                ui.set_path(text(path.display()));
                ui.invoke_import_file(text(path.display()), ui.get_discard_authorized());
            }
            Ok(None) => {}
            Err(error) => finish(&ui, &state.borrow(), Err(error)),
        }
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_save_as(move |format| {
        let Some(ui) = weak.upgrade() else { return };
        let kind = match format {
            0 => "dat",
            1 => "json",
            2 => "csv",
            _ => return,
        };
        if ui.get_form_dirty() {
            finish(
                &ui,
                &state.borrow(),
                Err(anyhow::anyhow!("Aplique el canal antes de guardar.")),
            );
            return;
        }
        let source = if ui.get_last_saved().is_empty() {
            state.borrow().source.clone()
        } else {
            Some(PathBuf::from(ui.get_last_saved().as_str()))
        };
        match file_dialog::choose(file_dialog::Action::Save { kind }, source.as_deref()) {
            Ok(Some(path)) => ui.invoke_export_file(text(path.display()), text(kind)),
            Ok(None) => {}
            Err(error) => finish(&ui, &state.borrow(), Err(error)),
        }
    });
    let weak = ui.as_weak();
    let directory = session.borrow().data_dir.clone();
    ui.on_identify_radio(move |name|  {
        let Some(ui)  =  weak.upgrade() else  {
            return
        } ;
        if ui.get_read_busy()  {
            return;
        }
        let port  =  name.trim().to_owned();
        if port.is_empty()  {
            ui.set_status("Introduzca el puerto de la radio; use Detectar puertos para consultarlo.".into());
            ui.set_error(true);
            return;
        }
        ui.set_read_busy(true);
        ui.set_transfer_progress("Identificando…".into());
        ui.set_connection_status(text(format!("Identificando en {port}…")));
        let weak  =  ui.as_weak();
        let directory  =  directory.clone();
        std::thread::spawn(move ||  {
            let result:  Result<String>  =  (||  {
                let mut radio  =  radio_serial::tk2402::RadioSession::open(&port)?;
                radio.finish()?;
                let folder  =  directory.join("serial");
                fs::create_dir_all(&folder)?;
                let tick  =  SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                let path  =  folder.join(format!("identity-{tick}.bin"));
                dat_format::atomic_write_new(&path, &radio.identity, None)?;
                Ok(format!("Identificación real: {} bytes. Salida del modo PC confirmada. Diagnóstico privado: {}.", radio.identity.len(), path.display()))
            } )();
            if let Err(error)  =  &result  {
                tracing::error!(%error, "Identificación física falló");
            }
            let _  =  weak.upgrade_in_event_loop(move |ui|  {
                ui.set_read_busy(false);
                match result  {
                    Ok(message) =>  {
                        ui.set_connection_status("Identificación recibida · puede iniciar la lectura".into());
                        ui.set_status(text(message));
                        ui.set_error(false);
                    }
                    Err(error) =>  {
                        ui.set_connection_status("No se confirmó la identificación".into());
                        ui.set_status(text(format!("{error:#}")));
                        ui.set_error(true);
                    }
                }
            } );
        } );
    } );
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_write_radio(move |name|  {
        let Some(ui) = weak.upgrade() else  {
            return
        } ;
        if ui.get_read_busy() {
            return;
        }
        if ui.get_form_dirty() {
            finish(&ui, &state.borrow(), Err(anyhow::anyhow!("Aplique el canal antes de escribir en la radio.")));
            return;
        }
        if state.borrow().current.channels.is_empty() {
            finish(&ui, &state.borrow(), Err(anyhow::anyhow!("Lea la radio o abra una configuración antes de escribir.")));
            return;
        }
        let port = name.trim().to_owned();
        if port.is_empty() {
            return;
        }
        let desired = state.borrow().current.clone();
        let directory = state.borrow().data_dir.join("serial");
        ui.set_read_busy(true);
        ui.set_error(false);
        ui.set_read_bytes(0);
        ui.set_connection_status("Preparando respaldo antes de escribir…".into());
        let progress_ui = ui.as_weak();
        let completion_ui = ui.as_weak();
        std::thread::spawn(move || {
            let result = programming::program(&port, &desired, &directory, |stage, done, total| {
                let message = format!("{stage} · {done} / {total}");
                let percent = (done*8192).checked_div(total).unwrap_or(0);
                let _ = progress_ui.upgrade_in_event_loop(move |ui| {
                    ui.set_read_bytes(percent as i32);
                    ui.set_transfer_progress(text(&message));
                    ui.set_connection_status(text(message));
                } );
            } );
            let _ = completion_ui.upgrade_in_event_loop(move |ui| {
                ui.set_read_busy(false);
                match result  {
                    Ok(result)=> {
                        ui.invoke_import_file(text(result.path.display()), true);
                        if !ui.get_error() {
                            ui.set_connection_status("Escritura verificada · salida confirmada".into());
                            ui.set_status(text(if result.blocks==0  {
                                "La radio ya tenía esa configuración. No se enviaron bloques de escritura.".to_owned()
                            } else {
                                format!("Escritura verificada: {} bytes modificados en {} bloques. Relectura completa idéntica al plan.", result.changed_bytes, result.blocks)
                            } ));
                            ui.set_last_saved(text(result.path.display()));
                        }
                    }
                    Err(error)=> {
                        ui.set_connection_status("Escritura no confirmada; revise el informe".into());
                        ui.set_status("Operación detenida. Se conservan el respaldo y los cambios del editor.".into());
                        ui.set_details(text(format!("{error:#}")));
                        ui.set_error(true);
                    }
                }
            } );
        } );
    } );
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_read_radio(move |name|  {
        let Some(ui) = weak.upgrade() else  {
            return
        } ;
        if ui.get_read_busy()  {
            return;
        }
        if state.borrow().dirty() || ui.get_form_dirty()  {
            finish(&ui, &state.borrow(), Err(anyhow::anyhow!("Guarde sus cambios o restablezca el formulario antes de leer la radio.")));
            return;
        }
        let port = name.trim().to_owned();
        if port.is_empty() {
            return;
        }
        ui.set_read_busy(true);
        ui.set_read_bytes(0);
        ui.set_error(false);
        ui.set_connection_status(text(format!("Leyendo la radio en {port}…")));
        let directory = state.borrow().data_dir.join("serial");
        let progress_ui = ui.as_weak();
        let completion_ui = ui.as_weak();
        std::thread::spawn(move ||  {
            let result: Result<PathBuf>=(|| {
                fs::create_dir_all(&directory)?;
                let tick = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                let prefix = directory.join(format!("read-{tick}"));
                let capture = radio_serial::tk2402::capture(&port, |count| {
                    let _ = progress_ui.upgrade_in_event_loop(move |ui| {
                        ui.set_read_bytes(count as i32);
                        ui.set_transfer_progress(text(format!("{count} / 8192 bytes · {} / 64 bloques", count/128)));
                        ui.set_connection_status(text(format!("Leyendo… {} / 64 bloques", count/128)));
                    } );
                } );
                let diagnostic = prefix.with_extension("diagnostic.json");
                dat_format::atomic_write_new(&diagnostic, &serde_json::to_vec_pretty(&capture.diagnostic)?, None)?;
                if !capture.identity.is_empty() {
                    dat_format::atomic_write_new(&prefix.with_extension("identity.bin"), &capture.identity, None)?;
                }
                if let Some(image) = &capture.image  {
                    dat_format::atomic_write_new(&prefix.with_extension("image.bin"), image, None)?;
                }
                if let Some(error) = &capture.diagnostic.error  {
                    bail!("{error}. Diagnóstico: {}", diagnostic.display());
                }
                let image = capture.image.context("No se recibió una imagen completa")?;
                let document = dat_format::DatDocument::from_radio_image(&image)?;
                let path = prefix.with_extension("dat");
                dat_format::atomic_write_new(&path, document.original_bytes(), None)?;
                Ok(path)
            } )();
            let _ = completion_ui.upgrade_in_event_loop(move |ui| {
                ui.set_read_busy(false);
                match result  {
                    Ok(path)=> {
                        ui.invoke_import_file(text(path.display()), false);
                        if !ui.get_error() {
                            ui.set_connection_status("Lectura completa · 8192 bytes · salida confirmada".into());
                            ui.set_status("Configuración leída de la radio. Respaldo DAT guardado; canales cargados.".into());
                            ui.set_last_saved(text(path.display()));
                        }
                    }
                    Err(error)=> {
                        ui.set_connection_status("Lectura detenida; revise el resultado".into());
                        ui.set_status("Lectura detenida. El diagnóstico aparece en el registro.".into());
                        ui.set_details(text(format!("{error:#}")));
                        ui.set_error(true);
                    }
                }
            } );
        } );
    } );
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_import_file(move |path, allowed| {
        let Some(ui) = weak.upgrade() else {
            return
        } ;
        if ui.get_read_busy() {
            return;
        }
        let result = (|| {
            if ui.get_form_dirty() && !allowed  {
                bail!("Aplique o restablezca el formulario antes de importar.");
            }
            state.borrow_mut().load(Path::new(path.trim()), allowed)?;
            ui.set_active_section(0);
            ui.set_discard_authorized(false);
            ui.set_last_saved("".into());
            ui.set_save_format(if state.borrow().document.is_some() {
                0
            } else {
                1
            } );
            fill_editor(&ui, &state.borrow(), 0);
            if let Some(doc) = &state.borrow().document  {
                ui.set_details(text(doc.warnings.join("\n")));
            } else {
                ui.set_details("Modelo JSON/CSV importado con validación. Sin contenedor DAT asociado. Compatibilidad del catálogo QT/DQT con cada variante aún pendiente de confirmar.".into());
            }
            Ok("Archivo importado. Respaldo de recuperación guardado.".into())
        } )();
        finish(&ui, &state.borrow(), result);
    } );
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_export_file(move |path, kind| {
        let Some(ui) = weak.upgrade() else { return };
        let result = (|| {
            if ui.get_form_dirty() {
                bail!("Aplique el formulario antes de guardar.");
            }
            state
                .borrow_mut()
                .export(Path::new(path.trim()), kind.as_str())?;
            ui.set_last_saved(text(path.trim()));
            Ok(format!(
                "{} guardado. Se exige un nombre nuevo en cada exportación.",
                kind.to_uppercase()
            ))
        })();
        finish(&ui, &state.borrow(), result);
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_select_channel(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_form_dirty() {
            ui.set_status("Aplique o restablezca el formulario antes de cambiar de canal.".into());
            ui.set_error(true);
            return;
        }
        if (0..16).contains(&index) {
            fill_editor(&ui, &state.borrow(), index);
        }
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_apply_editor(move || {
        let Some(ui) = weak.upgrade() else { return };
        let result = (|| {
            let c = parse_editor((ui.get_selected() + 1) as u8, &ui.get_editor())?;
            let number = c.number;
            state.borrow_mut().upsert(c)?;
            fill_editor(&ui, &state.borrow(), ui.get_selected());
            Ok(format!(
                "Canal {number} aplicado. Guarde la configuración desde Archivos."
            ))
        })();
        finish(&ui, &state.borrow(), result);
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_remove_channel(move || {
        let Some(ui) = weak.upgrade() else { return };
        let result = (|| {
            if ui.get_form_dirty() {
                bail!("Aplique o restablezca el formulario antes de eliminar.");
            }
            state.borrow_mut().remove((ui.get_selected() + 1) as u8)?;
            fill_editor(&ui, &state.borrow(), ui.get_selected());
            Ok("Canal eliminado del modelo. Puede deshacer.".into())
        })();
        finish(&ui, &state.borrow(), result);
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_undo(move || {
        let Some(ui) = weak.upgrade() else { return };
        let result = (|| {
            if ui.get_form_dirty() {
                bail!("Aplique o restablezca el formulario.");
            }
            state.borrow_mut().undo()?;
            fill_editor(&ui, &state.borrow(), ui.get_selected());
            Ok("Cambio deshecho.".into())
        })();
        finish(&ui, &state.borrow(), result);
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_redo(move || {
        let Some(ui) = weak.upgrade() else { return };
        let result = (|| {
            if ui.get_form_dirty() {
                bail!("Aplique o restablezca el formulario.");
            }
            state.borrow_mut().redo()?;
            fill_editor(&ui, &state.borrow(), ui.get_selected());
            Ok("Cambio rehecho.".into())
        })();
        finish(&ui, &state.borrow(), result);
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.on_ports(move || {
        let Some(ui) = weak.upgrade() else { return };
        let result = radio_serial::discover_ports()
            .map(|ports| {
                let names = ports.iter().map(|p| text(&p.name)).collect::<Vec<_>>();
                let previous_port = ui.get_port_options().row_data(ui.get_port_index() as usize);
                let index = previous_port
                    .and_then(|name| ports.iter().position(|port| port.name == name.as_str()))
                    .unwrap_or(0) as i32;
                ui.set_port_options(ModelRc::new(VecModel::from(names)));
                ui.set_port_index(index);
                ui.set_details(text(if ports.is_empty() {
                    "No hay puertos seriales visibles en este sistema.".into()
                } else {
                    ports
                        .iter()
                        .map(|p| {
                            format!(
                                "{} · {} · VID/PID {}/{}",
                                p.name,
                                p.adapter,
                                p.usb_vid
                                    .map(|v| format!("{v:04X}"))
                                    .unwrap_or_else(|| "—".into()),
                                p.usb_pid
                                    .map(|v| format!("{v:04X}"))
                                    .unwrap_or_else(|| "—".into())
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }));
                format!(
                    "{} puertos enumerados. No se ha abierto ninguno.",
                    ports.len()
                )
            })
            .map_err(anyhow::Error::from);
        finish(&ui, &state.borrow(), result);
    });
    let allow_close = Rc::new(Cell::new(false));
    let allowed = allow_close.clone();
    let closing_ui = ui.as_weak();
    ui.on_close_discard(move || {
        if let Some(ui) = closing_ui.upgrade()
            && ui.get_read_busy()
        {
            ui.set_status("Espere a que termine la identificación antes de cerrar.".into());
            return;
        }
        allowed.set(true);
        let _ = slint::quit_event_loop();
    });
    let weak = ui.as_weak();
    let state = session.clone();
    ui.window().on_close_requested(move || {
        if !allow_close.get() && let Some(ui) = weak.upgrade() && (state.borrow().dirty() || ui.get_form_dirty())  {
            ui.set_status("Hay cambios sin guardar. Guarde JSON/CSV o use Cerrar descartando cambios. Los cambios aplicados tienen recuperación automática.".into());
            ui.set_error(true);
            return slint::CloseRequestResponse::KeepWindowShown;
        }
        if let Some(ui)  =  weak.upgrade() && ui.get_read_busy()  {
            ui.set_status("Espere a que termine la identificación antes de cerrar.".into());
            return slint::CloseRequestResponse::KeepWindowShown;
        }
        slint::CloseRequestResponse::HideWindow
    } );
}

fn save_snapshot(ui: &MainWindow, path: &Path) -> Result<()> {
    let pixels = ui.window().take_snapshot()?;
    let file = fs::File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), pixels.width(), pixels.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels.as_bytes())?;
    writer.finish()?;
    Ok(())
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let mut input = None;
    let mut snapshot = None;
    let mut smoke = false;
    let mut light = false;
    let mut view = None;
    let mut data_dir = env::current_exe()?
        .parent()
        .context("Sin directorio ejecutable")?
        .join("open-radio-lab-data");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => data_dir = PathBuf::from(args.next().context("Falta --data-dir")?),
            "--snapshot" => {
                snapshot = Some(PathBuf::from(args.next().context("Falta ruta snapshot")?))
            }
            "--smoke-test" => smoke = true,
            "--light" => light = true,
            "--view" => {
                view = Some(match args.next().context("Falta vista")?.as_str() {
                    "channels" => 0,
                    "files" => 1,
                    "connection" => 2,
                    _ => bail!("Vista: channels, files o connection"),
                });
            }
            "--help" => {
                println!(
                    "radio-app [archivo.dat/json/csv] [--data-dir directorio] [--light] [--view channels/files/connection] [--smoke-test] [--snapshot PNG]\nDAT editable con campos admitidos. Identificación y lectura serial solo por acción explícita."
                );
                return Ok(());
            }
            _ if !arg.starts_with('-') && input.is_none() => input = Some(PathBuf::from(arg)),
            _ => bail!("Argumento desconocido: {arg}"),
        }
    }
    if data_dir.is_relative() {
        data_dir = env::current_dir()?.join(data_dir);
    }
    fs::create_dir_all(data_dir.join("diagnostics"))?;
    let tick = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let logfile = fs::File::create(
        data_dir
            .join("diagnostics")
            .join(format!("session-{tick}.log")),
    )?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(Mutex::new(logfile))
        .init();
    let ui = MainWindow::new()?;
    ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    if light {
        ui.invoke_toggle_theme();
    }
    let session = Rc::new(RefCell::new(Session::new(data_dir)));
    connect(&ui, &session);
    refresh(&ui, &session.borrow());
    fill_editor(&ui, &session.borrow(), 0);
    let initial_view = if input.is_some() { 0 } else { 1 };
    if let Some(path) = input {
        ui.set_path(text(path.display()));
        ui.invoke_import_file(text(path.display()), false);
        if smoke && ui.get_error() {
            bail!("Importación de smoke test falló: {}", ui.get_status());
        }
    }
    if smoke {
        // Real GUI callbacks exercise the editor, validation, undo and redo.
        let start = session.borrow().current.clone();
        let mut e = ui.get_editor();
        e.rx = "145.50000".into();
        e.tx = "145.60000".into();
        e.decode = "QT 94.8".into();
        e.encode = "DQT 023N".into();
        ui.set_editor(e);
        ui.invoke_apply_editor();
        if ui.get_error() {
            bail!("Apply falló: {}", ui.get_status());
        }
        let changed = session.borrow().current.clone();
        if changed == start {
            bail!("Smoke no ejercitó una edición");
        }
        if session.borrow().document.is_some() {
            let refused = session.borrow().data_dir.join("smoke-changed.dat");
            ui.invoke_export_file(text(refused.display()), "dat".into());
            if !ui.get_error() || refused.exists() || ui.get_can_dat() {
                bail!("La GUI permitió exportar un DAT editado sin verificar");
            }
        }
        ui.invoke_undo();
        if session.borrow().current != start {
            bail!("Undo GUI falló");
        }
        ui.invoke_redo();
        if session.borrow().current != changed {
            bail!("Redo GUI falló");
        }
        ui.invoke_undo();
        if session.borrow().current != start {
            bail!("Restauración GUI falló");
        }
        // Invalid form must leave the applied model intact.
        let mut invalid = ui.get_editor();
        invalid.rx = "999.0".into();
        ui.set_editor(invalid);
        ui.invoke_apply_editor();
        if !ui.get_error() || session.borrow().current != start {
            bail!("La GUI aceptó frecuencia inválida");
        }
        fill_editor(&ui, &session.borrow(), 0);
        let tick = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let qa_dir = session
            .borrow()
            .data_dir
            .join("smoke")
            .join(tick.to_string());
        fs::create_dir_all(&qa_dir)?;
        if session.borrow().document.is_some() && !start.channels.is_empty() {
            let mut e = ui.get_editor();
            e.rx = "145.50000".into();
            e.tx = "145.60000".into();
            e.decode = "QT 100.0".into();
            e.encode = "QT 100.0".into();
            e.power = 1;
            e.bandwidth = 1;
            e.scan = 2;
            ui.set_editor(e);
            ui.invoke_apply_editor();
            let edited = session.borrow().current.clone();
            let path = qa_dir.join("edited.dat");
            ui.invoke_export_file(text(path.display()), "dat".into());
            if ui.get_error() || dat_format::DatDocument::open(&path)?.configuration() != &edited {
                bail!("Round-trip DAT editado de GUI falló: {}", ui.get_status());
            }
            ui.invoke_undo();
            if session.borrow().current != start {
                bail!("Restauración después de DAT editado falló");
            }
        }
        for kind in ["json", "csv"] {
            let path = qa_dir.join(format!("roundtrip.{kind}"));
            ui.invoke_export_file(text(path.display()), text(kind));
            if ui.get_error() || state::load_configuration(&path)?.0 != start {
                bail!("Round-trip GUI {kind} falló: {}", ui.get_status());
            }
        }
        let original_snapshot = session.borrow().document.as_ref().map(|doc| {
            (
                doc.original_bytes().to_vec(),
                doc.source().map(Path::to_path_buf),
            )
        });
        if let Some((original_bytes, original_source)) = original_snapshot {
            let path = qa_dir.join("copy.dat");
            ui.invoke_export_file(text(path.display()), "dat".into());
            if ui.get_error() || fs::read(&path)? != original_bytes {
                bail!("La copia DAT de la GUI no coincide");
            }
            if let Some(original) = original_source
                && fs::read(original)? != original_bytes
            {
                bail!("El original DAT fue alterado");
            }
        }
        ui.set_error(false);
        ui.set_status(
            "Verificación GUI: editar, validar, deshacer y rehacer correctos. Sin operación física."
                .into(),
        );
    }
    ui.set_active_section(view.unwrap_or(initial_view));
    let snapshot_result = Rc::new(RefCell::new(None));
    if smoke || snapshot.is_some() {
        let w = ui.as_weak();
        let result = snapshot_result.clone();
        slint::Timer::single_shot(Duration::from_millis(600), move || {
            if let Some(ui) = w.upgrade() {
                let outcome = if let Some(path) = snapshot {
                    save_snapshot(&ui, &path)
                } else {
                    Ok(())
                };
                *result.borrow_mut() = Some(outcome.map_err(|e| e.to_string()));
            }
            let _ = slint::quit_event_loop();
        });
    }
    ui.run()?;
    if let Some(Err(error)) = snapshot_result.borrow_mut().take() {
        bail!("Snapshot GUI falló: {error}");
    }
    if smoke {
        println!(
            "PASS: Slint nativo inició sin radio; callbacks editor, validación, undo/redo y render ejecutados."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radio_model::SquelchSignaling;
    #[test]
    fn editor_validates_and_preserves_independent_tx() {
        let e = ChannelEditor {
            rx: "145.5".into(),
            tx: "146.6".into(),
            decode: "QT 94.8".into(),
            encode: "DQT 023N".into(),
            ..Default::default()
        };
        let c = parse_editor(1, &e).unwrap();
        assert_ne!(c.rx, c.tx);
        assert!(matches!(c.decode, SquelchSignaling::Ctcss(_)));
        assert!(matches!(c.encode, SquelchSignaling::Dcs(_)));
        let mut invalid = e;
        invalid.rx = "175.0".into();
        assert!(parse_editor(1, &invalid).is_err());
    }
}
