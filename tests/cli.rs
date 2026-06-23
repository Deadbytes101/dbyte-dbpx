use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn dbpx() -> &'static str {
    env!("CARGO_BIN_EXE_dbpx")
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_nanos();
    let mut dir = env::temp_dir();
    dir.push(format!("dbpx-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn version_prints_package_version() {
    let output = Command::new(dbpx())
        .arg("--version")
        .output()
        .expect("run dbpx --version");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version stdout utf8");
    assert_eq!(stdout.trim(), format!("dbpx {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn bench_prints_summary() {
    let output = Command::new(dbpx())
        .arg("bench")
        .arg("16")
        .arg("8")
        .arg("1")
        .output()
        .expect("run bench");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("bench stdout utf8");
    assert!(stdout.contains("bench: 16x8 RGB8"));
    assert!(stdout.contains("iterations: 1"));
    assert!(stdout.contains("auto-compression:"));
}

#[test]
fn encoder_flags_reject_bad_combinations() {
    let dir = temp_dir("flags");
    let ppm_path = dir.join("one.ppm");
    let dbpx_path = dir.join("one.dbpx");
    fs::write(&ppm_path, b"P6\n1 1\n255\n\x01\x02\x03").expect("write ppm");

    let conflict = Command::new(dbpx())
        .arg("enc-ppm")
        .arg(&ppm_path)
        .arg(&dbpx_path)
        .arg("--raw")
        .arg("--rle")
        .output()
        .expect("run conflict flags");
    assert!(!conflict.status.success());
    let stderr = String::from_utf8(conflict.stderr).expect("conflict stderr utf8");
    assert!(stderr.contains("cannot use multiple encoder modes together"));

    let unknown = Command::new(dbpx())
        .arg("enc-ppm")
        .arg(&ppm_path)
        .arg(&dbpx_path)
        .arg("--wat")
        .output()
        .expect("run unknown flag");
    assert!(!unknown.status.success());
    let stderr = String::from_utf8(unknown.stderr).expect("unknown stderr utf8");
    assert!(stderr.contains("unknown encoder flag"));

    fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn demo_info_dump_check_decode_pipeline() {
    let dir = temp_dir("pipeline");
    let dbpx_path = dir.join("demo.dbpx");
    let ppm_path = dir.join("demo.ppm");
    let bmp_path = dir.join("demo.bmp");

    let status = Command::new(dbpx())
        .arg("make-demo")
        .arg(&dbpx_path)
        .arg("32")
        .arg("16")
        .status()
        .expect("run make-demo");
    assert!(status.success());

    let info = Command::new(dbpx())
        .arg("info")
        .arg(&dbpx_path)
        .output()
        .expect("run info");
    assert!(info.status.success());
    let info_stdout = String::from_utf8(info.stdout).expect("info stdout utf8");
    assert!(info_stdout.contains("size: 32x16"));
    assert!(info_stdout.contains("color: RGB8"));
    assert!(info_stdout.contains("compression: raw"));

    let dump = Command::new(dbpx())
        .arg("dump")
        .arg(&dbpx_path)
        .output()
        .expect("run dump");
    assert!(dump.status.success());
    let dump_stdout = String::from_utf8(dump.stdout).expect("dump stdout utf8");
    assert!(dump_stdout.contains("header-bytes: 28"));
    assert!(dump_stdout.contains("chunks: 2"));
    assert!(dump_stdout.contains("PXLS"));
    assert!(dump_stdout.contains("END!"));
    assert!(dump_stdout.contains("crc=0x"));

    let check = Command::new(dbpx())
        .arg("check")
        .arg(&dbpx_path)
        .output()
        .expect("run check");
    assert!(check.status.success());
    let check_stdout = String::from_utf8(check.stdout).expect("check stdout utf8");
    assert!(check_stdout.contains("ok: 32x16 RGB8"));

    let status = Command::new(dbpx())
        .arg("dec-ppm")
        .arg(&dbpx_path)
        .arg(&ppm_path)
        .status()
        .expect("run dec-ppm");
    assert!(status.success());

    let ppm = fs::read(&ppm_path).expect("read ppm");
    assert!(ppm.starts_with(b"P6\n32 16\n255\n"));
    assert_eq!(ppm.len(), b"P6\n32 16\n255\n".len() + 32 * 16 * 3);

    let status = Command::new(dbpx())
        .arg("dec-bmp")
        .arg(&dbpx_path)
        .arg(&bmp_path)
        .status()
        .expect("run dec-bmp");
    assert!(status.success());

    let bmp = fs::read(&bmp_path).expect("read bmp");
    assert!(bmp.starts_with(b"BM"));
    assert_eq!(bmp.len(), 54 + 32 * 16 * 3);

    fs::remove_dir_all(dir).expect("remove temp dir");
}
