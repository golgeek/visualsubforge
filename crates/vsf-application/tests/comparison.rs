use serde::Deserialize;
use std::path::PathBuf;
use vsf_application::{
    comparison::{self, ExtraSections, Options, ReportOptions},
    subtitle_file,
};
#[derive(Deserialize)]
struct Case {
    name: String,
    extension: String,
    kinds: Vec<String>,
    options: Options,
}
#[test]
fn synthetic_fixture_classifications_word_diffs_metrics_and_json_contract() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/comparison");
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(root.join("cases.json")).unwrap()).unwrap();
    for case in cases {
        let dir = root.join(&case.name);
        let read = |name: &str| {
            subtitle_file::parse_document(
                &std::fs::read(dir.join(format!("{name}.{}", case.extension))).unwrap(),
                Some(&case.extension),
            )
            .unwrap()
        };
        let (a, b) = (read("original"), read("revised"));
        let result = comparison::compare(
            a.document.cues(),
            a.source.format(),
            b.document.cues(),
            b.source.format(),
            case.options,
            || Ok(()),
        )
        .unwrap();
        let actual = serde_json::to_value(&result).unwrap();
        let kinds: Vec<_> = actual["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["kind"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(kinds, case.kinds, "{}", case.name);
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("expected.json")).unwrap()).unwrap();
        assert_eq!(actual, expected, "{}", case.name);
        if matches!(case.name.as_str(), "hostile" | "block" | "ad-break") {
            assert_eq!(
                comparison::html(
                    &result,
                    &ReportOptions {
                        original_format: subtitle_formats::Format::Srt,
                        revised_format: subtitle_formats::Format::Srt,
                        original_name: "original.srt",
                        revised_name: "revised.srt",
                        date: "fixture",
                        include_unchanged: false
                    },
                    &ExtraSections::default()
                ),
                std::fs::read_to_string(dir.join("expected.html")).unwrap()
            );
        }
    }
}
