//! Exercise real process leases and abrupt death; thread-local drops cannot
//! establish either property. Every child uses a disposable test directory.
use super::*;
use std::{
    io::{BufRead, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const CHILD_DIRECTORY: &str = "VSF_RECOVERY_PROCESS_TEST_DIRECTORY";
const TEST_NAME: &str =
    "recovery_store::process_tests::process_death_releases_lease_and_preserves_last_commit";
const ORIGINAL: &str = "1\n00:00:01,000 --> 00:00:02,000\ncafé 日本語\n\n";

// A failed assertion must not leave a blocked child or a live lease behind.
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn child_writer(directory: &Path) {
    let session = Session::create(&directory.join("recovery")).unwrap();
    let mut record = Record::new(
        directory.join("source.srt").to_string_lossy().into_owned(),
        ORIGINAL.into(),
        ORIGINAL.replace("café", "accepted"),
        1,
        7,
    )
    .unwrap();
    session.prepare(&record).unwrap().commit().unwrap();
    record.accepted_srt = ORIGINAL.replace("café", "not published");
    record.revision = 8;
    // Keep a fully flushed replacement alive across interruption. It must never
    // become the recovered record without the publication/version check.
    let pending = session.prepare(&record).unwrap();
    crate::atomic_file::write(
        &directory.join("ready"),
        session.directory.file_name().unwrap().as_encoded_bytes(),
    )
    .unwrap();
    let mut command = String::new();
    std::io::stdin().lock().read_line(&mut command).unwrap();
    assert_eq!(command.trim(), "clear");
    drop(pending);
    session.clear().unwrap();
    // Normal destruction also verifies closing the lease before unlinking it.
}

fn start_writer(directory: &Path) -> (OwnedChild, String) {
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_DIRECTORY, directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(id) = fs::read_to_string(directory.join("ready")) {
            return (child, id);
        }
        assert!(child.0.try_wait().unwrap().is_none(), "writer exited early");
        assert!(
            Instant::now() < deadline,
            "writer did not publish readiness"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn process_death_releases_lease_and_preserves_last_commit() {
    // Re-enter only this test in the child, without exposing test operations in
    // production IPC or adding a helper executable to the shipped application.
    if let Some(directory) = std::env::var_os(CHILD_DIRECTORY) {
        child_writer(Path::new(&directory));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    for attempt in 0..4 {
        let directory = temp.path().join(attempt.to_string());
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("source.srt"), ORIGINAL).unwrap();
        let root = directory.join("recovery");
        let (mut child, id) = start_writer(&directory);
        let live = Session::create(&root).unwrap();
        let other = Record::new(
            directory.join("source.srt").to_string_lossy().into_owned(),
            ORIGINAL.into(),
            ORIGINAL.replace("café", "other live editor"),
            2,
            1,
        )
        .unwrap();
        live.prepare(&other).unwrap().commit().unwrap();
        assert!(list(&root).unwrap().is_empty());
        assert!(recover(&root, &id).is_err());
        assert!(discard(&root, &id).is_err());

        if attempt == 3 {
            writeln!(child.0.stdin.take().unwrap(), "clear").unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                assert!(Instant::now() < deadline, "writer did not exit");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!root.join(&id).exists());
        } else {
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let entries = list(&root).unwrap();
            assert_eq!(entries.len(), 1); // Other process's live record stays hidden.
            assert_eq!(entries[0].id, id);
            let record = recover(&root, &id).unwrap().record;
            assert_eq!(record.revision, 7);
            assert_eq!(record.accepted_srt, ORIGINAL.replace("café", "accepted"));
            assert_eq!(record.saved_srt, ORIGINAL);
            assert_eq!(recover(&root, &id).unwrap().record.revision, 7); // Non-consuming.
            discard(&root, &id).unwrap();
            assert!(!root.join(&id).join("snapshot.json").exists());
        }
        assert!(list(&root).unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(directory.join("source.srt")).unwrap(),
            ORIGINAL
        );
        live.clear().unwrap();
        let live_directory = live.directory.clone();
        drop(live);
        assert!(!live_directory.exists());
    }
}
