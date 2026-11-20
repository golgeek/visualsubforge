//! File preparation is off-lock. Adoption checks the review epoch, both subtitle
//! identities and the inspected file digest before publishing one transaction.
use super::discussion::Discussion;
use super::*;
use translation::context::Origin;
use vsf_application::review_package::{Package, Subtitle};

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub(super) struct PackageLink {
    pub path: String,
    pub sha256: String,
    pub original_name: String,
    pub revised_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    utf16_le: Option<bool>,
}
impl PackageLink {
    pub(super) fn validate(&self) -> Result<()> {
        if self.path.is_empty()
            || self.path.len() > 32768
            || self.path.contains('\0')
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("Invalid review package link".into());
        }
        for name in [&self.original_name, &self.revised_name] {
            let ext = name
                .rsplit_once('.')
                .map(|(_, e)| e)
                .ok_or("Invalid review filename")?;
            let format =
                subtitle_formats::Format::from_extension(ext).ok_or("Invalid review filename")?;
            if review_package::safe_name(name, format.extension()) != *name {
                return Err("Unsafe review filename".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportCase {
    New,
    Same,
    Different,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportChoice {
    Open,
    Merge,
    Keep,
    UseTheirs,
}
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub case: ImportCase,
    pub digest: String,
    pub exported_by: String,
    pub exported_at_ms: SafeU64,
}
#[derive(Clone, Copy, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OutputKind {
    Package,
    Report,
    Readable,
}
impl Loaded {
    fn working_bytes(&self) -> Result<Vec<u8>> {
        if !self.editor.dirty() && !self.recovered && !self.resynced {
            if let Some(le) = self
                .path
                .is_none()
                .then(|| self.review.package.as_ref()?.utf16_le)
                .flatten()
            {
                let text = std::str::from_utf8(&self.saved_bytes).map_err(|e| e.to_string())?;
                let mut bytes = if le { vec![255, 254] } else { vec![254, 255] };
                bytes.extend(
                    text.encode_utf16()
                        .flat_map(|u| if le { u.to_le_bytes() } else { u.to_be_bytes() }),
                );
                return Ok(bytes);
            }
            Ok(self.saved_bytes.clone())
        } else {
            Ok(self
                .source()
                .serialize(self.editor.document())?
                .into_bytes())
        }
    }
}
impl Store {
    fn import_case(&self, p: &Package) -> Result<ImportCase> {
        let Some(l) = &self.loaded else {
            return Ok(ImportCase::New);
        };
        if !l
            .review
            .reference
            .reference
            .as_ref()
            .is_some_and(|r| r.reference.fingerprint() == p.original.sha256)
        {
            return Ok(ImportCase::New);
        }
        Ok(
            if review_package::digest(&l.working_bytes()?) == p.revised.sha256 {
                ImportCase::Same
            } else {
                ImportCase::Different
            },
        )
    }
}
pub(super) fn reference(path: &Path, sub: &Subtitle) -> Result<ReferenceFile> {
    let selected = std::path::absolute(path).map_err(|e| e.to_string())?;
    let path = selected.canonicalize().map_err(|e| e.to_string())?;
    let origin = Origin { selected, path };
    origin.validate()?;
    Ok(ReferenceFile {
        origin,
        reference: vsf_application::translation::Reference::from_embedded(sub)?,
        offset: Default::default(),
    })
}
impl Context {
    pub(crate) fn verify_source(&self) -> Result<()> {
        if let Some(link) = &self.package {
            let p = vsf_application::review_file::read(Path::new(&link.path))?;
            if p.original.sha256 != link.sha256 {
                return Err("Review package original changed".into());
            }
            Ok(())
        } else {
            self.reference.verify_project_reference()
        }
    }
    pub(crate) fn suggestion(&self) -> Option<PathBuf> {
        let link = self.package.as_ref()?;
        Some(Path::new(&link.path).with_file_name(&link.revised_name))
    }
}
impl Documents {
    pub(super) fn import_review(
        &self,
        key: ReviewKey,
        path: &Path,
        accept: Option<(&str, ImportChoice, bool)>,
    ) -> Result<ReviewReply> {
        let (p, digest) = vsf_application::review_file::read_identified(path)?;
        let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
        store.check_review_key(key)?;
        let case = store.import_case(&p)?;
        let Some((expected, choice, discard)) = accept else {
            let mut r = store.review_reply(None, 0, 0, None)?;
            r.inspection = Some(Inspection {
                case,
                digest,
                exported_by: p.exported_by,
                exported_at_ms: p.exported_at_ms.try_into()?,
            });
            return Ok(r);
        };
        if digest != expected {
            return Err("Review package changed after inspection".into());
        }
        if !matches!(
            (case, choice),
            (ImportCase::New, ImportChoice::Open)
                | (ImportCase::Same, ImportChoice::Merge)
                | (
                    ImportCase::Different,
                    ImportChoice::Keep | ImportChoice::UseTheirs
                )
        ) {
            return Err("Review changed; inspect the package again".into());
        }
        let checkpoint = store.checkpoint_key();
        let prior = if case == ImportCase::New {
            store.check_discard(discard)?;
            Discussion::default()
        } else {
            store.loaded_mut()?.review.discussion.clone()
        };
        drop(store);
        let threads = review_package::merge(&prior.threads, &p.threads)?;
        let before: usize = prior.threads.iter().map(|t| t.messages.len()).sum();
        let added = threads.iter().map(|t| t.messages.len()).sum::<usize>() - before;
        let resolved_before: std::collections::BTreeSet<_> = prior
            .threads
            .iter()
            .filter(|t| t.resolved())
            .map(|t| &t.id)
            .collect();
        let resolved = threads
            .iter()
            .filter(|t| t.resolved() && !resolved_before.contains(&t.id))
            .count();
        let mut exported = prior.exported;
        exported.extend(
            p.threads
                .iter()
                .flat_map(|t| t.messages.iter().map(|m| m.id.clone())),
        );
        let discussion = Discussion {
            threads,
            exported,
            round: prior.round.max(p.round),
        };
        let target = matches!(choice, ImportChoice::Open | ImportChoice::UseTheirs)
            .then(|| p.revised.parsed())
            .transpose()?;
        let original = (case == ImportCase::New)
            .then(|| reference(path, &p.original))
            .transpose()?;
        let baseline = if original.is_some() {
            let bytes = p.revised.decode()?;
            let utf16_le = match bytes.get(..2) {
                Some([255, 254]) => Some(true),
                Some([254, 255]) => Some(false),
                _ => None,
            };
            Some((
                review_package::embedded_text(&bytes)?.into_owned(),
                utf16_le,
            ))
        } else {
            None
        };
        let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
        store.check_review_key(key)?;
        if store.checkpoint_key() != checkpoint {
            return Err("Workspace changed while opening review".into());
        }
        if target.is_none() && store.loaded_mut()?.review.discussion == discussion {
            let snapshot = store.snapshot();
            let mut reply = store.review_reply(Some(snapshot), 0, 0, None)?;
            reply.notice = Some("0 new messages · 0 threads resolved".into());
            return Ok(reply);
        }
        let anchors_changed =
            target.is_none() && !discussion.same_anchors(&store.loaded_mut()?.review.discussion);
        store.prospective_review_epoch()?;
        if let (Some(original), Some((saved_bytes, utf16_le))) = (original, baseline) {
            let parsed = target.ok_or("Missing embedded working document")?;
            let link = PackageLink {
                path: original.origin.path.to_string_lossy().into(),
                sha256: p.original.sha256,
                original_name: p.original.name,
                revised_name: p.revised.name,
                utf16_le,
            };
            store.loaded = Some(Loaded {
                editor: Editor::with_properties(parsed.document, parsed.source),
                path: None,
                saved_bytes,
                recovered: false,
                resynced: false,
                translation: Default::default(),
                extracted_from: None,
                review: Context {
                    reference: translation::Context {
                        reference: Some(original),
                        dirty: true,
                        copied_from: None,
                    },
                    options: p.options,
                    discussion,
                    package: Some(link),
                },
            });
            store.generation += 1;
        } else {
            let l = store.loaded_mut()?;
            if let Some(parsed) = target {
                let same_format = parsed.source.format() == l.source().format();
                let default_metadata = if same_format {
                    None
                } else {
                    l.source().default_metadata()?
                };
                let current = l.editor.document().cues();
                let cues = parsed
                    .document
                    .cues()
                    .iter()
                    .enumerate()
                    .map(|(i, c)| subtitle_core::CueReplacement {
                        id: current.get(i).map(|q| q.id()),
                        start: c.start(),
                        end: c.end(),
                        text: c.text().into(),
                        metadata: if same_format {
                            c.metadata().cloned()
                        } else {
                            current
                                .get(i)
                                .and_then(|q| q.metadata())
                                .cloned()
                                .or_else(|| default_metadata.clone())
                        },
                    })
                    .collect();
                l.apply_edit(key.revision.get(), Edit::ReplaceCues { cues })?;
            }
            l.review.discussion = discussion;
            l.review.reference.dirty = true;
        }
        if matches!(choice, ImportChoice::Open | ImportChoice::UseTheirs) {
            store.next_review_epoch()?;
            store.invalidate_checks()
        } else {
            store.advance_discussion(key, anchors_changed)?
        }
        let snapshot = store.snapshot();
        let mut r = store.review_reply(Some(snapshot), 0, 0, None)?;
        r.notice = Some(format!(
            "{added} new message{} · {resolved} thread{} resolved",
            if added == 1 { "" } else { "s" },
            if resolved == 1 { "" } else { "s" }
        ));
        Ok(r)
    }
    pub(super) fn publish_review(
        &self,
        key: ReviewKey,
        path: &Path,
        kind: OutputKind,
        author: &str,
        date: &str,
        include_unchanged: bool,
    ) -> Result<ReviewReply> {
        let importable = !matches!(kind, OutputKind::Readable);
        let (checkpoint, comparison, p) = {
            let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
            store.check_review_key(key)?;
            let c = if matches!(kind, OutputKind::Package) {
                None
            } else {
                Some(store.comparison(key)?.clone())
            };
            let l = store.loaded.as_ref().ok_or("Open subtitles")?;
            let r = &l.review;
            l.translation.protect(path)?;
            r.reference.protect(path)?;
            let orig = r.reference.reference.as_ref().ok_or("Attach an original")?;
            let original_name = r
                .package
                .as_ref()
                .map(|p| p.original_name.as_str())
                .or_else(|| orig.origin.path.file_name()?.to_str())
                .unwrap_or("Original");
            let revised_name = l
                .path
                .as_ref()
                .and_then(|p| p.file_name()?.to_str())
                .or_else(|| r.package.as_ref().map(|p| p.revised_name.as_str()))
                .unwrap_or("Untitled");
            let p = Package {
                format: "visualsubforge-review".into(),
                version: 1,
                round: if importable {
                    r.discussion
                        .round
                        .checked_add(1)
                        .ok_or("Review round exhausted")?
                } else {
                    r.discussion.round
                },
                exported_by: author.into(),
                exported_at_ms: now_ms()?,
                original: Subtitle::capture(
                    original_name,
                    orig.reference.format(),
                    orig.reference.original_bytes(),
                ),
                revised: Subtitle::capture(revised_name, l.source().format(), &l.working_bytes()?),
                options: r.options.clone(),
                threads: r.discussion.threads.clone(),
            };
            (store.checkpoint_key(), c, p)
        };
        let data = if let Some(c) = comparison {
            let sections = review_package::sections(&p, &c, matches!(kind, OutputKind::Report))?;
            engine::html(
                &c,
                &engine::ReportOptions {
                    original_name: &p.original.name,
                    revised_name: &p.revised.name,
                    date,
                    include_unchanged,
                },
                &sections,
            )
            .into_bytes()
        } else {
            p.write()?
        };
        if !matches!(kind, OutputKind::Package) {
            review_package::check_report_size(data.len())?;
        }
        let path = vsf_application::new_output::resolve(path).map_err(|e| e.to_string())?;
        let prepared = vsf_application::atomic_file::prepare(&path, &data)?;
        let mut store = self.0.lock().map_err(|_| "Document unavailable")?;
        store.check_review_key(key)?;
        if store.checkpoint_key() != checkpoint {
            return Err("Workspace changed while exporting".into());
        }
        if !importable {
            prepared.commit_new()?;
            return store.review_reply(None, 0, 0, None);
        }
        store.prospective_review_epoch()?;
        prepared.commit_new()?;
        let r = &mut store.loaded_mut()?.review;
        r.discussion.round = p.round;
        r.discussion.exported.extend(
            p.threads
                .iter()
                .flat_map(|t| t.messages.iter().map(|m| m.id.clone())),
        );
        r.reference.dirty = true;
        store.advance_discussion(key, false)?;
        store.review_reply(None, 0, 0, None)
    }
}
