//! Accepted-document preview, serialized on the existing media control worker.
//! mpv owns subtitle timing/composition. No UI clock or decoded frames cross IPC.
mod ass;
mod text;

#[derive(Debug, PartialEq)]
pub(crate) struct Source {
    pub url: String,
    styled: bool,
}

pub(crate) fn serialize(document: &subtitle_core::Document) -> Result<Option<Source>, String> {
    let styled = document
        .cues()
        .iter()
        .any(|cue| ass::has_markup(cue.text()));
    let url = if styled {
        ass::serialize(document)?
    } else {
        text::serialize(document)?.map(|text| format!("memory://{text}"))
    };
    Ok(url.map(|url| Source { url, styled }))
}

pub(crate) fn serialize_source(
    document: &subtitle_core::Document,
    source: &subtitle_formats::Source,
) -> Result<Option<Source>, String> {
    match source {
        subtitle_formats::Source::Ass(source) => {
            Ok(ass::serialize_document(document, source)?.map(|url| Source { url, styled: true }))
        }
        subtitle_formats::Source::Srt(_) => serialize(document),
        subtitle_formats::Source::WebVtt(_) => {
            if document.cues().is_empty() {
                return Ok(None);
            }
            if source.serialized_len(document)? > 4 * 1024 * 1024 {
                return Err("Document exceeds the 4 MiB native preview limit; editing and saving remain available".into());
            }
            // Native WebVTT decoding owns markup interpretation. Never route
            // literal ASS-looking text through SRT's fansub override encoder.
            // Normalize only this presentation copy for demuxer line handling.
            let text = source
                .serialize(document)?
                .trim_start_matches('\u{feff}')
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            Ok(Some(Source {
                url: format!("memory://{text}"),
                styled: false,
            }))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Revision(pub u64, pub u64);

pub struct Snapshot {
    pub revision: Revision,
    pub content: Result<Option<Source>, String>,
}

// Local seam for testing failures at the native command boundary.
trait Output {
    fn command(&mut self, args: &[&str]) -> Result<(), String>;
    fn selected_subtitle(&mut self) -> Option<u64>;
}

#[derive(Default)]
pub struct Renderer {
    session: Option<u64>,
    revision: Option<Revision>,
    track: Option<u64>,
    unknown_track: bool,
    pub error: Option<String>,
}
impl Renderer {
    pub fn reset(&mut self, session: u64) {
        if self.session != Some(session) {
            // Track IDs belong to the unloaded media, never to the next file.
            *self = Self {
                session: Some(session),
                ..Self::default()
            };
        }
    }
    pub fn revision(&self) -> Option<Revision> {
        self.revision
    }

    fn replace(&mut self, output: &mut impl Output, snapshot: Snapshot) {
        if self.revision == Some(snapshot.revision) {
            return;
        }
        self.revision = Some(snapshot.revision);
        if self.unknown_track {
            // Do not accumulate unidentifiable tracks on subsequent edits.
            self.error =
                Some("Subtitle preview: reopen media after an unidentifiable native track".into());
            return;
        }
        let result = (|| {
            // Hide the previous revision before replacement; propagate a hide failure
            // explicitly. Keep a failed-removal ID for the next revision.
            output.command(&["set", "sid", "no"])?;
            if let Some(id) = self.track {
                output.command(&["sub-remove", &id.to_string()])?;
                self.track = None;
            }
            if let Some(source) = snapshot.content? {
                output.command(&[
                    "set",
                    "sub-ass-override",
                    if source.styled { "no" } else { "yes" },
                ])?;
                output.command(&["sub-add", &source.url, "select", "VSF accepted subtitles"])?;
                self.track = output.selected_subtitle();
                if self.track.is_none() {
                    self.unknown_track = true;
                    output.command(&["set", "sid", "no"])?;
                    return Err("Native player did not select the accepted subtitle preview".into());
                }
            }
            Ok(())
        })();
        self.error = result
            .err()
            .map(|error: String| format!("Subtitle preview: {error}"));
    }
}

#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
impl Output for media_player::Player {
    fn command(&mut self, args: &[&str]) -> Result<(), String> {
        self.command(args)
    }
    fn selected_subtitle(&mut self) -> Option<u64> {
        self.number(c"sid")
            .filter(|id| id.is_finite() && *id > 0.0 && id.fract() == 0.0)
            .map(|id| id as u64)
    }
}

#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
impl Renderer {
    pub fn reconcile(
        &mut self,
        player: &mut media_player::Player,
        documents: &crate::document::Documents,
    ) {
        let started = crate::workflow_trace::enabled().then(std::time::Instant::now);
        match documents.preview_since(self.revision()) {
            Ok(Some(snapshot)) => {
                let revision = snapshot.revision;
                self.replace(player, snapshot);
                if let Some(started) = started {
                    crate::workflow_trace::preview_command(
                        revision.0,
                        revision.1,
                        started.elapsed().as_secs_f64() * 1000.0,
                        self.error.is_none(),
                    );
                }
            }
            Ok(None) => {}
            Err(error) => self.error = Some(format!("Subtitle preview: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn webvtt_transport_keeps_literal_ass_and_normalizes_only_preview_endings() {
        let input = "\u{feff}WEBVTT\r\n\r\n00:01.001 --> 00:02.999\r\n{\\an8}literal\\N &amp; <v Alice>voice</v>\r\n";
        let parsed =
            subtitle_formats::parse(input, Some(subtitle_formats::Format::WebVtt)).unwrap();
        let preview = serialize_source(&parsed.document, &parsed.source)
            .unwrap()
            .unwrap();
        assert!(!preview.styled);
        assert_eq!(
            preview.url,
            format!(
                "memory://{}",
                input.trim_start_matches('\u{feff}').replace("\r\n", "\n")
            )
        );
        assert_eq!(parsed.source.serialize(&parsed.document).unwrap(), input);
        let empty =
            subtitle_formats::parse("WEBVTT", Some(subtitle_formats::Format::WebVtt)).unwrap();
        assert!(
            serialize_source(&empty.document, &empty.source)
                .unwrap()
                .is_none()
        );
        let large = subtitle_formats::parse(
            &format!(
                "WEBVTT\n\n00:01.001 --> 00:02.999\n{}",
                "x".repeat(4 * 1024 * 1024)
            ),
            Some(subtitle_formats::Format::WebVtt),
        )
        .unwrap();
        assert!(
            serialize_source(&large.document, &large.source)
                .unwrap_err()
                .contains("4 MiB")
        );
    }
    #[derive(Default)]
    struct Fake {
        commands: Vec<Vec<String>>,
        fail: Option<&'static str>,
        id: u64,
    }
    impl Output for Fake {
        fn command(&mut self, args: &[&str]) -> Result<(), String> {
            self.commands
                .push(args.iter().map(|s| s.to_string()).collect());
            if self.fail == Some(args[0]) {
                return Err("injected failure".into());
            }
            if args[0] == "sub-add" {
                self.id += 1;
            }
            Ok(())
        }
        fn selected_subtitle(&mut self) -> Option<u64> {
            Some(self.id)
        }
    }
    fn snapshot(revision: u64, text: Option<&str>) -> Snapshot {
        Snapshot {
            revision: Revision(1, revision),
            content: Ok(text.map(|text| Source {
                url: format!("memory://{text}"),
                styled: false,
            })),
        }
    }
    #[test]
    fn replacement_removes_old_track_and_closed_document_clears_preview() {
        let mut r = Renderer::default();
        let mut p = Fake::default();
        r.reset(1);
        r.replace(&mut p, snapshot(0, Some("original")));
        r.replace(&mut p, snapshot(1, Some("accepted")));
        assert_eq!(r.track, Some(2));
        assert!(p.commands.iter().any(|c| c == &["sub-remove", "1"]));
        let count = p.commands.len();
        r.replace(&mut p, snapshot(1, Some("stale duplicate")));
        assert_eq!(p.commands.len(), count);
        r.replace(&mut p, snapshot(2, None));
        assert_eq!(r.track, None);
        assert!(r.error.is_none());
        assert_eq!(p.commands.last().unwrap(), &["sub-remove", "2"]);
    }
    #[test]
    fn failure_hides_old_preview_and_does_not_accumulate_tracks() {
        let mut r = Renderer::default();
        let mut p = Fake::default();
        r.replace(&mut p, snapshot(0, Some("original")));
        p.fail = Some("sub-remove");
        r.replace(&mut p, snapshot(1, Some("changed")));
        assert_eq!(r.track, Some(1));
        assert!(r.error.is_some());
        assert_eq!(p.id, 1);
        p.fail = None;
        r.replace(&mut p, snapshot(2, None));
        assert!(r.error.is_none());
        assert_eq!(r.track, None);
        assert_eq!(p.commands[3], ["set", "sid", "no"]);
    }
    #[test]
    fn new_media_never_removes_an_id_from_previous_session() {
        let mut r = Renderer::default();
        let mut p = Fake::default();
        r.reset(1);
        r.replace(&mut p, snapshot(0, Some("original")));
        r.reset(2);
        assert_eq!(r.revision(), None);
        assert_eq!(r.track, None);
        p.commands.clear();
        r.replace(&mut p, snapshot(0, Some("original")));
        assert!(!p.commands.iter().any(|c| c[0] == "sub-remove"));
    }

    #[test]
    fn style_mode_switches_and_failed_configuration_never_publishes_old_text() {
        let parsed =
            subtitle_formats::srt::parse("1\n00:00:01,001 --> 00:00:02,999\n{\\an8}Top\n\n")
                .unwrap();
        let mut r = Renderer::default();
        let mut p = Fake::default();
        r.replace(
            &mut p,
            Snapshot {
                revision: Revision(1, 1),
                content: serialize(&parsed.document),
            },
        );
        assert!(
            p.commands
                .iter()
                .any(|c| c == &["set", "sub-ass-override", "no"])
        );
        assert!(
            p.commands
                .iter()
                .any(|c| c[0] == "sub-add" && c[1].starts_with("hex://"))
        );
        r.replace(&mut p, snapshot(2, Some("WEBVTT\n\nplain")));
        assert!(
            p.commands
                .iter()
                .any(|c| c == &["set", "sub-ass-override", "yes"])
        );
        assert_eq!(r.track, Some(2));
        r.replace(
            &mut p,
            Snapshot {
                revision: Revision(1, 3),
                content: Err("oversize".into()),
            },
        );
        assert!(r.track.is_none());
        assert!(r.error.as_deref().unwrap().contains("oversize"));
        assert_eq!(p.commands.last().unwrap(), &["sub-remove", "2"]);
    }
}
