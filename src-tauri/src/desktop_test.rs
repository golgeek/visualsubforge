//! Compiled only by the explicit desktop-e2e feature. No document/media mocks.
//! The runner owns this data directory and the predetermined file selections.
use crate::desktop::FileChoice;
use serde::Deserialize;
use std::{
    collections::VecDeque,
    path::{Component, Path, PathBuf},
    sync::Mutex,
};

#[derive(Deserialize)]
struct Choice {
    kind: FileChoice,
    path: Option<String>,
}

pub struct Driver {
    root: PathBuf,
    choices: Mutex<VecDeque<Choice>>,
}

impl Driver {
    pub fn from_environment() -> Result<Self, String> {
        let root =
            PathBuf::from(std::env::var_os("VSF_E2E_ROOT").ok_or("VSF_E2E_ROOT is required")?);
        if !root.is_absolute() || root.parent().is_none() {
            return Err("Desktop tests require their own absolute data directory".into());
        }
        let choices: VecDeque<Choice> =
            serde_json::from_str(&std::env::var("VSF_E2E_CHOICES").unwrap_or_else(|_| "[]".into()))
                .map_err(|e| format!("Invalid test file choices: {e}"))?;
        for choice in &choices {
            if let Some(path) = &choice.path {
                let path = Path::new(path);
                if !path.starts_with(&root) || path.components().any(|c| c == Component::ParentDir)
                {
                    return Err("Test file selections must stay in the test data directory".into());
                }
            }
        }
        Ok(Self {
            root,
            choices: Mutex::new(choices),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn choose(&self, kind: FileChoice) -> Result<Option<String>, String> {
        let mut choices = self
            .choices
            .lock()
            .map_err(|_| "Test chooser lock poisoned")?;
        let choice = choices
            .front()
            .ok_or_else(|| format!("No queued test selection for {kind:?}"))?;
        if choice.kind != kind {
            return Err(format!(
                "Expected test selection {:?}, received {kind:?}",
                choice.kind
            ));
        }
        Ok(choices.pop_front().expect("front was present").path)
    }
}
