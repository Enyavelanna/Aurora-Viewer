//! Medals. Local KRS/KILLMARK rules, ported from hud/source/Core.inc and Effects.inc,
//! then reduced to eleven medals in three families. Five seconds between kills of a
//! combo is AuroraC2's requested rule (KRS used three).
use super::Outcome;
use aurora_i18n::tr;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use uuid::Uuid;

/// A streak this long, ended by a kill, earns the killer a Shutdown.
pub(crate) const SHUTDOWN_MIN: u64 = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Award {
    Double,
    Triple,
    Quad,
    Penta,
    Streak10,
    Streak20,
    Streak30,
    Streak50,
    Revenge,
    Domination,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Family {
    /// Kills a few seconds apart.
    Combo,
    /// Kills without dying.
    Streak,
    /// What happens between two people.
    Event,
}

impl Family {
    pub const ALL: [Self; 3] = [Self::Combo, Self::Streak, Self::Event];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The medals of the family, as slots of `Award::ALL`.
    pub fn slots(self) -> std::ops::Range<usize> {
        match self {
            Self::Combo => 0..4,
            Self::Streak => 4..8,
            Self::Event => 8..11,
        }
    }
}

impl Award {
    pub const ALL: [Self; 11] = [
        Self::Double,
        Self::Triple,
        Self::Quad,
        Self::Penta,
        Self::Streak10,
        Self::Streak20,
        Self::Streak30,
        Self::Streak50,
        Self::Revenge,
        Self::Domination,
        Self::Shutdown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Double => tr!("combat-award-double"),
            Self::Triple => tr!("combat-award-triple"),
            Self::Quad => tr!("combat-award-quad"),
            Self::Penta => tr!("combat-award-penta"),
            Self::Streak10 => tr!("combat-award-streak10"),
            Self::Streak20 => tr!("combat-award-streak20"),
            Self::Streak30 => tr!("combat-award-streak30"),
            Self::Streak50 => tr!("combat-award-streak50"),
            Self::Revenge => tr!("combat-award-revenge"),
            Self::Domination => tr!("combat-award-dominating"),
            Self::Shutdown => tr!("combat-award-shutdown"),
        }
    }

    pub fn family(self) -> Family {
        match self {
            Self::Double | Self::Triple | Self::Quad | Self::Penta => Family::Combo,
            Self::Streak10 | Self::Streak20 | Self::Streak30 | Self::Streak50 => Family::Streak,
            Self::Revenge | Self::Domination | Self::Shutdown => Family::Event,
        }
    }

    /// Where its sound, its image, its name and its switch are kept.
    pub fn slot(self) -> usize {
        self as usize
    }

    /// Which of two medals earned by one kill is announced: streaks, then
    /// events, then combos; the bigger one first inside a family.
    pub fn rank(self) -> u8 {
        match self {
            Self::Double => 10,
            Self::Triple => 11,
            Self::Quad => 12,
            Self::Penta => 13,
            Self::Domination => 31,
            Self::Shutdown => 32,
            Self::Revenge => 33,
            Self::Streak10 => 40,
            Self::Streak20 => 41,
            Self::Streak30 => 42,
            Self::Streak50 => 43,
        }
    }
}

/// One announcement among the medals a kill earned: the one that is
/// announced, and the others, to be shown small.
pub(crate) fn pick(earned: &[Award]) -> Option<(Award, Vec<Award>)> {
    let main = earned.iter().copied().max_by_key(|a| a.rank())?;
    let mut others: Vec<Award> = earned.iter().copied().filter(|a| *a != main).collect();
    others.sort_by_key(|a| std::cmp::Reverse(a.rank()));
    Some((main, others))
}

#[derive(Default)]
struct Opponent {
    kills: u64,
    deaths: u64,
    won: u64,
    lost: u64,
}

pub(crate) struct Tracker {
    opponents: HashMap<Uuid, Opponent>,
    streak: u64,
    rapid: u64,
    last: Option<Instant>,
    /// False once events were missed: what depends on the people met since
    /// the start is no longer certain.
    opponent_certain: bool,
    life_certain: bool,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            opponents: HashMap::new(),
            streak: 0,
            rapid: 0,
            last: None,
            opponent_certain: true,
            life_certain: true,
        }
    }
}

