//! One lazy worker and one replaceable pending request per analysis service.
//! Replacement cancels active work but waits for it to return before starting
//! the latest request. Ordinary blocking filesystem calls cannot be interrupted;
//! this bounds admission, not shutdown latency. Drain and shutdown wait only on
//! background threads. Drop is a cancellation fallback, never a blocking join.
use media_analysis::Cancellation;
use std::sync::{Arc, Condvar, Mutex};

type Result<T> = std::result::Result<T, String>;
struct Request {
    cancellation: Cancellation,
    run: Box<dyn FnOnce() + Send>,
}
#[derive(Default)]
struct Queue {
    closing: bool,
    active: Option<Cancellation>,
    pending: Option<Request>,
}
impl Queue {
    fn cancel(&mut self) {
        if let Some(active) = &self.active {
            active.cancel();
        }
        if let Some(pending) = self.pending.take() {
            pending.cancellation.cancel();
        }
    }
}
#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    idle: Condvar,
}
pub(crate) struct Worker {
    name: &'static str,
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            shared: Arc::default(),
            thread: None,
        }
    }
    pub fn is_closing(&self) -> bool {
        self.shared.queue.lock().map_or(true, |queue| queue.closing)
    }
    pub fn replace(
        &mut self,
        cancellation: Cancellation,
        run: impl FnOnce() + Send + 'static,
    ) -> Result<()> {
        let mut queue = self
            .shared
            .queue
            .lock()
            .map_err(|_| "Analysis queue unavailable")?;
        if queue.closing {
            return Err("Application is shutting down".into());
        }
        if self
            .thread
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            return Err(format!("{} worker stopped unexpectedly", self.name));
        }
        if self.thread.is_none() {
            let shared = self.shared.clone();
            self.thread = Some(
                std::thread::Builder::new()
                    .name(self.name.into())
                    .spawn(move || run_worker(shared))
                    .map_err(|error| format!("Cannot start {}: {error}", self.name))?,
            );
        }
        queue.cancel();
        queue.pending = Some(Request {
            cancellation,
            run: Box::new(run),
        });
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn cancel(&mut self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.cancel();
        }
    }
    /// Cancel and reap active analysis without retiring the reusable worker.
    /// Call only on a background thread while holding the service's worker lock.
    pub fn cancel_and_wait(&mut self) -> Result<()> {
        let mut queue = self
            .shared
            .queue
            .lock()
            .map_err(|_| "Analysis queue unavailable")?;
        queue.cancel();
        while queue.active.is_some() {
            if self
                .thread
                .as_ref()
                .is_some_and(|thread| thread.is_finished())
            {
                return Err("Analysis worker stopped before cleanup completed".into());
            }
            // A panicked closure cannot signal idle; periodically detect that
            // terminal state instead of waiting forever on its stale active flag.
            queue = self
                .shared
                .idle
                .wait_timeout(queue, std::time::Duration::from_millis(100))
                .map_err(|_| "Analysis queue unavailable")?
                .0;
        }
        Ok(())
    }
    fn close(&mut self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.closing = true;
            queue.cancel();
        }
        self.shared.wake.notify_one();
    }
    pub fn shutdown(&mut self) {
        self.close();
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.close();
    }
}
fn run_worker(shared: Arc<Shared>) {
    loop {
        let request = {
            let Ok(mut queue) = shared.queue.lock() else {
                return;
            };
            while !queue.closing && queue.pending.is_none() {
                let Ok(next) = shared.wake.wait(queue) else {
                    return;
                };
                queue = next;
            }
            if queue.closing {
                return;
            }
            let Some(request) = queue.pending.take() else {
                continue;
            };
            queue.active = Some(request.cancellation.clone());
            request
        };
        // Never hold the queue mutex during work or a service's result callback.
        if !request.cancellation.is_cancelled() {
            (request.run)();
        }
        let Ok(mut queue) = shared.queue.lock() else {
            return;
        };
        queue.active = None;
        shared.idle.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn drain_waits_for_cleanup_drops_pending_and_keeps_worker_reusable() {
        let mut worker = Worker::new("drain-analysis");
        let active = Cancellation::default();
        let (started, started_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        worker
            .replace(active.clone(), move || {
                started.send(std::thread::current().id()).unwrap();
                release_rx.recv_timeout(WAIT).unwrap();
            })
            .unwrap();
        let original_thread = started_rx.recv_timeout(WAIT).unwrap();
        let pending = Cancellation::default();
        worker
            .replace(pending.clone(), || panic!("Cancelled pending work ran"))
            .unwrap();
        let shared = worker.shared.clone();
        let (drained, drained_rx) = mpsc::channel();
        let drain = std::thread::spawn(move || {
            worker.cancel_and_wait().unwrap();
            drained.send(()).unwrap();
            worker
        });
        let deadline = std::time::Instant::now() + WAIT;
        while shared.queue.lock().unwrap().pending.is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(active.is_cancelled() && pending.is_cancelled());
        assert!(matches!(
            drained_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        release.send(()).unwrap();
        drained_rx.recv_timeout(WAIT).unwrap();
        let mut worker = drain.join().unwrap();
        let (reused, reused_rx) = mpsc::channel();
        worker
            .replace(Cancellation::default(), move || {
                reused.send(std::thread::current().id()).unwrap();
            })
            .unwrap();
        assert_eq!(reused_rx.recv_timeout(WAIT).unwrap(), original_thread);
        worker.shutdown();
    }

    #[test]
    fn drain_reports_a_panicked_worker_instead_of_waiting_forever() {
        let mut worker = Worker::new("panicked-analysis");
        let (started, started_rx) = mpsc::channel();
        worker
            .replace(Cancellation::default(), move || {
                started.send(()).unwrap();
                panic!("Controlled worker failure");
            })
            .unwrap();
        started_rx.recv_timeout(WAIT).unwrap();
        assert!(worker.cancel_and_wait().is_err());
        worker.shutdown();
    }

    #[test]
    fn replacements_coalesce_while_cancelled_work_is_still_running() {
        let mut worker = Worker::new("held-analysis");
        let first = Cancellation::default();
        let (started, started_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let (finished, finished_rx) = mpsc::channel();
        worker
            .replace(first.clone(), move || {
                started.send(std::thread::current().id()).unwrap();
                release_rx.recv_timeout(WAIT).unwrap();
            })
            .unwrap();
        let thread = started_rx.recv_timeout(WAIT).unwrap();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let mut tokens = Vec::new();
        for id in 1..=200 {
            let token = Cancellation::default();
            tokens.push(token.clone());
            let ran = ran.clone();
            let finished = finished.clone();
            worker
                .replace(token, move || {
                    ran.lock().unwrap().push(id);
                    finished.send(std::thread::current().id()).unwrap();
                })
                .unwrap();
        }
        assert!(first.is_cancelled());
        assert!(tokens[..199].iter().all(Cancellation::is_cancelled));
        assert!(!tokens[199].is_cancelled());
        assert!(ran.lock().unwrap().is_empty());
        assert!(worker.shared.queue.lock().unwrap().pending.is_some());
        release.send(()).unwrap();
        assert_eq!(finished_rx.recv_timeout(WAIT).unwrap(), thread);
        worker.shutdown();
        assert_eq!(*ran.lock().unwrap(), vec![200]);
        assert!(worker.thread.is_none());
    }

    #[test]
    fn shutdown_cancels_pending_work_and_waits_for_the_active_owner() {
        let mut worker = Worker::new("shutdown-analysis");
        let token = Cancellation::default();
        let (started, started_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        worker
            .replace(token.clone(), move || {
                started.send(()).unwrap();
                release_rx.recv_timeout(WAIT).unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(WAIT).unwrap();
        let pending = Cancellation::default();
        let (ran, ran_rx) = mpsc::channel();
        worker
            .replace(pending.clone(), move || {
                ran.send(()).unwrap();
            })
            .unwrap();
        let shared = worker.shared.clone();
        let (done, done_rx) = mpsc::channel();
        let shutdown = std::thread::spawn(move || {
            worker.shutdown();
            done.send(()).unwrap();
            worker
        });
        let deadline = std::time::Instant::now() + WAIT;
        while !shared.queue.lock().unwrap().closing {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(token.is_cancelled() && pending.is_cancelled());
        assert!(shared.queue.lock().unwrap().pending.is_none());
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        release.send(()).unwrap();
        done_rx.recv_timeout(WAIT).unwrap();
        let mut worker = shutdown.join().unwrap();
        assert!(worker.thread.is_none());
        assert!(worker.replace(Cancellation::default(), || {}).is_err());
        assert!(ran_rx.try_recv().is_err());
    }

    #[test]
    fn cancellation_drops_pending_work_and_allows_a_later_request() {
        let mut worker = Worker::new("cancel-analysis");
        let first = Cancellation::default();
        let (started, started_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        worker
            .replace(first.clone(), move || {
                started.send(()).unwrap();
                release_rx.recv_timeout(WAIT).unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(WAIT).unwrap();
        let pending = Cancellation::default();
        let (ran, ran_rx) = mpsc::channel();
        worker
            .replace(pending.clone(), move || {
                ran.send(()).unwrap();
            })
            .unwrap();
        worker.cancel();
        assert!(first.is_cancelled() && pending.is_cancelled());
        assert!(worker.shared.queue.lock().unwrap().pending.is_none());
        let (new, new_rx) = mpsc::channel();
        worker
            .replace(Cancellation::default(), move || {
                new.send(()).unwrap();
            })
            .unwrap();
        release.send(()).unwrap();
        new_rx.recv_timeout(WAIT).unwrap();
        worker.shutdown();
        assert!(ran_rx.try_recv().is_err());
    }
}
