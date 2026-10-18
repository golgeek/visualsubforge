use std::{fs, process::Command};
use subtitle_formats::Format;
use vsf_application::{quality, subtitle_file};

#[test]
fn check_and_stats_decode_webvtt_text_match_shared_reports_and_preserve_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.vtt");
    let source = "\u{feff}WEBVTT\r\n\r\ncaption,1\r\n00:01.001 --> 00:04.999 line:10%,start\r\n<v Misspelt><lang fr>Été &amp; &#x1F600; <ruby>字<rt>じ</rt></ruby></lang></v>\r\n{\\i1}literal\\N\r\n";
    fs::write(&path, source).unwrap();
    let parsed = subtitle_file::parse_document(source.as_bytes(), Some("vtt")).unwrap();
    let profile = quality::Profile::default();
    let expected = quality::evaluate(
        &parsed.document,
        quality::Evaluation::from(&profile).with_format(Format::WebVtt),
        |_, _| Ok(()),
    )
    .unwrap();
    let lines = ["Été & 😀 字", "{\\i1}literal\\N"];
    assert_eq!(
        expected.metrics[0].characters,
        lines
            .iter()
            .map(|line| line.encode_utf16().count() as u32)
            .sum::<u32>()
            + 2
    );
    assert_eq!(
        expected.text_interpretation.as_deref(),
        Some("webvtt_utf16_v1")
    );
    let stats = quality::statistics::summarize(
        expected.clone(),
        quality::statistics::Selection::default(),
        || Ok(()),
    )
    .unwrap();
    for (command, expected) in [
        ("check", serde_json::to_value(expected).unwrap()),
        ("stats", serde_json::to_value(stats).unwrap()),
    ] {
        for json in [true, false] {
            let mut process = Command::new(env!("CARGO_BIN_EXE_vsf"));
            process.args([
                command,
                "--subtitles",
                path.to_str().unwrap(),
                "--profile",
                "vsf-default",
            ]);
            if json {
                process.arg("--json");
            }
            let output = process
                .env("VSF_FFMPEG", "/missing-tools")
                .env("VSF_FFPROBE", "/missing-tools")
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(3), "{output:?}");
            assert!(output.stderr.is_empty());
            if json {
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
                    expected
                );
            } else {
                assert!(
                    String::from_utf8(output.stdout)
                        .unwrap()
                        .contains("Text interpretation: webvtt_utf16_v1")
                );
            }
            assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}
