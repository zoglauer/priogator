//! Data model. Every record carries a `Meta` so that the state written by
//! different devices can be merged (last-writer-wins per record).

use jiff::civil::Date;
use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Version information attached to every record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub id: Uuid,
    pub updated_at: Timestamp,
    /// Device that made the last change; breaks ties on equal timestamps.
    pub device: String,
    /// Tombstone: deleted records are kept so deletes propagate to other devices.
    #[serde(default)]
    pub deleted: bool,
}

impl Meta {
    pub fn new(device: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            updated_at: Timestamp::now(),
            device: device.to_owned(),
            deleted: false,
        }
    }

    /// Mark the record as changed by `device` now.
    pub fn touch(&mut self, device: &str) {
        let now = Timestamp::now();
        // Never go backwards, even if the clock was adjusted.
        self.updated_at = if now > self.updated_at {
            now
        } else {
            self.updated_at
                .checked_add(SignedDuration::from_millis(1))
                .unwrap_or(self.updated_at)
        };
        self.device = device.to_owned();
    }

    /// True if `self` is a newer version than `other`.
    pub fn is_newer_than(&self, other: &Meta) -> bool {
        (self.updated_at, &self.device) > (other.updated_at, &other.device)
    }
}

/// Something that carries a `Meta`.
pub trait Versioned {
    fn meta(&self) -> &Meta;
    fn meta_mut(&mut self) -> &mut Meta;
}

/// A column, e.g. "Work" or "House".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub meta: Meta,
    pub name: String,
    pub order: f64,
    /// When the area is active; outside that time it's shown folded.
    /// `None`: always active.
    #[serde(default)]
    pub active: Option<crate::active::ActiveHours>,
    /// Background colour of the area's column; `None`: the standard one.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

/// A colour-coded sub-group within an area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskGroup {
    pub meta: Meta,
    pub area: Uuid,
    pub name: String,
    /// RGB colour.
    pub color: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskItem {
    pub meta: Meta,
    pub area: Uuid,
    pub group: Option<Uuid>,
    pub title: String,
    #[serde(default)]
    pub notes: String,
    /// Tasks with a deadline are pinned on top of their column.
    pub deadline: Option<Date>,
    /// Position in the priority list (smaller = higher priority).
    pub rank: f64,
    /// Shown in the top section regardless of its deadline.
    #[serde(default)]
    pub pinned: bool,
    /// A checklist within the task. Synced as part of the task.
    #[serde(default)]
    pub subtasks: Vec<Subtask>,
    /// Position in the top section once arranged by hand; `None` until then
    /// (such tasks follow the arranged ones, by date).
    #[serde(default)]
    pub top_rank: Option<f64>,
    /// Moves to the top this many days before its deadline; `None` uses the
    /// app-wide setting.
    #[serde(default)]
    pub lead_days: Option<i64>,
    /// Recurring task: when the next occurrence is due.
    #[serde(default)]
    pub repeat: Option<crate::repeat::Repeat>,
    /// Card colour chosen for this task; overrides the colour scheme.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    pub done: bool,
    pub created_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Subtask {
    pub id: Uuid,
    pub title: String,
    pub done: bool,
}

impl Subtask {
    pub fn new(title: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.trim().to_owned(),
            done: false,
        }
    }
}

impl Versioned for Area {
    fn meta(&self) -> &Meta {
        &self.meta
    }

    fn meta_mut(&mut self) -> &mut Meta {
        &mut self.meta
    }
}

impl Versioned for TaskGroup {
    fn meta(&self) -> &Meta {
        &self.meta
    }

    fn meta_mut(&mut self) -> &mut Meta {
        &mut self.meta
    }
}

impl Versioned for TaskItem {
    fn meta(&self) -> &Meta {
        &self.meta
    }

