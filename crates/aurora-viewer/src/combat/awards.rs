//! Local KRS/KILLMARK rules, ported from hud/source/Core.inc and Effects.inc.
//! Five seconds between rapid kills is AuroraC2's requested rule (KRS used three).
use super::Outcome;
use aurora_i18n::tr;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Award {
    First,
    Double,
    Triple,
    Multi,
    Ultra,
    Five,
    Ten,
    Fifteen,
    Twenty,
    Thirty,
    Revenge,
    Dominating,
    Comeback,
}
impl Award {
    pub const ALL: [Self; 13] = [
        Self::First,
        Self::Double,
        Self::Triple,
        Self::Multi,
        Self::Ultra,
        Self::Five,
        Self::Ten,
        Self::Fifteen,
        Self::Twenty,
        Self::Thirty,
        Self::Revenge,
        Self::Dominating,
        Self::Comeback,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::First => tr!("combat-award-first"),
            Self::Double => tr!("combat-award-double"),
            Self::Triple => tr!("combat-award-triple"),
            Self::Multi => tr!("combat-award-multi"),
            Self::Ultra => tr!("combat-award-ultra"),
            Self::Five => tr!("combat-award-five"),
            Self::Ten => tr!("combat-award-ten"),
            Self::Fifteen => tr!("combat-award-fifteen"),
            Self::Twenty => tr!("combat-award-twenty"),
            Self::Thirty => tr!("combat-award-thirty"),
            Self::Revenge => tr!("combat-award-revenge"),
            Self::Dominating => tr!("combat-award-dominating"),
            Self::Comeback => tr!("combat-award-comeback"),
        }
    }
    pub fn sound_slot(self) -> Option<usize> {
        Some(self as usize)
    }
    pub fn medal_slot(self) -> Option<usize> {
        Some(self as usize)
    }
}
#[derive(Default)]
struct Opponent {
    kills: u64,
    deaths: u64,
    won: u64,
    lost: u64,
    comeback: bool,
}
pub(crate) struct Tracker {
    opponents: HashMap<Uuid, Opponent>,
    first_seen: bool,
    streak: u64,
    rapid: u64,
    last: Option<Instant>,
    opponent_certain: bool,
    life_certain: bool,
}
impl Default for Tracker {
    fn default() -> Self {
        Self {
            opponents: HashMap::new(),
            first_seen: false,
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
        // Missing earlier kills must not invent a first-frag award.
        self.first_seen = true;
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
                if o.deaths >= o.kills + 3 {
                    o.comeback = true;
                }
            }
            return result;
        }
        // Before any gap the default empty tracker has complete observations.
        if !self.first_seen {
            self.opponent_certain = true;
            result.push(Award::First);
            self.first_seen = true;
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
            4 => result.push(Award::Multi),
            5 => result.push(Award::Ultra),
            _ => {}
        }
        match self.streak {
            5 if self.life_certain => result.push(Award::Five),
            10 if self.life_certain => result.push(Award::Ten),
            15 if self.life_certain => result.push(Award::Fifteen),
            20 if self.life_certain => result.push(Award::Twenty),
            30 if self.life_certain => result.push(Award::Thirty),
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
            result.push(Award::Dominating);
        }
        if self.opponent_certain && o.comeback && o.kills > o.deaths {
            result.push(Award::Comeback);
            o.comeback = false;
        }
        result
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
        assert!(t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::First));
        assert!(
            t.accept(Outcome::Kill, ME, OTHER, now + Duration::from_secs(5))
                .contains(&Award::Double)
        );
        assert!(t.accept(Outcome::Kill, ME, OTHER, now + Duration::from_millis(10001)).is_empty());
        t.accept(Outcome::Suicide, ME, ME, now + Duration::from_secs(11));
        assert!(t.accept(Outcome::Kill, ME, OTHER, now + Duration::from_secs(12)).is_empty());
    }
    #[test]
    fn combat_krs_all_life_thresholds_fire_once_and_revenge_is_local() {
        let now = Instant::now();
        let mut t = Tracker::default();
        let mut got = Vec::new();
        for n in 0..31 {
            got.extend(t.accept(Outcome::Kill, ME, Uuid::from_u128(n + 2), now + Duration::from_secs(n as u64 * 6)));
        }
        for a in [Award::Five, Award::Ten, Award::Fifteen, Award::Twenty, Award::Thirty] {
            assert_eq!(got.iter().filter(|&&v| v == a).count(), 1);
        }
        let mut t = Tracker::default();
        for _ in 0..5 {
            t.accept(Outcome::Death, OTHER, ME, now);
        }
        assert!(t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Revenge));
        assert!(!t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Revenge));
    }
    #[test]
    fn combat_krs_comeback_dominating_and_gap_do_not_invent_awards() {
        let now = Instant::now();
        let mut t = Tracker::default();
        for _ in 0..3 {
            t.accept(Outcome::Death, OTHER, ME, now);
        }
        for _ in 0..3 {
            assert!(!t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Comeback));
        }
        assert!(t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Comeback));
        assert!(t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Dominating));
        assert!(!t.accept(Outcome::Kill, ME, OTHER, now).contains(&Award::Dominating));
        t.invalidate();
        for _ in 0..5 {
            let a = t.accept(Outcome::Kill, ME, OTHER, now);
            assert!(!a.contains(&Award::First) && !a.contains(&Award::Dominating) && !a.contains(&Award::Comeback));
        }
    }
}
