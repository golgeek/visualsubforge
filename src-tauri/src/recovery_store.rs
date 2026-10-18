//! Recovery files are separate from originals. A session lease excludes live
//! writers across processes; atomic replacement preserves the previous record.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, String>;
const MAX_RECORD: u64 = 200 * 1024 * 1024; // JSON escaping can expand both SRT strings.

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    version: u32,
    pub source_path: Option<String>,
    pub saved_srt: String,
    pub accepted_srt: String,
    pub generation: u64,
    pub revision: u64,
    captured_ms: u64,
    // Version 3 persists pending resync Save As protection. Legacy records
    // default to ordinary recovery behavior, without changing their bytes.
    #[serde(default)]
    pub resynced: bool,
    // Version 4 retains source format. The legacy *_srt JSON keys now contain
    // exact source-format text; old SRT records remain readable without rewriting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}
impl Record {
    #[cfg(test)]
    pub fn new(
        source_path: impl Into<Option<String>>,
        saved_srt: String,
        accepted_srt: String,
        generation: u64,
        revision: u64,
    ) -> Result<Self> {
        Self::new_document(
            subtitle_formats::Format::Srt,
            source_path,
            saved_srt,
            accepted_srt,
            generation,
            revision,
        )
    }
    pub fn new_document(
        format: subtitle_formats::Format,
        source_path: impl Into<Option<String>>,
        saved_srt: String,
        accepted_srt: String,
        generation: u64,
        revision: u64,
    ) -> Result<Self> {
        let record = Self {
            version: if format == subtitle_formats::Format::Srt {
                3
            } else {
                4
            },
            format: (format != subtitle_formats::Format::Srt).then(|| format.extension().into()),
            source_path: source_path.into(),
            saved_srt,
            accepted_srt,
            generation,
            revision,
            resynced: false,
            captured_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis()
                .try_into()
                .map_err(|_| "Recovery timestamp exceeds range")?,
        };
        record.validate()?;
        Ok(record)
    }
    pub fn format_extension(&self) -> &str {
        self.format.as_deref().unwrap_or("srt")
    }
    // Return the validated accepted document so recovery can adopt it directly.
    // Re-parsing after this boundary repeats the most expensive part of recovery.
    fn validate(&self) -> Result<subtitle_formats::Parsed> {
        if !matches!(self.version, 1..=4)
            || (self.version < 3 && self.resynced)
            || (self.version < 4 && self.format.is_some())
            || (self.version == 4
                && self
                    .format
                    .as_deref()
                    .and_then(subtitle_formats::Format::from_extension)
                    .is_none())
        {
            return Err("Unsupported recovery format; record retained".into());
        }
        match &self.source_path {
            Some(path) if !Path::new(path).is_absolute() => {
                return Err("Recovery source path must be absolute".into());
            }
            None if self.version == 1 => {
                return Err("Version 1 recovery requires a source path".into());
            }
            _ => {}
        }
        vsf_application::subtitle_file::parse_document(
            self.saved_srt.as_bytes(),
            Some(self.format_extension()),
        )?;
        vsf_application::subtitle_file::parse_document(
            self.accepted_srt.as_bytes(),
            Some(self.format_extension()),
        )
    }
}

