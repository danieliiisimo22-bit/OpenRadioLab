// SPDX-License-Identifier: GPL-3.0-only
fn main() {
    slint_build::compile_with_config(
        "ui/main.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent".into()),
    )
    .expect("No se pudo compilar la interfaz Slint");
}