    fn meta_mut(&mut self) -> &mut Meta {
        &mut self.meta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r#"{"id":"00000000-0000-0000-0000-000000000001","updated_at":"2026-01-01T00:00:00Z","device":"mac"}"#;

    #[test]
    fn a_task_from_an_old_version_loads_with_defaults() {
        // Only the fields the first version wrote.
        let json = format!(
            r#"{{"meta":{META},"area":"00000000-0000-0000-0000-0000000000aa","group":null,
            "title":"old","deadline":null,"rank":2.0,"done":false,
            "created_at":"2026-01-01T00:00:00Z","completed_at":null}}"#
        );
        let task: TaskItem = serde_json::from_str(&json).unwrap();
        assert_eq!(task.title, "old");
        assert!(!task.meta.deleted);
        assert!(task.notes.is_empty());
        assert!(!task.pinned);
        assert!(task.subtasks.is_empty());
        assert_eq!(task.top_rank, None);
        assert_eq!(task.lead_days, None);
        assert_eq!(task.repeat, None);
        assert_eq!(task.color, None);
    }

    #[test]
    fn an_area_from_an_old_version_loads_with_defaults() {
        let json = format!(r#"{{"meta":{META},"name":"Work","order":0.0}}"#);
        let area: Area = serde_json::from_str(&json).unwrap();
        assert_eq!(area.name, "Work");
        assert_eq!(area.active, None);
        assert_eq!(area.color, None);
    }

    #[test]
    fn every_field_of_a_task_survives_a_round_trip() {
        let task = TaskItem {
            meta: Meta {
                deleted: true,
                ..Meta::new("mac")
            },
            area: Uuid::new_v4(),
            group: Some(Uuid::new_v4()),
            title: "Ümlauts & \"quotes\" ✓".into(),
            notes: "line 1\nline 2".into(),
            deadline: Some(jiff::civil::date(2026, 12, 24)),
            rank: -3.5,
            pinned: true,
            subtasks: vec![Subtask::new("one"), Subtask::new("two")],
            top_rank: Some(1.0),
            lead_days: Some(0),
            repeat: Some(crate::repeat::Repeat::AfterDone {
                every: 2,
                unit: crate::repeat::Unit::Months,
            }),
            color: Some([1, 2, 3]),
            done: true,
            created_at: Timestamp::now(),
            completed_at: Some(Timestamp::now()),
        };
        let json = serde_json::to_string(&task).unwrap();
        let back: TaskItem = serde_json::from_str(&json).unwrap();
        assert_eq!(back, task);
    }

    #[test]
    fn every_field_of_an_area_and_group_survives_a_round_trip() {
        let area = Area {
            meta: Meta::new("linux"),
            name: "House".into(),
            order: 2.0,
            active: Some(crate::active::ActiveHours::default()),
            color: Some([200, 100, 0]),
        };
        let back: Area = serde_json::from_str(&serde_json::to_string(&area).unwrap()).unwrap();
        assert_eq!(back, area);
        let group = TaskGroup {
            meta: Meta::new("linux"),
            area: area.meta.id,
            name: "Garden".into(),
            color: [1, 2, 3],
        };
        let back: TaskGroup =
            serde_json::from_str(&serde_json::to_string(&group).unwrap()).unwrap();
        assert_eq!(back, group);
    }

    #[test]
    fn touching_never_goes_back_in_time() {
        let mut meta = Meta::new("mac");
        // Last changed "in the future", e.g. on a computer whose clock was ahead.
        let future = Timestamp::now() + SignedDuration::from_hours(1);
        meta.updated_at = future;
        meta.touch("linux");
        assert!(meta.updated_at > future);
        assert_eq!(meta.device, "linux");

        let before = meta.updated_at;
        meta.touch("linux");
        assert!(meta.updated_at > before, "each touch is newer");
    }

    #[test]
    fn equal_times_are_decided_by_device_name() {
        let a = Meta::new("alpha");
        let b = Meta {
            device: "beta".into(),
            ..a.clone()
        };
        assert!(b.is_newer_than(&a));
        assert!(!a.is_newer_than(&b));
        assert!(!a.is_newer_than(&a), "not newer than itself");
    }

    #[test]
    fn subtask_titles_are_trimmed_and_ids_unique() {
        let a = Subtask::new("  buy milk \n");
        let b = Subtask::new("buy milk");
        assert_eq!(a.title, "buy milk");
        assert!(!a.done);
        assert_ne!(a.id, b.id);
    }
}
