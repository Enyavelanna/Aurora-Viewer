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
}
impl Journal {
    pub fn open(&mut self, folder: &Path) {
        if self.worker.is_some() || self.error.is_some() {
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
        let Some(tx) = &self.worker else {
            return;
        };
        // Bound serialization work per frame even when recovering a large backlog.
        for _ in 0..64 {
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
