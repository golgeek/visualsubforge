use std::{fs, path::Path, process::Command};
use vsf_application::{review_file, review_package::Thread};
#[test]
fn compare_show_and_concurrent_reviewers_in_both_orders() {
    let d = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_vsf"))
            .current_dir(d.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    };
    for file in ["a.srt", "b.srt"] {
        fs::write(
            d.path().join(file),
            "1\n00:00:00,000 --> 00:00:01,000\nHello\n",
        )
        .unwrap()
    }
    run(&[
        "compare",
        "a.srt",
        "b.srt",
        "--package",
        "a.vsfreview",
        "--html",
        "a.html",
    ]);
    let show = run(&["review", "show", "a.html", "--json"]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&show).unwrap()["round"],
        1
    );
    let mut p = review_file::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vsf-application/tests/fixtures/review-v1.vsfreview")
            .as_path(),
    )
    .unwrap();
    let anchor = p.threads[0].anchor.clone();
    fs::write(d.path().join("first"), p.write().unwrap()).unwrap();
    p.threads = vec![Thread::new(anchor, "Sam".into(), 11, "Agree".into()).unwrap()];
    fs::write(d.path().join("second"), p.write().unwrap()).unwrap();
    for (a, b, out) in [("first", "second", "ab"), ("second", "first", "ba")] {
        run(&["review", "merge", a, b, "-o", out]);
    }
    assert_eq!(
        fs::read(d.path().join("ab")).unwrap(),
        fs::read(d.path().join("ba")).unwrap()
    );
    assert_eq!(
        review_file::read(&d.path().join("ba"))
            .unwrap()
            .threads
            .len(),
        2
    );
}
