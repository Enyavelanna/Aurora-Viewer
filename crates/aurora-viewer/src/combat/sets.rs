//! Region-scoped annotation collections. The active collection is explicit;
//! imports always receive fresh identities and cannot overwrite local work.
use super::tactical::{Layer, MAX_LAYERS, Marker};
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
}

/// The version of the one format a marker set has, in a file and in the
/// parts that travel between residents.
pub(crate) const SET_FORMAT: u8 = 4;
/// The largest file or transfer taken in, in bytes.
const SET_BYTES: usize = 1_000_000;

/// A marker set as written to a file or sent to residents: its name, its
/// layers and its markers, under a format number. Nothing else is read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SetFile {
    pub format: u8,
    pub name: String,
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub markers: Vec<Marker>,
}

impl SetFile {
    pub fn new(name: String, layers: Vec<Layer>, markers: Vec<Marker>) -> Self {
        Self {
            format: SET_FORMAT,
            name,
            layers,
            markers,
        }
    }

    /// Within the bounds a set has: nothing a file or a peer sends can
    /// exceed them.
    pub fn valid(&self) -> bool {
        let layers = self.layers.len().max(1);
        self.format == SET_FORMAT
            && self.name.len() <= 128
            && !self.name.chars().any(char::is_control)
            && self.markers.len() <= 256
            && self.markers.iter().all(|m| m.valid() && usize::from(m.style.layer) < layers)
            && self.layers.len() <= MAX_LAYERS
            && self
                .layers
                .iter()
                .all(|l| l.name.len() <= 96 && !l.name.chars().any(char::is_control))
    }

    /// Compact JSON, for the parts sent to residents.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ()> {
        if !self.valid() {
            return Err(());
        }
        serde_json::to_vec(self).map_err(|_| ())
    }

    /// Readable JSON, for a file.
    pub fn to_file_bytes(&self) -> Result<Vec<u8>, ()> {
        if !self.valid() {
            return Err(());
        }
        serde_json::to_vec_pretty(self).map_err(|_| ())
    }

    /// A set from a file or the parts of a transfer; None when it is not one.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > SET_BYTES {
            return None;
        }
        serde_json::from_slice::<Self>(bytes).ok().filter(Self::valid)
    }
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

    pub fn save_layers(&mut self, layers: &[Layer]) {
        if let Some(set) = self.sets.iter_mut().find(|s| Some(s.id) == self.active) {
            set.layers = layers.to_vec();
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
        self.sets.push(MarkerSet {
            id,
            name,
            markers,
            layers: Layer::defaults(),
        });
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

    #[test]
    fn a_set_file_round_trips_and_nothing_outside_the_format_is_read() {
        let mut cfg = super::super::tactical::Settings::default();
        cfg.place_marker(super::super::tactical::Shape::Pin, [10.0, 20.0, 30.0]);
        cfg.markers[0].name = "Spawn".into();
        let file = SetFile::new("Attack".into(), cfg.layers.clone(), cfg.markers.clone());
        let pretty = file.to_file_bytes().expect("file");
        assert!(pretty.len() > file.to_bytes().expect("compact").len(), "a file is readable JSON");
        assert_eq!(SetFile::from_bytes(&pretty), Some(file.clone()));
        // another format number, a marker on a layer the set does not have, too many markers or bytes
        let mut other = file.clone();
        other.format = 3;
        assert!(
            SetFile::from_bytes(&serde_json::to_vec(&other).expect("json")).is_none(),
            "the old format is not read"
        );
        let mut stray = file.clone();
        stray.markers[0].style.layer = 7;
        assert!(!stray.valid());
        let mut many = file.clone();
        many.markers = (0..257).map(|_| file.markers[0].clone()).collect();
        assert!(!many.valid());
        assert!(SetFile::from_bytes(&vec![b' '; 1_000_001]).is_none());
        assert!(SetFile::from_bytes(b"not json").is_none());
        assert!(
            SetFile::from_bytes(br#"{"format":4,"name":"x"}"#).is_some(),
            "layers and markers may be absent"
        );
    }

    #[test]
    fn importing_a_set_makes_a_new_active_set_with_its_layers_and_new_identities() {
        let mut cfg = super::super::tactical::Settings::default();
        let key = RegionKey {
            grid: "g".into(),
            region: 1,
        };
        cfg.enter_region(key, "R");
        cfg.place_marker(super::super::tactical::Shape::Pin, [1.0, 2.0, 3.0]);
        let layers = vec![
            Layer {
                name: "Mine".into(),
                visible: true,
                locked: true,
            },
            Layer::default(),
        ];
        let theirs = cfg.markers[0].clone();
        let file = SetFile::new("Imported".into(), layers.clone(), vec![theirs.clone()]);
        assert!(cfg.import_set(file));
        assert_eq!(cfg.layers, layers);
        assert_eq!(cfg.markers.len(), 1);
        assert_ne!(cfg.markers[0].id, theirs.id, "an import never overwrites by identity");
        assert_eq!(cfg.active_set_name(), "Imported");
        assert!(cfg.library.regions[0].sets.len() >= 2, "the set that was there is kept");
        let mut none = super::super::tactical::Settings::default();
        assert!(
            !none.import_set(SetFile::new("x".into(), Vec::new(), Vec::new())),
            "no region, nowhere to put it"
        );
    }
}