impl Tracker {
    pub fn invalidate(&mut self) {
        self.streak = 0;
        self.rapid = 0;
        self.last = None;
        self.opponents.clear();
        self.opponent_certain = false;
        self.life_certain = false;
    }

    /// Kills since the last death.
    pub fn streak(&self) -> u64 {
        self.streak
    }

    pub fn accept(&mut self, outcome: Outcome, actor: Uuid, target: Uuid, now: Instant) -> Vec<Award> {
        let mut result = Vec::new();
        if outcome == Outcome::Observed {
            return result;
        }
        if outcome != Outcome::Kill {
            self.streak = 0;
            self.rapid = 0;
            self.last = None;
            self.life_certain = true;
            if outcome == Outcome::Death {
                let o = self.opponents.entry(actor).or_default();
                o.deaths += 1;
                o.lost += 1;
                o.won = 0;
            }
            return result;
        }
        self.rapid = if self
            .last
            .is_some_and(|t| now.saturating_duration_since(t) <= Duration::from_secs(5))
        {
            self.rapid + 1
        } else {
            1
        };
        self.last = Some(now);
        self.streak += 1;
        match self.rapid {
            2 => result.push(Award::Double),
            3 => result.push(Award::Triple),
            4 => result.push(Award::Quad),
            5 => result.push(Award::Penta),
            _ => {}
        }
        match self.streak {
            10 if self.life_certain => result.push(Award::Streak10),
            20 if self.life_certain => result.push(Award::Streak20),
            30 if self.life_certain => result.push(Award::Streak30),
            50 if self.life_certain => result.push(Award::Streak50),
            _ => {}
        }
        let o = self.opponents.entry(target).or_default();
        if self.opponent_certain && o.lost >= 5 {
            result.push(Award::Revenge);
        }
        o.kills += 1;
        o.won += 1;
        o.lost = 0;
        if self.opponent_certain && o.won == 5 {
            result.push(Award::Domination);
        }
        result
    }
}

/// The streaks and rivalries of everyone else on the region, worked out from
/// the deaths received, so that what they earn can be listed too.
#[derive(Default)]
pub(crate) struct Roster {
    runs: HashMap<Uuid, Tracker>,
}

impl Roster {
    const CAP: usize = 512;

    pub fn clear(&mut self) {
        self.runs.clear();
    }

    pub fn streak(&self, id: &Uuid) -> u64 {
        self.runs.get(id).map_or(0, Tracker::streak)
    }

    /// `target` died: its streak ends.
    pub fn died(&mut self, target: Uuid, actor: Uuid, now: Instant) {
        if self.runs.len() >= Self::CAP && !self.runs.contains_key(&target) {
            self.runs.clear();
        }
        let outcome = if actor == target {
            Outcome::Suicide
        } else if actor.is_nil() {
            Outcome::Environment
        } else {
            Outcome::Death
        };
        self.runs.entry(target).or_default().accept(outcome, actor, target, now);
    }

