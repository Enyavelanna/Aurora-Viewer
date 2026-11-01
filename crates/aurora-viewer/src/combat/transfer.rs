//! Bounded, atomic same-region set transfers. The helper remains a byte relay.
use super::{sets::MarkerSet, tactical::Marker};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};
use uuid::Uuid;

const MAX_PARTS: usize = 512;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Chunk {
    pub version: u8,
    pub transfer: Uuid,
    pub index: usize,
    pub count: usize,
    pub data: String,
}
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    at: Instant,
    parts: Vec<Option<Vec<u8>>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Inbox {
    pending: BTreeMap<(Uuid, Uuid), Pending>,
    completed: BTreeMap<(Uuid, Uuid), Instant>,
    pub previews: Vec<MarkerSet>,
}
pub(crate) fn encode(name: String, markers: Vec<Marker>) -> Result<Vec<String>, ()> {
    if markers.len() > 256 || markers.iter().any(|m| !m.valid()) || name.len() > 128 {
        return Err(());
    }
    let transfer = Uuid::new_v4();
    let bytes = serde_json::to_vec(&MarkerSet {
        id: transfer,
        name,
        markers,
    })
    .map_err(|_| ())?;
    let count = bytes.len().div_ceil(200);
    if count > MAX_PARTS {
        return Err(());
    }
    bytes
        .chunks(200)
        .enumerate()
        .map(|(index, data)| {
            serde_json::to_string(&Chunk {
                version: 3,
                transfer,
                index,
                count,
                data: STANDARD.encode(data),
            })
            .map_err(|_| ())
        })
        .collect()
}
impl Inbox {
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.pending.len();
        self.pending.retain(|_, p| now.saturating_duration_since(p.at).as_secs() < 120);
        self.completed.retain(|_, at| now.saturating_duration_since(*at).as_secs() < 300);
        self.pending.len() != before
    }
    pub fn accept(&mut self, sender: Uuid, chunk: Chunk, now: Instant) -> Result<(), ()> {
        if sender.is_nil()
            || chunk.version != 3
            || chunk.transfer.is_nil()
            || chunk.count == 0
            || chunk.count > MAX_PARTS
            || chunk.index >= chunk.count
            || chunk.data.len() > 268
        {
            return Err(());
        }
        let key = (sender, chunk.transfer);
        if self.completed.contains_key(&key) {
            return Ok(());
        }
        if !self.pending.contains_key(&key) && self.pending.len() >= 8 {
            return Err(());
        }
        let data = STANDARD.decode(&chunk.data).map_err(|_| ())?;
        if data.is_empty() || data.len() > 200 {
            return Err(());
        }
        let pending = self.pending.entry(key).or_insert_with(|| Pending {
            at: now,
            parts: vec![None; chunk.count],
        });
        if pending.parts.len() != chunk.count {
            return Err(());
        }
        if let Some(existing) = &pending.parts[chunk.index] {
            return if existing == &data { Ok(()) } else { Err(()) };
        }
        pending.parts[chunk.index] = Some(data);
        if pending.parts.iter().any(Option::is_none) {
            return Ok(());
        }
        let bytes: Vec<u8> = pending.parts.iter().flatten().flatten().copied().collect();
        self.pending.remove(&key);
        let set: MarkerSet = serde_json::from_slice(&bytes).map_err(|_| ())?;
        if set.id != chunk.transfer
            || set.name.len() > 128
            || set.name.chars().any(char::is_control)
            || set.markers.len() > 256
            || set.markers.iter().any(|m| !m.valid())
            || self.previews.len() >= 8
            || self.completed.len() >= 512
        {
            return Err(());
        }
        self.completed.insert(key, now);
        self.previews.push(set);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combat_set_transfer_is_atomic_reordered_deduplicated_and_expires() {
        let mut cfg = super::super::tactical::Settings::default();
        for n in 0..40 {
            cfg.add_marker([n as f64, 2.0, 3.0]);
        }
        let parts = encode("Objectives".into(), cfg.markers.clone()).expect("encode");
        assert!(parts.len() > 1 && parts.iter().all(|p| p.len() <= 480));
        let sender = Uuid::new_v4();
        let now = Instant::now();
        let mut inbox = Inbox::default();
        for part in parts.iter().rev().skip(1) {
            let chunk: Chunk = serde_json::from_str(part).expect("chunk");
            inbox.accept(sender, chunk.clone(), now).expect("receive");
            inbox.accept(sender, chunk, now).expect("duplicate");
            assert!(inbox.previews.is_empty());
        }
        inbox
            .accept(sender, serde_json::from_str(parts.last().expect("part")).expect("chunk"), now)
            .expect("complete");
        assert_eq!(inbox.previews.len(), 1);
        assert_eq!(inbox.previews[0].markers, cfg.markers);
        for part in &parts {
            inbox
                .accept(sender, serde_json::from_str(part).expect("chunk"), now)
                .expect("retransmission");
        }
        assert_eq!(inbox.previews.len(), 1);
        let other = Uuid::new_v4();
        inbox
            .accept(other, serde_json::from_str(&parts[0]).expect("chunk"), now)
            .expect("partial");
        assert!(inbox.expire(now + std::time::Duration::from_secs(121)));
        assert_eq!(inbox.previews.len(), 1);
    }
    #[test]
    fn combat_set_transfer_rejects_unbounded_and_conflicting_parts() {
        let mut inbox = Inbox::default();
        let sender = Uuid::new_v4();
        let mut chunk = Chunk {
            version: 3,
            transfer: Uuid::new_v4(),
            index: 0,
            count: 513,
            data: STANDARD.encode(b"a"),
        };
        assert!(inbox.accept(sender, chunk.clone(), Instant::now()).is_err());
        chunk.count = 2;
        assert!(inbox.accept(sender, chunk.clone(), Instant::now()).is_ok());
        chunk.data = STANDARD.encode(b"b");
        assert!(inbox.accept(sender, chunk, Instant::now()).is_err());
        assert!(inbox.previews.is_empty());
    }
}
