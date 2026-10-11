use super::*;
use crate::project::tests::fixture;

fn attach(projects: &Projects, path: &Path) -> u64 {
    let opened = projects.open(path).unwrap();
    projects.attach(opened.token).unwrap().token
}

#[test]
fn stale_writer_preserves_newer_metadata_and_can_save_a_separate_copy() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(directory.path());
    let path = directory.path().join("shared.vsfproject");
    write(&path, &original).unwrap();
    let a = Projects::default();
    let b = Projects::default();
    let a_token = attach(&a, &path);
    let b_token = attach(&b, &path);
    let mut newer = original.clone();
    newer.options.timing.enabled = true;
    newer.position_seconds = 42.;
    b.save(&path, Some(b_token), &newer).unwrap();
    let saved = fs::read(&path).unwrap();
    assert!(
        a.save(&path, Some(a_token), &original)
            .err()
            .unwrap()
            .contains("changed on disk")
    );
    assert_eq!(fs::read(&path).unwrap(), saved);
    let copy = directory.path().join("my-copy.vsfproject");
    let copied = a.save(&copy, Some(a_token), &original).unwrap();
    assert_eq!(
        read(&copy).unwrap().position_seconds,
        original.position_seconds
    );
    assert_eq!(fs::read(&path).unwrap(), saved);
    assert!(a.save(&copy, Some(a_token), &original).is_err());
    a.save(&copy, Some(copied.token), &newer).unwrap();
    let reloaded = attach(&a, &path);
    a.save(&path, Some(reloaded), &newer).unwrap();
}

#[test]
fn cancelled_or_failed_open_preserves_active_save_identity_and_stale_attach_fails() {
    let directory = tempfile::tempdir().unwrap();
    let project = fixture(directory.path());
    let a = directory.path().join("active.vsfproject");
    let b = directory.path().join("pending.vsfproject");
    write(&a, &project).unwrap();
    write(&b, &project).unwrap();
    let projects = Projects::default();
    let active = attach(&projects, &a);
    let pending = projects.open(&b).unwrap();
    assert!(
        projects
            .open(&directory.path().join("missing.vsfproject"))
            .is_err()
    );
    let saved = projects.save(&a, Some(active), &project).unwrap();
    assert!(projects.attach(pending.token).is_err());
    projects.save(&a, Some(saved.token), &project).unwrap();
    let obsolete = projects.open(&b).unwrap();
    let newest = projects.open(&b).unwrap();
    assert!(projects.attach(obsolete.token).is_err());
    projects.attach(newest.token).unwrap();
    projects.detach().unwrap();
    assert!(projects.save(&b, Some(newest.token), &project).is_err());
}

#[test]
fn concurrent_instances_cannot_both_replace_the_same_loaded_version() {
    let directory = tempfile::tempdir().unwrap();
    let original = fixture(directory.path());
    let path = directory.path().join("shared.vsfproject");
    write(&path, &original).unwrap();
    let a = Projects::default();
    let b = Projects::default();
    let a_token = attach(&a, &path);
    let b_token = attach(&b, &path);
    let barrier = std::sync::Barrier::new(2);
    let mut newer = original.clone();
    newer.position_seconds = 42.;
    let results = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            a.save(&path, Some(a_token), &newer)
        });
        let b = scope.spawn(|| {
            barrier.wait();
            b.save(&path, Some(b_token), &newer)
        });
        [a.join().unwrap().is_ok(), b.join().unwrap().is_ok()]
    });
    assert_eq!(results.into_iter().filter(|success| *success).count(), 1);
    assert_eq!(read(&path).unwrap().position_seconds, 42.);
}

#[test]
fn failed_write_and_external_deletion_preserve_the_attachment() {
    let directory = tempfile::tempdir().unwrap();
    let project = fixture(directory.path());
    let path = directory.path().join("saved.vsfproject");
    write(&path, &project).unwrap();
    let projects = Projects::default();
    let token = attach(&projects, &path);
    let original = fs::read(&path).unwrap();
    let mut invalid = project.clone();
    invalid.position_seconds = -1.;
    assert!(projects.save(&path, Some(token), &invalid).is_err());
    assert!(
        projects
            .save(
                &directory.path().join("missing/new.vsfproject"),
                Some(token),
                &project
            )
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_file(&path).unwrap();
    assert!(projects.save(&path, Some(token), &project).is_err());
    assert!(!path.exists());
    fs::write(&path, &original).unwrap();
    projects.save(&path, Some(token), &project).unwrap();
}

#[test]
fn sidecar_lock_excludes_another_process_and_releases_after_owner_drop() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shared.vsfproject");
    let lease = save_lease(&path).unwrap();
    let child = |expected| {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "project::store::tests::project_save_lock_child",
                "--ignored",
            ])
            .env("VSF_PROJECT_LOCK_TEST_PATH", &path)
            .env("VSF_PROJECT_LOCK_TEST_EXPECT", expected)
            .output()
            .unwrap()
    };
    let locked = child("locked");
    assert!(
        locked.status.success(),
        "{}",
        String::from_utf8_lossy(&locked.stdout)
    );
    drop(lease);
    let released = child("available");
    assert!(
        released.status.success(),
        "{}",
        String::from_utf8_lossy(&released.stdout)
    );
}

#[test]
#[ignore = "Subprocess entry point for sidecar lock ownership regression"]
fn project_save_lock_child() {
    let path = PathBuf::from(
        std::env::var_os("VSF_PROJECT_LOCK_TEST_PATH").expect("parent supplies path"),
    );
    let expected = std::env::var("VSF_PROJECT_LOCK_TEST_EXPECT").unwrap();
    assert_eq!(save_lease(&path).is_ok(), expected == "available");
}

#[cfg(unix)]
#[test]
fn symlink_aliases_share_the_canonical_save_identity() {
    let directory = tempfile::tempdir().unwrap();
    let project = fixture(directory.path());
    let path = directory.path().join("real.vsfproject");
    let alias = directory.path().join("alias.vsfproject");
    write(&path, &project).unwrap();
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    let projects = Projects::default();
    let token = attach(&projects, &alias);
    let mut external = project.clone();
    external.position_seconds = 42.;
    write(&path, &external).unwrap();
    assert!(projects.save(&alias, Some(token), &project).is_err());
    assert_eq!(read(&path).unwrap().position_seconds, 42.);
    assert!(alias.is_symlink());
}
