//! Aurora combat model. Simulator fields follow wiki.secondlife.com/wiki/Combat_Log.
//! Award rules are adapted from our KRS/KILLMARK hud/source/Core.inc (local beta).
//! No legacy viewer hit grammar is accepted.
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, time::Instant};
use uuid::Uuid;
pub(crate) mod awards;
pub(crate) mod journal;
pub(crate) mod navigation;
pub(crate) mod ping_ui;
pub(crate) mod pings;
pub(crate) mod sets;
pub(crate) mod tactical;
pub(crate) mod transfer;
pub(crate) mod ui;
pub(crate) mod wire;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub enabled: bool,
    pub journal: bool,
    pub show_tab: bool,
    #[serde(alias = "hitmarker")]
    pub killmarker: bool,
    pub kill_sound: bool,
    pub kill_asset: String,
    pub kill_volume: f32,
    pub marker_size: f32,
    pub marker_color: [u8; 4],
    pub marker_opacity: f32,
    pub marker_seconds: f32,
    pub feed: bool,
    pub feed_position: [f32; 2],
    pub feed_size: f32,
    pub feed_opacity: f32,
    pub feed_seconds: f32,
    pub feed_entries: usize,
    pub voices: bool,
    pub medals: bool,
    pub voice_volume: f32,
    /// One explicit sound UUID per award event.  Kept as a vector so older
    /// settings files with the former 10-entry array remain readable.
    pub announcement_assets: Vec<String>,
    /// One explicit medal-image UUID per award event, in the same order as
    /// `announcement_assets` (Award::ALL).  Empty entries are valid.
    pub medal_textures: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            journal: false,
            show_tab: true,
            killmarker: false,
            kill_sound: false,
            kill_asset: String::new(),
            kill_volume: 0.7,
            marker_size: 20.0,
            marker_color: [255, 255, 255, 230],
            marker_opacity: 1.0,
            marker_seconds: 0.25,
            feed: false,
            feed_position: [0.98, 0.35],
            feed_size: 14.0,
            feed_opacity: 0.9,
            feed_seconds: 6.0,
            feed_entries: 5,
            voices: false,
            medals: false,
            voice_volume: 0.7,
            announcement_assets: vec![String::new(); awards::Award::ALL.len()],
            // KRS_MedalPopup source art, excluding server-only Clean Sweep.
            // The order is Award::ALL, so every sound row has a matching image row.
            medal_textures: [
                "42a4f332-f201-6eb6-6392-640ab313509a", // First
                "68165772-e609-2897-5ae9-02eae412fa17", // Double
                "b9bd132a-4a61-d794-33ce-7036a43ece8c", // Triple
                "",                                     // Multi
                "3cd81a7b-8397-5360-9e74-a720e8e18176", // Ultra
                "",                                     // Five
                "643173ea-daa2-82c2-1b16-365bfb961c8a", // Ten
                "",                                     // Fifteen
                "",                                     // Twenty
                "a5316d97-14fe-91a6-0580-b99279a08ddb", // Thirty
                "9a33124d-8c5b-7cf4-934b-1d96fadcf3bd", // Revenge
                "2cc4115f-dcb0-0206-106d-274129539dab", // Dominating
                "ede97764-1792-e112-0d0f-3226693712a8", // Comeback
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}
impl Settings {
    pub fn active(&self) -> bool {
        self.enabled && (self.journal || self.killmarker || self.kill_sound || self.feed || self.voices || self.medals)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Kind {
    Death,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Event {
    pub actor: Uuid,
    pub target: Uuid,
    pub kind: Kind,
    pub damage: Option<f32>,
    pub damage_type: Option<i32>,
    pub weapon: Option<String>,
    pub source: Option<Uuid>,
    pub rezzer: Option<Uuid>,
    pub target_is_avatar: Option<bool>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Kill,
    Death,
    Suicide,
    Environment,
    Observed,
}
impl Event {
    pub fn outcome(&self, agent: Uuid) -> Outcome {
        if self.target == agent {
            if self.actor == agent {
                Outcome::Suicide
            } else if self.actor.is_nil() {
                Outcome::Environment
            } else {
                Outcome::Death
            }
        } else if self.actor == agent {
            Outcome::Kill
        } else {
            Outcome::Observed
        }
    }
    pub fn own_kill(&self, agent: Uuid) -> bool {
        self.kind == Kind::Death && self.outcome(agent) == Outcome::Kill
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecordedEvent {
    #[serde(skip, default = "Instant::now")]
    pub at: Instant,
    pub timestamp: String,
    pub event: Event,
}
#[derive(Default)]
pub(crate) struct State {
    pub history: Vec<RecordedEvent>,
    pub personal: Vec<usize>,
    pub feed: VecDeque<RecordedEvent>,
    pub personal_view: bool,
    pub last_kill: Option<Instant>,
    pub awards: awards::Tracker,
    pub voices: VecDeque<(awards::Award, Instant)>,
    pub medals: VecDeque<(awards::Award, Instant)>,
    pub preview_medal: Option<(awards::Award, Instant)>,
    pub medal_started: Option<Instant>,
    pub voice_after: Option<Instant>,
    pub journal: journal::Journal,
    pub received: u64,
    pub lost: u64,
}
impl State {
    pub fn record(&mut self, event: Event, agent: Uuid, cfg: &Settings, now: Instant) {
        if !cfg.enabled {
            return;
        }
        self.received += 1;
        let outcome = event.outcome(agent);
        if event.own_kill(agent) {
            self.last_kill = Some(now);
        }
        for award in self.awards.accept(outcome, event.actor, event.target, now) {
            if cfg.voices && award.sound_slot().is_some() {
                if self.voices.len() == 8 {
                    self.voices.pop_front();
                }
                self.voices.push_back((award, now));
            }
            if cfg.medals && award.medal_slot().is_some() {
                if self.medals.len() == 8 {
                    self.medals.pop_back();
                }
                self.medals.push_back((award, now));
            }
        }
        let row = RecordedEvent {
            at: now,
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            event,
        };
        if self.feed.len() == 64 {
            self.feed.pop_front();
        }
        self.feed.push_back(row.clone());
        if cfg.journal {
            if outcome != Outcome::Observed {
                self.personal.push(self.history.len());
            }
            self.history.push(row);
        }
    }
    pub fn preview_award(&mut self, award: awards::Award, now: Instant) {
        self.preview_medal = Some((award, now));
    }
    pub fn note_loss(&mut self, count: u64) {
        if count == 0 {
            return;
        }
        self.lost += count;
        self.awards.invalidate();
    }
    pub fn voice(&mut self, cfg: &Settings, now: Instant) -> Option<(Uuid, f32)> {
        if !cfg.enabled || !cfg.voices {
            self.voices.clear();
            return None;
        }
        if self.voice_after.is_some_and(|t| t > now) {
            return None;
        }
        while let Some((award, at)) = self.voices.pop_front() {
            if now.saturating_duration_since(at).as_secs_f32() > 6.0 {
                continue;
            }
            if let Some(slot) = award.sound_slot()
                && let Some(asset) = cfg.announcement_assets.get(slot)
                && let Ok(id) = Uuid::parse_str(asset)
                && !id.is_nil()
            {
                self.voice_after = Some(now + std::time::Duration::from_millis(1800));
                return Some((id, cfg.voice_volume));
            }
        }
        None
    }
    pub fn medal(&mut self, enabled: bool, now: Instant) -> Option<(awards::Award, f32, f32)> {
        let visual = |age: f32| {
            let intro = (age / 0.22).clamp(0.0, 1.0);
            let reveal = 1.0 - (1.0 - intro).powi(3);
            let alpha = (age / 0.1).clamp(0.0, 1.0) * ((2.55 - age) / 0.35).clamp(0.0, 1.0);
            (alpha, reveal)
        };
        if let Some((award, started)) = self.preview_medal {
            let age = now.saturating_duration_since(started).as_secs_f32();
            if age < 2.55 {
                let (alpha, reveal) = visual(age);
                return Some((award, alpha, reveal));
            }
            self.preview_medal = None;
        }
        if !enabled {
            self.medals.clear();
            self.medal_started = None;
            return None;
        }
        loop {
            let &(award, queued) = self.medals.front()?;
            if self.medal_started.is_none() && now.saturating_duration_since(queued).as_secs_f32() > 8.0 {
                self.medals.pop_front();
                continue;
            }
            let start = *self.medal_started.get_or_insert(now);
            let age = now.saturating_duration_since(start).as_secs_f32();
            if age >= 2.55 {
                self.medals.pop_front();
                self.medal_started = None;
                continue;
            }
            let (alpha, reveal) = visual(age);
            return Some((award, alpha, reveal));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn death(actor: u128, target: u128) -> Event {
        Event {
            kind: Kind::Death,
            actor: Uuid::from_u128(actor),
            target: Uuid::from_u128(target),
            damage: None,
            damage_type: Some(5),
            weapon: None,
            source: None,
            rezzer: None,
            target_is_avatar: Some(true),
        }
    }
    #[test]
    fn combat_marker_only_accepts_confirmed_own_deaths() {
        let me = Uuid::from_u128(1);
        let cfg = Settings {
            journal: true,
            killmarker: true,
            ..Default::default()
        };
        let mut state = State::default();
        let now = Instant::now();
        for e in [death(2, 1), death(1, 1), death(0, 1)] {
            state.record(e, me, &cfg, now);
            assert!(state.last_kill.is_none());
        }
        state.record(death(1, 2), me, &cfg, now);
        assert_eq!(state.last_kill, Some(now));
        assert_eq!(state.history.len(), 4);
    }
    #[test]
    fn combat_hidden_session_history_is_not_truncated() {
        let cfg = Settings {
            journal: true,
            show_tab: false,
            ..Default::default()
        };
        let mut state = State::default();
        let me = Uuid::from_u128(1);
        for _ in 0..600 {
            state.record(death(1, 2), me, &cfg, Instant::now());
        }
        assert_eq!(state.history.len(), 600);
        assert_eq!(state.personal.len(), 600);
        assert_eq!(state.feed.len(), 64);
    }
    #[test]
    fn combat_voices_medals_and_marker_switches_are_independent_of_recording() {
        let me = Uuid::from_u128(1);
        let now = Instant::now();
        let mut cfg = Settings {
            journal: true,
            voices: true,
            ..Default::default()
        };
        cfg.announcement_assets[0] = Uuid::from_u128(90).to_string();
        let mut state = State::default();
        state.record(death(1, 2), me, &cfg, now);
        state.record(death(1, 3), me, &cfg, now);
        assert_eq!(state.voice(&cfg, now).map(|v| v.0), Some(Uuid::from_u128(90)));
        assert!(state.medal(cfg.medals, now).is_none());
        cfg.voices = false;
        cfg.medals = true;
        state.record(death(1, 4), me, &cfg, now);
        assert!(state.voice(&cfg, now).is_none());
        assert_eq!(state.medal(cfg.medals, now).map(|v| v.0), Some(awards::Award::Triple));
        assert_eq!(state.history.len(), 3);
        assert!(!cfg.killmarker && !cfg.kill_sound);
    }

    #[test]
    fn combat_old_settings_migrate_and_discard_hit_sounds() {
        let cfg: Settings =
            serde_json::from_str(r#"{"hitmarker":true,"hit_sound":true,"hit_asset":"old","marker_size":33,"journal":true}"#).expect("old");
        assert!(cfg.killmarker && cfg.journal && cfg.show_tab);
        assert_eq!(cfg.marker_size, 33.0);
        assert!(!cfg.voices && !cfg.medals);
        assert!(!serde_json::to_string(&cfg).expect("serialize").contains("hit_sound"));
    }
    #[test]
    fn combat_master_suspends_recording_and_rewards_without_changing_preferences() {
        let cfg = Settings {
            enabled: false,
            journal: true,
            voices: true,
            medals: true,
            killmarker: true,
            ..Default::default()
        };
        let mut state = State::default();
        state.record(death(1, 2), Uuid::from_u128(1), &cfg, Instant::now());
        assert!(state.history.is_empty() && state.last_kill.is_none() && state.medals.is_empty());
        assert_eq!(state.received, 0);
        assert!(cfg.journal && cfg.voices && cfg.medals && cfg.killmarker);
    }
    #[test]
    fn combat_reward_migration_preserves_event_slots_and_preview_ignores_medal_toggle() {
        let mut settings = crate::settings::Settings::default();
        settings.combat.announcement_assets = (0..10).map(|n| format!("sound-{n}")).collect();
        settings.combat.medal_textures = (0..9).map(|n| format!("image-{n}")).collect();
        let cfg = settings.sanitized().combat;
        for (old, new) in [0, 1, 2, 4, 6, 9, 10, 12, 11].into_iter().enumerate() {
            assert_eq!(cfg.medal_textures[new], format!("image-{old}"));
        }
        for n in 0..10 {
            assert_eq!(cfg.announcement_assets[n + 1], format!("sound-{n}"));
        }
        let mut state = State::default();
        let now = Instant::now();
        for (slot, award) in awards::Award::ALL.into_iter().enumerate() {
            assert_eq!(award.sound_slot(), Some(slot));
            assert_eq!(award.medal_slot(), Some(slot));
            state.preview_award(award, now);
            assert_eq!(state.medal(false, now).map(|v| v.0), Some(award));
        }
    }

    #[test]
    fn combat_medal_stages_its_intro_hold_and_exit() {
        let mut state = State::default();
        let now = Instant::now();
        state.preview_award(awards::Award::Ultra, now);

        let (_, first_alpha, first_reveal) = state.medal(false, now).expect("preview");
        assert_eq!((first_alpha, first_reveal), (0.0, 0.0));

        let (_, intro_alpha, intro_reveal) = state.medal(false, now + Duration::from_millis(110)).expect("intro");
        assert!(intro_alpha > 0.99);
        assert!(intro_reveal > 0.5 && intro_reveal < 1.0);

        let (_, hold_alpha, hold_reveal) = state.medal(false, now + Duration::from_millis(500)).expect("hold");
        assert!(hold_alpha > 0.99 && hold_reveal > 0.99);

        let (_, exit_alpha, _) = state.medal(false, now + Duration::from_millis(2_400)).expect("exit");
        assert!(exit_alpha > 0.0 && exit_alpha < 1.0);
        assert!(state.medal(false, now + Duration::from_millis(2_600)).is_none());
    }
}
