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
        "--embed-package",
    ]);
    run(&[
        "compare",
        "a.srt",
        "b.srt",
        "--html",
        "plain.html",
        "--package",
        "separate.vsfreview",
    ]);
    assert!(
        review_file::read(&d.path().join("plain.html"))
            .unwrap_err()
            .contains("doesn't contain review data")
    );
    assert!(review_file::read(&d.path().join("separate.vsfreview")).is_ok());
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

#[test]
fn show_uses_singular_for_one_change_and_one_thread() {
    let mut package = review_file::read(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vsf-application/tests/fixtures/review-v1.vsfreview"),
    )
    .unwrap();
    package.original = vsf_application::review_package::Subtitle::capture(
        "original.srt",
        subtitle_formats::Format::Srt,
        b"1\n00:00:00,000 --> 00:00:01,000\nHello there\n",
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("single.vsfreview");
    fs::write(&path, package.write().unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_vsf"))
        .args(["review", "show"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().lines().next(),
        Some("Round 1 · 1 change · 1 thread")
    );
}

fn compare_outputs(dir: &Path, package: &str, html: &str) -> i32 {
    fs::write(
        dir.join("a.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nHello\n",
    )
    .unwrap();
    Command::new(env!("CARGO_BIN_EXE_vsf"))
        .current_dir(dir)
        .args([
            "compare",
            "a.srt",
            "a.srt",
            "--package",
            package,
            "--html",
            html,
            "--embed-package",
        ])
        .output()
        .unwrap()
        .status
        .code()
        .unwrap()
}

#[test]
fn compare_outputs_existing_html_allows_corrected_retry() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("existing.html"), "existing").unwrap();
    assert_eq!(compare_outputs(root, "new.vsfreview", "existing.html"), 5);
    assert_eq!(fs::read(root.join("existing.html")).unwrap(), b"existing");
    assert!(!root.join("new.vsfreview").exists());
    assert_eq!(compare_outputs(root, "new.vsfreview", "fresh.html"), 0);
    for name in ["new.vsfreview", "fresh.html"] {
        assert_eq!(review_file::read(&root.join(name)).unwrap().round, 1);
    }
}

#[test]
fn compare_outputs_refuses_colliding_names() {
    let dir = tempfile::tempdir().unwrap();
    for alias in ["./same.html", "SAME.HTML"] {
        assert_eq!(compare_outputs(dir.path(), "same.html", alias), 2);
        assert!(!dir.path().join("same.html").exists());
        assert!(!dir.path().join(alias).exists());
    }
}