pub(crate) struct Session {
    directory: PathBuf,
    // Held until this background owner is dropped. No UI/native affinity.
    _lease: Option<crate::file_lease::FileLease>,
}
impl Session {
    pub fn create(root: &Path) -> Result<Self> {
        fs::create_dir_all(root).map_err(|e| format!("Cannot create recovery directory: {e}"))?;
        let directory = tempfile::Builder::new()
            .prefix("session-")
            .tempdir_in(root)
            .map_err(|e| e.to_string())?;
        let lease = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.path().join("lease"))
            .map_err(|e| e.to_string())?;
        let lease = crate::file_lease::FileLease::lock(lease).map_err(|e| e.to_string())?;
        Ok(Self {
            directory: directory.keep(),
            _lease: Some(lease),
        })
    }
    pub fn prepare(&self, record: &Record) -> Result<crate::atomic_file::Prepared> {
        record.validate()?;
        let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
        crate::atomic_file::prepare(&self.directory.join("snapshot.json"), &bytes)
    }
    pub fn clear(&self) -> Result<()> {
        remove_snapshot(&self.directory.join("snapshot.json"))
    }
    #[cfg(test)]
    pub fn write(&self, record: Option<&Record>) -> Result<()> {
        let target = self.directory.join("snapshot.json");
        match record {
            Some(record) => {
                record.validate()?;
                let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
                crate::atomic_file::write(&target, &bytes)
            }
            None => remove_snapshot(&target),
        }
    }
}
// Cleanup never recursively deletes a session: unknown files are retained.
// A closed lease must precede unlinking on Windows. Failures leave harmless
// metadata and are diagnostic only; recovery contents are never removed here.
fn cleanup_empty(directory: &Path) {
    if !matches!(fs::symlink_metadata(directory.join("snapshot.json")), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
    {
        return;
    }
    let result = fs::remove_file(directory.join("lease")).and_then(|_| fs::remove_dir(directory));
    if let Err(error) = result {
        eprintln!(
            "Cannot remove empty recovery session {}: {error}",
            directory.display()
        );
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        drop(self._lease.take());
        cleanup_empty(&self.directory);
    }
}
fn remove_snapshot(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Cannot clear recovery snapshot: {e}")),
    }
}
fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    if !id.starts_with("session-")
        || id.len() < 9
        || id.len() > 80
        || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err("Invalid recovery identifier".into());
    }
    let path = root.join(id);
    let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Recovery entry must be a real directory".into());
    }
    Ok(path)
}
fn lease(path: &Path) -> Result<Option<crate::file_lease::FileLease>> {
    let lease_path = path.join("lease");
    if !fs::symlink_metadata(&lease_path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("Invalid recovery lease".into());
    }
    let file = File::options()
        .read(true)
        .write(true)
        .open(lease_path)
        .map_err(|e| e.to_string())?;
    match crate::file_lease::FileLease::try_lock(file) {
        Ok(lease) => Ok(Some(lease)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(e) => Err(format!("Cannot acquire recovery lease: {e}")),
    }
}
pub(crate) struct Recovered {
    pub record: Record,
    pub parsed: subtitle_formats::Parsed,
}

fn read(path: &Path) -> Result<Recovered> {
    let path = path.join("snapshot.json");
    if !fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("Recovery snapshot must be a regular file".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err("Recovery record exceeds its size limit".into());
    }
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read recovery record: {e}"))?;
    let parsed = record.validate()?;
    Ok(Recovered { record, parsed })
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    pub id: String,
    pub source_path: Option<String>,
    #[serde(with = "crate::ipc_number::optional")]
    #[specta(type = Option<crate::ipc_number::SafeU64>)]
    pub captured_ms: Option<u64>,
    #[serde(with = "crate::ipc_number::optional")]
    #[specta(type = Option<crate::ipc_number::SafeU64>)]
    pub revision: Option<u64>,
    pub error: Option<String>,
}
pub(crate) fn list(root: &Path) -> Result<Vec<Entry>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("Cannot list recovery records: {e}")),
    };
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if !id.starts_with("session-") {
            continue;
        }
        // A session may disappear after read_dir when its owner exits normally.
        if matches!(fs::symlink_metadata(entry.path()), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        {
            continue;
        }
        let inspected = (|| -> Result<Option<Record>> {
            let path = directory(root, &id)?;
            if !path
                .join("snapshot.json")
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                return Ok(None);
            }
            let Some(_lease) = lease(&path)? else {
                return Ok(None);
            };
            if !path
                .join("snapshot.json")
                .try_exists()
                .map_err(|e| e.to_string())?
            {
                return Ok(None);
            }
            read(&path).map(|recovered| Some(recovered.record))
        })();
        let mut item = Entry {
            id,
            source_path: None,
            captured_ms: None,
            revision: None,
            error: None,
        };
        match inspected {
            Ok(Some(record)) => {
                item.source_path = record.source_path;
                // These fields are informational; recovery creates fresh editor
                // history. Unrepresentable old metadata must not hide an otherwise
                // recoverable SRT or make the entire listing fail serialization.
                item.captured_ms = Some(record.captured_ms)
                    .filter(|value| *value <= crate::ipc_number::MAX_SAFE_INTEGER);
                item.revision = Some(record.revision)
                    .filter(|value| *value <= crate::ipc_number::MAX_SAFE_INTEGER);
            }
            Ok(None) => continue,
            Err(error) => item.error = Some(error),
        }
        result.push(item);
    }
    result.sort_by(|a, b| b.captured_ms.cmp(&a.captured_ms).then(a.id.cmp(&b.id)));
    Ok(result)
}
pub(crate) fn recover(root: &Path, id: &str) -> Result<Recovered> {
    let path = directory(root, id)?;
    let _lease = lease(&path)?.ok_or("That recovery session is still running")?;
    // Reading does not consume the only durable copy. Explicit discard is separate.
    read(&path)
}
pub(crate) fn discard(root: &Path, id: &str) -> Result<()> {
    let path = directory(root, id)?;
    let _lease = lease(&path)?.ok_or("That recovery session is still running")?;
    remove_snapshot(&path.join("snapshot.json"))?;
    drop(_lease);
    cleanup_empty(&path);
    Ok(())
}

