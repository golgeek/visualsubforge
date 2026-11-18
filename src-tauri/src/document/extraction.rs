//! Sole accepted-document publication boundary for extracted subtitle results.
use super::*;
use vsf_application::subtitle_extraction::{Prepared, Provenance};
#[derive(Clone)]
pub(crate) struct ExtractionCapture(CheckpointKey);
impl Documents {
    pub(crate) fn capture_extraction(
        &self,
        generation: u64,
        revision: u64,
        discard: bool,
    ) -> Result<ExtractionCapture> {
        let store = self.0.lock().map_err(|_| "Document worker unavailable")?;
        if store.generation != generation || store.snapshot().revision != revision {
            return Err("Subtitles changed before extraction".into());
        }
        store.check_discard(discard)?;
        Ok(ExtractionCapture(store.checkpoint_key()))
    }
    pub(crate) fn accept_extraction(
        &self,
        capture: &ExtractionCapture,
        prepared: Prepared,
    ) -> Result<Snapshot> {
        let mut store = self.0.lock().map_err(|_| "Document worker unavailable")?;
        if store.checkpoint_key() != capture.0 {
            return Err("Subtitles changed during extraction; extract again".into());
        }
        let saved_bytes = prepared
            .parsed
            .source
            .serialize(&prepared.parsed.document)?
            .into_bytes();
        store.loaded = Some(Loaded {
            editor: Editor::with_properties(prepared.parsed.document, prepared.parsed.source),
            path: None,
            saved_bytes,
            recovered: false,
            resynced: false,
            translation: translation::Context::default(),
            extracted_from: Some(prepared.provenance),
        });
        store.generation += 1;
        store.invalidate_checks();
        Ok(store.snapshot())
    }
}
pub(crate) type ExtractedSource = Provenance;

#[cfg(test)]
mod tests {
    use super::*;
    fn prepared() -> Prepared {
        Prepared {
            parsed: vsf_application::subtitle_file::parse_document(
                b"1\n00:00:01,000 --> 00:00:02,000\nExtracted\n",
                Some("srt"),
            )
            .unwrap(),
            provenance: Provenance {
                media_path: std::env::temp_dir().join("vsf-synthetic-video.mkv"),
                stream_index: 2,
                codec: "subrip".into(),
                language: Some("fra".into()),
                title: None,
                suggested_name: "video.fr.srt".into(),
            },
            font_count: 0,
            repaired_cues: vec![],
        }
    }
    #[test]
    fn adoption_requires_save_as_and_rejects_stale_capture() {
        let documents = Documents::default();
        let capture = documents.capture_extraction(0, 0, false).unwrap();
        let snapshot = documents.accept_extraction(&capture, prepared()).unwrap();
        assert!(snapshot.dirty && snapshot.requires_save_as && snapshot.path.is_none());
        assert_eq!(
            snapshot.suggested_save_path,
            Some(std::env::temp_dir().join("video.fr.srt"))
        );
        assert_eq!(
            snapshot.extracted_from.unwrap().suggested_name,
            "video.fr.srt"
        );
        assert!(documents.accept_extraction(&capture, prepared()).is_err());
        assert!(documents.capture_extraction(1, 0, false).is_err());
    }
    #[test]
    fn recovery_round_trip_retains_provenance_without_reading_video() {
        let directory = tempfile::tempdir().unwrap();
        let value = prepared();
        let text = value
            .parsed
            .source
            .serialize(&value.parsed.document)
            .unwrap();
        let mut record = crate::recovery_store::Record::new_document(
            subtitle_formats::Format::Srt,
            None,
            text.clone(),
            text,
            1,
            0,
        )
        .unwrap();
        record.set_extracted_from(Some(value.provenance));
        let session = crate::recovery_store::Session::create(directory.path()).unwrap();
        session.write(Some(&record)).unwrap();
        // Validate the serialized record and version at the public recovery boundary.
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["version"], 6);
        assert_eq!(json["extracted_from"]["suggested_name"], "video.fr.srt");
        drop(session);
        let entries = crate::recovery_store::list(directory.path()).unwrap();
        let mut recovered = Store {
            recovery_root: Some(directory.path().into()),
            ..Store::default()
        };
        let snapshot = recovered
            .apply(
                Action::Recover {
                    id: entries[0].id.clone(),
                    discard: true,
                },
                0,
                0,
            )
            .unwrap();
        assert!(snapshot.dirty && snapshot.requires_save_as);
        assert_eq!(
            snapshot.suggested_save_path,
            Some(std::env::temp_dir().join("video.fr.srt"))
        );
        assert_eq!(
            snapshot.extracted_from.unwrap().suggested_name,
            "video.fr.srt"
        );
        assert_eq!(snapshot.cues[0].text, "Extracted");
    }
}
