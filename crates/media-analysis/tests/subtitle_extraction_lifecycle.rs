//! Controlled subprocesses exercise extraction admission and cleanup, not media decoding.
#[cfg(unix)]
#[test]
fn cancellation_and_timeout_reap_the_extraction_child_without_a_result() {
    use media_analysis::{Cancellation, Error, Tools, subtitle_tracks};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    for cancel in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let probe = directory.path().join("probe");
        fs::write(&probe, "#!/bin/sh\nprintf '%s' '{\"streams\":[{\"index\":2,\"codec_type\":\"subtitle\",\"codec_name\":\"subrip\"}]}'\n").unwrap();
        let decoder = directory.path().join("decoder");
        fs::write(
            &decoder,
            "#!/bin/sh\nprintf '%s' \"$$\" > \"$(dirname \"$0\")/child.pid\"\nexec sleep 60\n",
        )
        .unwrap();
        for script in [&probe, &decoder] {
            fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let input = directory.path().join("synthetic.mkv");
        fs::write(&input, []).unwrap();
        let tools = Tools {
            ffprobe: probe,
            ffmpeg: decoder,
        };
        let token = Cancellation::default();
        let worker_token = token.clone();
        let started = Instant::now();
        let worker = thread::spawn(move || {
            subtitle_tracks::extract(&tools, &input, 2, &worker_token, Duration::from_millis(500))
        });
        let marker = directory.path().join("child.pid");
        while !marker.exists() && started.elapsed() < Duration::from_secs(3) {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(marker.exists());
        if cancel {
            token.cancel();
        }
        let result = worker.join().unwrap();
        assert!(if cancel {
            matches!(result, Err(Error::Cancelled))
        } else {
            matches!(result, Err(Error::Timeout))
        });
        assert!(started.elapsed() < Duration::from_secs(3));
        let pid = fs::read_to_string(marker).unwrap();
        assert!(
            !Command::new("kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }
}
