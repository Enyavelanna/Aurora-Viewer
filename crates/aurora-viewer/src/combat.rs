//! Aurora combat model. Simulator fields follow wiki.secondlife.com/wiki/Combat_Log.
//! Medal rules are adapted from our KRS/KILLMARK hud/source/Core.inc (local beta).
//! No legacy viewer hit grammar is accepted.
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, time::Instant};
use uuid::Uuid;
pub(crate) mod awards;
pub(crate) mod export;
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
    /// Mouselook turn follows the zoom: 0 (off) to 1 (same on-screen speed as unzoomed).
    pub zoom_sensitivity: f32,
    pub feed: bool,
    pub feed_position: [f32; 2],
    pub feed_size: f32,
    pub feed_opacity: f32,
    pub feed_seconds: f32,
    pub feed_entries: usize,
    pub voices: bool,
    pub medals: bool,
    pub voice_volume: f32,
    /// One explicit sound UUID per medal, in the order of `Award::ALL`.
    pub announcement_assets: Vec<String>,
    /// One explicit medal-image UUID per medal, in the same order as
    /// `announcement_assets`.  Empty entries are valid.
    pub medal_textures: Vec<String>,
    /// The name the person gave each medal; empty keeps the default name.
    pub medal_names: Vec<String>,
    /// Each medal's own switch, and the switch of each family.
    pub medal_enabled: Vec<bool>,
    pub family_on: [bool; 3],
    /// How loud each family sounds, against `voice_volume`.
    pub family_volume: [f32; 3],
    /// The other medals a kill earned, under the banner.
    pub chips: bool,
    /// Medals earned by anyone on the region, in the killfeed and the journal.
    pub feed_medals: bool,
    pub banner_scale: f32,
    pub banner_seconds: f32,
    pub banner_top: f32,
    /// The banner fades without sliding or growing.
    pub banner_calm: bool,
    /// Which layout the medal lists are in: 1 had thirteen medals, 2 has
    /// the eleven of today. A settings file without it is of the first.
    #[serde(default = "old_reward_layout")]
    pub reward_version: u8,
}
/// The medals of the settings files that predate `reward_version`.
const OLD_MEDALS: usize = 13;
const REWARD_VERSION: u8 = 2;
fn old_reward_layout() -> u8 {
    1
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
            zoom_sensitivity: 0.0,
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
                "68165772-e609-2897-5ae9-02eae412fa17", // Double
                "b9bd132a-4a61-d794-33ce-7036a43ece8c", // Triple
                "",                                     // Quad
                "3cd81a7b-8397-5360-9e74-a720e8e18176", // Penta
                "643173ea-daa2-82c2-1b16-365bfb961c8a", // 10
                "",                                     // 20
                "a5316d97-14fe-91a6-0580-b99279a08ddb", // 30
                "",                                     // 50
                "9a33124d-8c5b-7cf4-934b-1d96fadcf3bd", // Revenge
                "2cc4115f-dcb0-0206-106d-274129539dab", // Domination
                "",                                     // Shutdown
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            medal_names: vec![String::new(); awards::Award::ALL.len()],
            medal_enabled: vec![true; awards::Award::ALL.len()],
            family_on: [true; 3],
            family_volume: [1.0; 3],
            chips: true,
            feed_medals: true,
            banner_scale: 1.0,
            banner_seconds: 2.55,
            banner_top: 62.0,
            banner_calm: false,
            reward_version: REWARD_VERSION,
        }
    }
}
impl Settings {
    pub fn active(&self) -> bool {
        self.enabled && (self.journal || self.killmarker || self.kill_sound || self.feed || self.voices || self.medals)
    }
    /// A medal is announced, shown and listed: its family and its own switch are on.
    pub fn medal_on(&self, award: awards::Award) -> bool {
        self.family_on[award.family().index()] && self.medal_enabled.get(award.slot()).copied().unwrap_or(true)
    }
    /// The name the person gave the medal, else its default one.
    pub fn medal_name(&self, award: awards::Award) -> &str {
        self.medal_names
            .get(award.slot())
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| award.label())
    }
    /// Brings the medal lists of an older settings file to today's medals
    /// and every value within its bounds.
    pub fn migrate_rewards(&mut self) {
        let count = awards::Award::ALL.len();
        if self.reward_version < REWARD_VERSION {
            // lists that predate the paired rows
            if self.announcement_assets.len() == 10 {
                let old = std::mem::take(&mut self.announcement_assets);
                let mut migrated = vec![String::new(); OLD_MEDALS];
                for (old_slot, new_slot) in (1..=10).enumerate() {
                    migrated[new_slot] = old[old_slot].clone();
                }
                self.announcement_assets = migrated;
            }
            if self.medal_textures.len() == 9 {
                let old = std::mem::take(&mut self.medal_textures);
                let mut migrated = vec![String::new(); OLD_MEDALS];
                for (old_slot, new_slot) in [0, 1, 2, 4, 6, 9, 10, 12, 11].into_iter().enumerate() {
                    migrated[new_slot] = old[old_slot].clone();
                }
                self.medal_textures = migrated;
            }
            self.announcement_assets.resize(OLD_MEDALS, String::new());
            self.medal_textures.resize(OLD_MEDALS, String::new());
            // thirteen medals to eleven: what was Double, Triple, Multi, Ultra,
            // 10, 20, 30 kills, Revenge and Dominating keeps its sound and image
            let keep: [(usize, usize); 9] = [(1, 0), (2, 1), (3, 2), (4, 3), (6, 4), (8, 5), (9, 6), (10, 8), (11, 9)];
            let mut sounds = vec![String::new(); count];
            let mut images = vec![String::new(); count];
            for (old, new) in keep {
                sounds[new] = std::mem::take(&mut self.announcement_assets[old]);
                images[new] = std::mem::take(&mut self.medal_textures[old]);
            }
            self.announcement_assets = sounds;
            self.medal_textures = images;
            self.reward_version = REWARD_VERSION;
        }
        self.announcement_assets.resize(count, String::new());
        self.medal_textures.resize(count, String::new());
        self.medal_names.resize(count, String::new());
        self.medal_enabled.resize(count, true);
        for name in &mut self.medal_names {
            *name = name
                .chars()
                .filter(|c| !c.is_control())
                .take(32)
                .collect::<String>()
                .trim()
                .to_owned();
        }
        for volume in &mut self.family_volume {
            *volume = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 1.0 };
        }
        let clamp = |v: f32, lo: f32, hi: f32, default: f32| if v.is_finite() { v.clamp(lo, hi) } else { default };
        self.banner_scale = clamp(self.banner_scale, 0.5, 2.0, 1.0);
        self.banner_seconds = clamp(self.banner_seconds, 1.0, 8.0, 2.55);
        self.banner_top = clamp(self.banner_top, 0.0, 400.0, 62.0);
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
    /// The medals this death earned its killer, the biggest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub awards: Vec<awards::Award>,
}
/// A medal on the banner, with the others the same kill earned.
pub(crate) struct Shown {
    pub award: awards::Award,
    pub chips: Vec<awards::Award>,
    pub at: Instant,
}
/// What the banner draws this frame.
pub(crate) struct MedalView {
    pub award: awards::Award,
    pub chips: Vec<awards::Award>,
    pub alpha: f32,
    pub reveal: f32,
}
/// Your fights: a run of kills and deaths with no more than a minute
/// between two of them.
const FIGHT_GAP_SECONDS: u64 = 60;
const RECAPS: usize = 20;
struct Fight {
    started: String,
    first: Instant,
    last: Instant,
    kills: u32,
    deaths: u32,
    best_streak: u64,
    medals: std::collections::BTreeMap<awards::Award, u32>,
}
/// What the lists (the killfeed, the journal) need of the settings.
#[derive(Default)]
pub(crate) struct Display {
    pub medals: bool,
    names: Vec<String>,
}
impl Display {
    pub fn name(&self, award: awards::Award) -> &str {
        self.names.get(award.slot()).map_or_else(|| award.label(), String::as_str)
    }
}
/// A finished fight, for the journal.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Recap {
    pub timestamp: String,
    pub seconds: u64,
    pub kills: u32,
    pub deaths: u32,
    pub best_streak: u64,
    pub medals: Vec<(awards::Award, u32)>,
}
#[derive(Default)]
pub(crate) struct State {
    pub history: Vec<RecordedEvent>,
    pub personal: Vec<usize>,
    pub feed: VecDeque<RecordedEvent>,
    pub personal_view: bool,
    pub last_kill: Option<Instant>,
    pub awards: awards::Tracker,
    /// The streaks and rivalries of everyone else.
    pub roster: awards::Roster,
    pub voices: VecDeque<(awards::Award, Instant)>,
    pub medals: VecDeque<Shown>,
    pub preview_medal: Option<(awards::Award, Instant)>,
    pub medal_started: Option<Instant>,
    pub voice_after: Option<Instant>,
    pub journal: journal::Journal,
    pub received: u64,
    pub lost: u64,
    fight: Option<Fight>,
    pub recaps: Vec<Recap>,
    pub display: Display,
}
impl State {
    /// Keeps the names and the switch the lists use up to date.
    pub fn sync_display(&mut self, cfg: &Settings) {
        self.display.medals = cfg.feed_medals;
        let same = self.display.names.len() == awards::Award::ALL.len()
            && awards::Award::ALL
                .iter()
                .all(|a| self.display.names[a.slot()] == cfg.medal_name(*a));
        if !same {
            self.display.names = awards::Award::ALL.iter().map(|a| cfg.medal_name(*a).to_owned()).collect();
        }
    }
    pub fn record(&mut self, event: Event, agent: Uuid, cfg: &Settings, now: Instant) {
        if !cfg.enabled {
            return;
        }
        self.received += 1;
        self.close_fight(now);
        let outcome = event.outcome(agent);
        if event.own_kill(agent) {
            self.last_kill = Some(now);
        }
        let (actor, target) = (event.actor, event.target);
        let shutdown = |streak: u64| streak >= awards::SHUTDOWN_MIN && actor != target && !actor.is_nil();
        let mut earned = match outcome {
            Outcome::Observed => {
                let shut = shutdown(self.roster.streak(&target));
                self.roster.died(target, actor, now);
                let mut got = self.roster.killed(actor, target, now);
                if shut {
                    got.push(awards::Award::Shutdown);
                }
                got
            }
            Outcome::Kill => {
                let shut = shutdown(self.roster.streak(&target));
                self.roster.died(target, actor, now);
                let mut got = self.awards.accept(outcome, actor, target, now);
                if shut {
                    got.push(awards::Award::Shutdown);
                }
                got
            }
            Outcome::Death => {
                let shut = shutdown(self.awards.streak());
                self.awards.accept(outcome, actor, target, now);
                let mut got = self.roster.killed(actor, target, now);
                if shut {
                    got.push(awards::Award::Shutdown);
                }
                got
            }
            Outcome::Suicide | Outcome::Environment => {
                self.awards.accept(outcome, actor, target, now);
                Vec::new()
            }
        };
        earned.retain(|a| cfg.medal_on(*a));
        let picked = awards::pick(&earned);
        if outcome == Outcome::Kill
            && let Some((main, chips)) = &picked
        {
            if cfg.voices {
                if self.voices.len() == 8 {
                    self.voices.pop_front();
                }
                self.voices.push_back((*main, now));
            }
            if cfg.medals {
                if self.medals.len() == 8 {
                    self.medals.pop_back();
                }
                self.medals.push_back(Shown {
                    award: *main,
                    chips: if cfg.chips { chips.clone() } else { Vec::new() },
                    at: now,
                });
            }
        }
        self.note_fight(outcome, &earned, now);
        let row = RecordedEvent {
            at: now,
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            event,
            awards: picked.map_or_else(Vec::new, |(main, mut others)| {
                others.insert(0, main);
                others
            }),
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
    /// Your kills and deaths, grouped into fights.
    fn note_fight(&mut self, outcome: Outcome, earned: &[awards::Award], now: Instant) {
        if matches!(outcome, Outcome::Observed) {
            return;
        }
        let fight = self.fight.get_or_insert_with(|| Fight {
            started: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            first: now,
            last: now,
            kills: 0,
            deaths: 0,
            best_streak: 0,
            medals: std::collections::BTreeMap::new(),
        });
        fight.last = now;
        if outcome == Outcome::Kill {
            fight.kills += 1;
            fight.best_streak = fight.best_streak.max(self.awards.streak());
            for award in earned {
                *fight.medals.entry(*award).or_default() += 1;
            }
        } else {
            fight.deaths += 1;
        }
    }
    /// A fight is over a minute after its last kill or death.
    pub fn close_fight(&mut self, now: Instant) {
        if self
            .fight
            .as_ref()
            .is_some_and(|f| now.saturating_duration_since(f.last).as_secs() >= FIGHT_GAP_SECONDS)
            && let Some(f) = self.fight.take()
        {
            if self.recaps.len() == RECAPS {
                self.recaps.remove(0);
            }
            self.recaps.push(Recap {
                timestamp: f.started,
                seconds: f.last.saturating_duration_since(f.first).as_secs(),
                kills: f.kills,
                deaths: f.deaths,
                best_streak: f.best_streak,
                medals: f.medals.into_iter().rev().collect(),
            });
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
        self.roster.clear();
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
            if let Some(asset) = cfg.announcement_assets.get(award.slot())
                && let Ok(id) = Uuid::parse_str(asset)
                && !id.is_nil()
            {
                self.voice_after = Some(now + std::time::Duration::from_millis(1800));
                return Some((id, cfg.voice_volume * cfg.family_volume[award.family().index()]));
            }
        }
        None
    }
    pub fn medal(&mut self, cfg: &Settings, now: Instant) -> Option<MedalView> {
        let hold = cfg.banner_seconds.clamp(1.0, 8.0);
        let visual = |age: f32| {
            let intro = if cfg.banner_calm { 1.0 } else { (age / 0.22).clamp(0.0, 1.0) };
            let reveal = 1.0 - (1.0 - intro).powi(3);
            let alpha = (age / 0.1).clamp(0.0, 1.0) * ((hold - age) / 0.35).clamp(0.0, 1.0);
            (alpha, reveal)
        };
        if let Some((award, started)) = self.preview_medal {
            let age = now.saturating_duration_since(started).as_secs_f32();
            if age < hold {
                let (alpha, reveal) = visual(age);
                return Some(MedalView {
                    award,
                    chips: Vec::new(),
                    alpha,
                    reveal,
                });
            }
            self.preview_medal = None;
        }
        if !cfg.medals {
            self.medals.clear();
            self.medal_started = None;
            return None;
        }
        loop {
            let front = self.medals.front()?;
            if self.medal_started.is_none() && now.saturating_duration_since(front.at).as_secs_f32() > 8.0 {
                self.medals.pop_front();
                continue;
            }
            let start = *self.medal_started.get_or_insert(now);
            let age = now.saturating_duration_since(start).as_secs_f32();
            if age >= hold {
                self.medals.pop_front();
                self.medal_started = None;
                continue;
            }
            let (alpha, reveal) = visual(age);
            return Some(MedalView {
                award: front.award,
                chips: front.chips.clone(),
                alpha,
                reveal,
            });
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
        cfg.announcement_assets[awards::Award::Double.slot()] = Uuid::from_u128(90).to_string();
        let mut state = State::default();
        state.record(death(1, 2), me, &cfg, now);
        state.record(death(1, 3), me, &cfg, now);
        assert_eq!(state.voice(&cfg, now).map(|v| v.0), Some(Uuid::from_u128(90)));
        assert!(state.medal(&cfg, now).is_none());
        cfg.voices = false;
        cfg.medals = true;
        state.record(death(1, 4), me, &cfg, now);
        assert!(state.voice(&cfg, now).is_none());
        assert_eq!(state.medal(&cfg, now).map(|v| v.award), Some(awards::Award::Triple));
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
    fn combat_reward_migration_keeps_the_sound_and_image_of_each_medal_that_stays() {
        // the thirteen medals of a settings file without `reward_version`
        let mut settings = crate::settings::Settings::default();
        settings.combat.reward_version = 1;
        settings.combat.announcement_assets = (0..13).map(|n| format!("sound-{n}")).collect();
        settings.combat.medal_textures = (0..13).map(|n| format!("image-{n}")).collect();
        let cfg = settings.sanitized().combat;
        assert_eq!(cfg.reward_version, 2);
        // Double, Triple, Multi, Ultra, 10, 20, 30, Revenge, Dominating
        for (old, new) in [(1, 0), (2, 1), (3, 2), (4, 3), (6, 4), (8, 5), (9, 6), (10, 8), (11, 9)] {
            assert_eq!(cfg.announcement_assets[new], format!("sound-{old}"));
            assert_eq!(cfg.medal_textures[new], format!("image-{old}"));
        }
        for gone in [7, 10] {
            assert!(cfg.announcement_assets[gone].is_empty() && cfg.medal_textures[gone].is_empty());
        }
        assert_eq!(cfg.medal_names.len(), awards::Award::ALL.len());
        assert!(cfg.medal_enabled.iter().all(|on| *on));
        // the lists before the paired rows (ten sounds, nine images) arrive the same way
        let mut settings = crate::settings::Settings::default();
        settings.combat.reward_version = 1;
        settings.combat.announcement_assets = (0..10).map(|n| format!("sound-{n}")).collect();
        settings.combat.medal_textures = (0..9).map(|n| format!("image-{n}")).collect();
        let cfg = settings.sanitized().combat;
        assert_eq!(cfg.announcement_assets[0], "sound-0");
        assert_eq!(cfg.medal_textures[3], "image-3");
        assert_eq!(cfg.medal_textures[8], "image-6");
        // today's lists are left as they are
        let mut settings = crate::settings::Settings::default();
        settings.combat.announcement_assets[10] = "mine".into();
        assert_eq!(settings.sanitized().combat.announcement_assets[10], "mine");
    }
    #[test]
    fn combat_preview_ignores_the_medal_toggle() {
        let mut state = State::default();
        let now = Instant::now();
        let cfg = Settings::default();
        for award in awards::Award::ALL {
            state.preview_award(award, now);
            assert_eq!(state.medal(&cfg, now).map(|v| v.award), Some(award));
        }
    }

    #[test]
    fn combat_medal_stages_its_intro_hold_and_exit() {
        let mut state = State::default();
        let now = Instant::now();
        let cfg = Settings::default();
        state.preview_award(awards::Award::Penta, now);

        let first = state.medal(&cfg, now).expect("preview");
        assert_eq!((first.alpha, first.reveal), (0.0, 0.0));

        let intro = state.medal(&cfg, now + Duration::from_millis(110)).expect("intro");
        assert!(intro.alpha > 0.99);
        assert!(intro.reveal > 0.5 && intro.reveal < 1.0);

        let hold = state.medal(&cfg, now + Duration::from_millis(500)).expect("hold");
        assert!(hold.alpha > 0.99 && hold.reveal > 0.99);

        let exit = state.medal(&cfg, now + Duration::from_millis(2_400)).expect("exit");
        assert!(exit.alpha > 0.0 && exit.alpha < 1.0);
        assert!(state.medal(&cfg, now + Duration::from_millis(2_600)).is_none());
    }
    #[test]
    fn a_calm_banner_only_fades_and_stays_as_long_as_asked() {
        let mut state = State::default();
        let now = Instant::now();
        let cfg = Settings {
            banner_calm: true,
            banner_seconds: 4.0,
            ..Settings::default()
        };
        state.preview_award(awards::Award::Double, now);
        assert_eq!(state.medal(&cfg, now).expect("start").reveal, 1.0, "nothing slides or grows");
        assert!(state.medal(&cfg, now + Duration::from_millis(3_000)).expect("held").alpha > 0.99);
        assert!(state.medal(&cfg, now + Duration::from_millis(4_100)).is_none());
    }
    #[test]
    fn one_kill_is_announced_once_and_its_other_medals_become_chips() {
        let me = Uuid::from_u128(1);
        let cfg = Settings {
            medals: true,
            ..Default::default()
        };
        let mut state = State::default();
        let now = Instant::now();
        // one person kills me five times (and so is on a streak), then I kill them after
        // another kill: Revenge, a Shutdown and a Double on the same kill
        for _ in 0..5 {
            state.record(death(2, 1), me, &cfg, now);
        }
        state.record(death(1, 3), me, &cfg, now);
        state.record(death(1, 2), me, &cfg, now);
        let shown = state.medals.back().expect("a banner");
        assert_eq!(shown.award, awards::Award::Revenge);
        assert_eq!(shown.chips, vec![awards::Award::Shutdown, awards::Award::Double]);
        assert_eq!(state.medals.len(), 1, "the first kill earned nothing");
        let row = state.feed.back().expect("row");
        assert_eq!(
            row.awards,
            vec![awards::Award::Revenge, awards::Award::Shutdown, awards::Award::Double]
        );
    }
    #[test]
    fn a_medal_that_is_switched_off_is_neither_announced_nor_listed() {
        let me = Uuid::from_u128(1);
        let mut cfg = Settings {
            medals: true,
            voices: true,
            ..Default::default()
        };
        cfg.medal_enabled[awards::Award::Double.slot()] = false;
        let mut state = State::default();
        let now = Instant::now();
        state.record(death(1, 2), me, &cfg, now);
        state.record(death(1, 3), me, &cfg, now);
        assert!(state.medals.is_empty() && state.voices.is_empty());
        assert!(state.feed.back().expect("row").awards.is_empty());
        cfg.medal_enabled[awards::Award::Double.slot()] = true;
        cfg.family_on[awards::Family::Combo.index()] = false;
        assert!(!cfg.medal_on(awards::Award::Double), "a family off silences its medals");
    }
    #[test]
    fn other_people_earn_medals_that_the_feed_lists_and_a_shutdown_ends_a_streak() {
        let me = Uuid::from_u128(1);
        let cfg = Settings::default();
        let mut state = State::default();
        let now = Instant::now();
        // 7 and 8 are not me: 7 kills five people in a row
        for n in 0..5 {
            state.record(death(7, 100 + n), me, &cfg, now + Duration::from_secs(n as u64 * 6));
        }
        // then I kill 7: a Shutdown for me, and a feed row that says so
        state.record(death(1, 7), me, &cfg, now + Duration::from_secs(40));
        assert_eq!(state.feed.back().expect("row").awards, vec![awards::Award::Shutdown]);
        // 8 kills 7's killer's... a stranger ends a streak too
        for n in 0..5 {
            state.record(death(8, 200 + n), me, &cfg, now + Duration::from_secs(50 + n as u64 * 6));
        }
        state.record(death(9, 8), me, &cfg, now + Duration::from_secs(90));
        assert_eq!(state.feed.back().expect("row").awards, vec![awards::Award::Shutdown]);
        // and a stranger's double kill is in the feed
        state.record(death(9, 300), me, &cfg, now + Duration::from_secs(91));
        assert_eq!(state.feed.back().expect("row").awards, vec![awards::Award::Double]);
        // a gap in the events forgets everyone
        state.note_loss(1);
        assert_eq!(state.roster.streak(&Uuid::from_u128(9)), 0);
    }
    #[test]
    fn a_fight_ends_a_minute_after_its_last_event_and_is_recapped() {
        let me = Uuid::from_u128(1);
        let cfg = Settings::default();
        let mut state = State::default();
        let now = Instant::now();
        state.record(death(1, 2), me, &cfg, now);
        state.record(death(1, 3), me, &cfg, now + Duration::from_secs(2));
        state.record(death(4, 1), me, &cfg, now + Duration::from_secs(10));
        state.record(death(5, 6), me, &cfg, now + Duration::from_secs(20));
        state.close_fight(now + Duration::from_secs(60));
        assert!(state.recaps.is_empty(), "still within the minute");
        state.close_fight(now + Duration::from_secs(70));
        let recap = state.recaps.last().expect("recap");
        assert_eq!((recap.kills, recap.deaths, recap.best_streak, recap.seconds), (2, 1, 2, 10));
        assert_eq!(recap.medals, vec![(awards::Award::Double, 1)]);
        // the next event starts another fight
        state.record(death(1, 9), me, &cfg, now + Duration::from_secs(200));
        state.close_fight(now + Duration::from_secs(300));
        assert_eq!(state.recaps.len(), 2);
    }
}
