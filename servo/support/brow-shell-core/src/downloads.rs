/* Download manager: a validated state machine with progress accounting. */

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::bookmarks::write_atomic;
use crate::StoreError;

pub type DownloadId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    Queued,
    Connecting,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadState {
    pub fn is_terminal(self) -> bool {
        matches!(self, DownloadState::Completed | DownloadState::Failed | DownloadState::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: DownloadId,
    pub url: String,
    pub destination: PathBuf,
    pub state: DownloadState,
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    /// Exponentially-weighted moving average of speed, bytes/sec.
    pub speed_bps: f64,
    pub error: Option<String>,
    pub started_at_unix: u64,
    pub finished_at_unix: Option<u64>,
    #[serde(skip)]
    last_sample: Option<(u64, f64)>, // (received, monotonic seconds)
}

impl Download {
    /// Estimated seconds remaining, from the EMA speed.
    pub fn eta_secs(&self) -> Option<f64> {
        let total = self.total_bytes?;
        if self.speed_bps <= 0.0 {
            return None;
        }
        let remaining = total.saturating_sub(self.received_bytes) as f64;
        Some(remaining / self.speed_bps)
    }

    pub fn progress_fraction(&self) -> Option<f64> {
        let total = self.total_bytes?;
        if total == 0 {
            return None;
        }
        Some((self.received_bytes as f64 / total as f64).clamp(0.0, 1.0))
    }
}

/// Errors for invalid state transitions.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DownloadError {
    #[error("unknown download {0}")]
    Unknown(DownloadId),
    #[error("invalid transition: {0:?} is not startable")]
    NotStartable(DownloadState),
    #[error("invalid transition: cannot pause {0:?}")]
    NotPausable(DownloadState),
    #[error("invalid transition: cannot resume {0:?}")]
    NotResumable(DownloadState),
    #[error("invalid transition: cannot cancel {0:?}")]
    NotCancellable(DownloadState),
    #[error("download {0} already finished")]
    Finished(DownloadId),
}

const SPEED_EMA_ALPHA: f64 = 0.3;

/// Owns downloads and validates every transition.
#[derive(Debug, Default)]
pub struct DownloadManager {
    downloads: Vec<Download>,
    next_id: DownloadId,
    path: Option<PathBuf>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let downloads: Vec<Download> = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(path)?)?
        } else {
            Vec::new()
        };
        let next_id = downloads.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        Ok(Self {
            downloads,
            next_id,
            path: Some(path.to_path_buf()),
        })
    }

    pub fn downloads(&self) -> &[Download] {
        &self.downloads
    }

    pub fn get(&self, id: DownloadId) -> Result<&Download, DownloadError> {
        self.downloads.iter().find(|d| d.id == id).ok_or(DownloadError::Unknown(id))
    }

    fn get_mut(&mut self, id: DownloadId) -> Result<&mut Download, DownloadError> {
        self.downloads.iter_mut().find(|d| d.id == id).ok_or(DownloadError::Unknown(id))
    }

    /// Enqueue a new download in the `Queued` state.
    pub fn enqueue(
        &mut self,
        url: &str,
        destination: PathBuf,
        total_bytes: Option<u64>,
        started_at_unix: u64,
    ) -> DownloadId {
        let id = self.next_id;
        self.next_id += 1;
        self.downloads.push(Download {
            id,
            url: url.to_string(),
            destination,
            state: DownloadState::Queued,
            received_bytes: 0,
            total_bytes,
            speed_bps: 0.0,
            error: None,
            started_at_unix,
            finished_at_unix: None,
            last_sample: None,
        });
        self.persist();
        id
    }

    pub fn start(&mut self, id: DownloadId) -> Result<(), DownloadError> {
        let d = self.get_mut(id)?;
        match d.state {
            DownloadState::Queued | DownloadState::Paused => {
                d.state = DownloadState::Connecting;
                d.last_sample = None;
                self.persist();
                Ok(())
            }
            s => Err(DownloadError::NotStartable(s)),
        }
    }

    /// The network layer reports that the connection is established.
    pub fn mark_connecting(&mut self, id: DownloadId) -> Result<(), DownloadError> {
        let d = self.get_mut(id)?;
        if d.state == DownloadState::Connecting {
            d.state = DownloadState::Downloading;
            self.persist();
        }
        Ok(())
    }

    /// Progress callback. Updates bytes + EMA speed. `monotonic_secs` should
    /// come from a monotonic clock (e.g. `Instant::elapsed` totals).
    pub fn progress(
        &mut self,
        id: DownloadId,
        received_bytes: u64,
        total_bytes: Option<u64>,
        monotonic_secs: f64,
    ) -> Result<(), DownloadError> {
        let d = self.get_mut(id)?;
        if d.state.is_terminal() {
            return Err(DownloadError::Finished(id));
        }
        if let Some(total) = total_bytes {
            d.total_bytes = Some(total);
        }
        if let Some((last_bytes, last_time)) = d.last_sample {
            let dt = monotonic_secs - last_time;
            if dt > 0.05 {
                let inst = (received_bytes.saturating_sub(last_bytes)) as f64 / dt;
                d.speed_bps = if d.speed_bps == 0.0 {
                    inst
                } else {
                    SPEED_EMA_ALPHA * inst + (1.0 - SPEED_EMA_ALPHA) * d.speed_bps
                };
            }
        }
        d.last_sample = Some((received_bytes, monotonic_secs));
        d.received_bytes = received_bytes;
        if d.state == DownloadState::Connecting {
            d.state = DownloadState::Downloading;
        }
        self.persist();
        Ok(())
    }

    pub fn pause(&mut self, id: DownloadId) -> Result<(), DownloadError> {
        let d = self.get_mut(id)?;
        match d.state {
            DownloadState::Connecting | DownloadState::Downloading => {
                d.state = DownloadState::Paused;
                d.speed_bps = 0.0;
                d.last_sample = None;
                self.persist();
                Ok(())
            }
            s => Err(DownloadError::NotPausable(s)),
        }
    }

    pub fn resume(&mut self, id: DownloadId) -> Result<(), DownloadError> {
        self.start(id)
    }

    pub fn complete(&mut self, id: DownloadId, finished_at_unix: u64) -> Result<(), DownloadError> {
        let d = self.get_mut(id)?;
        if d.state.is_terminal() {
            return Err(DownloadError::Finished(id));
        }
        d.state = DownloadState::Completed;
        d.finished_at_unix = Some(finished_at_unix);
        d.speed_bps = 0.0;
        self.persist();
        Ok(())
    }

    pub fn fail(&mut self, id: DownloadId, error: &str, finished_at_unix: u64) -> Result<(), DownloadError> {
        let d = self.get_mut(id)?;
        if d.state.is_terminal() {
            return Err(DownloadError::Finished(id));
        }
        d.state = DownloadState::Failed;
        d.error = Some(error.to_string());
        d.finished_at_unix = Some(finished_at_unix);
        d.speed_bps = 0.0;
        self.persist();
        Ok(())
    }

    pub fn cancel(&mut self, id: DownloadId, finished_at_unix: u64) -> Result<(), DownloadError> {
        let d = self.get_mut(id)?;
        match d.state {
            DownloadState::Completed | DownloadState::Failed | DownloadState::Cancelled => {
                Err(DownloadError::NotCancellable(d.state))
            }
            _ => {
                d.state = DownloadState::Cancelled;
                d.finished_at_unix = Some(finished_at_unix);
                d.speed_bps = 0.0;
                self.persist();
                Ok(())
            }
        }
    }

    /// Drop terminal downloads from the list (housekeeping).
    pub fn clear_finished(&mut self) {
        let before = self.downloads.len();
        self.downloads.retain(|d| !d.state.is_terminal());
        if self.downloads.len() != before {
            self.persist();
        }
    }

    fn persist(&self) {
        if let Some(path) = &self.path {
            if let Ok(json) = serde_json::to_string_pretty(&self.downloads) {
                if let Err(e) = write_atomic(path, json.as_bytes()) {
                    log::warn!("download persist failed: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> DownloadManager {
        DownloadManager::new()
    }

    #[test]
    fn happy_path() {
        let mut m = mgr();
        let id = m.enqueue("https://f.example/x.bin", PathBuf::from("/tmp/x.bin"), Some(1000), 100);

        m.start(id).unwrap();
        assert_eq!(m.get(id).unwrap().state, DownloadState::Connecting);

        m.mark_connecting(id).unwrap();
        assert_eq!(m.get(id).unwrap().state, DownloadState::Downloading);

        m.progress(id, 500, None, 1.0).unwrap();
        m.progress(id, 1000, None, 2.0).unwrap();
        let d = m.get(id).unwrap();
        assert_eq!(d.received_bytes, 1000);
        assert!((d.progress_fraction().unwrap() - 1.0).abs() < 1e-9);

        m.complete(id, 102).unwrap();
        assert_eq!(m.get(id).unwrap().state, DownloadState::Completed);
        assert_eq!(m.get(id).unwrap().finished_at_unix, Some(102));
    }

    #[test]
    fn pause_resume_flow() {
        let mut m = mgr();
        let id = m.enqueue("u", PathBuf::from("/tmp/p"), Some(100), 1);
        m.start(id).unwrap();
        m.mark_connecting(id).unwrap();
        m.progress(id, 10, None, 0.0).unwrap();
        m.pause(id).unwrap();
        assert_eq!(m.get(id).unwrap().state, DownloadState::Paused);
        assert_eq!(m.get(id).unwrap().speed_bps, 0.0);
        // Progress while paused is still accounted (the socket may drain).
        m.progress(id, 20, None, 1.0).unwrap();
        m.resume(id).unwrap();
        assert_eq!(m.get(id).unwrap().state, DownloadState::Connecting);
    }

    #[test]
    fn invalid_transitions_rejected() {
        let mut m = mgr();
        let id = m.enqueue("u", PathBuf::from("/tmp/i"), None, 1);
        // Can't pause before starting.
        assert_eq!(m.pause(id), Err(DownloadError::NotPausable(DownloadState::Queued)));
        m.start(id).unwrap();
        m.complete(id, 5).unwrap();
        // Terminal states reject everything.
        assert_eq!(m.complete(id, 6), Err(DownloadError::Finished(id)));
        assert_eq!(m.cancel(id, 6), Err(DownloadError::NotCancellable(DownloadState::Completed)));
        assert_eq!(
            m.progress(id, 10, None, 9.0),
            Err(DownloadError::Finished(id))
        );
    }

    #[test]
    fn ema_speed_and_eta() {
        let mut m = mgr();
        let id = m.enqueue("u", PathBuf::from("/tmp/e"), Some(10_000), 1);
        m.start(id).unwrap();
        m.mark_connecting(id).unwrap();
        m.progress(id, 0, None, 0.0).unwrap();
        m.progress(id, 1000, None, 1.0).unwrap(); // inst 1000 B/s
        m.progress(id, 2000, None, 2.0).unwrap(); // inst 1000 B/s
        let d = m.get(id).unwrap();
        assert!((d.speed_bps - 1000.0).abs() < 1e-6);
        assert!((d.eta_secs().unwrap() - 8.0).abs() < 1e-6);
    }

    #[test]
    fn cancel_and_fail() {
        let mut m = mgr();
        let a = m.enqueue("u1", PathBuf::from("/tmp/a"), None, 1);
        let b = m.enqueue("u2", PathBuf::from("/tmp/b"), None, 1);
        m.start(a).unwrap();
        m.cancel(a, 9).unwrap();
        assert_eq!(m.get(a).unwrap().state, DownloadState::Cancelled);
        m.fail(b, "HTTP 503", 10).unwrap();
        assert_eq!(m.get(b).unwrap().error.as_deref(), Some("HTTP 503"));
        m.clear_finished();
        assert!(m.downloads().is_empty());
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("downloads.json");
        {
            let mut m = DownloadManager::open(&path).unwrap();
            let id = m.enqueue("https://s.example/f", PathBuf::from("/tmp/f"), Some(10), 7);
            m.start(id).unwrap();
            m.progress(id, 4, None, 0.5).unwrap();
        }
        let m = DownloadManager::open(&path).unwrap();
        assert_eq!(m.downloads().len(), 1);
        assert_eq!(m.downloads()[0].received_bytes, 4);
        assert_eq!(m.downloads()[0].state, DownloadState::Downloading);
    }
}
