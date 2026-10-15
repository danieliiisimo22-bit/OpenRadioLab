// SPDX-License-Identifier: GPL-3.0-only
use std::{fs, process::Command};

fn fixture() -> Vec<u8> {
    let mut bytes = vec![0xff; 8257];
    bytes[..7].copy_from_slice(b"KPG158D");
    bytes[10..15].copy_from_slice(b"V2.20");
    bytes[16..22].copy_from_slice(b"TK2402");
    bytes[1000] = 0x71;
    bytes
}

#[test]
fn cli_detects_invalid_model_and_truncation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.dat");
    fs::write(&path, [1, 2, 3]).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dat-analyzer"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("8257"));
    let mut b = fixture();
    b[16] = b'X';
    fs::write(&path, b).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dat-analyzer"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("TK2402"));
}

#[test]
fn cli_copy_creates_backup_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("original.dat");
    let output = dir.path().join("copy.dat");
    let bytes = fixture();
    fs::write(&input, &bytes).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_dat-analyzer"))
            .arg("copy")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    assert_eq!(fs::read(&input).unwrap(), bytes);
    assert_eq!(fs::read(&output).unwrap(), bytes);
    let backups = fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(backups[0].path()).unwrap(), bytes);
    assert!(!run().status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);
}

#[test]
fn cli_diff_reports_offsets_values_and_context() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.dat");
    let b = dir.path().join("b.dat");
    fs::write(&a, [1, 2, 3]).unwrap();
    fs::write(&b, [1, 4, 3, 5]).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dat-analyzer"))
        .arg("diff")
        .arg(a)
        .arg(b)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("0x0001: 02 -> 04"));
    assert!(text.contains("0x0003: -- -> 05"));
    assert!(text.contains("A @0x"));
    assert!(text.contains("B @0x"));
}
