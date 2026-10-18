//! Compile the actual transport policy against a deterministic client boundary.
//! These tests validate transactions/errors, not mpv or native audio output.
extern crate self as media_player;

use std::{collections::BTreeMap, ffi::CStr};

#[path = "../../src-tauri/src/media_transport.rs"]
mod media_transport;

pub struct Player {
    properties: BTreeMap<String, String>,
    commands: Vec<Vec<String>>,
    fail: Vec<Vec<String>>,
}

impl Player {
    fn new(paused: bool) -> Self {
        let mut player = Self {
            properties: [
                ("ab-loop-a", ".5"),
                ("ab-loop-b", ".823"),
                ("aid", "1"),
                ("audio-pts", ".65"),
                ("time-pos", ".66"),
                ("vsf-audio-continuation-version", "1"),
                ("vsf-audio-continuation-pts", ".7"),
                ("current-ao", "coreaudio"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
            commands: vec![],
            fail: vec![],
        };
        player
            .properties
            .insert("pause".into(), if paused { "yes" } else { "no" }.into());
        player
    }

    pub fn text(&mut self, name: &CStr) -> Option<String> {
        self.properties.get(name.to_str().unwrap()).cloned()
    }

    pub fn number(&mut self, name: &CStr) -> Option<f64> {
        self.text(name)?.parse().ok()
    }

    pub fn flag(&mut self, name: &CStr) -> bool {
        self.text(name).as_deref() == Some("yes")
    }

    pub fn paused(&mut self) -> bool {
        self.flag(c"pause")
    }

    pub fn seeking(&mut self) -> bool {
        self.flag(c"seeking")
    }

    pub fn command(&mut self, args: &[&str]) -> Result<(), String> {
        self.commands
            .push(args.iter().map(|s| (*s).into()).collect());
        if let Some(index) = self.fail.iter().position(|failure| failure == args) {
            self.fail.remove(index);
            return Err("injected client failure".into());
        }
        if args[0] == "set" {
            self.properties.insert(args[1].into(), args[2].into());
        }
        Ok(())
    }
}

#[test]
fn resting_cursor_requires_a_settled_pause_at_the_client_boundary() {
    let mut player = Player::new(true);
    player.properties.remove("audio-pts");
    player.properties.insert("time-pos".into(), "2.5".into());
    assert_eq!(media_transport::position(&mut player, true), Some(2.5));

    player.properties.insert("seeking".into(), "yes".into());
    assert_eq!(media_transport::position(&mut player, true), None);

    player.properties.insert("seeking".into(), "no".into());
    player.properties.insert("pause".into(), "no".into());
    assert_eq!(media_transport::position(&mut player, true), None);

    player.properties.insert("audio-pts".into(), "2.49".into());
    assert_eq!(media_transport::position(&mut player, true), Some(2.49));
}

#[test]
fn submitted_clear_preserves_pause_and_does_not_change_follow_clock() {
    for paused in [false, true] {
        let mut player = Player::new(paused);
        media_transport::clear_loop(&mut player, true).unwrap();
        assert_eq!(player.paused(), paused);
        assert_eq!(player.text(c"ab-loop-a").as_deref(), Some("no"));
        assert_eq!(player.text(c"ab-loop-b").as_deref(), Some("no"));
        assert!(
            player
                .commands
                .iter()
                .any(|c| c == &["seek", "0.7", "absolute+exact"])
        );
        assert_eq!(media_transport::position(&mut player, true), Some(0.65));
    }
}

#[test]
fn unavailable_continuation_never_falls_back_to_audible_position() {
    for paused in [false, true] {
        let mut player = Player::new(paused);
        player.properties.remove("vsf-audio-continuation-pts");
        assert!(
            media_transport::clear_loop(&mut player, true)
                .unwrap_err()
                .contains("continuation")
        );
        assert!(!player.commands.iter().any(|c| c[0] == "seek"));
        assert_eq!(player.text(c"ab-loop-a").as_deref(), Some(".5"));
        assert_eq!(player.text(c"ab-loop-b").as_deref(), Some(".823"));
        assert_eq!(player.paused(), paused);
    }
}

#[test]
fn other_outputs_and_old_libraries_keep_their_existing_policy() {
    for (key, value) in [
        ("current-ao", "wasapi"),
        ("vsf-audio-continuation-version", "0"),
    ] {
        let mut player = Player::new(false);
        player.properties.insert(key.into(), value.into());
        media_transport::clear_loop(&mut player, true).unwrap();
        assert!(
            player
                .commands
                .iter()
                .any(|c| c == &["seek", "0.65", "absolute+exact"])
        );
    }
}

#[test]
fn failed_seek_restores_endpoints_and_pause() {
    let mut player = Player::new(false);
    player
        .fail
        .push(vec!["seek".into(), "0.7".into(), "absolute+exact".into()]);
    assert!(media_transport::clear_loop(&mut player, true).is_err());
    assert_eq!(player.text(c"ab-loop-a").as_deref(), Some(".5"));
    assert_eq!(player.text(c"ab-loop-b").as_deref(), Some(".823"));
    assert!(!player.paused());
}

#[test]
fn failed_rollback_reports_the_failure_and_keeps_output_paused() {
    let mut player = Player::new(false);
    player.properties.remove("vsf-audio-continuation-pts");
    player
        .fail
        .push(vec!["set".into(), "ab-loop-a".into(), ".5".into()]);
    assert!(
        media_transport::clear_loop(&mut player, true)
            .unwrap_err()
            .contains("remains paused")
    );
    assert!(player.paused());
}
