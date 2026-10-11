//! One background checkpoint owner. Temporary writes stay outside the document
//! mutex; the final version check and atomic promotion are serialized with edits.
use super::{Documents, Result};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use subtitle_formats::srt;
#[derive(Default)]
pub(super) struct RecoveryControl {
    pub(super) stop: Arc<std::sync::atomic::AtomicBool>,
    pub(super) thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}
impl Drop for RecoveryControl {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Ok(Some(handle)) = self.thread.get_mut() {
            handle.thread().unpark();
        }
    }
}
impl Documents {
    pub fn start_recovery(&self, root: PathBuf) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| "Document worker unavailable")?
            .recovery_root = Some(root.clone());
        let weak = Arc::downgrade(&self.0);
        let stop = self.1.stop.clone();
        let handle = std::thread::Builder::new()
            .name("subtitle-recovery".into())
            .spawn(move || {
                // Storage can become available after launch. Keep the same
                // worker alive to retry, without retaining a document snapshot
                // or its mutex while waiting. Shutdown unparks this wait too.
                let session = loop {
                    if stop.load(std::sync::atomic::Ordering::Acquire) {
                        return;
                    }
                    let Some(shared) = weak.upgrade() else { return };
                    match crate::recovery_store::Session::create(&root) {
                        Ok(session) => break session,
                        Err(error) => {
                            let Ok(mut store) = shared.lock() else { return };
                            store.recovery_error = Some(error);
                        }
                    }
                    drop(shared);
                    std::thread::park_timeout(std::time::Duration::from_secs(2));
                };
                let mut previous = None;
                loop {
                    let stopping = stop.load(std::sync::atomic::Ordering::Acquire);
                    let Some(shared) = weak.upgrade() else { return };
                    let (key, image) = {
                        let Ok(store) = shared.lock() else { return };
                        let key = store.checkpoint_key();
                        let image = if Some(&key) != previous.as_ref() && key.2 {
                            store.loaded.as_ref().map(|d| {
                                (
                                    d.editor.document().clone(),
                                    d.source.clone(),
                                    d.path.clone(),
                                    d.saved_bytes.clone(),
                                )
                            })
                        } else {
                            None
                        };
                        (key, image)
                    };
                    if Some(&key) != previous.as_ref() {
                        // Serialization, temporary IO and fsync happen without
                        // the document lock. Only the atomic rename/clear holds it.
                        let prepared = (|| -> Result<Option<crate::atomic_file::Prepared>> {
                            match image {
                                Some((document, source, path, saved)) => {
                                    let saved =
                                        String::from_utf8(saved).map_err(|e| e.to_string())?;
                                    let record = crate::recovery_store::Record::new(
                                        path.map(|path| path.to_string_lossy().into_owned()),
                                        saved,
                                        srt::serialize(&document, &source)?,
                                        key.0,
                                        key.1,
                                    )?;
                                    session.prepare(&record).map(Some)
                                }
                                None => Ok(None),
                            }
                        })();
                        let Ok(mut store) = shared.lock() else { return };
                        if store.checkpoint_key() == key {
                            let result = prepared.and_then(|prepared| {
                                store.publish_checkpoint(&key, prepared, &session)
                            });
                            match result {
                                Ok(true) => {
                                    previous = Some(key);
                                    store.recovery_error = None;
                                }
                                Ok(false) => {}
                                Err(error) => store.recovery_error = Some(error),
                            }
                        }
                    }
                    if stopping {
                        return;
                    }
                    drop(shared);
                    std::thread::park_timeout(std::time::Duration::from_secs(2));
                }
            })
            .map_err(|e| format!("Cannot start recovery worker: {e}"))?;
        *self
            .1
            .thread
            .lock()
            .map_err(|_| "Recovery worker unavailable")? = Some(handle);
        Ok(())
    }
}
