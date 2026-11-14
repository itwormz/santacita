//! One running copy per data folder. The lock file is held for the life of the process; a second copy that finds
//! it taken leaves a request file and exits, and the first copy shows its window when it sees the request.
use std::{fs::File, io, path::{Path, PathBuf}, time::{Duration, Instant}};

pub struct InstanceLock {
    _file: File,
    request: PathBuf,
}
pub enum Acquire {
    Owned(InstanceLock),
    /// Another copy runs on this folder; it has been asked to show its window.
    AlreadyRunning,
    /// The lock file cannot be used (read-only folder, ...): start anyway rather than lock the person out.
    Unavailable,
}
impl InstanceLock {
    /// Lock `<root>/instance.lock`, waiting up to `wait` for a previous copy that is still closing.
    pub fn acquire(root: &Path, wait: Duration) -> Acquire {
        let path = root.join("instance.lock");
        let request = root.join("instance.show");
        let Ok(file) = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path) else {
            return Acquire::Unavailable;
        };
        let until = Instant::now() + wait;
        loop {
            match ceh_store::file_lock::try_exclusive(&file) {
                Ok(()) => return Acquire::Owned(Self { _file: file, request }),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= until {
                        let _ = std::fs::write(&request, b"show");
                        return Acquire::AlreadyRunning;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return Acquire::Unavailable,
            }
        }
    }
    /// Whether a second copy asked this one to show itself (read once).
    pub fn take_activation_request(&self) -> bool {
        std::fs::remove_file(&self.request).is_ok()
    }
}
