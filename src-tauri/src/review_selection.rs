//! DTO translation for timing scope and bounded on-screen preview projections.
use crate::ipc_number::SafeU64;
use serde::Deserialize;
use std::collections::HashSet;
use subtitle_core::CueId;
use vsf_application::quality::{MAX_CUES, timing_selection};

#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Selection {
    pub scope: Option<Vec<SafeU64>>,
    pub excluded: Vec<SafeU64>,
}
impl Selection {
    pub(crate) fn domain(self) -> Result<timing_selection::Selection, String> {
        if self.scope.as_ref().is_some_and(|ids| ids.len() > MAX_CUES)
            || self.excluded.len() > MAX_CUES
        {
            return Err("Timing selection exceeds 100,000 cues".into());
        }
        Ok(timing_selection::Selection {
            scope: self
                .scope
                .map(|ids| ids.into_iter().map(|id| CueId(id.get())).collect()),
            excluded: self
                .excluded
                .into_iter()
                .map(|id| CueId(id.get()))
                .collect(),
        })
    }
}
pub(crate) fn preview_ids(ids: Option<Vec<SafeU64>>) -> Result<Option<HashSet<CueId>>, String> {
    ids.map(|ids| {
        if ids.len() > 200 {
            return Err("Timing preview projections are limited to 200 cues".into());
        }
        Ok(ids.into_iter().map(|id| CueId(id.get())).collect())
    })
    .transpose()
}
pub(crate) fn visible(ids: Option<&HashSet<CueId>>, id: &str) -> bool {
    ids.is_none_or(|ids| id.parse().ok().is_some_and(|id| ids.contains(&CueId(id))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_projection_is_bounded_and_selection_preserves_empty_scope() {
        assert!(preview_ids(Some(vec![1.try_into().unwrap(); 201])).is_err());
        let empty = preview_ids(Some(vec![])).unwrap();
        assert!(!visible(empty.as_ref(), "1"));
        assert!(visible(None, "1"));
        let selection = Selection {
            scope: Some(vec![]),
            excluded: vec![],
        }
        .domain()
        .unwrap();
        assert_eq!(selection.scope, Some(vec![]));
        assert!(
            Selection {
                scope: None,
                excluded: vec![1.try_into().unwrap(); MAX_CUES + 1]
            }
            .domain()
            .is_err()
        );
    }
}
