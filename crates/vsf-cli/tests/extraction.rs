use std::{path::PathBuf, process::Command};
#[test]
#[ignore = "requires synthetic media-fixtures.py corpus and FFmpeg"]
fn lists_extracts_and_refuses_input_overwrite() {
    let video = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/generated/subtitle-extraction/offset.mkv")
        .canonicalize()
        .unwrap();
    let listing = Command::new(env!("CARGO_BIN_EXE_vsf"))
        .args(["extract", "--list", "--json"])
        .arg(&video)
        .output()
        .unwrap();
    assert!(
        listing.status.success(),
        "{}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let listing: serde_json::Value = serde_json::from_slice(&listing.stdout).unwrap();
    assert_eq!(listing["schema_version"], 1);
    assert_eq!(listing["tracks"][0]["index"], 2);
    assert_eq!(listing["tracks"][0]["language"], "fra");
    let original = std::fs::read(&video).unwrap();
    let refusal = Command::new(env!("CARGO_BIN_EXE_vsf"))
        .arg("extract")
        .arg(&video)
        .args(["--track", "2", "-o"])
        .arg(&video)
        .output()
        .unwrap();
    assert!(!refusal.status.success());
    assert_eq!(std::fs::read(&video).unwrap(), original);
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("new.srt");
    let extraction = Command::new(env!("CARGO_BIN_EXE_vsf"))
        .arg("extract")
        .arg(&video)
        .args(["--track", "2", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        extraction.status.success(),
        "{}",
        String::from_utf8_lossy(&extraction.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "1\n00:00:01,000 --> 00:00:02,000\nFirst\n\n2\n00:00:03,000 --> 00:00:04,000\nSecond\n\n"
    );
}