    /// `actor` killed `target`: what that earned the actor.
    pub fn killed(&mut self, actor: Uuid, target: Uuid, now: Instant) -> Vec<Award> {
        if actor.is_nil() || actor == target {
            return Vec::new();
        }
        if self.runs.len() >= Self::CAP && !self.runs.contains_key(&actor) {
            self.runs.clear();
        }
        self.runs.entry(actor).or_default().accept(Outcome::Kill, actor, target, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ME: Uuid = Uuid::from_u128(1);
    const OTHER: Uuid = Uuid::from_u128(2);
    #[test]
    fn combat_krs_rapid_window_is_five_seconds_inclusive_and_death_resets_it() {
        let mut t = Tracker::default();
        let now = Instant::now();
        assert!(t.accept(Outcome::Kill, ME, OTHER, now).is_empty());
        assert!(
            t.accept(Outcome::Kill, ME, OTHER, now + Duration::from_secs(5))
                .contains(&Award::Double)
        );
        assert!(t.accept(Outcome::Kill, ME, OTHER, now + Duration::from_millis(10001)).is_empty());
        t.accept(Outcome::Suicide, ME, ME, now + Duration::from_secs(11));
        assert!(t.accept(Outcome::Kill, ME, OTHER, now + Duration::from_secs(12)).is_empty());
    }
    #[test]
    fn combos_go_up_to_five_kills_and_each_fires_once() {
        let mut t = Tracker::default();
        let now = Instant::now();
        let mut got = Vec::new();
        for n in 0..6 {
            got.extend(t.accept(Outcome::Kill, ME, Uuid::from_u128(10 + n), now + Duration::from_secs(n as u64)));
        }
        assert_eq!(got, vec![Award::Double, Award::Triple, Award::Quad, Award::Penta]);
    }
    #[test]
    fn combat_krs_life_thresholds_fire_once_and_revenge_is_local() {
        let now = Instant::now();
        let mut t = Tracker::default();
        let mut got = Vec::new();
        for n in 0..51 {
            got.extend(t.accept(Outcome::Kill, ME, Uuid::from_u128(n + 2), now + Duration::from_secs(n as u64 * 6)));
        }
        for a in [Award::Streak10, Award::Streak20, Award::Streak30, Award::Streak50] {
            assert_eq!(got.iter().filter(|&&v| v == a).count(), 1);
        }
        assert_eq!(t.streak(), 51);
        let mut t = Tracker::default();
        for _ in 0..5 {
            t.accept(Outcome::Death, OTHER, ME, now);
        }
        assert!(t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Revenge));
        assert!(!t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Revenge));
    }
    #[test]
    fn combat_krs_domination_and_gap_do_not_invent_awards() {
        let now = Instant::now();
        let mut t = Tracker::default();
        for _ in 0..4 {
            assert!(!t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Domination));
        }
        assert!(t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Domination));
        assert!(!t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Domination));
        t.invalidate();
        for _ in 0..5 {
            let a = t.accept(Outcome::Kill, ME, OTHER, now);
            assert!(!a.contains(&Award::Domination));
        }
    }
    #[test]
    fn one_medal_is_announced_streaks_first_then_events_then_combos() {
        assert_eq!(pick(&[]), None);
        assert_eq!(
            pick(&[Award::Double, Award::Revenge, Award::Streak10]),
            Some((Award::Streak10, vec![Award::Revenge, Award::Double]))
        );
        assert_eq!(
            pick(&[Award::Triple, Award::Domination]),
            Some((Award::Domination, vec![Award::Triple]))
        );
        assert_eq!(pick(&[Award::Penta, Award::Quad]), Some((Award::Penta, vec![Award::Quad])));
    }
    #[test]
    fn every_medal_has_a_slot_in_order_and_a_family() {
        for (slot, award) in Award::ALL.into_iter().enumerate() {
            assert_eq!(award.slot(), slot);
            assert!(award.family().slots().contains(&slot));
        }
        assert_eq!(Family::ALL.iter().map(|f| f.slots().len()).sum::<usize>(), Award::ALL.len());
    }
    #[test]
    fn the_roster_follows_other_people_and_ends_their_streaks() {
        let now = Instant::now();
        let mut roster = Roster::default();
        let (a, b) = (Uuid::from_u128(7), Uuid::from_u128(8));
        for n in 0..10 {
            roster.died(Uuid::from_u128(100 + n), a, now);
            let got = roster.killed(a, Uuid::from_u128(100 + n), now + Duration::from_secs(n as u64 * 6));
            assert_eq!(got.contains(&Award::Streak10), n == 9);
        }
        assert_eq!(roster.streak(&a), 10);
        roster.died(a, b, now);
        assert_eq!(roster.streak(&a), 0, "a death ends a streak");
        assert!(roster.killed(Uuid::nil(), a, now).is_empty(), "the world earns nothing");
        assert!(roster.killed(a, a, now).is_empty(), "nor does a suicide");
    }
}
