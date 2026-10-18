//! Conservative structural-edit policy, independent of desktop and renderer.
use crate::subtitle_markup::{MAX_CUE_BYTES, Projection};
use subtitle_core::{Cue, Result};

pub fn validate_split(cue: &Cue, format: subtitle_formats::Format) -> Result<()> {
    if cue
        .metadata()
        .and_then(|m| m.get("Effect"))
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(
            "Split cannot preserve cue-relative event effects; edit these cues manually".into(),
        );
    }
    if !cue.text().contains("{\\") && !cue.text().contains('<') {
        return Ok(());
    }
    if cue.text().len() > MAX_CUE_BYTES {
        return Err("Split requires manual review for text larger than 64 KiB".into());
    }
    let projection = Projection::for_format(cue.text(), format);
    if projection.timed || projection.ambiguous.is_some() {
        return Err("Split cannot preserve cue-relative effects or ambiguous markup; edit these cues manually".into());
    }
    Ok(())
}

pub fn validate_merge(first: &Cue, second: &Cue, format: subtitle_formats::Format) -> Result<()> {
    for cue in [first, second] {
        validate_split(cue, format)?;
        if !cue.text().contains("{\\") && !cue.text().contains('<') {
            continue;
        }
        let projection = Projection::for_format(cue.text(), format);
        if projection.layout
            || (format == subtitle_formats::Format::Srt && cue.text().contains("{\\"))
        {
            return Err("Merge cannot preserve independent positioning or mixed SRT/ASS style and escape contexts; edit these cues manually".into());
        }
    }
    Ok(())
}
