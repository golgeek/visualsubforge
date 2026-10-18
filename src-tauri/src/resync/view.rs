//! Bounded review pages; the browser receives no candidate subtitle file or mutable proposal.
use super::*;
use crate::ipc_number::{SafeU64, finite::FiniteF64};
use serde::Serialize;
use vsf_application::resync::{CueOutcome, UnresolvedReason};

const PAGE_SIZE: usize = 100;

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Analyzing,
    Cancelling,
    Reviewing,
    Accepting,
    Accepted,
    Cancelled,
    Failed,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct View {
    id: SafeU64,
    phase: Phase,
    stage: String,
    error: Option<String>,
    complete: bool,
    format: String,
    time_step_ms: u32,
    total_cues: u32,
    unresolved_cues: u32,
    total_sections: u32,
    cue_offset: u32,
    section_offset: u32,
    cues: Vec<CueRow>,
    sections: Vec<SectionRow>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct CueRow {
    id: String,
    text: String,
    source_start_ms: SafeU64,
    source_end_ms: SafeU64,
    start_ms: Option<SafeU64>,
    end_ms: Option<SafeU64>,
    issue: Option<String>,
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct SectionRow {
    source_start_us: SafeU64,
    source_end_us: SafeU64,
    target_start_us: SafeU64,
    target_end_us: SafeU64,
    anchors: u32,
    max_residual_us: FiniteF64,
}

impl Resync {
    pub(super) fn view(
        &self,
        id: u64,
        cue_offset: u32,
        section_offset: u32,
        media: &Media,
        documents: &Documents,
    ) -> Result<View> {
        let mut owner = self.owner.lock().map_err(|_| "Resync state unavailable")?;
        let accepting = owner.accepting;
        let job = owner
            .job
            .as_mut()
            .filter(|job| job.id == id)
            .ok_or("Resync job was replaced")?;
        job.refresh();
        if !job.accepted
            && (media.current_session() != job.session
                || !documents.resync_is_current(&job.capture))
        {
            job.cancel();
            job.error = Some("Subtitles or target video changed; analyze again".into());
        }
        let phase = if accepting {
            Phase::Accepting
        } else if job.worker.is_some() {
            if job.cancel.is_cancelled() {
                Phase::Cancelling
            } else {
                Phase::Analyzing
            }
        } else if job.accepted {
            Phase::Accepted
        } else if job.error.is_some() {
            Phase::Failed
        } else if job.cancel.is_cancelled() {
            Phase::Cancelled
        } else {
            Phase::Reviewing
        };
        let stage = stage_text(
            *job.stage
                .lock()
                .map_err(|_| "Resync progress unavailable")?,
        )
        .into();
        let mut view = View {
            id: SafeU64::try_from(id)?,
            phase,
            stage,
            error: job.error.clone(),
            complete: false,
            format: job.capture.snapshot.format().extension().into(),
            time_step_ms: job.capture.snapshot.format().time_step_ms() as u32,
            total_cues: 0,
            unresolved_cues: 0,
            total_sections: 0,
            cue_offset,
            section_offset,
            cues: vec![],
            sections: vec![],
        };
        if let Some(result) = &job.result {
            let proposal = result.proposal();
            view.complete = proposal.is_complete();
            view.total_cues = count(proposal.cues().len())?;
            view.unresolved_cues = count(
                proposal
                    .cues()
                    .iter()
                    .filter(|cue| matches!(cue.outcome, CueOutcome::Unresolved { .. }))
                    .count(),
            )?;
            view.total_sections = count(proposal.alignment().segments.len())?;
            for (cue, source) in proposal
                .cues()
                .iter()
                .zip(proposal.snapshot().document().cues())
                .skip(cue_offset as usize)
                .take(PAGE_SIZE)
            {
                let (start_ms, end_ms, issue) = match cue.outcome {
                    CueOutcome::Mapped {
                        start_ms, end_ms, ..
                    } => (
                        Some(SafeU64::try_from(start_ms)?),
                        Some(SafeU64::try_from(end_ms)?),
                        None,
                    ),
                    CueOutcome::Unresolved { reason } => (
                        None,
                        None,
                        Some(
                            match reason {
                                UnresolvedReason::ReorderedContent => "Content order differs",
                                UnresolvedReason::UnsupportedCoverage => {
                                    "No reliable matching content"
                                }
                                UnresolvedReason::CrossesDiscontinuity => {
                                    "Cue crosses an edit or unsupported section"
                                }
                                UnresolvedReason::TimedEffectsNeedReview => {
                                    if proposal.snapshot().format() == subtitle_formats::Format::WebVtt {
                                        "Timing change affects absolute WebVTT markers or ambiguous markup; manual review required"
                                    } else {
                                        "Duration change affects timed tags, event effects or ambiguous markup; manual review required"
                                    }
                                }
                                UnresolvedReason::InvalidMappedRange => {
                                    "Mapped cue has no valid duration"
                                }
                            }
                            .into(),
                        ),
                    ),
                };
                view.cues.push(CueRow {
                    id: cue.cue_id.clone(),
                    text: source.text().into(),
                    source_start_ms: SafeU64::try_from(cue.source_start_ms)?,
                    source_end_ms: SafeU64::try_from(cue.source_end_ms)?,
                    start_ms,
                    end_ms,
                    issue,
                });
            }
            for section in proposal
                .alignment()
                .segments
                .iter()
                .skip(section_offset as usize)
                .take(PAGE_SIZE)
            {
                view.sections.push(SectionRow {
                    source_start_us: SafeU64::try_from(section.source_start_us)?,
                    source_end_us: SafeU64::try_from(section.source_end_us)?,
                    target_start_us: SafeU64::try_from(section.target_start_us)?,
                    target_end_us: SafeU64::try_from(section.target_end_us)?,
                    anchors: count(section.anchors)?,
                    max_residual_us: FiniteF64::try_from(section.max_residual_us)?,
                });
            }
        }
        Ok(view)
    }
}
fn count(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| "Resync result exceeds display limits".into())
}
fn stage_text(stage: Stage) -> &'static str {
    match stage {
        Stage::ReadInputs => "Checking source and target files",
        Stage::ToolVersions => "Checking analysis tools",
        Stage::SourceCuts => "Finding scene changes in the source video",
        Stage::SourceSamples => "Sampling the source video",
        Stage::TargetCuts => "Finding scene changes in the target video",
        Stage::TargetSamples => "Sampling the target video",
        Stage::Match => "Matching content",
        Stage::VerifySamples => "Verifying matched sections",
        Stage::Propose => "Calculating subtitle timings",
        Stage::VerifyInputs => "Checking that input files are unchanged",
    }
}

