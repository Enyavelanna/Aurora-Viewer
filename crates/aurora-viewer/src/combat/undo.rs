//! The undo of a removal in the Combat window: a card reset, the ally list
//! cleared, a marker set removed. One step, for a few seconds, like the
//! undo of a reset in Preferences (`ui/options.rs`, `UNDO_SECONDS`). The
//! markers of the active set have their own history (`tactical::History`);
//! this one covers what that history does not.
//!
//! The step waits in the tactical session (`Session::undo`), which every
//! tab, the ally list and the set panel shared with the region editor can
//! reach. It ends when its time is up, when another removal takes its place,
//! or when the windows that show it close.
use super::{Settings as Combat, tactical::Settings as Tactical};
use std::time::{Duration, Instant};

/// How long "Undo" stays: the same as Preferences.
pub(crate) const SECONDS: f32 = 8.0;

/// The settings as they were before the removal.
#[derive(Debug)]
enum Before {
    Combat(Box<Combat>),
    Tactical(Box<Tactical>),
}

#[derive(Debug)]
pub(crate) struct Undo {
    /// What was removed, as the strip says it.
    pub text: String,
    before: Before,
    at: Instant,
}

impl Undo {
    /// Before a change to the Combat settings (feedback, awards).
    pub fn combat(before: &Combat, text: impl Into<String>, now: Instant) -> Self {
        Self {
            text: text.into(),
            before: Before::Combat(Box::new(before.clone())),
            at: now,
        }
    }
    /// Before a change to the tactical settings (mini-map, sets, pings,
    /// allies). A copy of the settings starts with an empty session, so the
    /// step holds no pings and no earlier step.
    pub fn tactical(before: &Tactical, text: impl Into<String>, now: Instant) -> Self {
        Self {
            text: text.into(),
            before: Before::Tactical(Box::new(before.clone())),
            at: now,
        }
    }
    pub fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) >= Duration::from_secs_f32(SECONDS)
    }
    /// The step needs the Combat settings, which the region editor does not
    /// hold: it shows only the tactical steps.
    pub fn needs_combat(&self) -> bool {
        matches!(self.before, Before::Combat(_))
    }
    /// Puts the settings back as they were. The live session (pings in
    /// flight, transfers, the set's history) stays: only what is saved goes
    /// back. False when the step needs the Combat settings and they were not
    /// given.
    pub fn restore(self, combat: Option<&mut Combat>, tactical: &mut Tactical) -> bool {
        match self.before {
            Before::Combat(before) => {
                let Some(combat) = combat else { return false };
                *combat = *before;
            }
            Before::Tactical(before) => {
                let session = std::mem::take(&mut tactical.session);
                *tactical = *before;
                tactical.session = session;
            }
        }
        tactical.mark_unsaved();
        true
    }
}

/// The step to show this frame: the one waiting, unless its time is up.
pub(crate) fn current(tactical: &mut Tactical, now: Instant) -> Option<&Undo> {
    if tactical.session.undo.as_ref().is_some_and(|u| u.expired(now)) {
        tactical.session.undo = None;
    }
    tactical.session.undo.as_ref()
}

/// A new step: it takes the place of the one waiting, which is then final.
pub(crate) fn record(tactical: &mut Tactical, undo: Undo) {
    tactical.session.undo = Some(undo);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_undo_puts_the_card_back_and_keeps_the_tactical_settings() {
        let now = Instant::now();
        let mut combat = Combat {
            kill_sound: true,
            kill_asset: "a pasted id".into(),
            ..Combat::default()
        };
        let mut tactical = Tactical::default();
        let undo = Undo::combat(&combat, "Kill sound reset", now);
        combat.kill_sound = false;
        combat.kill_asset.clear();
        tactical.dot_scale = 2.0;
        assert!(undo.needs_combat());
        assert!(undo.restore(Some(&mut combat), &mut tactical));
        assert!(combat.kill_sound);
        assert_eq!(combat.kill_asset, "a pasted id");
        assert_eq!(tactical.dot_scale, 2.0, "a Combat step leaves the tactical settings");
        assert!(tactical.session.unsaved, "the restored settings are saved");
    }

    #[test]
    fn combat_undo_without_the_combat_settings_changes_nothing() {
        let now = Instant::now();
        let mut combat = Combat::default();
        let undo = Undo::combat(&combat, "Kill feed reset", now);
        combat.feed = true;
        let mut tactical = Tactical::default();
        assert!(!undo.restore(None, &mut tactical));
        assert!(!tactical.session.unsaved);
    }

    #[test]
    fn tactical_undo_puts_back_absent_allies_and_keeps_the_live_session() {
        let now = Instant::now();
        let mut tactical = Tactical::default();
        let away = uuid::Uuid::from_u128(7);
        tactical.recipients.insert(away, true);
        tactical.player_colors.insert(away, [1, 2, 3, 255]);
        let undo = Undo::tactical(&tactical, "1 ally removed", now);
        tactical.recipients.clear();
        tactical.player_colors.clear();
        tactical.session.editing = true;
        assert!(!undo.needs_combat());
        assert!(undo.restore(None, &mut tactical));
        assert_eq!(tactical.recipients.get(&away), Some(&true));
        assert_eq!(tactical.player_colors.get(&away), Some(&[1, 2, 3, 255]));
        assert!(tactical.session.editing, "the live session stays as it is");
    }

    #[test]
    fn a_step_lasts_as_long_as_in_preferences_and_the_newest_wins() {
        let now = Instant::now();
        let mut tactical = Tactical::default();
        record(&mut tactical, Undo::tactical(&Tactical::default(), "first", now));
        record(&mut tactical, Undo::tactical(&Tactical::default(), "second", now));
        assert_eq!(current(&mut tactical, now).map(|u| u.text.as_str()), Some("second"));
        let almost = now + Duration::from_secs_f32(SECONDS - 0.5);
        assert!(current(&mut tactical, almost).is_some());
        let later = now + Duration::from_secs_f32(SECONDS);
        assert!(current(&mut tactical, later).is_none());
        assert!(tactical.session.undo.is_none(), "an expired step is final");
    }

    #[test]
    fn a_copy_of_the_settings_never_carries_the_waiting_step() {
        let now = Instant::now();
        let mut tactical = Tactical::default();
        record(&mut tactical, Undo::tactical(&Tactical::default(), "removed", now));
        assert!(tactical.clone().session.undo.is_none());
    }
}