#[cfg(test)]
#[path = "recovery_process_tests.rs"]
mod process_tests;

#[cfg(test)]
mod tests {
    use super::*;
    const SRT: &str = "\u{feff}007\r\n00:00:01,000 --> 00:00:02,000\r\n日本語 café\r\n\r\n";
    fn record(path: &Path) -> Record {
        Record::new(
            path.to_string_lossy().into_owned(),
            SRT.into(),
            SRT.replace("café", "edited"),
            4,
            9,
        )
        .unwrap()
    }
    #[test]
    fn pathless_version_two_and_legacy_version_one_records_are_validated() {
        let directory = tempfile::tempdir().unwrap();
        let old = record(&directory.path().join("original.srt"));
        let mut json = serde_json::to_value(old).unwrap();
        json.as_object_mut().unwrap().remove("resynced");
        json["version"] = 1.into();
        let legacy: Record = serde_json::from_value(json.clone()).unwrap();
        legacy.validate().unwrap();
        assert!(!legacy.resynced);
        json["source_path"] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<Record>(json.clone())
                .unwrap()
                .validate()
                .is_err()
        );
        json["version"] = 2.into();
        serde_json::from_value::<Record>(json.clone())
            .unwrap()
            .validate()
            .unwrap();
        json["source_path"] = "relative.srt".into();
        assert!(
            serde_json::from_value::<Record>(json)
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn live_sessions_are_excluded_and_recovery_never_changes_the_source() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("source.srt");
        fs::write(&original, SRT).unwrap();
        let session = Session::create(root.path()).unwrap();
        let id = session
            .directory
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        session.write(Some(&record(&original))).unwrap();
        assert!(list(root.path()).unwrap().is_empty());
        assert!(recover(root.path(), &id).is_err());
        assert!(discard(root.path(), &id).is_err());
        drop(session);
        let entries = list(root.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].revision, Some(9));
        fs::write(&original, "externally changed").unwrap();
        let restored = recover(root.path(), &id).unwrap().record;
        assert_eq!(restored.saved_srt, SRT);
        assert!(restored.accepted_srt.contains("edited"));
        assert_eq!(fs::read_to_string(&original).unwrap(), "externally changed");
        assert_eq!(list(root.path()).unwrap().len(), 1);
        discard(root.path(), &id).unwrap();
        assert!(list(root.path()).unwrap().is_empty());
        assert_eq!(fs::read_to_string(&original).unwrap(), "externally changed");
    }

    #[test]
    fn recovery_returns_exact_accepted_document_and_validates_both_texts() {
        use subtitle_formats::Format;
        for (format, saved, from) in [
            (Format::Srt, SRT, "café"),
            (
                Format::Ass,
                include_str!("../../tests/fixtures/subtitles/styles.ass"),
                "Red title",
            ),
            (
                Format::Ssa,
                include_str!("../../tests/fixtures/subtitles/styles.ssa"),
                "Red title",
            ),
            (
                Format::WebVtt,
                include_str!("../../tests/fixtures/subtitles/styles.vtt"),
                "Été",
            ),
        ] {
            assert!(saved.contains(from));
            let accepted = saved.replace(from, "Recovered");
            let root = tempfile::tempdir().unwrap();
            let session = Session::create(root.path()).unwrap();
            let record =
                Record::new_document(format, None, saved.into(), accepted.clone(), 1, 1).unwrap();
            session.write(Some(&record)).unwrap();
            let path = session.directory.join("snapshot.json");
            let id = session
                .directory
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            let bytes = fs::read(&path).unwrap();
            drop(session);
            let restored = recover(root.path(), &id).unwrap();
            assert_eq!(restored.record.saved_srt, saved);
            assert_eq!(restored.parsed.source.format(), format);
            assert_eq!(
                restored
                    .parsed
                    .source
                    .serialize(&restored.parsed.document)
                    .unwrap(),
                accepted
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
            for field in ["saved_srt", "accepted_srt"] {
                let mut corrupt = serde_json::to_value(&record).unwrap();
                corrupt[field] = "invalid subtitle data".into();
                let corrupt = serde_json::to_vec(&corrupt).unwrap();
                fs::write(&path, &corrupt).unwrap();
                assert!(list(root.path()).unwrap()[0].error.is_some());
                assert!(recover(root.path(), &id).is_err());
                assert_eq!(fs::read(&path).unwrap(), corrupt);
            }
        }
    }

    #[test]
    fn styled_record_validates_format_before_replacing_checkpoint() {
        let root = tempfile::tempdir().unwrap();
        let session = Session::create(root.path()).unwrap();
        let text = include_str!("../../tests/fixtures/subtitles/styles.ass");
        let record = Record::new_document(
            subtitle_formats::Format::Ass,
            None,
            text.into(),
            text.into(),
            1,
            1,
        )
        .unwrap();
        session.write(Some(&record)).unwrap();
        let path = session.directory.join("snapshot.json");
        let before = fs::read(&path).unwrap();
        for (version, format) in [
            (3, Some("ass")),
            (4, None),
            (4, Some("unknown")),
            (4, Some("ssa")),
            (4, Some("srt")),
        ] {
            let mut rejected = record.clone();
            rejected.version = version;
            rejected.format = format.map(str::to_owned);
            assert!(session.write(Some(&rejected)).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
    #[test]
    fn rejected_record_preserves_previous_snapshot_and_corruption_is_reported() {
        let root = tempfile::tempdir().unwrap();
        let session = Session::create(root.path()).unwrap();
        let mut data = record(&root.path().join("source.srt"));
        session.write(Some(&data)).unwrap();
        let path = session.directory.join("snapshot.json");
        let previous = fs::read(&path).unwrap();
        data.version = 999;
        assert!(session.write(Some(&data)).is_err());
        assert_eq!(fs::read(&path).unwrap(), previous);
        let id = session
            .directory
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        fs::write(&path, "broken JSON").unwrap();
        drop(session);
        assert!(list(root.path()).unwrap()[0].error.is_some());
        assert!(recover(root.path(), &id).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken JSON");
        for invalid in ["../source.srt", "session-../outside", "/tmp/session-123456"] {
            assert!(discard(root.path(), invalid).is_err());
        }
    }
    #[test]
    fn normal_empty_sessions_are_removed_and_malformed_entries_do_not_hide_records() {
        let root = tempfile::tempdir().unwrap();
        let empty = Session::create(root.path()).unwrap();
        let empty_path = empty.directory.clone();
        drop(empty);
        assert!(!empty_path.exists());
        let session = Session::create(root.path()).unwrap();
        let kept = session.directory.clone();
        session
            .write(Some(&record(&root.path().join("original.srt"))))
            .unwrap();
        drop(session);
        assert!(kept.join("snapshot.json").is_file());
        fs::write(root.path().join("session-invalid"), "unrelated data").unwrap();
        let entries = list(root.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries.iter().filter(|e| e.error.is_none()).count(), 1);
        assert_eq!(entries.iter().filter(|e| e.error.is_some()).count(), 1);
        let id = kept.file_name().unwrap().to_str().unwrap();
        discard(root.path(), id).unwrap();
        assert!(!kept.exists());
        assert_eq!(
            fs::read_to_string(root.path().join("session-invalid")).unwrap(),
            "unrelated data"
        );
    }
    #[test]
    fn clean_checkpoint_clears_only_its_own_record() {
        let root = tempfile::tempdir().unwrap();
        let a = Session::create(root.path()).unwrap();
        let b = Session::create(root.path()).unwrap();
        let data = record(&root.path().join("source.srt"));
        a.write(Some(&data)).unwrap();
        b.write(Some(&data)).unwrap();
        a.write(None).unwrap();
        a.write(None).unwrap();
        drop(a);
        drop(b);
        assert_eq!(list(root.path()).unwrap().len(), 1);
    }

    #[test]
    fn wide_metadata_does_not_hide_recoverable_subtitles() {
        let root = tempfile::tempdir().unwrap();
        let normal = Session::create(root.path()).unwrap();
        let unusual = Session::create(root.path()).unwrap();
        let mut data = record(&root.path().join("source.srt"));
        normal.write(Some(&data)).unwrap();
        data.revision = crate::ipc_number::MAX_SAFE_INTEGER + 1;
        data.captured_ms = crate::ipc_number::MAX_SAFE_INTEGER + 1;
        unusual.write(Some(&data)).unwrap();
        let id = unusual
            .directory
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let snapshot_path = unusual.directory.join("snapshot.json");
        let before = fs::read(&snapshot_path).unwrap();
        drop(normal);
        drop(unusual);

        let entries = list(root.path()).unwrap();
        assert_eq!(entries.len(), 2);
        let entry = entries.iter().find(|entry| entry.id == id).unwrap();
        assert!(entry.error.is_none());
        assert_eq!(entry.captured_ms, None);
        assert_eq!(entry.revision, None);
        assert!(serde_json::to_value(&entries).is_ok());
        assert_eq!(
            recover(root.path(), &id).unwrap().record.accepted_srt,
            data.accepted_srt
        );
        assert_eq!(fs::read(snapshot_path).unwrap(), before);
    }
}
