//! The download of an update running in the background: its progress, its result, and the one thing the
//! person can do with it (open the verified package). Nothing is installed by this app itself.
use ceh_services::updates::UpdateError;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Downloading,
    Ready,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Status {
    pub phase: Phase,
    pub percent: u8,
    pub error: Option<&'static str>,
}
#[derive(Default)]
struct Inner {
    phase: Option<Phase>,
    percent: u8,
    error: Option<&'static str>,
    path: Option<PathBuf>,
}
/// The state is plain data, so a thread that died holding the lock leaves nothing half-written worth refusing.
fn lock(state: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
#[derive(Clone, Default)]
pub struct UpdateJob {
    inner: Arc<Mutex<Inner>>,
    cancel: Arc<AtomicBool>,
}
impl UpdateJob {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn status(&self) -> Status {
        let inner = lock(&self.inner);
        Status { phase: inner.phase.unwrap_or(Phase::Idle), percent: inner.percent, error: inner.error }
    }
    /// The verified package, only while a finished download is current.
    pub fn install_path(&self) -> Option<PathBuf> {
        let inner = lock(&self.inner);
        (inner.phase == Some(Phase::Ready)).then(|| inner.path.clone()).flatten()
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    /// Run `work` on its own thread; `false` while another download runs.
    pub fn start_with<F>(&self, work: F) -> bool
    where
        F: FnOnce(&AtomicBool, &mut dyn FnMut(u8)) -> Result<PathBuf, UpdateError> + Send + 'static,
    {
        {
            let mut inner = lock(&self.inner);
            if inner.phase == Some(Phase::Downloading) {
                return false;
            }
            *inner = Inner { phase: Some(Phase::Downloading), ..Inner::default() };
        }
        self.cancel.store(false, Ordering::Release);
        let (state, cancel) = (self.inner.clone(), self.cancel.clone());
        std::thread::spawn(move || {
            let progress_state = state.clone();
            let result = work(&cancel, &mut |percent| {
                lock(&progress_state).percent = percent;
            });
            let mut inner = lock(&state);
            match result {
                Ok(path) => {
                    inner.phase = Some(Phase::Ready);
                    inner.percent = 100;
                    inner.path = Some(path);
                }
                Err(error) => {
                    inner.phase = Some(Phase::Failed);
                    inner.error = Some(match error {
                        UpdateError::Cancelled => "cancelled",
                        UpdateError::Network => "network",
                        UpdateError::Invalid => "verification_failed",
                    });
                }
            }
        });
        true
    }
    pub fn clear_ready(&self) {
        let mut inner=lock(&self.inner);
        if inner.phase!=Some(Phase::Downloading){*inner=Inner::default();}
    }
    /// Download and verify the latest release for this system into `dir`.
    pub fn start(&self, current: &'static str, dir: PathBuf) -> bool {
        self.start_channel(current,dir,false)
    }
    pub fn start_channel(&self,current:&'static str,dir:PathBuf,include_beta:bool)->bool{
        self.start_with(move |cancel,progress|ceh_services::updates::download_channel(current,&dir,cancel,progress,include_beta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_state_lock_does_not_take_the_update_screen_down() {
        let job = UpdateJob::new();
        let inner = job.inner.clone();
        let _ = std::thread::spawn(move || {
            let _held = inner.lock().unwrap();
            panic!("a thread died holding the update state");
        })
        .join();
        assert_eq!(job.status().phase, Phase::Idle);
        assert!(job.install_path().is_none());
        job.clear_ready();
        assert!(job.start_with(|_, _| Err(UpdateError::Cancelled)));
    }

    #[test]
    fn a_download_after_a_poisoned_lock_still_shows_its_progress() {
        let job = UpdateJob::new();
        let inner = job.inner.clone();
        let _ = std::thread::spawn(move || {
            let _held = inner.lock().unwrap();
            panic!("a thread died holding the update state");
        })
        .join();
        let (seen, shown) = std::sync::mpsc::channel();
        let watcher = job.clone();
        assert!(job.start_with(move |_, progress| {
            progress(42);
            let _ = seen.send(watcher.status().percent);
            Err(UpdateError::Cancelled)
        }));
        let percent = shown.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(percent, 42, "the progress of the running download was dropped");
    }
}
