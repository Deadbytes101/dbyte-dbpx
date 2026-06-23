use std::process::Command;

fn corpus() -> &'static str {
    env!("CARGO_BIN_EXE_dbpx-corpus")
}

#[test]
fn corpus_prints_codec_winners() {
    let output = Command::new(corpus()).output().expect("run dbpx-corpus");
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("corpus stdout utf8");
    assert!(stdout.contains("case,width,height,raw,rle,indexed,auto,winner,decoded"));
    assert!(stdout.contains("gradient,"));
    assert!(stdout.contains("solid,"));
    assert!(stdout.contains("flat2,"));
    assert!(stdout.contains("stripes,"));
    assert!(stdout.contains(",raw,"));
    assert!(stdout.contains(",dbpx-rle,"));
    assert!(stdout.contains(",dbpx-indexed,"));
}
