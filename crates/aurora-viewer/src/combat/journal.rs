//! Session-only combat journal with acknowledged background writes.
//! Unlike best-effort chat transcripts, a full queue is retried next frame.
//! Every received death remains in session memory, including on disk failure.
use super::RecordedEvent;
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub(crate) struct Journal {
    worker: Option<Sender<(usize, String)>>,
    replies: Option<Receiver<Result<usize, String>>>,
    pub error: Option<String>,
    pub warned: bool,
    pub path: Option<PathBuf>,
    sent: usize,
    pub written: usize,
    thread: Option<std::thread::JoinHandle<()>>,
    clearing: Option<usize>,
    archive: Option<Receiver<Result<(), String>>>,
    pub cleared: Option<usize>,
}
impl Journal {
    pub fn open(&mut self, folder: &Path) {
        if self.worker.is_some() || self.error.is_some() || self.clearing.is_some() {
            return;
        }
        let path = folder.join(format!(
            "{}-{}.jsonl",
            chrono::Utc::now().format("%Y%m%dT%H%M%S"),
            uuid::Uuid::new_v4()
        ));
        let (tx, rx) = crossbeam_channel::bounded::<(usize, String)>(512);
        let (done, replies) = crossbeam_channel::unbounded();
        let output = path.clone();
        match std::thread::Builder::new().name("combat-journal".into()).spawn(move || {
            let run = || -> Result<(), String> {
                let parent = output.parent().ok_or("missing journal folder")?;
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                let mut file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&output)
                    .map_err(|e| e.to_string())?;
                while let Ok((index, mut text)) = rx.recv() {
                    let mut last = index;
                    // Amortize physical disk flushes during a burst. Acknowledgment
                    // covers only the contiguous records in this durable batch.
                    for (next, line) in rx.try_iter().take(63) {
                        last = next;
                        text.push_str(&line);
                    }
                    file.write_all(text.as_bytes())
                        .and_then(|_| file.sync_data())
                        .map_err(|e| e.to_string())?;
                    let _ = done.send(Ok(last + 1));
                }
                Ok(())
            };
            if let Err(e) = run() {
                let _ = done.send(Err(e));
            }
        }) {
            Ok(thread) => {
                self.thread = Some(thread);
                self.path = Some(path);
                self.worker = Some(tx);
                self.replies = Some(replies);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn request_clear(&mut self, count: usize) {
        if self.clearing.is_none() && self.error.is_none() {
            if self.worker.is_none() {
                self.cleared = Some(count);
            } else {
                self.clearing = Some(count);
            }
        }
    }
    pub fn clearing(&self) -> bool {
        self.clearing.is_some()
    }
    pub fn flush(&mut self, rows: &[RecordedEvent]) {
        if self.worker.is_none() {
            return;
        }
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.written < rows.len() && self.error.is_none() {
            self.poll(rows);
            if std::time::Instant::now() >= limit {
                self.error = Some("shutdown write timeout".into());
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        if self.error.is_some() {
            log::warn!("combat journal: some session events have not been saved");
        }
    }
    pub fn poll(&mut self, rows: &[RecordedEvent]) {
        if let Some(result) = self.archive.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.archive = None;
            let count = self.clearing.take().unwrap_or(0);
            match result {
                Ok(()) => {
                    self.cleared = Some(count);
                    self.path = None;
                    self.sent = 0;
                    self.written = 0;
                    self.replies = None;
                }
                Err(error) => self.error = Some(error),
            }
            return;
        }
        if let Some(rx) = &self.replies {
            for reply in rx.try_iter() {
                match reply {
                    Ok(n) => self.written = n,
                    Err(e) => self.error = Some(e),
                }
            }
        }
        if self.error.is_some() {
            return;
        }
        if self.clearing.is_some_and(|count| self.written >= count) && self.archive.is_none() {
            let Some(path) = self.path.clone() else {
                return;
            };
            self.worker.take();
            let thread = self.thread.take();
            let (tx, rx) = crossbeam_channel::bounded(1);
            self.archive = Some(rx);
            match std::thread::Builder::new().name("combat-journal-trash".into()).spawn(move || {
                let result = (|| -> Result<(), String> {
                    if let Some(thread) = thread {
                        thread.join().map_err(|_| "journal writer panicked")?;
                    }
                    let trash = path.parent().ok_or("missing journal folder")?.join("trash");
                    std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
                    let name = path.file_name().ok_or("missing journal name")?;
                    std::fs::rename(&path, trash.join(name)).map_err(|e| e.to_string())
                })();
                let _ = tx.send(result);
            }) {
                Ok(_) => {}
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.archive = None;
                    self.clearing = None;
                }
            }
            return;
        }
        let Some(tx) = &self.worker else {
            return;
        };
        // Bound serialization work per frame even when recovering a large backlog.
        for _ in 0..64 {
            if self.clearing.is_some_and(|count| self.sent >= count) {
                break;
            }
            let Some(row) = rows.get(self.sent) else {
                break;
            };
            let Ok(mut line) = serde_json::to_string(row) else {
                self.error = Some("serialization".into());
                break;
            };
            line.push('\n');
            match tx.try_send((self.sent, line)) {
                Ok(()) => self.sent += 1,
                Err(TrySendError::Full(_)) => break,
                Err(TrySendError::Disconnected(_)) => {
                    self.error = Some("writer stopped".into());
                    break;
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{Event, Kind};
    use std::time::{Duration, Instant};
    #[test]
    fn combat_journal_retries_queue_pressure_and_acknowledges_every_death() {
        let dir = crate::settings::config_dir()
            .join("combat-tests")
            .join(uuid::Uuid::new_v4().to_string());
        let rows: Vec<_> = (0..620)
            .map(|_| RecordedEvent {
                at: Instant::now(),
                timestamp: "2026-10-31 00:00:00".into(),
                event: Event {
                    kind: Kind::Death,
                    actor: uuid::Uuid::from_u128(1),
                    target: uuid::Uuid::from_u128(2),
                    damage: None,
                    damage_type: None,
                    weapon: None,
                    source: None,
                    rezzer: None,
                    target_is_avatar: None,
                },
                awards: Vec::new(),
            })
            .collect();
        let mut journal = Journal::default();
        journal.open(&dir);
        let limit = Instant::now() + Duration::from_secs(20);
        while journal.written < rows.len() && journal.error.is_none() && Instant::now() < limit {
            journal.poll(&rows);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(journal.error.is_none(), "{:?}", journal.error);
        assert_eq!(journal.written, rows.len());
        let text = std::fs::read_to_string(journal.path.as_ref().expect("path")).expect("journal");
        assert_eq!(text.lines().count(), rows.len());
        assert!(text.lines().all(|s| serde_json::from_str::<RecordedEvent>(s).is_ok()));
        let path = journal.path.clone().expect("path");
        journal.request_clear(rows.len());
        let limit = Instant::now() + Duration::from_secs(10);
        while journal.cleared.is_none() && journal.error.is_none() && Instant::now() < limit {
            journal.poll(&rows);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(journal.error.is_none(), "{:?}", journal.error);
        assert_eq!(journal.cleared.take(), Some(rows.len()));
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("trash").join(path.file_name().expect("name"))).expect("recoverable"),
            text
        );
        journal.open(&dir);
        journal.flush(&rows[..1]);
        assert_eq!(journal.written, 1);
        // A trash path that is a file must preserve the original journal.
        let second_dir = dir.join("blocked-trash");
        std::fs::create_dir_all(&second_dir).expect("folder");
        std::fs::write(second_dir.join("trash"), b"blocked").expect("blocker");
        let mut blocked = Journal::default();
        blocked.open(&second_dir);
        blocked.flush(&rows[..1]);
        blocked.request_clear(1);
        let limit = Instant::now() + Duration::from_secs(10);
        while blocked.error.is_none() && Instant::now() < limit {
            blocked.poll(&rows[..1]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(blocked.error.is_some());
        assert!(blocked.cleared.is_none());
        assert!(blocked.path.as_ref().expect("preserved path").exists());
    }
    #[test]
    fn combat_journal_disk_failure_is_observable_and_retains_session_rows() {
        let base = crate::settings::config_dir().join("combat-tests");
        std::fs::create_dir_all(&base).expect("test folder");
        let blocker = base.join(uuid::Uuid::new_v4().to_string());
        std::fs::write(&blocker, b"not a directory").expect("blocker");
        let mut journal = Journal::default();
        journal.open(&blocker);
        let limit = Instant::now() + Duration::from_secs(3);
        while journal.error.is_none() && Instant::now() < limit {
            journal.poll(&[]);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(journal.error.is_some());
        assert_eq!(journal.written, 0);
    }
}
