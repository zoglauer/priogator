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
