//! Versioned Aurora bridge transport of official Combat Log fields.
//! Envelope identity is private to a helper instance: it is not an anti-cheat claim.
use super::{Event, Kind};
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct Inbox {
    deaths: VecDeque<Event>,
    epoch: Option<Uuid>,
    seen: HashSet<u64>,
    order: VecDeque<u64>,
    highest: u64,
    retired: VecDeque<Uuid>,
    missing: std::collections::BTreeMap<u64, std::time::Instant>,
    pub received: u64,
    pub rejected: u64,
    pub lost: u64,
}
pub(crate) fn official(v: &serde_json::Value, avatar: Option<bool>, weapon: Option<String>) -> Option<Event> {
    let kind = match v.get("event")?.as_str()? {
        "DEATH" => Kind::Death,
        _ => return None,
    };
    let target = Uuid::parse_str(v.get("target")?.as_str()?).ok().filter(|id| !id.is_nil())?;
    let actor = v
        .get("owner")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .filter(|id| !id.is_nil())
        .unwrap_or_default();
    let damage = match v.get("damage") {
        None => None,
        Some(v) => {
            let d = v.as_f64()?;
            if !d.is_finite() || d < 0.0 || d > f32::MAX as f64 {
                return None;
            }
            Some(d as f32)
        }
    };
    let damage_type = v.get("type").and_then(|v| v.as_i64()).and_then(|n| i32::try_from(n).ok());
    let weapon = weapon.filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control));
    Some(Event {
        actor,
        target,
        kind,
        damage,
        damage_type,
        weapon,
        source: v
            .get("source")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .filter(|id| !id.is_nil()),
        rezzer: v
            .get("rezzer")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .filter(|id| !id.is_nil()),
        target_is_avatar: avatar,
    })
}
impl Inbox {
    pub fn receive(&mut self, body: &str) {
        self.received += 1;
        if !self.decode(body) {
            self.rejected += 1;
        }
    }
    fn decode(&mut self, body: &str) -> bool {
        if body.len() > 1100 {
            return false;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
            return false;
        };
        if v.get("v").and_then(|v| v.as_u64()) != Some(2) {
            return false;
        }
        let Some(epoch) = v
            .get("epoch")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok())
            .filter(|id| !id.is_nil())
        else {
            return false;
        };
        let Some(seq) = v.get("seq").and_then(|v| v.as_u64()).filter(|n| *n > 0 && *n <= i32::MAX as u64) else {
            return false;
        };
        if self.epoch != Some(epoch) {
            if self.retired.contains(&epoch) {
                return true;
            }
            if let Some(old) = self.epoch {
                if self.retired.len() == 16 {
                    self.retired.pop_front();
                }
                self.retired.push_back(old);
            }
            self.lost += self.missing.len() as u64;
            self.missing.clear();
            self.epoch = Some(epoch);
            self.seen.clear();
            self.order.clear();
            self.highest = 0;
        }
        if self.seen.contains(&seq) || seq <= self.highest.saturating_sub(8192) {
            return true;
        }
        let Some(event) = v.get("report").and_then(|r| {
            official(
                r,
                v.get("avatar").and_then(|v| v.as_bool()),
                v.get("weapon").and_then(|v| v.as_str()).map(str::to_owned),
            )
        }) else {
            return false;
        };
        if self.highest > 0 && seq > self.highest + 1 {
            let count = seq - self.highest - 1;
            if count <= 8192 && self.missing.len() + count as usize <= 8192 {
                let now = std::time::Instant::now();
                for absent in (self.highest + 1)..seq {
                    self.missing.insert(absent, now);
                }
            } else {
                self.lost = self.lost.saturating_add(count);
            }
        }
        self.missing.remove(&seq);
        self.highest = self.highest.max(seq);
        self.seen.insert(seq);
        self.order.push_back(seq);
        if self.order.len() > 8192
            && let Some(old) = self.order.pop_front()
        {
            self.seen.remove(&old);
        }
        if self.deaths.len() >= 4096 {
            self.lost += 1;
        } else {
            self.deaths.push_back(event);
        }
        true
    }
    pub fn drain(&mut self) -> Vec<Event> {
        // Confirmation feedback and journaling must not sit behind a hit burst.
        self.deaths.drain(..).collect()
    }
    pub fn take_loss(&mut self) -> u64 {
        let before = self.missing.len();
        self.missing.retain(|_, at| at.elapsed().as_secs_f32() < 1.0);
        self.lost += (before - self.missing.len()) as u64;
        std::mem::take(&mut self.lost)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn packet(seq: u64, kind: &str) -> String {
        serde_json::json!({"v":2,"epoch":Uuid::from_u128(9),"seq":seq,"avatar":true,"report":{"event":kind,"owner":Uuid::from_u128(1),"target":Uuid::from_u128(2),"damage":20}}).to_string()
    }
    #[test]
    fn combat_relay_deduplicates_sequences_without_debouncing_real_kills() {
        let mut q = Inbox::default();
        q.receive(&packet(1, "DEATH"));
        q.receive(&packet(1, "DEATH"));
        q.receive(&packet(2, "DEATH"));
        assert_eq!(q.drain().len(), 2);
        q.receive("D|00000000-0000-0000-0000-000000000002|1|7|24");
        assert_eq!(q.rejected, 1);
    }
    #[test]
    fn combat_relay_allows_reordering_and_ignores_retired_helper_replays() {
        let mut q = Inbox::default();
        q.receive(&packet(1, "DEATH"));
        q.receive(&packet(3, "DEATH"));
        q.receive(&packet(2, "DEATH"));
        assert_eq!(q.take_loss(), 0);
        assert_eq!(q.drain().len(), 3);
        let mut next: serde_json::Value = serde_json::from_str(&packet(1, "DEATH")).expect("packet");
        next["epoch"] = serde_json::json!(Uuid::from_u128(10));
        q.receive(&next.to_string());
        q.receive(&packet(4, "DEATH"));
        assert_eq!(q.drain().len(), 1);
    }
    #[test]
    fn combat_relay_ignores_all_hits_and_keeps_rapid_deaths() {
        let mut q = Inbox::default();
        for i in 1..=1200 {
            q.receive(&packet(i, "DAMAGE"));
        }
        for i in 1201..=1500 {
            q.receive(&packet(i, "DEATH"));
        }
        assert_eq!(q.drain().len(), 300);
        assert_eq!(q.rejected, 1200);
    }
    #[test]
    fn combat_relay_reports_overflow_and_sequence_gaps() {
        let mut q = Inbox::default();
        q.receive(&packet(1, "DEATH"));
        q.receive(&packet(3, "DEATH"));
        for at in q.missing.values_mut() {
            *at -= std::time::Duration::from_secs(2);
        }
        assert_eq!(q.take_loss(), 1);
        for i in 4..=5000 {
            q.receive(&packet(i, "DEATH"));
        }
        assert_eq!(q.drain().len(), 4096);
        assert!(q.take_loss() > 0);
    }
}
