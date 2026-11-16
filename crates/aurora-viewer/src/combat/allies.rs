//! Allies: the residents you stand with, declared in the Allies tab of the
//! Combat window. They are the residents you ticked there (the same list as
//! the Sharing teammates), your worn group, and every group tag you allied.
//! Pings can be shared with them, and the minimap draws them in a colour of
//! their own.
//!
//! A resident around us shows the *title* of the group they wear (the second
//! line of their name tag) but not the group itself, so the group of someone
//! else is allied by that title: whoever around us wears it is an ally.
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// `tactical::Settings::ping_audience` for "my allies".
pub(crate) const AUDIENCE: u8 = 3;
const MAX_TAGS: usize = 32;
const MAX_TAG_CHARS: usize = 64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Allies {
    /// Nothing below counts while this is off; the declared allies are kept.
    pub enabled: bool,
    /// The members of the group you wear are allies.
    pub active_group: bool,
    /// Group titles whose wearers around us are allies.
    pub tags: Vec<String>,
    /// The minimap draws allies in `color`.
    pub minimap: bool,
    pub color: [u8; 4],
    /// A ring around the ally's dot, for those who cannot tell the colours apart.
    pub ring: bool,
}

impl Default for Allies {
    fn default() -> Self {
        Self {
            enabled: true,
            active_group: false,
            tags: Vec::new(),
            minimap: true,
            color: [64, 196, 255, 255],
            ring: true,
        }
    }
}

/// A title as it is stored and compared: single spaces, no control character.
pub(crate) fn clean_tag(tag: &str) -> String {
    tag.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TAG_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn same_tag(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

impl Allies {
    pub fn has_tag(&self, title: &str) -> bool {
        let title = clean_tag(title);
        !title.is_empty() && self.tags.iter().any(|t| same_tag(t, &title))
    }

    /// False when the title is empty, already allied or the list is full.
    pub fn add_tag(&mut self, title: &str) -> bool {
        let title = clean_tag(title);
        if title.is_empty() || self.has_tag(&title) || self.tags.len() >= MAX_TAGS {
            return false;
        }
        self.tags.push(title);
        true
    }

    pub fn remove_tag(&mut self, title: &str) {
        let title = clean_tag(title);
        self.tags.retain(|t| !same_tag(t, &title));
    }

    /// Brings a settings file within its bounds.
    pub fn sanitize(&mut self) {
        let tags = std::mem::take(&mut self.tags);
        for tag in tags {
            self.add_tag(&tag);
        }
    }

    /// Who counts as an ally: the ticked residents, the members of the worn
    /// group when it is on, and the residents whose group title is allied.
    /// Never yourself.
    pub fn gather(
        &self,
        people: impl IntoIterator<Item = Uuid>,
        group: impl IntoIterator<Item = Uuid>,
        titled: impl IntoIterator<Item = (Uuid, Option<String>)>,
        me: Uuid,
    ) -> BTreeSet<Uuid> {
        if !self.enabled {
            return BTreeSet::new();
        }
        let mut allies: BTreeSet<Uuid> = people.into_iter().collect();
        if self.active_group {
            allies.extend(group);
        }
        if !self.tags.is_empty() {
            allies.extend(
                titled
                    .into_iter()
                    .filter(|(_, title)| title.as_deref().is_some_and(|t| self.has_tag(t)))
                    .map(|(id, _)| id),
            );
        }
        allies.remove(&me);
        allies.remove(&Uuid::nil());
        allies
    }

    /// The allies right now: `recipients` is the Sharing teammates list.
    pub fn resolve(&self, world: &World, recipients: &BTreeMap<Uuid, bool>) -> BTreeSet<Uuid> {
        let active = world.groups.active;
        self.gather(
            recipients.iter().filter(|(_, on)| **on).map(|(id, _)| *id),
            world
                .group_data
                .get(&active)
                .filter(|g| g.members_complete && !active.is_nil())
                .into_iter()
                .flat_map(|g| g.members.keys().copied()),
            world.objects.avatars().map(|(_, o)| (o.full_id, o.group_title())),
            world.agent_id,
        )
    }

    /// The worn group is on but its member list has not arrived.
    pub fn group_loading(&self, world: &World) -> bool {
        let active = world.groups.active;
        self.enabled && self.active_group && !active.is_nil() && !world.group_data.get(&active).is_some_and(|g| g.members_complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn allies_are_the_ticked_the_worn_group_and_the_allied_titles() {
        let mut a = Allies {
            active_group: true,
            ..Allies::default()
        };
        a.add_tag("Violet Wolf");
        let titled = [
            (id(10), Some("violet  wolf".to_owned())),
            (id(11), Some("Other Crew".to_owned())),
            (id(12), None),
        ];
        let got = a.gather([id(1)], [id(2), id(3)], titled.clone(), id(99));
        assert_eq!(got, BTreeSet::from([id(1), id(2), id(3), id(10)]));
        a.active_group = false;
        assert_eq!(a.gather([id(1)], [id(2)], titled, id(99)), BTreeSet::from([id(1), id(10)]));
    }

    #[test]
    fn nobody_is_an_ally_while_the_allies_are_off_and_never_yourself() {
        let mut a = Allies::default();
        assert_eq!(a.gather([id(99), id(1), Uuid::nil()], [], [], id(99)), BTreeSet::from([id(1)]));
        a.enabled = false;
        assert!(a.gather([id(1)], [id(2)], [], id(99)).is_empty());
        assert!(a.tags.is_empty(), "turning them off keeps what was declared");
    }

    #[test]
    fn titles_are_cleaned_compared_without_case_and_not_repeated() {
        let mut a = Allies::default();
        assert!(a.add_tag("  Northern \t Wolves "));
        assert_eq!(a.tags, ["Northern Wolves"]);
        assert!(!a.add_tag("northern wolves"));
        assert!(!a.add_tag("   "));
        assert!(a.has_tag("NORTHERN WOLVES"));
        a.remove_tag("northern  wolves");
        assert!(a.tags.is_empty());
        for n in 0..40 {
            a.add_tag(&format!("Crew {n}"));
        }
        assert_eq!(a.tags.len(), MAX_TAGS);
        assert!(clean_tag(&"x".repeat(200)).chars().count() == MAX_TAG_CHARS);
    }

    #[test]
    fn old_settings_without_allies_load_with_the_defaults_and_bad_titles_are_dropped() {
        let a: Allies = serde_json::from_str("{}").expect("empty");
        assert_eq!(a, Allies::default());
        let mut b: Allies = serde_json::from_str(r#"{"tags":["A","a","","B\u0007"],"color":[1,2,3,4]}"#).expect("tags");
        b.sanitize();
        assert_eq!(b.tags, ["A", "B"]);
        assert_eq!(b.color, [1, 2, 3, 4]);
    }
}
