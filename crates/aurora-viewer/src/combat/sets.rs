//! Region-scoped annotation collections. The active collection is explicit;
//! imports always receive fresh identities and cannot overwrite local work.
use super::tactical::Marker;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RegionKey {
    pub grid: String,
    pub region: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct MarkerSet {
    pub id: Uuid,
    pub name: String,
    pub markers: Vec<Marker>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RegionSets {
    pub key: RegionKey,
    pub name: String,
    pub active: Option<Uuid>,
    pub sets: Vec<MarkerSet>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Library {
    pub regions: Vec<RegionSets>,
    pub unassigned: Vec<Marker>,
}

impl Library {
    pub fn region_mut(&mut self, key: &RegionKey, name: &str) -> &mut RegionSets {
        let index = self.regions.iter().position(|r| &r.key == key).unwrap_or_else(|| {
            self.regions.push(RegionSets {
                key: key.clone(),
                name: name.into(),
                active: None,
                sets: Vec::new(),
            });
            self.regions.len() - 1
        });
        &mut self.regions[index]
    }
}

impl RegionSets {
    pub fn selected(&self) -> Option<&MarkerSet> {
        self.sets.iter().find(|s| Some(s.id) == self.active)
    }

    pub fn save(&mut self, markers: &[Marker]) {
        if let Some(set) = self.sets.iter_mut().find(|s| Some(s.id) == self.active) {
            set.markers = markers.to_vec();
        }
    }

    pub fn add(&mut self, name: String, markers: &[Marker]) -> Uuid {
        let id = Uuid::new_v4();
        let markers = markers
            .iter()
            .cloned()
            .map(|mut m| {
                m.id = Uuid::new_v4();
                m.selected = false;
                m
            })
            .collect();
        self.sets.push(MarkerSet { id, name, markers });
        self.active = Some(id);
        id
    }

    pub fn remove_active(&mut self) {
        self.sets.retain(|s| Some(s.id) != self.active);
        self.active = self.sets.first().map(|s| s.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combat_sets_migrate_once_and_restore_by_grid_and_region() {
        let mut cfg = super::super::tactical::Settings::default();
        cfg.add_marker([100.0, 200.0, 30.0]);
        let legacy = cfg.markers.clone();
        let a = RegionKey {
            grid: "grid-a".into(),
            region: 1,
        };
        let b = RegionKey {
            grid: "grid-b".into(),
            region: 1,
        };
        cfg.enter_region(a.clone(), "Region");
        assert_eq!(cfg.library.unassigned, legacy);
        cfg.add_marker([110.0, 210.0, 30.0]);
        let markers = cfg.markers.clone();
        cfg.enter_region(b.clone(), "Region");
        assert!(cfg.markers.is_empty());
        cfg.enter_region(a.clone(), "Region");
        assert_eq!(cfg.markers, markers);
        cfg.save_set();
        let mut loaded: super::super::tactical::Settings =
            serde_json::from_str(&serde_json::to_string(&cfg).expect("encode")).expect("reload");
        loaded.enter_region(a, "Region");
        assert_eq!(loaded.library.unassigned, legacy);
        assert_eq!(loaded.markers, markers);
        let region = loaded.library.region_mut(&b, "");
        let original = region.add("copy".into(), &markers);
        assert_ne!(region.selected().expect("set").markers[0].id, markers[0].id);
        region.remove_active();
        assert_ne!(region.active, Some(original));
    }
}