#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    serde_json::to_value(View {
        id: SafeU64::try_from(crate::ipc_number::MAX_SAFE_INTEGER).unwrap(),
        phase: Phase::Reviewing,
        stage: stage_text(Stage::Propose).into(),
        error: None,
        complete: false,
        format: "srt".into(),
        time_step_ms: 1,
        total_cues: 2,
        unresolved_cues: 1,
        total_sections: 1,
        cue_offset: 0,
        section_offset: 0,
        cues: vec![
            CueRow {
                id: "1".into(),
                text: "Mapped".into(),
                source_start_ms: SafeU64::try_from(1000).unwrap(),
                source_end_ms: SafeU64::try_from(2000).unwrap(),
                start_ms: Some(SafeU64::try_from(3000).unwrap()),
                end_ms: Some(SafeU64::try_from(4000).unwrap()),
                issue: None,
            },
            CueRow {
                id: "2".into(),
                text: "Missing section".into(),
                source_start_ms: SafeU64::try_from(6000).unwrap(),
                source_end_ms: SafeU64::try_from(7000).unwrap(),
                start_ms: None,
                end_ms: None,
                issue: Some("No reliable matching content".into()),
            },
        ],
        sections: vec![SectionRow {
            source_start_us: SafeU64::try_from(0).unwrap(),
            source_end_us: SafeU64::try_from(5_000_000).unwrap(),
            target_start_us: SafeU64::try_from(2_000_000).unwrap(),
            target_end_us: SafeU64::try_from(7_000_000).unwrap(),
            anchors: 3,
            max_residual_us: FiniteF64::try_from(12.5).unwrap(),
        }],
    })
    .unwrap()
}
