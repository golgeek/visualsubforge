use super::{
    Budget, Cancellation, Error, Proposal, Result, SubtitleSnapshot,
    identity::{self, FileIdentity},
};
use media_analysis::{Tools, alignment, scenes::visual, tools::Versions};
use serde::Serialize;
use std::{io::Write, path::PathBuf, time::Duration};

pub const ALGORITHM_VERSION: &str = "scene-visual-sampled-v2";
pub const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;

pub enum Subtitles {
    File(PathBuf),
    Snapshot(SubtitleSnapshot),
}
pub struct Request {
    pub source_video: PathBuf,
    pub target_video: PathBuf,
    pub subtitles: Subtitles,
}

#[derive(Clone, Copy, Debug)]
pub enum Stage {
    ReadInputs,
    ToolVersions,
    SourceCuts,
    SourceSamples,
    TargetCuts,
    TargetSamples,
    Match,
    VerifySamples,
    Propose,
    VerifyInputs,
}

pub struct Analysis {
    pub(super) proposal: Proposal,
    pub(super) source_video: FileIdentity,
    pub(super) target_video: FileIdentity,
    pub(super) subtitle_file: Option<FileIdentity>,
    pub(super) versions: Versions,
}

/// First video stream only, normalized to each container's origin. Children and
/// decode passes run sequentially; no native playback or GUI is required.
pub fn analyze(
    tools: &Tools,
    request: Request,
    cancel: &Cancellation,
    timeout: Duration,
    mut progress: impl FnMut(Stage),
) -> Result<Analysis> {
    let budget = Budget::new(cancel, timeout);
    progress(Stage::ReadInputs);
    let (snapshot, subtitle_file) = match request.subtitles {
        Subtitles::File(path) => {
            let (identity, bytes) = identity::read_subtitles(&path, &budget)?;
            let parsed = crate::subtitle_file::parse(&bytes).map_err(Error::InvalidInput)?;
            (
                SubtitleSnapshot::new(parsed.document, parsed.source)?,
                Some(identity),
            )
        }
        Subtitles::Snapshot(snapshot) => (snapshot, None),
    };
    let source_video =
        FileIdentity::capture(&request.source_video, identity::MAX_MEDIA_BYTES, &budget)?;
    let target_video =
        FileIdentity::capture(&request.target_video, identity::MAX_MEDIA_BYTES, &budget)?;
    progress(Stage::ToolVersions);
    let versions = tools.versions(cancel, budget.remaining()?.min(Duration::from_secs(10)))?;
    progress(Stage::SourceCuts);
    let source_cuts = visual::extract(
        tools,
        &source_video.canonical_path,
        cancel,
        budget.remaining()?,
    )?;
    progress(Stage::SourceSamples);
    let source_samples = visual::sample(
        tools,
        &source_video.canonical_path,
        cancel,
        budget.remaining()?,
    )?;
    progress(Stage::TargetCuts);
    let target_cuts = visual::extract(
        tools,
        &target_video.canonical_path,
        cancel,
        budget.remaining()?,
    )?;
    progress(Stage::TargetSamples);
    let target_samples = visual::sample(
        tools,
        &target_video.canonical_path,
        cancel,
        budget.remaining()?,
    )?;
    progress(Stage::Match);
    let mut alignment = alignment::align(&source_cuts, &target_cuts, cancel, budget.remaining()?)?;
    progress(Stage::VerifySamples);
    alignment::verify::refine(
        &mut alignment,
        &source_cuts,
        &target_cuts,
        &source_samples,
        &target_samples,
        cancel,
        budget.remaining()?,
    )?;
    progress(Stage::Propose);
    let proposal = Proposal::build(snapshot, alignment, &budget)?;
    let analysis = Analysis {
        proposal,
        source_video,
        target_video,
        subtitle_file,
        versions,
    };
    progress(Stage::VerifyInputs);
    analysis.verify_inputs(&budget)?;
    Ok(analysis)
}

impl Analysis {
    pub fn proposal(&self) -> &Proposal {
        &self.proposal
    }
    pub fn verify_inputs(&self, budget: &Budget<'_>) -> Result<()> {
        self.source_video.verify(budget)?;
        self.target_video.verify(budget)?;
        if let Some(file) = &self.subtitle_file {
            file.verify(budget)?;
        }
        budget.check()
    }

    pub fn report_json(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct Report<'a> {
            schema_version: u32,
            algorithm_version: &'a str,
            // Fixed analysis choices are covered by algorithm_version. Recording
            // this explicitly avoids implying user-selected stream support.
            video_stream_selection: &'a str,
            timestamp_origin: &'a str,
            cue_gap_policy: &'a str,
            source_video: &'a FileIdentity,
            target_video: &'a FileIdentity,
            source_subtitle_file: &'a Option<FileIdentity>,
            subtitle_snapshot_sha256: &'a str,
            tools: &'a Versions,
            alignment: &'a alignment::Alignment,
            cues: &'a [super::CueProposal],
            complete: bool,
            proposed_srt_sha256: Option<&'a str>,
            publication: &'a str,
        }
        let report = Report {
            schema_version: 2,
            algorithm_version: ALGORITHM_VERSION,
            video_stream_selection: "first_video_stream",
            timestamp_origin: "container_start",
            cue_gap_policy: "content_mapped_endpoints_v1",
            source_video: &self.source_video,
            target_video: &self.target_video,
            source_subtitle_file: &self.subtitle_file,
            subtitle_snapshot_sha256: self.proposal.snapshot().sha256(),
            tools: &self.versions,
            alignment: self.proposal.alignment(),
            cues: self.proposal.cues(),
            complete: self.proposal.is_complete(),
            proposed_srt_sha256: self.proposal.srt_sha256(),
            publication: "Proposal only; this report does not prove SRT publication",
        };
        let mut writer = ReportWriter(Vec::new());
        serde_json::to_writer_pretty(&mut writer, &report)
            .map_err(|e| Error::InvalidInput(format!("Cannot serialize bounded report: {e}")))?;
        Ok(writer.0)
    }
}

struct ReportWriter(Vec<u8>);
impl Write for ReportWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_REPORT_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("Report exceeds 64 MiB limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
