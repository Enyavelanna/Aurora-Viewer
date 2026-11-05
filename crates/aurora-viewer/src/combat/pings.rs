//! Session-only coordination. Sender identity comes from the worn bridge relay,
//! never from the message. Acknowledgements are intentions, not delivery receipts.
use super::tactical::{Marker, Ping, Settings};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Instant};
use uuid::Uuid;

pub(crate) const VERSION: u8 = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Message {
    pub version: u8,
    #[serde(flatten)]
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum Action {
    Place { id: Uuid, position: [f64; 3], kind: usize },
    Acknowledge { id: Uuid },
    Cancel { id: Uuid },
    Contact { id: Uuid },
}

impl Message {
    pub fn new(action: Action) -> Self {
        Self { version: VERSION, action }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Outbound {
    pub targets: Vec<Uuid>,
    pub message: Message,
    pub at: Instant,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ShareState {
    #[default]
    Local,
    Queued,
    Attempted,
    Failed,
}

impl Ping {
    pub fn new(marker: Marker, kind: usize, at: Instant) -> Self {
        Self {
            marker,
            kind,
            at,
            sender: Uuid::nil(),
            responders: BTreeSet::new(),
            recipients: BTreeSet::new(),
            acknowledged: false,
            sharing: ShareState::Local,
        }
    }

    /// Nil is a local ping, not an identity supplied over the wire.
    pub fn own(&self) -> bool {
        self.sender.is_nil()
    }
}

impl Settings {
    pub fn recent_tap(&self, now: Instant) -> Option<Uuid> {
        if !self.double_tap {
            return None;
        }
        self.session
            .last_tap
            .filter(|(_, at)| now.saturating_duration_since(*at).as_millis() <= 350)
            .map(|(id, _)| id)
            .filter(|id| self.session.pings.iter().any(|p| p.own() && p.marker.id == *id))
    }

    pub fn tap(&mut self, position: Option<[f64; 3]>, contact: bool, upgrade: Option<Uuid>, now: Instant) -> bool {
        if !self.available() || !self.pings_enabled {
            return false;
        }
        if let Some(id) = upgrade {
            let Some(ping) = self.session.pings.iter_mut().find(|p| {
                p.own() && p.marker.id == id && now.saturating_duration_since(p.at).as_secs_f32() < self.seconds.clamp(1.0, 60.0)
            }) else {
                return false;
            };
            if ping.kind == 1 {
                return true;
            }
            if ping.kind != 0 {
                return false;
            }
            if self.share && self.send_enabled && !ping.recipients.is_empty() {
                if self.session.ping_controls.len() >= 64 {
                    self.session.share_error = true;
                    return false;
                }
                self.session.ping_controls.push_back(Outbound {
                    targets: ping.recipients.iter().copied().collect(),
                    message: Message::new(Action::Contact { id }),
                    at: now,
                });
            }
            ping.kind = 1;
            ping.marker.name = super::tactical::kind_name(1).into();
            ping.marker.color = self.colors[1];
            for (marker, kind) in &mut self.session.outgoing {
                if marker.id == id {
                    *marker = ping.marker.clone();
                    *kind = 1;
                }
            }
            self.session.last_tap = None;
            return true;
        }
        if let Some(position) = position
            && self.ping_at(position, usize::from(contact && self.auto_contact), now)
        {
            self.session.last_tap = self.session.pings.last().map(|p| (p.marker.id, now));
            true
        } else {
            false
        }
    }

    pub fn cooldown(&self, now: Instant) -> f32 {
        self.session
            .last_ping
            .map_or(0.0, |at| (3.0 - now.saturating_duration_since(at).as_secs_f32()).max(0.0))
    }

    pub fn respond(&mut self, id: Uuid, now: Instant) -> bool {
        if !self.available() || !self.pings_enabled {
            return false;
        }
        let Some(index) = self
            .session
            .pings
            .iter()
            .position(|p| p.marker.id == id && now.saturating_duration_since(p.at).as_secs_f32() < self.seconds.clamp(1.0, 60.0))
        else {
            return false;
        };
        let ping = &self.session.pings[index];
        if ping.own() {
            // Removing a queued placement prevents a later send resurrecting it.
            self.session.outgoing.retain(|(m, _)| m.id != id);
            if self.share && self.send_enabled && !ping.recipients.is_empty() {
                if self.session.ping_controls.len() >= 64 {
                    self.session.share_error = true;
                    return false;
                }
                self.session.ping_controls.push_back(Outbound {
                    targets: ping.recipients.iter().copied().collect(),
                    message: Message::new(Action::Cancel { id }),
                    at: now,
                });
            }
            self.session.pings.remove(index);
            return true;
        }
        if !self.share || !self.send_enabled || ping.acknowledged {
            return false;
        }
        if self.session.ping_controls.len() >= 64 {
            self.session.share_error = true;
            return false;
        }
        self.session.ping_controls.push_back(Outbound {
            targets: vec![ping.sender],
            message: Message::new(Action::Acknowledge { id }),
            at: now,
        });
        self.session.pings[index].acknowledged = true;
        true
    }

    /// Peer admission is enforced by the bridge dispatcher. Control traffic is
    /// separate from the three-second placement limiter and is idempotent.
    pub fn accept_ping(&mut self, sender: Uuid, message: Message, now: Instant) -> bool {
        if !self.available() || !self.pings_enabled || sender.is_nil() || message.version != VERSION {
            return false;
        }
        match message.action {
            Action::Place { id, position, kind } => {
                if id.is_nil() || kind >= 4 || self.session.cancelled_pings.contains_key(&(sender, id)) {
                    return false;
                }
                let kind = if self.session.pending_contacts.remove(&(sender, id)).is_some() {
                    1
                } else {
                    kind
                };
                let marker = Marker {
                    id,
                    position,
                    name: super::tactical::kind_name(kind).into(),
                    color: self.colors[kind],
                    ..Marker::default()
                };
                self.accept_place(sender, marker, kind, now)
            }
            Action::Acknowledge { id } => {
                let Some(ping) = self
                    .session
                    .pings
                    .iter_mut()
                    .find(|p| p.own() && p.marker.id == id && p.recipients.contains(&sender))
                else {
                    return false;
                };
                now.saturating_duration_since(ping.at).as_secs_f32() < self.seconds.clamp(1.0, 60.0) && ping.responders.insert(sender)
            }
            Action::Cancel { id } => {
                if id.is_nil() || self.session.cancelled_pings.contains_key(&(sender, id)) {
                    return false;
                }
                if self.session.cancelled_pings.len() >= 512 {
                    self.session.share_error = true;
                    return false;
                }
                // Even an out-of-order cancel prevents that owner's late place.
                self.session.cancelled_pings.insert((sender, id), now);
                self.session.pings.retain(|p| p.sender != sender || p.marker.id != id);
                self.session.pending_contacts.remove(&(sender, id));
                true
            }
            Action::Contact { id } => {
                if id.is_nil() || self.session.cancelled_pings.contains_key(&(sender, id)) {
                    return false;
                }
                if let Some(ping) = self.session.pings.iter_mut().find(|p| p.sender == sender && p.marker.id == id) {
                    if ping.kind != 0 || now.saturating_duration_since(ping.at).as_secs_f32() >= self.seconds.clamp(1.0, 60.0) {
                        return false;
                    }
                    ping.kind = 1;
                    ping.marker.name = super::tactical::kind_name(1).into();
                    ping.marker.color = self.colors[1];
                    true
                } else if self.session.pending_contacts.len() < 128 && !self.session.seen_messages.contains_key(&id) {
                    self.session.pending_contacts.insert((sender, id), now).is_none()
                } else {
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> Settings {
        Settings {
            pings_enabled: true,
            share: true,
            ping_audience: 1,
            ..Settings::default()
        }
    }
    #[test]
    fn ping_responses_require_the_actual_recipient_and_are_idempotent() {
        let now = Instant::now();
        let peer = Uuid::from_u128(1);
        let mut cfg = cfg();
        assert!(cfg.ping_at([1.0; 3], 0, now));
        let id = cfg.session.pings[0].marker.id;
        cfg.session.pings[0].recipients.insert(peer);
        let ack = Message::new(Action::Acknowledge { id });
        assert!(!cfg.accept_ping(Uuid::from_u128(2), ack.clone(), now));
        assert!(cfg.accept_ping(peer, ack.clone(), now));
        assert!(!cfg.accept_ping(peer, ack.clone(), now));
        assert_eq!(cfg.session.pings[0].responders.len(), 1);
        assert!(!cfg.accept_ping(peer, ack, now + std::time::Duration::from_secs(9)));
    }
    #[test]
    fn ping_cancel_is_owner_bound_and_survives_reordered_place() {
        let now = Instant::now();
        let owner = Uuid::from_u128(1);
        let id = Uuid::new_v4();
        let place = Message::new(Action::Place {
            id,
            position: [1.0; 3],
            kind: 2,
        });
        let mut cfg = cfg();
        assert!(cfg.accept_ping(owner, place.clone(), now));
        assert_eq!(cfg.session.pings[0].sender, owner);
        assert!(cfg.accept_ping(Uuid::from_u128(2), Message::new(Action::Cancel { id }), now));
        assert_eq!(cfg.session.pings.len(), 1);
        assert!(cfg.accept_ping(owner, Message::new(Action::Cancel { id }), now));
        assert!(cfg.session.pings.is_empty());
        assert!(!cfg.accept_ping(owner, place, now + std::time::Duration::from_secs(4)));
    }
    #[test]
    fn ping_response_and_cancel_do_not_consume_placement_cooldown() {
        let now = Instant::now();
        let mut cfg = cfg();
        let owner = Uuid::from_u128(1);
        let id = Uuid::new_v4();
        assert!(cfg.accept_ping(
            owner,
            Message::new(Action::Place {
                id,
                position: [1.0; 3],
                kind: 1
            }),
            now
        ));
        assert!(cfg.respond(id, now));
        assert!(!cfg.respond(id, now));
        assert!(cfg.session.last_ping.is_none());
        assert_eq!(cfg.session.ping_controls[0].targets, vec![owner]);
        assert!(cfg.ping_at([2.0; 3], 0, now));
        let own = cfg.session.pings.last().unwrap().marker.id;
        assert!(cfg.respond(own, now));
        assert!(cfg.session.outgoing.is_empty());
        assert_eq!(cfg.cooldown(now), 3.0);
    }
    #[test]
    fn ping_wire_is_small_versioned_and_rejects_invalid_coordinates() {
        let now = Instant::now();
        let peer = Uuid::from_u128(1);
        let mut cfg = cfg();
        for (position, kind, version) in [([f64::NAN; 3], 0, VERSION), ([1.0; 3], 4, VERSION), ([1.0; 3], 0, 99)] {
            assert!(!cfg.accept_ping(
                peer,
                Message {
                    version,
                    action: Action::Place {
                        id: Uuid::new_v4(),
                        position,
                        kind
                    }
                },
                now
            ));
        }
        let msg = Message::new(Action::Place {
            id: Uuid::new_v4(),
            position: [1e9, -1e9, 4096.0],
            kind: 3,
        });
        let wire = serde_json::to_vec(&msg).unwrap();
        assert!(wire.len() <= 480);
        assert_eq!(serde_json::from_slice::<Message>(&wire).unwrap(), msg);
        cfg.session.ping_controls.push_back(Outbound {
            targets: vec![peer],
            message: msg,
            at: now,
        });
        let stored = serde_json::to_string(&cfg).unwrap();
        assert!(!stored.contains("ping_controls"));
    }

    #[test]
    fn double_tap_upgrades_one_ping_without_restarting_its_timer_or_cooldown() {
        let now = Instant::now();
        let later = now + std::time::Duration::from_millis(180);
        let mut cfg = cfg();
        assert!(cfg.tap(Some([1.0; 3]), false, None, now));
        let id = cfg.recent_tap(later).unwrap();
        assert!(cfg.tap(None, false, Some(id), later));
        assert_eq!(cfg.session.pings.len(), 1);
        assert_eq!(cfg.session.pings[0].kind, 1);
        assert_eq!(cfg.session.pings[0].at, now);
        assert_eq!(cfg.session.last_ping, Some(now));
        assert_eq!(cfg.session.outgoing.len(), 1);
        assert_eq!(cfg.session.outgoing[0].1, 1);
        assert!(!cfg.tap(Some([2.0; 3]), true, None, later));
        assert_eq!(cfg.session.pings.len(), 1);
        assert!(cfg.recent_tap(now + std::time::Duration::from_secs(1)).is_none());
    }
    #[test]
    fn contact_promotion_reaches_original_peers_and_tolerates_reordering() {
        let now = Instant::now();
        let sender = Uuid::from_u128(1);
        let peer = Uuid::from_u128(2);
        let mut local = cfg();
        assert!(local.tap(Some([1.0; 3]), false, None, now));
        let id = local.session.pings[0].marker.id;
        local.session.outgoing.clear();
        local.session.pings[0].recipients.insert(peer);
        local.ping_audience = 0;
        assert!(local.tap(None, false, Some(id), now));
        let control = local.session.ping_controls.pop_front().unwrap();
        assert_eq!(control.targets, vec![peer]);
        let mut remote = cfg();
        assert!(remote.accept_ping(sender, control.message.clone(), now));
        assert!(remote.accept_ping(
            sender,
            Message::new(Action::Place {
                id,
                position: [1.0; 3],
                kind: 0
            }),
            now
        ));
        assert_eq!(remote.session.pings[0].kind, 1);
        assert!(!remote.accept_ping(sender, control.message, now));
        assert_eq!(remote.session.pings.len(), 1);
        assert_eq!(remote.session.pings[0].at, now);
    }
    #[test]
    fn contact_shortcuts_are_independent_and_preserve_legacy_settings() {
        let now = Instant::now();
        let mut cfg: Settings = serde_json::from_str(r#"{"pings_enabled":true}"#).unwrap();
        assert!(cfg.auto_contact && cfg.double_tap);
        assert!(cfg.tap(Some([1.0; 3]), true, None, now));
        assert_eq!(cfg.session.pings[0].kind, 1);
        cfg.double_tap = false;
        assert!(cfg.recent_tap(now).is_none());
        cfg.auto_contact = false;
        assert!(cfg.tap(Some([2.0; 3]), true, None, now + std::time::Duration::from_secs(3)));
        assert_eq!(cfg.session.pings[1].kind, 0);
    }
    #[test]
    fn ping_controls_respect_independent_send_and_master_switches_and_queue_bounds() {
        let now = Instant::now();
        let peer = Uuid::from_u128(1);
        let id = Uuid::new_v4();
        let mut cfg = cfg();
        assert!(cfg.accept_ping(
            peer,
            Message::new(Action::Place {
                id,
                position: [1.0; 3],
                kind: 1
            }),
            now
        ));
        cfg.send_enabled = false;
        assert!(!cfg.respond(id, now));
        cfg.send_enabled = true;
        let packet = Outbound {
            targets: vec![peer],
            message: Message::new(Action::Acknowledge { id }),
            at: now,
        };
        cfg.session.ping_controls.extend(std::iter::repeat_n(packet, 64));
        assert!(!cfg.respond(id, now));
        assert!(!cfg.session.pings[0].acknowledged);
        assert!(cfg.session.share_error);
        cfg.session.ping_controls.clear();
        cfg.combat_enabled = false;
        assert!(!cfg.respond(id, now));
        assert!(!cfg.tap(Some([2.0; 3]), true, None, now));
        assert!(!cfg.accept_ping(peer, Message::new(Action::Cancel { id }), now));
    }
}
