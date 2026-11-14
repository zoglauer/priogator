//! The in-memory board: all areas, groups and tasks, the operations that
//! change them, and the merge of state coming from other devices.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

use crate::model::{Area, Meta, Subtask, TaskGroup, TaskItem, Versioned};
use crate::repeat::Repeat;

/// Areas created on first start. Fixed ids, so two devices that both start
/// empty create the *same* areas and merge cleanly instead of duplicating them.
const DEFAULT_AREAS: [(u128, &str); 4] = [
    (0x5052_494f_0000_0000_0000_0000_0000_0001, "Work"),
    (
        0x5052_494f_0000_0000_0000_0000_0000_0002,
        "Private ventures",
    ),
    (0x5052_494f_0000_0000_0000_0000_0000_0003, "House"),
    (0x5052_494f_0000_0000_0000_0000_0000_0004, "Family"),
];

/// Below this gap between neighbouring ranks the list gets renumbered.
const MIN_RANK_GAP: f64 = 1e-6;
/// Upper limit for day counts read from files (lead times, done days):
/// far beyond what the UI allows, well within what dates can do.
const MAX_DAYS: i64 = 36_500;

/// Default for how many days ahead a deadline puts a task in the top section.
pub const DEFAULT_HORIZON_DAYS: i64 = 10;
/// Default for how many days completed tasks stay visible.
pub const DEFAULT_DONE_DAYS: i64 = 7;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Board {
    /// Name of this device, recorded on every change.
    pub device: String,
    /// Today's date, which decides which deadlines count as close.
    pub today: Date,
    /// Tasks due within this many days are in the top section.
    pub horizon_days: i64,
    /// Completed tasks stay visible for this many days.
    pub done_days: i64,
    pub areas: BTreeMap<Uuid, Area>,
    pub groups: BTreeMap<Uuid, TaskGroup>,
    pub tasks: BTreeMap<Uuid, TaskItem>,
}

/// Statistics shown in the sidebar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub open: usize,
    /// The tasks themselves (earliest deadline first), so the toolbar can
    /// say which ones it counts.
    pub overdue: Vec<Uuid>,
    pub due_this_week: Vec<Uuid>,
    /// Completed within the last `Board::done_days` days.
    pub done_recently: usize,
}

/// The user-editable fields of a task.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskDetails {
    pub title: String,
    pub notes: String,
    pub area: Uuid,
    pub group: Option<Uuid>,
    pub deadline: Option<Date>,
    pub pinned: bool,
    pub subtasks: Vec<Subtask>,
    pub lead_days: Option<i64>,
    pub repeat: Option<Repeat>,
    pub color: Option<[u8; 3]>,
}

impl From<&TaskItem> for TaskDetails {
    fn from(t: &TaskItem) -> Self {
        Self {
            title: t.title.clone(),
            notes: t.notes.clone(),
            area: t.area,
            group: t.group,
            deadline: t.deadline,
            pinned: t.pinned,
            subtasks: t.subtasks.clone(),
            lead_days: t.lead_days,
            repeat: t.repeat.clone(),
            color: t.color,
        }
    }
}

/// Colours offered for a single task's card, red to pink.
pub const TASK_COLORS: [[u8; 3]; 8] = [
    [229, 57, 53],
    [251, 140, 0],
    [253, 216, 53],
    [67, 160, 71],
    [0, 172, 193],
    [30, 136, 229],
    [142, 36, 170],
    [216, 27, 96],
];

/// Colours offered for new groups, in order.
pub const GROUP_COLORS: [[u8; 3]; 8] = [
    [66, 133, 244],
    [52, 168, 83],
    [251, 140, 0],
    [171, 71, 188],
    [0, 172, 193],
    [229, 57, 53],
    [124, 179, 66],
    [141, 110, 99],
];

impl Board {
    pub fn new(device: &str) -> Self {
        Self {
            device: device.to_owned(),
            today: jiff::Zoned::now().date(),
            horizon_days: DEFAULT_HORIZON_DAYS,
            done_days: DEFAULT_DONE_DAYS,
            ..Default::default()
        }
    }

    pub fn add_default_areas(&mut self) {
        for (i, (id, name)) in DEFAULT_AREAS.iter().enumerate() {
            let mut meta = Meta::new(&self.device);
            meta.id = Uuid::from_u128(*id);
            // A fixed old timestamp, so any real rename wins the merge.
            meta.updated_at = Timestamp::UNIX_EPOCH;
            meta.device = String::new();
            self.areas.entry(meta.id).or_insert(Area {
                meta,
                name: (*name).to_owned(),
                order: i as f64,
                active: None,
                color: None,
            });
        }
    }

    // ----- Merge ---------------------------------------------------------

    /// Merge another device's state into this one. Returns true if anything changed.
    pub fn merge(
        &mut self,
        areas: Vec<Area>,
        groups: Vec<TaskGroup>,
        tasks: Vec<TaskItem>,
    ) -> bool {
        let a = merge_records(&mut self.areas, areas);
        let g = merge_records(&mut self.groups, groups);
        let t = merge_records(&mut self.tasks, tasks);
        self.follow_moved_groups();
        self.keep_completions();
        a | g | t
    }

    /// A recurring task done on one computer and edited (or skipped) on the
    /// other before they synced: the edit is newer, so the task arrives open
    /// again, while its next occurrence (made by completing it) is there too.
    /// Both would show. The completion stands; the edit is kept as well.
    /// (Reopening deletes an open next occurrence, so this can't be meant.)
    fn keep_completions(&mut self) {
        let redone: Vec<(Uuid, Timestamp)> = self
            .tasks
            .values()
            .filter(|t| !t.done && !t.meta.deleted && t.repeat.is_some() && !t.reopened)
            .filter_map(|t| {
                let next = self.tasks.get(&successor_id(t.meta.id))?;
                (!next.done && !next.meta.deleted).then_some((t.meta.id, next.created_at))
            })
            .collect();
        for (id, completed_at) in redone {
            self.update_task(id, |t| {
                t.done = true;
                t.completed_at = Some(completed_at);
            });
        }
    }

    /// A task's group must be in the task's area. After two computers
    /// changed things at the same time it may not be, e.g. one added a task
    /// to a group while the other moved the group elsewhere. Then the task
    /// follows its group, as if it had been there for the move; whichever
    /// order the changes arrive in, the result is the same.
    fn follow_moved_groups(&mut self) {
        let moves: Vec<(Uuid, Uuid)> = self
            .tasks
            .values()
            .filter(|t| !t.meta.deleted)
            .filter_map(|t| {
                let group = self.group(t.group?)?;
                let to = group.area;
                (to != t.area && self.areas.contains_key(&to)).then_some((t.meta.id, to))
            })
            .collect();
        for (id, area) in moves {
            self.update_task(id, |t| {
                t.area = area;
                t.top_rank = None;
            });
        }
    }

    /// Undo: bring every record back to its state in `snapshot`.
    /// Restored records count as new changes, so they also win on the
    /// other device; records created since the snapshot are deleted.
    pub fn restore(&mut self, snapshot: &Board) {
        let device = self.device.clone();
        restore_records(&mut self.areas, &snapshot.areas, &device);
        restore_records(&mut self.groups, &snapshot.groups, &device);
        restore_records(&mut self.tasks, &snapshot.tasks, &device);
    }

    // ----- Queries -------------------------------------------------------

    pub fn areas_sorted(&self) -> Vec<&Area> {
        // A deleted column stays visible if the other device added tasks to
        // it in the meantime, so those tasks don't vanish.
        let mut areas: Vec<_> = self
            .areas
            .values()
            .filter(|a| !a.meta.deleted || self.open_tasks(a.meta.id).next().is_some())
            .collect();
        areas.sort_by(|a, b| a.order.total_cmp(&b.order).then(a.meta.id.cmp(&b.meta.id)));
        areas
    }

    pub fn groups_of(&self, area: Uuid) -> Vec<&TaskGroup> {
        let mut groups: Vec<_> = self
            .groups
            .values()
            .filter(|g| g.area == area && !g.meta.deleted)
            .collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        groups
    }

    fn open_tasks(&self, area: Uuid) -> impl Iterator<Item = &TaskItem> {
        self.tasks
            .values()
            .filter(move |t| t.area == area && !t.done && !t.meta.deleted)
    }

    /// Whether the task's deadline is close enough to put it on top: within
    /// its own lead time, or the app-wide one. Overdue counts too.
    fn due_soon(&self, task: &TaskItem) -> bool {
        // (Limited: a damaged file's huge value mustn't panic at every frame.)
        let lead = task
            .lead_days
            .unwrap_or(self.horizon_days)
            .clamp(0, MAX_DAYS);
        task.deadline
            .is_some_and(|d| d <= self.today.saturating_add(jiff::Span::new().days(lead)))
    }

    /// Whether a task belongs in the top section: pinned, or due soon.
    pub fn in_top(&self, task: &TaskItem) -> bool {
        task.pinned || self.due_soon(task)
    }

    /// Open and past its deadline.
    pub fn overdue(&self, task: &TaskItem) -> bool {
        !task.done && task.deadline.is_some_and(|d| d < self.today)
    }

    /// Open and due today or earlier: shown in bold.
    pub fn urgent(&self, task: &TaskItem) -> bool {
        !task.done && task.deadline.is_some_and(|d| d <= self.today)
    }

    /// The top section: tasks arranged by hand first, in that order; then
    /// the others with deadlines earliest first, then pinned ones without.
    pub fn top_list(&self, area: Uuid) -> Vec<&TaskItem> {
        let mut list: Vec<_> = self.open_tasks(area).filter(|t| self.in_top(t)).collect();
        list.sort_by(|a, b| {
            let arranged = |t: &TaskItem| t.top_rank.unwrap_or(f64::INFINITY);
            arranged(a)
                .total_cmp(&arranged(b))
                // `None` sorts first in `Option`, so compare "no deadline" as last.
                .then((a.deadline.is_none(), a.deadline).cmp(&(b.deadline.is_none(), b.deadline)))
                .then(a.rank.total_cmp(&b.rank))
                .then(a.meta.id.cmp(&b.meta.id))
        });
        list
    }

    /// The priority list: all other open tasks, in priority order.
    pub fn priority_list(&self, area: Uuid) -> Vec<&TaskItem> {
        let mut list: Vec<_> = self.open_tasks(area).filter(|t| !self.in_top(t)).collect();
        list.sort_by(|a, b| a.rank.total_cmp(&b.rank).then(a.meta.id.cmp(&b.meta.id)));
        list
    }

    /// Tasks completed after this count as recently done; older ones are hidden.
    fn done_since(&self) -> Timestamp {
        Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_hours(
                24 * self.done_days.clamp(0, MAX_DAYS),
            ))
            .unwrap_or(Timestamp::UNIX_EPOCH)
    }

    /// Tasks completed within the last `done_days` days, most recent first.
    pub fn done_list(&self, area: Uuid) -> Vec<&TaskItem> {
        let since = self.done_since();
        let mut list: Vec<_> = self
            .tasks
            .values()
            .filter(|t| t.area == area && t.done && !t.meta.deleted)
            .filter(|t| t.completed_at.is_some_and(|c| c >= since))
            .collect();
        list.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
        list
    }

    /// Every completed task, however long ago, most recent first: the log of
    /// what was achieved. Only live areas, or just `area`.
    pub fn achieved(&self, area: Option<Uuid>) -> Vec<&TaskItem> {
        let live_areas: Vec<Uuid> = self.areas_sorted().iter().map(|a| a.meta.id).collect();
        let mut list: Vec<_> = self
            .tasks
            .values()
            .filter(|t| t.done && !t.meta.deleted && t.completed_at.is_some())
            .filter(|t| area.map_or(live_areas.contains(&t.area), |a| t.area == a))
            .collect();
        list.sort_by_key(|t| std::cmp::Reverse(t.completed_at));
        list
    }

    /// The group, unless it has been deleted.
    pub fn group(&self, id: Uuid) -> Option<&TaskGroup> {
        self.groups.get(&id).filter(|g| !g.meta.deleted)
    }

    pub fn stats(&self, today: Date) -> Stats {
        let week_end = today.saturating_add(jiff::Span::new().days(7));
        let done_since = self.done_since();
        let live_areas: Vec<Uuid> = self.areas_sorted().iter().map(|a| a.meta.id).collect();
        let mut stats = Stats::default();
        for t in self.tasks.values() {
            if t.meta.deleted || !live_areas.contains(&t.area) {
                continue;
            }
            if t.done {
                if t.completed_at.is_some_and(|c| c >= done_since) {
                    stats.done_recently += 1;
                }
                continue;
            }
            stats.open += 1;
            match t.deadline {
                Some(d) if d < today => stats.overdue.push(t.meta.id),
                Some(d) if d <= week_end => stats.due_this_week.push(t.meta.id),
                _ => {}
            }
        }
        for list in [&mut stats.overdue, &mut stats.due_this_week] {
            list.sort_by_key(|id| (self.tasks[id].deadline, self.tasks[id].title.clone()));
        }
        stats
    }

    // ----- Changes -------------------------------------------------------

    /// Add a task at the bottom of its area's priority list.
    pub fn add_task(
        &mut self,
        area: Uuid,
        title: &str,
        group: Option<Uuid>,
        deadline: Option<Date>,
    ) -> Uuid {
        self.revive_area(area);
        let rank = self.bottom_rank(area, None);
        let meta = Meta::new(&self.device);
        let id = meta.id;
        self.tasks.insert(
            id,
            TaskItem {
                meta,
                area,
                group,
                title: title.trim().to_owned(),
                notes: String::new(),
                deadline,
                rank,
                pinned: false,
                subtasks: Vec::new(),
                top_rank: None,
                lead_days: None,
                repeat: None,
                color: None,
                repeat_day: None,
                reopened: false,
                done: false,
                created_at: Timestamp::now(),
                completed_at: None,
            },
        );
        id
    }

    /// Change a task and record the change.
    pub fn update_task(&mut self, id: Uuid, change: impl FnOnce(&mut TaskItem)) {
        if let Some(task) = self.tasks.get_mut(&id) {
            change(task);
            task.meta.touch(&self.device);
        }
    }

    /// Complete or reopen a task. Completing a recurring task also creates
    /// its next occurrence; reopening it removes that again.
    /// Reopening keeps a next occurrence that was edited meanwhile (a note,
    /// another date, a ticked subtask): then both are open.
    pub fn set_done(&mut self, id: Uuid, done: bool) {
        self.update_task(id, |t| {
            t.done = done;
            t.completed_at = done.then(Timestamp::now);
            t.reopened = false;
        });
        let Some(task) = self.tasks.get(&id) else {
            return;
        };
        if task.repeat.is_none() {
            return;
        }
        let next_id = successor_id(id);
        if done {
            self.create_successor(id, next_id);
            return;
        }
        let Some(next) = self
            .tasks
            .get(&next_id)
            .filter(|t| !t.done && !t.meta.deleted)
        else {
            return;
        };
        // Any change gets a new stamp; created and stamped together otherwise.
        let edited = next.meta.updated_at
            > next
                .created_at
                .checked_add(jiff::SignedDuration::from_secs(1))
                .unwrap_or(next.created_at);
        if edited {
            self.update_task(id, |t| t.reopened = true);
        } else {
            self.delete_task(next_id);
        }
    }

    /// The next occurrence of recurring task `id`, in its place: same area,
    /// group, notes, position and pinning; subtasks unticked; new due date.
    fn create_successor(&mut self, id: Uuid, next_id: Uuid) {
        // Still there (maybe done or edited since): keep it as it is.
        if self.tasks.get(&next_id).is_some_and(|t| !t.meta.deleted) {
            return;
        }
        let task = &self.tasks[&id];
        let Some(repeat) = &task.repeat else {
            return;
        };
        let due = task.deadline.unwrap_or(self.today);
        let day = task.repeat_day.unwrap_or(due.day());
        let mut next = task.clone();
        next.deadline = Some(repeat.next_on_day(due, self.today, day));
        next.repeat_day = Some(day);
        next.done = false;
        next.completed_at = None;
        next.reopened = false;
        next.created_at = Timestamp::now();
        for subtask in &mut next.subtasks {
            subtask.done = false;
        }
        next.meta = match self.tasks.get(&next_id) {
            // Re-done after a reopen: the same record, as a newer change.
            Some(previous) => previous.meta.clone(),
            None => Meta::new(&self.device),
        };
        next.meta.id = next_id;
        next.meta.deleted = false;
        next.meta.touch(&self.device);
        self.tasks.insert(next_id, next);
    }

    /// Move a recurring task on to its next date without counting it as done.
    pub fn skip(&mut self, id: Uuid) {
        let today = self.today;
        self.update_task(id, |t| {
            if let Some(repeat) = &t.repeat {
                let due = t.deadline.unwrap_or(today);
                let day = t.repeat_day.unwrap_or(due.day());
                t.deadline = Some(repeat.next_on_day(due, today, day));
                t.repeat_day = Some(day);
            }
        });
    }

    pub fn add_subtask(&mut self, task: Uuid, title: &str) -> Uuid {
        let subtask = Subtask::new(title);
        let id = subtask.id;
        self.update_task(task, |t| t.subtasks.push(subtask));
        id
    }

    pub fn set_subtask_done(&mut self, task: Uuid, subtask: Uuid, done: bool) {
        self.update_task(task, |t| {
            if let Some(s) = t.subtasks.iter_mut().find(|s| s.id == subtask) {
                s.done = done;
            }
        });
    }

    pub fn delete_task(&mut self, id: Uuid) {
        self.update_task(id, |t| t.meta.deleted = true);
    }

    /// A rank below everything in `area`'s priority list (ignoring `except`).
    fn bottom_rank(&self, area: Uuid, except: Option<Uuid>) -> f64 {
        self.priority_list(area)
            .iter()
            .rfind(|t| Some(t.meta.id) != except)
            .map_or(0.0, |t| t.rank + 1.0)
    }

    /// Whether the task's group may stay when it moves to `area`.
    fn keeps_group(&self, id: Uuid, area: Uuid) -> bool {
        self.tasks
            .get(&id)
            .and_then(|t| t.group)
            .and_then(|g| self.group(g))
            .is_some_and(|g| g.area == area)
    }

    /// Put a task into `area`'s top section.
    pub fn pin(&mut self, id: Uuid, area: Uuid) {
        let keep_group = self.keeps_group(id, area);
        self.update_task(id, |t| {
            t.area = area;
            t.pinned = true;
            // A position from an earlier time on top doesn't apply any more.
            t.top_rank = None;
            if !keep_group {
                t.group = None;
            }
        });
    }

    /// Move a task into `area`'s top section at position `index` (0 = first),
    /// pinning it unless it's there anyway because it's due soon. Fixes the
    /// current order of the section, so nothing else moves.
    pub fn move_in_top(&mut self, id: Uuid, area: Uuid, index: usize) {
        self.revive_area(area);
        let Some(task) = self.tasks.get(&id) else {
            return;
        };
        let due_soon = self.due_soon(task);
        let keep_group = self.keeps_group(id, area);
        let mut order: Vec<Uuid> = self
            .top_list(area)
            .iter()
            .map(|t| t.meta.id)
            .filter(|t| *t != id)
            .collect();
        order.insert(index.min(order.len()), id);
        self.update_task(id, |t| {
            t.area = area;
            t.pinned |= !due_soon;
            if !keep_group {
                t.group = None;
            }
        });
        for (i, task) in order.into_iter().enumerate() {
            if self.tasks[&task].top_rank != Some(i as f64) {
                self.update_task(task, |t| t.top_rank = Some(i as f64));
            }
        }
    }

    /// Put a task first in its area's top section, before those arranged by
    /// hand and those sorted by deadline (overdue ones too). The others keep
    /// their places. For a new task due today.
    pub fn put_first_on_top(&mut self, id: Uuid) {
        let Some(area) = self.tasks.get(&id).map(|t| t.area) else {
            return;
        };
        let first = self
            .top_list(area)
            .iter()
            .filter(|t| t.meta.id != id)
            .filter_map(|t| t.top_rank)
            .fold(0.0, f64::min);
        self.update_task(id, |t| t.top_rank = Some(first - 1.0));
    }

    /// Move a task into `area`'s priority list at position `index` (0 = top).
    /// Unpins it; a deadline close enough to keep it in the top section is
    /// removed, farther ones stay.
    pub fn move_task(&mut self, id: Uuid, area: Uuid, index: usize) {
        self.revive_area(area);
        let (mut order, others): (Vec<Uuid>, Vec<f64>) = self
            .priority_list(area)
            .into_iter()
            .filter(|t| t.meta.id != id)
            .map(|t| (t.meta.id, t.rank))
            .unzip();
        let index = index.min(others.len());
        order.insert(index, id);
        let prev = index.checked_sub(1).map(|i| others[i]);
        let next = others.get(index).copied();
        let rank = match (prev, next) {
            (None, None) => 0.0,
            (None, Some(next)) => next - 1.0,
            (Some(prev), None) => prev + 1.0,
            (Some(prev), Some(next)) => (prev + next) / 2.0,
        };
        let keep_group = self.keeps_group(id, area);
        let due_soon = self.tasks.get(&id).is_some_and(|t| self.due_soon(t));
        self.update_task(id, |t| {
            t.area = area;
            t.pinned = false;
            t.top_rank = None;
            // (A recurring task keeps its date; it needs it for the next one.)
            if due_soon && t.repeat.is_none() {
                t.deadline = None;
            }
            t.rank = rank;
            if !keep_group {
                t.group = None;
            }
        });
        let too_close = |other: Option<f64>| other.is_some_and(|o| (o - rank).abs() < MIN_RANK_GAP);
        if too_close(prev) || too_close(next) {
            // E.g. between two equal ranks (added on two devices): the
            // requested order decides, not the order the list sorts them in.
            self.renumber(order);
        }
    }

    /// Save edited details. A task that newly lands in a priority list
    /// (unpinned, deadline removed or moved further out, or moved to another
    /// column) goes to its bottom.
    pub fn update_details(&mut self, id: Uuid, details: TaskDetails) {
        if !self.tasks.contains_key(&id) {
            return;
        }
        self.revive_area(details.area);
        let task = &self.tasks[&id];
        let was_in_list = !self.in_top(task) && task.area == details.area;
        let group = details
            .group
            .filter(|g| self.group(*g).is_some_and(|g| g.area == details.area));
        self.update_task(id, |t| {
            // Deleted on another computer while its dialog was open here:
            // saving is the newer change and brings it back.
            t.meta.deleted = false;
            // A position from an earlier time on top doesn't apply any more.
            if t.pinned != details.pinned || t.area != details.area {
                t.top_rank = None;
            }
            t.title = details.title.trim().to_owned();
            t.notes = details.notes;
            t.area = details.area;
            t.group = group;
            // A deadline set by hand starts the series anew on its day.
            if t.deadline != details.deadline {
                t.repeat_day = None;
            }
            t.deadline = details.deadline;
            t.pinned = details.pinned;
            t.lead_days = details.lead_days;
            t.repeat = details.repeat;
            t.color = details.color;
            t.subtasks = details
                .subtasks
                .into_iter()
                .filter(|s| !s.title.trim().is_empty())
                .map(|s| Subtask {
                    title: s.title.trim().to_owned(),
                    ..s
                })
                .collect();
        });
        let now_in_list = !self.in_top(&self.tasks[&id]);
        if now_in_list && !was_in_list {
            let rank = self.bottom_rank(details.area, Some(id));
            self.update_task(id, |t| t.rank = rank);
        }
    }

    // ----- Areas (columns) -----------------------------------------------

    pub fn add_area(&mut self, name: &str) -> Uuid {
        let order = self.areas_sorted().last().map_or(0.0, |a| a.order + 1.0);
        let meta = Meta::new(&self.device);
        let id = meta.id;
        self.areas.insert(
            id,
            Area {
                meta,
                name: name.trim().to_owned(),
                order,
                active: None,
                color: None,
            },
        );
        id
    }

    /// Change an area and record the change. Changing one that was deleted
    /// on another computer (it still shows while it has open tasks) brings
    /// it back for good, like saving a task deleted elsewhere does.
    fn update_area(&mut self, id: Uuid, change: impl FnOnce(&mut Area)) {
        if let Some(area) = self.areas.get_mut(&id) {
            area.meta.deleted = false;
            change(area);
            area.meta.touch(&self.device);
        }
    }

    /// Tasks put into a deleted area (shown because of its open tasks) bring
    /// it back, so it doesn't vanish again once those are done.
    fn revive_area(&mut self, id: Uuid) {
        if self.areas.get(&id).is_some_and(|a| a.meta.deleted) {
            self.update_area(id, |_| {});
        }
    }

    pub fn rename_area(&mut self, id: Uuid, name: &str) {
        self.update_area(id, |a| a.name = name.trim().to_owned());
    }

    pub fn set_active_hours(&mut self, id: Uuid, active: Option<crate::active::ActiveHours>) {
        if self.areas.get(&id).is_some_and(|a| a.active != active) {
            self.update_area(id, |a| a.active = active);
        }
    }

    pub fn set_area_color(&mut self, id: Uuid, color: Option<[u8; 3]>) {
        if self.areas.get(&id).is_some_and(|a| a.color != color) {
            self.update_area(id, |a| a.color = color);
        }
    }

    /// What deleting a column would take with it: (open tasks, done tasks, groups).
    pub fn area_contents(&self, id: Uuid) -> (usize, usize, usize) {
        let open = self.open_tasks(id).count();
        let done = self.done_list(id).len();
        (open, done, self.groups_of(id).len())
    }

    /// Delete a column together with its tasks and groups.
    pub fn delete_area(&mut self, id: Uuid) {
        let tasks: Vec<Uuid> = self
            .tasks
            .values()
            .filter(|t| t.area == id && !t.meta.deleted)
            .map(|t| t.meta.id)
            .collect();
        for task in tasks {
            self.delete_task(task);
        }
        let groups: Vec<Uuid> = self.groups_of(id).iter().map(|g| g.meta.id).collect();
        for group in groups {
            self.delete_group(group);
        }
        self.update_area(id, |a| a.meta.deleted = true);
    }

    /// Move a column `delta` places to the right (negative: left).
    pub fn shift_area(&mut self, id: Uuid, delta: isize) {
        let ids: Vec<Uuid> = self.areas_sorted().iter().map(|a| a.meta.id).collect();
        if let Some(from) = ids.iter().position(|a| *a == id) {
            self.move_area(id, from.saturating_add_signed(delta));
        }
    }

    /// Move a column to position `index` (0 = leftmost), counted among the
    /// other columns.
    pub fn move_area(&mut self, id: Uuid, index: usize) {
        let mut ids: Vec<Uuid> = self.areas_sorted().iter().map(|a| a.meta.id).collect();
        if !ids.contains(&id) {
            return;
        }
        ids.retain(|a| *a != id);
        ids.insert(index.min(ids.len()), id);
        for (i, id) in ids.into_iter().enumerate() {
            if self.areas[&id].order != i as f64 {
                self.update_area(id, |a| a.order = i as f64);
            }
        }
    }

    // ----- Groups --------------------------------------------------------

    pub fn add_group(&mut self, area: Uuid, name: &str, color: [u8; 3]) -> Uuid {
        let meta = Meta::new(&self.device);
        let id = meta.id;
        self.groups.insert(
            id,
            TaskGroup {
                meta,
                area,
                name: name.trim().to_owned(),
                color,
            },
        );
        id
    }

    /// (Like areas: saving a group deleted elsewhere meanwhile brings it back.)
    pub fn update_group(&mut self, id: Uuid, name: &str, color: [u8; 3]) {
        if let Some(group) = self.groups.get_mut(&id) {
            group.meta.deleted = false;
            group.name = name.trim().to_owned();
            group.color = color;
            group.meta.touch(&self.device);
        }
    }

    /// Move a group with all its tasks to another area. Its open tasks go to
    /// the bottom of that area's priority list in their current order (those
    /// due soon or pinned show in its top section); done ones move along.
    pub fn move_group(&mut self, group: Uuid, area: Uuid) {
        let Some(from) = self.group(group).map(|g| g.area) else {
            return;
        };
        if from == area || !self.areas.contains_key(&area) {
            return;
        }
        let in_list: Vec<Uuid> = self
            .priority_list(from)
            .iter()
            .filter(|t| t.group == Some(group))
            .map(|t| t.meta.id)
            .collect();
        let others: Vec<Uuid> = self
            .tasks
            .values()
            .filter(|t| t.group == Some(group) && t.area == from && !t.meta.deleted)
            .map(|t| t.meta.id)
            .filter(|id| !in_list.contains(id))
            .collect();

        if let Some(g) = self.groups.get_mut(&group) {
            g.area = area;
            g.meta.touch(&self.device);
        }
        let mut rank = self.bottom_rank(area, None);
        for id in in_list {
            self.update_task(id, |t| {
                t.area = area;
                t.rank = rank;
                t.top_rank = None;
            });
            rank += 1.0;
        }
        for id in others {
            self.update_task(id, |t| {
                t.area = area;
                t.top_rank = None;
            });
        }
    }

    /// Delete a group; its tasks stay, without a group.
    pub fn delete_group(&mut self, id: Uuid) {
        if let Some(group) = self.groups.get_mut(&id) {
            group.meta.deleted = true;
            group.meta.touch(&self.device);
        }
    }

    /// A colour for the next new group in `area`, avoiding ones in use.
    pub fn next_group_color(&self, area: Uuid) -> [u8; 3] {
        let used: Vec<[u8; 3]> = self.groups_of(area).iter().map(|g| g.color).collect();
        GROUP_COLORS
            .into_iter()
            .find(|c| !used.contains(c))
            .unwrap_or(GROUP_COLORS[used.len() % GROUP_COLORS.len()])
    }

    /// Give the priority list of `area` evenly spaced ranks 0, 1, 2, …
    /// Ranks 0, 1, 2… in the given order.
    fn renumber(&mut self, ids: Vec<Uuid>) {
        for (i, id) in ids.into_iter().enumerate() {
            if self.tasks[&id].rank != i as f64 {
                self.update_task(id, |t| t.rank = i as f64);
            }
        }
    }
}

fn restore_records<T: Versioned + Clone + PartialEq>(
    current: &mut BTreeMap<Uuid, T>,
    snapshot: &BTreeMap<Uuid, T>,
    device: &str,
) {
    for (id, old) in snapshot {
        let Some(now) = current.get(id) else {
            // Only happens when restoring a backup: a record this board never had.
            let mut record = old.clone();
            record.meta_mut().touch(device);
            current.insert(*id, record);
            continue;
        };
        // Only records whose content (or deletion) differs; the version stamp
        // alone isn't a change worth syncing.
        let mut unstamped = now.clone();
        *unstamped.meta_mut() = crate::model::Meta {
            deleted: now.meta().deleted,
            ..old.meta().clone()
        };
        if unstamped != *old {
            let mut record = old.clone();
            // Continue from the newest timestamp, so the restore is the latest change.
            record.meta_mut().updated_at = now.meta().updated_at.max(old.meta().updated_at);
            record.meta_mut().touch(device);
            current.insert(*id, record);
        }
    }
    for (id, record) in current.iter_mut() {
        if !snapshot.contains_key(id) && !record.meta().deleted {
            record.meta_mut().deleted = true;
            record.meta_mut().touch(device);
        }
    }
}

/// The id of a recurring task's next occurrence, derived from its own, so two
/// devices completing the same task create the same next one, not two.
fn successor_id(id: Uuid) -> Uuid {
    Uuid::new_v5(&id, b"next occurrence")
}

/// Last-writer-wins merge of `incoming` into `existing`.
fn merge_records<T: Versioned + Clone>(existing: &mut BTreeMap<Uuid, T>, incoming: Vec<T>) -> bool {
    let mut changed = false;
    for record in incoming {
        let id = record.meta().id;
        let take = existing
            .get(&id)
            .is_none_or(|current| record.meta().is_newer_than(current.meta()));
        if take {
            existing.insert(id, record);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    use jiff::civil::date;

    /// Tests run "on" this day, with the default 10-day horizon.
    const TODAY: Date = date(2026, 10, 29);

    fn board_with_area(device: &str) -> (Board, Uuid) {
        let mut board = Board::new(device);
        board.today = TODAY;
        board.add_default_areas();
        let area = board.areas_sorted()[0].meta.id;
        (board, area)
    }

    fn titles(board: &Board, area: Uuid) -> Vec<&str> {
        board
            .priority_list(area)
            .iter()
            .map(|t| t.title.as_str())
            .collect()
    }

    /// Simulate syncing: `to` reads everything `from` has written.
    fn sync(from: &Board, to: &mut Board) -> bool {
        to.merge(
            from.areas.values().cloned().collect(),
            from.groups.values().cloned().collect(),
            from.tasks.values().cloned().collect(),
        )
    }

    #[test]
    fn default_areas_do_not_duplicate_across_devices() {
        let (mut mac, _) = board_with_area("mac");
        let (linux, _) = board_with_area("linux");
        sync(&linux, &mut mac);
        assert_eq!(mac.areas_sorted().len(), DEFAULT_AREAS.len());
    }

    #[test]
    fn new_tasks_go_to_the_bottom() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "a", None, None);
        b.add_task(area, "b", None, None);
        assert_eq!(titles(&b, area), ["a", "b"]);
    }

    fn top_titles(board: &Board, area: Uuid) -> Vec<&str> {
        board
            .top_list(area)
            .iter()
            .map(|t| t.title.as_str())
            .collect()
    }

    #[test]
    fn close_deadlines_are_on_top_sorted_by_date() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "in 8 days", None, Some(date(2026, 11, 6)));
        b.add_task(area, "plain", None, None);
        b.add_task(area, "in 3 days", None, Some(date(2026, 11, 1)));
        b.add_task(area, "overdue", None, Some(date(2026, 10, 1)));
        assert_eq!(top_titles(&b, area), ["overdue", "in 3 days", "in 8 days"]);
        assert_eq!(titles(&b, area), ["plain"]);
    }

    #[test]
    fn far_deadlines_stay_in_the_list_until_they_come_close() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "plain", None, None);
        b.add_task(area, "in 20 days", None, Some(date(2026, 11, 18)));
        assert_eq!(titles(&b, area), ["plain", "in 20 days"]);
        assert!(b.top_list(area).is_empty());

        b.today = date(2026, 11, 8); // now 10 days away
        assert_eq!(top_titles(&b, area), ["in 20 days"]);

        b.horizon_days = 5;
        assert!(b.top_list(area).is_empty());
    }

    #[test]
    fn pinned_tasks_are_on_top_after_deadlines() {
        let (mut b, area) = board_with_area("mac");
        let a = b.add_task(area, "pinned a", None, None);
        let far = b.add_task(area, "pinned, far deadline", None, Some(date(2027, 3, 1)));
        b.add_task(area, "due soon", None, Some(date(2026, 11, 2)));
        b.add_task(area, "plain", None, None);
        b.pin(a, area);
        b.pin(far, area);
        assert_eq!(
            top_titles(&b, area),
            ["due soon", "pinned, far deadline", "pinned a"]
        );
        assert_eq!(titles(&b, area), ["plain"]);

        // Dragging back into the list unpins and keeps the far deadline.
        b.move_task(far, area, 0);
        assert_eq!(titles(&b, area), ["pinned, far deadline", "plain"]);
        assert_eq!(b.tasks[&far].deadline, Some(date(2027, 3, 1)));
    }

    #[test]
    fn reordering_a_far_deadline_task_keeps_its_deadline() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "a", None, None);
        let t = b.add_task(area, "far", None, Some(date(2026, 12, 24)));
        b.move_task(t, area, 0);
        assert_eq!(titles(&b, area), ["far", "a"]);
        assert_eq!(b.tasks[&t].deadline, Some(date(2026, 12, 24)));
    }

    #[test]
    fn unpinning_in_the_editor_puts_task_at_bottom_of_list() {
        let (mut b, area) = board_with_area("mac");
        let t = b.add_task(area, "t", None, None);
        b.add_task(area, "b", None, None);
        b.pin(t, area);
        let mut details = TaskDetails::from(&b.tasks[&t]);
        details.pinned = false;
        b.update_details(t, details);
        assert_eq!(titles(&b, area), ["b", "t"]);
    }

    #[test]
    fn move_task_reorders() {
        let (mut b, area) = board_with_area("mac");
        let a = b.add_task(area, "a", None, None);
        b.add_task(area, "b", None, None);
        let c = b.add_task(area, "c", None, None);
        b.move_task(c, area, 0);
        assert_eq!(titles(&b, area), ["c", "a", "b"]);
        b.move_task(a, area, 3);
        assert_eq!(titles(&b, area), ["c", "b", "a"]);
        b.move_task(a, area, 1);
        assert_eq!(titles(&b, area), ["c", "a", "b"]);
    }

    #[test]
    fn moving_between_equal_ranks_lands_where_dropped() {
        // Two devices added tasks independently: same rank. Equal ranks sort
        // by id, and these ids put "t" last, wherever it's dropped.
        let (mut b, area) = board_with_area("mac");
        let mut add = |title: &str, id: u128| {
            let random = b.add_task(area, title, None, None);
            let mut task = b.tasks.remove(&random).unwrap();
            task.meta.id = Uuid::from_u128(id);
            task.rank = 0.0;
            b.tasks.insert(task.meta.id, task);
            Uuid::from_u128(id)
        };
        add("a", 1);
        add("c", 2);
        let t = add("t", 3);
        assert_eq!(titles(&b, area), ["a", "c", "t"]);
        b.move_task(t, area, 1);
        assert_eq!(titles(&b, area), ["a", "t", "c"]);
        b.move_task(t, area, 0);
        assert_eq!(titles(&b, area), ["t", "a", "c"]);
    }

    #[test]
    fn move_task_to_other_area_clears_deadline() {
        let (mut b, area) = board_with_area("mac");
        let other = b.areas_sorted()[1].meta.id;
        let t = b.add_task(area, "t", None, Some(date(2026, 11, 1)));
        b.move_task(t, other, 0);
        assert_eq!(titles(&b, other), ["t"]);
        assert!(b.top_list(area).is_empty());
    }

    #[test]
    fn many_moves_to_same_spot_keep_order_stable() {
        let (mut b, area) = board_with_area("mac");
        let ids: Vec<_> = (0..5)
            .map(|i| b.add_task(area, &i.to_string(), None, None))
            .collect();
        // Repeatedly squeeze tasks between the first two, exhausting the rank gap.
        for _ in 0..30 {
            b.move_task(ids[4], area, 1);
            b.move_task(ids[3], area, 1);
        }
        assert_eq!(titles(&b, area).len(), 5);
        assert_eq!(titles(&b, area)[0], "0");
    }

    #[test]
    fn edits_on_both_devices_are_merged() {
        let (mut mac, area) = board_with_area("mac");
        let t1 = mac.add_task(area, "from mac", None, None);
        let mut linux = Board::new("linux");
        sync(&mac, &mut linux);

        linux.add_task(area, "from linux", None, None);
        mac.update_task(t1, |t| t.title = "renamed on mac".into());

        sync(&linux, &mut mac);
        sync(&mac, &mut linux);
        assert_eq!(mac.tasks, linux.tasks);
        assert_eq!(titles(&mac, area), ["renamed on mac", "from linux"]);
    }

    #[test]
    fn newer_change_wins() {
        let (mut mac, area) = board_with_area("mac");
        let t = mac.add_task(area, "original", None, None);
        let mut linux = Board::new("linux");
        sync(&mac, &mut linux);

        mac.update_task(t, |t| t.title = "mac".into());
        std::thread::sleep(std::time::Duration::from_millis(5));
        linux.update_task(t, |t| t.title = "linux".into());

        sync(&linux, &mut mac);
        sync(&mac, &mut linux);
        assert_eq!(mac.tasks[&t].title, "linux");
        assert_eq!(linux.tasks[&t].title, "linux");
    }

    #[test]
    fn deletes_propagate() {
        let (mut mac, area) = board_with_area("mac");
        let t = mac.add_task(area, "doomed", None, None);
        let mut linux = Board::new("linux");
        sync(&mac, &mut linux);

        linux.delete_task(t);
        sync(&linux, &mut mac);
        assert!(titles(&mac, area).is_empty());
        // Merging the old version again must not resurrect it.
        let (mut stale, _) = board_with_area("mac");
        stale.tasks.insert(t, linux.tasks[&t].clone());
        stale.tasks.get_mut(&t).unwrap().meta.deleted = false;
        stale.tasks.get_mut(&t).unwrap().meta.updated_at = Timestamp::UNIX_EPOCH;
        assert!(!sync(&stale, &mut mac));
        assert!(titles(&mac, area).is_empty());
    }

    #[test]
    fn merging_same_state_reports_no_change() {
        let (mut mac, area) = board_with_area("mac");
        mac.add_task(area, "t", None, None);
        let copy = mac.clone();
        assert!(!sync(&copy, &mut mac));
    }

    #[test]
    fn stats_count_overdue_and_due_soon() {
        let (mut b, area) = board_with_area("mac");
        let today = date(2026, 10, 29);
        b.add_task(area, "overdue", None, Some(date(2026, 10, 1)));
        b.add_task(area, "soon", None, Some(date(2026, 11, 2)));
        b.add_task(area, "far", None, Some(date(2027, 1, 1)));
        let done = b.add_task(area, "done", None, None);
        b.set_done(done, true);
        let s = b.stats(today);
        let titles = |ids: &[Uuid]| -> Vec<String> {
            ids.iter().map(|id| b.tasks[id].title.clone()).collect()
        };
        assert_eq!(s.open, 3);
        assert_eq!(titles(&s.overdue), ["overdue"]);
        assert_eq!(titles(&s.due_this_week), ["soon"]);
        assert_eq!(s.done_recently, 1);
    }

    #[test]
    fn clearing_deadline_puts_task_at_bottom_of_list() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "a", None, None);
        let t = b.add_task(area, "t", None, Some(date(2026, 11, 1)));
        b.add_task(area, "b", None, None);
        let mut details = TaskDetails::from(&b.tasks[&t]);
        details.deadline = None;
        b.update_details(t, details);
        assert_eq!(titles(&b, area), ["a", "b", "t"]);
    }

    #[test]
    fn moving_to_other_column_drops_foreign_group() {
        let (mut b, area) = board_with_area("mac");
        let other = b.areas_sorted()[1].meta.id;
        let g = b.add_group(area, "g", GROUP_COLORS[0]);
        let t = b.add_task(area, "t", Some(g), None);
        let mut details = TaskDetails::from(&b.tasks[&t]);
        details.area = other;
        b.update_details(t, details);
        assert_eq!(b.tasks[&t].group, None);
        assert_eq!(titles(&b, other), ["t"]);
    }

    #[test]
    fn columns_can_be_reordered() {
        let (mut b, _) = board_with_area("mac");
        let names = |b: &Board| {
            b.areas_sorted()
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
        };
        let house = b.areas_sorted()[2].meta.id;
        b.shift_area(house, -1);
        assert_eq!(names(&b), ["Work", "House", "Private ventures", "Family"]);
        b.shift_area(house, -5);
        assert_eq!(names(&b)[0], "House");
        b.shift_area(house, 10);
        assert_eq!(names(&b)[3], "House");
    }

    #[test]
    fn columns_can_be_moved_to_a_position() {
        let (mut b, _) = board_with_area("mac");
        let names = |b: &Board| {
            b.areas_sorted()
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
        };
        let family = b.areas_sorted()[3].meta.id;
        b.move_area(family, 0);
        assert_eq!(names(&b), ["Family", "Work", "Private ventures", "House"]);
        b.move_area(family, 2);
        assert_eq!(names(&b), ["Work", "Private ventures", "Family", "House"]);
        b.move_area(family, 99);
        assert_eq!(names(&b)[3], "Family");
    }

    #[test]
    fn deleting_column_deletes_its_tasks_and_groups() {
        let (mut b, area) = board_with_area("mac");
        let g = b.add_group(area, "g", GROUP_COLORS[0]);
        b.add_task(area, "open", Some(g), None);
        let done = b.add_task(area, "done", None, None);
        b.set_done(done, true);
        assert_eq!(b.area_contents(area), (1, 1, 1));

        b.delete_area(area);
        assert_eq!(b.areas_sorted().len(), DEFAULT_AREAS.len() - 1);
        assert_eq!(b.area_contents(area), (0, 0, 0));
        assert_eq!(b.stats(date(2026, 10, 29)).open, 0);
    }

    #[test]
    fn deleting_column_can_be_undone() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "t", None, None);
        let snapshot = b.clone();
        b.delete_area(area);
        b.restore(&snapshot);
        assert_eq!(titles(&b, area), ["t"]);
        assert!(b.areas_sorted().iter().any(|a| a.meta.id == area));
    }

    #[test]
    fn deleted_column_reappears_if_other_device_added_tasks() {
        let (mut mac, area) = board_with_area("mac");
        let mut linux = mac.clone();
        linux.device = "linux".into();
        mac.delete_area(area);
        linux.add_task(area, "added on linux", None, None);
        sync(&linux, &mut mac);
        assert!(mac.areas_sorted().iter().any(|a| a.meta.id == area));
    }

    #[test]
    fn group_colors_avoid_ones_in_use() {
        let (mut b, area) = board_with_area("mac");
        let first = b.next_group_color(area);
        b.add_group(area, "g", first);
        assert_ne!(b.next_group_color(area), first);
    }

    #[test]
    fn undo_restores_edits_and_removes_additions() {
        let (mut b, area) = board_with_area("mac");
        let t = b.add_task(area, "original", None, None);
        let snapshot = b.clone();
        b.update_task(t, |t| t.title = "changed".into());
        b.add_task(area, "added later", None, None);
        b.restore(&snapshot);
        assert_eq!(titles(&b, area), ["original"]);
    }

    #[test]
    fn undo_wins_on_the_other_device() {
        let (mut mac, area) = board_with_area("mac");
        let t = mac.add_task(area, "original", None, None);
        let snapshot = mac.clone();
        mac.delete_task(t);
        let mut linux = Board::new("linux");
        sync(&mac, &mut linux);
        assert!(titles(&linux, area).is_empty());

        mac.restore(&snapshot);
        sync(&mac, &mut linux);
        assert_eq!(titles(&linux, area), ["original"]);
    }

    #[test]
    fn subtasks_can_be_checked_off_and_edited() {
        let (mut b, area) = board_with_area("mac");
        let t = b.add_task(area, "t", None, None);
        let a = b.add_subtask(t, "first");
        b.add_subtask(t, "second");
        b.set_subtask_done(t, a, true);
        let done: Vec<bool> = b.tasks[&t].subtasks.iter().map(|s| s.done).collect();
        assert_eq!(done, [true, false]);

        // The editor drops empty entries and trims the rest.
        let mut details = TaskDetails::from(&b.tasks[&t]);
        details.subtasks[1].title = "  ".into();
        details.subtasks.push(Subtask::new(" third "));
        b.update_details(t, details);
        let titles: Vec<&str> = b.tasks[&t]
            .subtasks
            .iter()
            .map(|s| s.title.as_str())
            .collect();
        assert_eq!(titles, ["first", "third"]);
    }

    #[test]
    fn subtask_changes_sync_with_their_task() {
        let (mut mac, area) = board_with_area("mac");
        let t = mac.add_task(area, "t", None, None);
        let s = mac.add_subtask(t, "step");
        let mut linux = Board::new("linux");
        sync(&mac, &mut linux);
        linux.set_subtask_done(t, s, true);
        sync(&linux, &mut mac);
        assert!(mac.tasks[&t].subtasks[0].done);
    }

    #[test]
    fn old_done_tasks_are_hidden() {
        let (mut b, area) = board_with_area("mac");
        let recent = b.add_task(area, "done today", None, None);
        let old = b.add_task(area, "done 10 days ago", None, None);
        b.set_done(recent, true);
        b.set_done(old, true);
        let ten_days_ago = Timestamp::now() - jiff::SignedDuration::from_hours(24 * 10);
        b.update_task(old, |t| t.completed_at = Some(ten_days_ago));

        let shown = |b: &Board| -> Vec<String> {
            b.done_list(area).iter().map(|t| t.title.clone()).collect()
        };
        assert_eq!(shown(&b), ["done today"]);
        assert_eq!(b.stats(TODAY).done_recently, 1);

        b.done_days = 14;
        assert_eq!(shown(&b), ["done today", "done 10 days ago"]);
        assert_eq!(b.stats(TODAY).done_recently, 2);
    }

    #[test]
    fn put_first_on_top_goes_before_arranged_and_overdue_tasks() {
        let (mut b, area) = board_with_area("mac");
        let yesterday = TODAY.yesterday().unwrap();
        let overdue = b.add_task(area, "overdue", None, Some(yesterday));
        let pinned = b.add_task(area, "pinned", None, None);
        b.pin(pinned, area);
        let arranged = b.add_task(area, "arranged", None, Some(TODAY));
        b.move_in_top(arranged, area, 0);
        let titles = |b: &Board| -> Vec<String> {
            b.top_list(area).iter().map(|t| t.title.clone()).collect()
        };
        let before = titles(&b);
        let stamps: Vec<_> = [overdue, pinned, arranged]
            .iter()
            .map(|id| b.tasks[id].meta.updated_at)
            .collect();

        let new = b.add_task(area, "new", None, Some(TODAY));
        b.put_first_on_top(new);
        let after = titles(&b);
        assert_eq!(after[0], "new");
        assert_eq!(after[1..], before[..]);
        // Nobody else was touched (nothing extra to sync).
        let now: Vec<_> = [overdue, pinned, arranged]
            .iter()
            .map(|id| b.tasks[id].meta.updated_at)
            .collect();
        assert_eq!(now, stamps);
        // A second new one goes before the first.
        let newer = b.add_task(area, "newer", None, Some(TODAY));
        b.put_first_on_top(newer);
        assert_eq!(titles(&b)[..2], ["newer", "new"]);
    }

    #[test]
    fn the_log_keeps_every_completed_task() {
        let (mut b, work) = board_with_area("mac");
        let house = b.add_area("House");
        let gone = b.add_area("Gone");
        let recent = b.add_task(work, "done today", None, None);
        let old = b.add_task(house, "done a year ago", None, None);
        let open = b.add_task(work, "still open", None, None);
        let deleted = b.add_task(work, "done, then deleted", None, None);
        let in_gone = b.add_task(gone, "done in a deleted area", None, None);
        for id in [recent, old, deleted, in_gone] {
            b.set_done(id, true);
        }
        let year_ago = Timestamp::now() - jiff::SignedDuration::from_hours(24 * 365);
        b.update_task(old, |t| t.completed_at = Some(year_ago));
        b.delete_task(deleted);
        b.delete_area(gone);

        let titles = |list: Vec<&TaskItem>| -> Vec<String> {
            list.iter().map(|t| t.title.clone()).collect()
        };
        // Older than done_days still counts, newest first.
        assert_eq!(titles(b.achieved(None)), ["done today", "done a year ago"]);
        assert_eq!(titles(b.achieved(Some(house))), ["done a year ago"]);
        assert!(b.achieved(Some(work)).iter().all(|t| t.meta.id != open));
        // Reopened: no longer achieved.
        b.set_done(recent, false);
        assert_eq!(titles(b.achieved(None)), ["done a year ago"]);
    }

    #[test]
    fn top_section_can_be_rearranged() {
        let (mut b, area) = board_with_area("mac");
        let sooner = b.add_task(area, "sooner", None, Some(date(2026, 11, 1)));
        b.add_task(area, "later", None, Some(date(2026, 11, 5)));
        let pinned = b.add_task(area, "pinned", None, None);
        b.pin(pinned, area);
        assert_eq!(top_titles(&b, area), ["sooner", "later", "pinned"]);

        b.move_in_top(pinned, area, 0);
        assert_eq!(top_titles(&b, area), ["pinned", "sooner", "later"]);
        b.move_in_top(sooner, area, 2);
        assert_eq!(top_titles(&b, area), ["pinned", "later", "sooner"]);

        // A task coming close later goes after the arranged ones.
        b.add_task(area, "tomorrow", None, Some(date(2026, 10, 30)));
        assert_eq!(
            top_titles(&b, area),
            ["pinned", "later", "sooner", "tomorrow"]
        );
    }

    #[test]
    fn dragging_from_the_list_into_the_top_pins_at_that_spot() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "due", None, Some(date(2026, 11, 1)));
        let plain = b.add_task(area, "plain", None, None);
        b.move_in_top(plain, area, 0);
        assert_eq!(top_titles(&b, area), ["plain", "due"]);
        assert!(b.tasks[&plain].pinned);

        // Dragged back down, it forgets its spot in the top section.
        b.move_task(plain, area, 0);
        assert_eq!(b.tasks[&plain].top_rank, None);
        assert!(!b.tasks[&plain].pinned);
    }

    fn weekly_friday() -> Repeat {
        Repeat::Schedule {
            every: 1,
            unit: crate::repeat::Unit::Weeks,
            weekdays: vec![5],
        }
    }

    fn make_recurring(b: &mut Board, id: Uuid, repeat: Repeat) {
        let mut details = TaskDetails::from(&b.tasks[&id]);
        details.repeat = Some(repeat);
        b.update_details(id, details);
    }

    #[test]
    fn each_task_can_have_its_own_lead_time() {
        let (mut b, area) = board_with_area("mac");
        // Friday Oct 30 is one day after TODAY; Nov 6 is eight days.
        let trash = b.add_task(area, "trash", None, Some(date(2026, 11, 6)));
        assert_eq!(
            top_titles(&b, area),
            ["trash"],
            "within the default 10 days"
        );
        let mut details = TaskDetails::from(&b.tasks[&trash]);
        details.lead_days = Some(1);
        b.update_details(trash, details);
        assert!(b.top_list(area).is_empty(), "only 1 day before now");
        b.today = date(2026, 11, 5);
        assert_eq!(top_titles(&b, area), ["trash"]);
    }

    #[test]
    fn completing_a_recurring_task_creates_the_next_one() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "other", None, None);
        let trash = b.add_task(area, "trash", None, Some(date(2026, 10, 30)));
        b.add_subtask(trash, "bins to the street");
        make_recurring(&mut b, trash, weekly_friday());
        b.set_subtask_done(trash, b.tasks[&trash].subtasks[0].id, true);

        b.set_done(trash, true);
        let next = &b.tasks[&successor_id(trash)];
        assert!(!next.done);
        assert_eq!(next.deadline, Some(date(2026, 11, 6)));
        assert_eq!(next.title, "trash");
        assert!(!next.subtasks[0].done, "subtasks start unticked");
        assert_eq!(b.done_list(area).len(), 1, "the done one counts");

        // Reopening removes the next one again; doing it again brings it back.
        b.set_done(trash, false);
        assert!(b.tasks[&successor_id(trash)].meta.deleted);
        b.set_done(trash, true);
        assert!(!b.tasks[&successor_id(trash)].meta.deleted);
        let open: Vec<_> = b
            .tasks
            .values()
            .filter(|t| t.title == "trash" && !t.done && !t.meta.deleted)
            .collect();
        assert_eq!(open.len(), 1);
    }

    #[test]
    fn redoing_an_earlier_occurrence_keeps_the_later_ones() {
        let (mut b, area) = board_with_area("mac");
        let first = b.add_task(area, "trash", None, Some(date(2026, 10, 30)));
        make_recurring(&mut b, first, weekly_friday());
        b.set_done(first, true);
        let second = successor_id(first);
        b.update_task(second, |t| t.notes = "used the new bins".to_owned());
        b.set_done(second, true);
        let third = successor_id(second);

        // The second one is done, so reopening the first leaves it alone…
        b.set_done(first, false);
        assert!(!b.tasks[&second].meta.deleted);
        // …and completing the first again doesn't overwrite it.
        b.set_done(first, true);
        let kept = &b.tasks[&second];
        assert!(kept.done);
        assert_eq!(kept.notes, "used the new bins");
        let open: Vec<_> = b
            .tasks
            .values()
            .filter(|t| t.title == "trash" && !t.done && !t.meta.deleted)
            .map(|t| t.meta.id)
            .collect();
        assert_eq!(open, [third], "still only one open occurrence");
    }

    #[test]
    fn overdue_recurring_task_stays_until_done_then_jumps_ahead() {
        let (mut b, area) = board_with_area("mac");
        let trash = b.add_task(area, "trash", None, Some(date(2026, 10, 9)));
        make_recurring(&mut b, trash, weekly_friday());
        assert!(
            b.overdue(&b.tasks[&trash]),
            "missed Fridays leave it overdue"
        );
        b.set_done(trash, true);
        assert_eq!(
            b.tasks[&successor_id(trash)].deadline,
            Some(date(2026, 10, 30)),
            "the next Friday after today, not the missed ones"
        );
    }

    #[test]
    fn skipping_moves_on_without_counting_as_done() {
        let (mut b, area) = board_with_area("mac");
        let trash = b.add_task(area, "trash", None, Some(date(2026, 10, 30)));
        make_recurring(&mut b, trash, weekly_friday());
        b.skip(trash);
        assert_eq!(b.tasks[&trash].deadline, Some(date(2026, 11, 6)));
        assert!(b.done_list(area).is_empty());
    }

    #[test]
    fn completing_on_both_devices_makes_one_next_occurrence() {
        let (mut mac, area) = board_with_area("mac");
        let trash = mac.add_task(area, "trash", None, Some(date(2026, 10, 30)));
        make_recurring(&mut mac, trash, weekly_friday());
        let mut linux = mac.clone();
        linux.device = "linux".into();
        mac.set_done(trash, true);
        linux.set_done(trash, true);
        sync(&linux, &mut mac);
        let open = mac
            .tasks
            .values()
            .filter(|t| t.title == "trash" && !t.done && !t.meta.deleted)
            .count();
        assert_eq!(open, 1);
    }

    #[test]
    fn due_today_and_overdue_are_urgent() {
        let (mut b, area) = board_with_area("mac");
        let today = b.add_task(area, "today", None, Some(TODAY));
        let late = b.add_task(area, "late", None, Some(date(2026, 10, 20)));
        let soon = b.add_task(area, "tomorrow", None, Some(date(2026, 10, 30)));
        assert!(b.urgent(&b.tasks[&today]) && !b.overdue(&b.tasks[&today]));
        assert!(b.urgent(&b.tasks[&late]) && b.overdue(&b.tasks[&late]));
        assert!(!b.urgent(&b.tasks[&soon]));
        b.set_done(late, true);
        assert!(!b.urgent(&b.tasks[&late]), "done isn't urgent");
    }

    #[test]
    fn groups_move_to_another_area_with_their_tasks() {
        let (mut b, work) = board_with_area("mac");
        let house = b.areas_sorted()[2].meta.id;
        let clients = b.add_group(work, "Clients", GROUP_COLORS[0]);
        b.add_task(house, "house task", None, None);
        b.add_task(work, "a", Some(clients), None);
        b.add_task(work, "other", None, None);
        b.add_task(work, "b", Some(clients), None);
        let due = b.add_task(work, "due", Some(clients), Some(date(2026, 10, 30)));
        let done = b.add_task(work, "done", Some(clients), None);
        b.set_done(done, true);

        b.move_group(clients, house);
        assert_eq!(b.groups[&clients].area, house);
        assert_eq!(titles(&b, work), ["other"]);
        assert_eq!(
            titles(&b, house),
            ["house task", "a", "b"],
            "appended, order kept"
        );
        assert_eq!(top_titles(&b, house), ["due"]);
        assert_eq!(b.tasks[&due].group, Some(clients), "tasks keep their group");
        assert_eq!(b.tasks[&done].area, house, "done tasks move along");
        assert!(b.groups_of(work).is_empty());
    }

    #[test]
    fn a_task_added_to_a_group_moved_meanwhile_follows_the_group() {
        // Both computers start from the same state.
        let (mut mac, work) = board_with_area("mac");
        let house = mac.areas_sorted()[2].meta.id;
        let clients = mac.add_group(work, "Clients", GROUP_COLORS[0]);
        let old = mac.add_task(work, "already there", Some(clients), None);
        let mut linux = mac.clone();
        linux.device = "linux".into();

        // Offline: the Mac adds a task to Clients, Linux moves Clients.
        let new = mac.add_task(work, "added on the Mac", Some(clients), None);
        linux.move_group(clients, house);

        // Either way round, both end up the same: everything in House.
        let mut mac_first = mac.clone();
        sync(&linux, &mut mac_first);
        let mut linux_first = linux.clone();
        sync(&mac, &mut linux_first);
        for board in [&mac_first, &linux_first] {
            assert_eq!(board.groups[&clients].area, house);
            for task in [old, new] {
                assert_eq!(board.tasks[&task].area, house);
                assert_eq!(board.tasks[&task].group, Some(clients));
            }
            assert!(titles(board, work).is_empty());
        }
        // And stay so once each has the other's result.
        sync(&linux_first, &mut mac_first);
        sync(&mac_first, &mut linux_first);
        assert_eq!(mac_first.tasks[&new].area, house);
        assert_eq!(linux_first.tasks[&new].area, house);
        assert_eq!(mac_first.tasks[&new], linux_first.tasks[&new]);
    }

    #[test]
    fn a_task_in_a_deleted_group_stays_where_it_is() {
        let (mut mac, work) = board_with_area("mac");
        let clients = mac.add_group(work, "Clients", GROUP_COLORS[0]);
        let mut linux = mac.clone();
        linux.device = "linux".into();
        let task = mac.add_task(work, "added on the Mac", Some(clients), None);
        linux.delete_group(clients);
        sync(&linux, &mut mac);
        assert_eq!(mac.tasks[&task].area, work);
        assert!(mac.group(clients).is_none());
    }

    #[test]
    fn done_on_one_computer_and_edited_on_the_other_stays_done() {
        let (mut mac, area) = board_with_area("mac");
        let trash = mac.add_task(area, "trash", None, Some(date(2026, 10, 30)));
        make_recurring(&mut mac, trash, weekly_friday());
        let mut linux = mac.clone();
        linux.device = "linux".into();

        mac.set_done(trash, true);
        // Later, not synced yet: Linux adds a note.
        linux.update_task(trash, |t| t.notes = "blue bin".into());

        let mut mac_after = mac.clone();
        sync(&linux, &mut mac_after);
        let mut linux_after = linux.clone();
        sync(&mac, &mut linux_after);
        for board in [&mac_after, &linux_after] {
            let open: Vec<_> = board
                .tasks
                .values()
                .filter(|t| t.title == "trash" && !t.done && !t.meta.deleted)
                .collect();
            assert_eq!(open.len(), 1, "only the next one is open");
            assert!(board.tasks[&trash].done);
            assert_eq!(board.tasks[&trash].notes, "blue bin", "the edit stays");
        }
        // Reopening on purpose still works.
        mac_after.set_done(trash, false);
        assert!(!mac_after.tasks[&trash].done);
        assert!(mac_after.tasks[&successor_id(trash)].meta.deleted);
    }

    #[test]
    fn an_area_deleted_elsewhere_stays_once_it_is_used_here() {
        let (mut mac, work) = board_with_area("mac");
        let mut linux = mac.clone();
        linux.device = "linux".into();
        mac.delete_area(work);
        let task = linux.add_task(work, "added on Linux", None, None);
        sync(&linux, &mut mac);
        assert!(
            mac.areas_sorted().iter().any(|a| a.meta.id == work),
            "shown"
        );

        // Renamed here, then its only task done: it stays.
        mac.rename_area(work, "Job");
        mac.set_done(task, true);
        assert!(mac.areas_sorted().iter().any(|a| a.meta.id == work));
        // A task added to such an area keeps it too.
        let (mut mac2, _) = board_with_area("mac");
        let mut linux2 = mac2.clone();
        linux2.device = "linux".into();
        mac2.delete_area(work);
        let t = linux2.add_task(work, "added on Linux", None, None);
        sync(&linux2, &mut mac2);
        mac2.add_task(work, "added here", None, None);
        mac2.set_done(t, true);
        assert!(!mac2.areas[&work].meta.deleted);
    }

    #[test]
    fn a_group_deleted_elsewhere_comes_back_when_saved() {
        let (mut b, work) = board_with_area("mac");
        let clients = b.add_group(work, "Clients", GROUP_COLORS[0]);
        b.delete_group(clients);
        b.update_group(clients, "Customers", GROUP_COLORS[1]);
        assert_eq!(b.group(clients).map(|g| g.name.as_str()), Some("Customers"));
    }

    #[test]
    fn huge_numbers_from_a_damaged_file_dont_panic() {
        let (mut b, area) = board_with_area("mac");
        let t = b.add_task(area, "far", None, Some(date(2026, 11, 1)));
        b.update_task(t, |t| t.lead_days = Some(i64::MAX));
        b.done_days = i64::MAX;
        assert_eq!(top_titles(&b, area), ["far"]);
        b.update_task(t, |t| t.lead_days = Some(i64::MIN));
        b.horizon_days = i64::MIN;
        let _ = b.top_list(area);
        let _ = b.done_list(area);
        let _ = b.stats(TODAY);
    }

    #[test]
    fn pinning_starts_at_the_end_of_the_arranged_ones() {
        let (mut b, area) = board_with_area("mac");
        let a = b.add_task(area, "a", None, None);
        let c = b.add_task(area, "c", None, None);
        let t = b.add_task(area, "t", None, None);
        b.move_in_top(a, area, 0);
        b.move_in_top(t, area, 0);
        b.move_in_top(c, area, 0);
        assert_eq!(top_titles(&b, area), ["c", "t", "a"]);
        // t is unpinned in its dialog, and pinned again later: not back at
        // its old place.
        let mut details = TaskDetails::from(&b.tasks[&t]);
        details.pinned = false;
        b.update_details(t, details);
        assert_eq!(top_titles(&b, area), ["c", "a"]);
        b.pin(t, area);
        assert_eq!(top_titles(&b, area), ["c", "a", "t"]);
    }

    #[test]
    fn reopening_keeps_a_next_occurrence_edited_meanwhile() {
        let (mut mac, area) = board_with_area("mac");
        let trash = mac.add_task(area, "trash", None, Some(date(2026, 10, 30)));
        make_recurring(&mut mac, trash, weekly_friday());
        mac.set_done(trash, true);
        let next = successor_id(trash);
        mac.update_task(next, |t| t.notes = "blue bin".into());
        // (An edit comes later than the second it was made in.)
        let later = mac.tasks[&next]
            .created_at
            .checked_add(jiff::SignedDuration::from_secs(60))
            .unwrap();
        mac.tasks.get_mut(&next).unwrap().meta.updated_at = later;

        mac.set_done(trash, false);
        assert!(!mac.tasks[&trash].done);
        let kept = &mac.tasks[&next];
        assert!(!kept.meta.deleted && !kept.done);
        assert_eq!(kept.notes, "blue bin");

        // Syncing doesn't take it for a completion elsewhere.
        let mut linux = Board::new("linux");
        linux.today = TODAY;
        sync(&mac, &mut linux);
        sync(&linux, &mut mac);
        assert!(!mac.tasks[&trash].done);
        assert!(!linux.tasks[&trash].done);

        // Done again: no third occurrence, the edited one stays the next.
        mac.set_done(trash, true);
        let open: Vec<Uuid> = mac
            .tasks
            .values()
            .filter(|t| t.title == "trash" && !t.done && !t.meta.deleted)
            .map(|t| t.meta.id)
            .collect();
        assert_eq!(open, [next]);
        assert_eq!(mac.tasks[&next].notes, "blue bin");
        assert!(!mac.tasks[&trash].reopened);
    }

    #[test]
    fn a_task_keeps_its_colour() {
        let mut b = Board::new("mac");
        b.today = TODAY;
        b.add_default_areas();
        let area = b.areas_sorted()[0].meta.id;
        let id = b.add_task(area, "trash", None, Some(TODAY));
        let mut details = TaskDetails::from(&b.tasks[&id]);
        details.color = Some(TASK_COLORS[3]);
        details.repeat = Some(Repeat::Schedule {
            every: 1,
            unit: crate::repeat::Unit::Weeks,
            weekdays: vec![],
        });
        b.update_details(id, details);
        assert_eq!(b.tasks[&id].color, Some(TASK_COLORS[3]));
        // The next occurrence of a recurring task has it too.
        b.set_done(id, true);
        assert_eq!(b.tasks[&successor_id(id)].color, Some(TASK_COLORS[3]));
    }

    // ----- Done, reopen, skip ---------------------------------------------

    #[test]
    fn done_tasks_leave_the_lists_and_can_be_reopened() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "task", None, Some(TODAY));
        b.set_done(id, true);
        assert!(b.top_list(area).is_empty() && b.priority_list(area).is_empty());
        assert_eq!(b.done_list(area).len(), 1);
        assert!(b.tasks[&id].completed_at.is_some());
        assert!(!b.overdue(&b.tasks[&id]) && !b.urgent(&b.tasks[&id]));

        b.set_done(id, false);
        b.set_done(id, false);
        assert_eq!(b.tasks[&id].completed_at, None);
        assert!(b.done_list(area).is_empty());
        assert_eq!(top_titles(&b, area), ["task"]);
    }

    #[test]
    fn done_tasks_are_listed_most_recent_first() {
        let (mut b, area) = board_with_area("mac");
        let first = b.add_task(area, "first", None, None);
        let second = b.add_task(area, "second", None, None);
        b.set_done(first, true);
        b.set_done(second, true);
        let earlier = Timestamp::now() - jiff::SignedDuration::from_hours(1);
        b.update_task(first, |t| t.completed_at = Some(earlier));
        let done: Vec<&str> = b.done_list(area).iter().map(|t| t.title.as_str()).collect();
        assert_eq!(done, ["second", "first"]);
    }

    #[test]
    fn skipping_a_plain_task_changes_nothing() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "plain", None, Some(date(2026, 11, 2)));
        b.skip(id);
        assert_eq!(b.tasks[&id].deadline, Some(date(2026, 11, 2)));
        assert!(!b.tasks[&id].done);
    }

    fn weekly(b: &mut Board, id: Uuid) {
        let mut details = TaskDetails::from(&b.tasks[&id]);
        details.repeat = Some(Repeat::Schedule {
            every: 1,
            unit: crate::repeat::Unit::Weeks,
            weekdays: vec![],
        });
        b.update_details(id, details);
    }

    #[test]
    fn the_next_occurrence_starts_fresh_but_keeps_the_rest() {
        let (mut b, area) = board_with_area("mac");
        let group = b.add_group(area, "Home", GROUP_COLORS[0]);
        let id = b.add_task(area, "trash", Some(group), Some(TODAY));
        let step = b.add_subtask(id, "bins out");
        b.set_subtask_done(id, step, true);
        b.update_task(id, |t| {
            t.notes = "blue bin".into();
            t.pinned = true;
            t.lead_days = Some(1);
        });
        weekly(&mut b, id);
        b.set_done(id, true);

        let next = &b.tasks[&successor_id(id)];
        assert_eq!(next.deadline, Some(date(2026, 11, 5)));
        assert!(!next.done && next.completed_at.is_none());
        assert!(next.subtasks.iter().all(|s| !s.done), "subtasks unticked");
        assert_eq!(next.subtasks[0].title, "bins out");
        assert_eq!(next.group, Some(group));
        assert_eq!(next.notes, "blue bin");
        assert!(next.pinned);
        assert_eq!(next.lead_days, Some(1));
        // The done one keeps its ticks.
        assert!(b.tasks[&id].subtasks[0].done);
    }

    #[test]
    fn reopening_removes_the_next_occurrence_and_redoing_brings_it_back() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "trash", None, Some(TODAY));
        weekly(&mut b, id);
        b.set_done(id, true);
        let next = successor_id(id);
        assert!(!b.tasks[&next].meta.deleted);
        b.set_done(id, false);
        assert!(b.tasks[&next].meta.deleted);
        assert_eq!(b.top_list(area).len(), 1);
        b.set_done(id, true);
        assert!(!b.tasks[&next].meta.deleted, "the same record again");
        assert_eq!(b.tasks.len(), 2);
    }

    #[test]
    fn a_chain_of_occurrences_gets_new_ids() {
        let (mut b, area) = board_with_area("mac");
        let first = b.add_task(area, "trash", None, Some(TODAY));
        weekly(&mut b, first);
        b.set_done(first, true);
        let second = successor_id(first);
        b.set_done(second, true);
        let third = successor_id(second);
        assert_ne!(third, first);
        assert_eq!(b.tasks[&third].deadline, Some(date(2026, 11, 12)));
        assert_eq!(b.tasks.values().filter(|t| !t.done).count(), 1);
    }

    #[test]
    fn reopening_keeps_an_edited_next_occurrence_if_done_already() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "trash", None, Some(TODAY));
        weekly(&mut b, id);
        b.set_done(id, true);
        let next = successor_id(id);
        b.set_done(next, true);
        // Reopening the first doesn't throw away the one already done.
        b.set_done(id, false);
        assert!(!b.tasks[&next].meta.deleted);
    }

    // ----- Subtasks and edits ---------------------------------------------

    #[test]
    fn unknown_ids_are_ignored() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "task", None, None);
        let before = b.clone();
        let nobody = Uuid::new_v4();
        b.update_task(nobody, |t| t.title = "x".into());
        b.set_done(nobody, true);
        b.skip(nobody);
        b.delete_task(nobody);
        b.set_subtask_done(id, nobody, true);
        b.set_subtask_done(nobody, nobody, true);
        b.update_details(nobody, TaskDetails::from(&before.tasks[&id]));
        b.move_in_top(nobody, area, 0);
        b.rename_area(nobody, "x");
        b.update_group(nobody, "x", [0, 0, 0]);
        b.delete_group(nobody);
        b.move_group(nobody, area);
        b.move_area(nobody, 0);
        b.shift_area(nobody, 1);
        // Only the unknown subtask's task got touched, nothing else changed.
        let mut after = b.clone();
        after.tasks.get_mut(&id).unwrap().meta = before.tasks[&id].meta.clone();
        assert_eq!(after, before);
    }

    #[test]
    fn edits_are_trimmed_and_empty_subtasks_dropped() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "  spaced  ", None, None);
        assert_eq!(b.tasks[&id].title, "spaced");
        let mut details = TaskDetails::from(&b.tasks[&id]);
        details.title = "  new title \n".into();
        details.subtasks = vec![
            Subtask::new("keep"),
            Subtask::new("   "),
            Subtask {
                title: "  padded ".into(),
                ..Subtask::new("x")
            },
        ];
        b.update_details(id, details);
        let t = &b.tasks[&id];
        assert_eq!(t.title, "new title");
        let subtasks: Vec<&str> = t.subtasks.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(subtasks, ["keep", "padded"]);
    }

    #[test]
    fn saving_unchanged_details_keeps_the_task_where_it_is() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "a", None, None);
        let id = b.add_task(area, "b", None, None);
        b.add_task(area, "c", None, None);
        b.update_details(id, TaskDetails::from(&b.tasks[&id]));
        assert_eq!(titles(&b, area), ["a", "b", "c"]);
    }

    #[test]
    fn a_group_from_another_area_is_dropped_on_save() {
        let (mut b, work) = board_with_area("mac");
        let house = b.areas_sorted()[2].meta.id;
        let garden = b.add_group(house, "Garden", GROUP_COLORS[0]);
        let id = b.add_task(work, "task", None, None);
        let mut details = TaskDetails::from(&b.tasks[&id]);
        details.group = Some(garden);
        b.update_details(id, details.clone());
        assert_eq!(b.tasks[&id].group, None);
        // Moving it to that area with the group keeps it.
        details.area = house;
        b.update_details(id, details);
        assert_eq!(b.tasks[&id].group, Some(garden));
    }

    #[test]
    fn a_deleted_group_cannot_be_chosen() {
        let (mut b, area) = board_with_area("mac");
        let group = b.add_group(area, "Old", GROUP_COLORS[0]);
        b.delete_group(group);
        let id = b.add_task(area, "task", None, None);
        let mut details = TaskDetails::from(&b.tasks[&id]);
        details.group = Some(group);
        b.update_details(id, details);
        assert_eq!(b.tasks[&id].group, None);
    }

    #[test]
    fn moving_a_task_to_another_area_in_the_editor_puts_it_at_the_bottom() {
        let (mut b, work) = board_with_area("mac");
        let house = b.areas_sorted()[2].meta.id;
        b.add_task(house, "h1", None, None);
        b.add_task(house, "h2", None, None);
        let id = b.add_task(work, "moved", None, None);
        let mut details = TaskDetails::from(&b.tasks[&id]);
        details.area = house;
        b.update_details(id, details);
        assert_eq!(titles(&b, house), ["h1", "h2", "moved"]);
        assert!(titles(&b, work).is_empty());
    }

    #[test]
    fn subtasks_of_other_tasks_are_left_alone() {
        let (mut b, area) = board_with_area("mac");
        let a = b.add_task(area, "a", None, None);
        let other = b.add_task(area, "b", None, None);
        let step = b.add_subtask(a, "step");
        b.set_subtask_done(other, step, true);
        assert!(!b.tasks[&a].subtasks[0].done);
    }

    // ----- Top section, pinning, moving -----------------------------------

    #[test]
    fn pinning_into_another_area_drops_a_foreign_group() {
        let (mut b, work) = board_with_area("mac");
        let house = b.areas_sorted()[2].meta.id;
        let clients = b.add_group(work, "Clients", GROUP_COLORS[0]);
        let stays = b.add_task(work, "stays", Some(clients), None);
        let moves = b.add_task(work, "moves", Some(clients), None);
        b.pin(stays, work);
        b.pin(moves, house);
        assert_eq!(b.tasks[&stays].group, Some(clients));
        assert_eq!(b.tasks[&moves].group, None);
        assert_eq!(top_titles(&b, house), ["moves"]);
    }

    #[test]
    fn moving_into_the_list_unpins_and_forgets_the_top_position() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "pinned", None, None);
        b.move_in_top(id, area, 0);
        assert!(b.tasks[&id].pinned && b.tasks[&id].top_rank.is_some());
        b.move_task(id, area, 0);
        let t = &b.tasks[&id];
        assert!(!t.pinned);
        assert_eq!(t.top_rank, None);
        assert_eq!(titles(&b, area), ["pinned"]);
    }

    #[test]
    fn a_position_past_the_end_means_the_end() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "a", None, None);
        let id = b.add_task(area, "b", None, None);
        b.add_task(area, "c", None, None);
        b.move_task(id, area, 99);
        assert_eq!(titles(&b, area), ["a", "c", "b"]);
        let top1 = b.add_task(area, "t1", None, Some(TODAY));
        b.add_task(area, "t2", None, Some(TODAY));
        b.move_in_top(top1, area, 99);
        assert_eq!(top_titles(&b, area), ["t2", "t1"]);
    }

    #[test]
    fn a_task_due_soon_is_not_pinned_by_arranging_it() {
        let (mut b, area) = board_with_area("mac");
        let due = b.add_task(area, "due", None, Some(TODAY));
        b.add_task(area, "also", None, Some(date(2026, 10, 30)));
        b.move_in_top(due, area, 1);
        assert!(!b.tasks[&due].pinned);
        assert_eq!(top_titles(&b, area), ["also", "due"]);
    }

    #[test]
    fn dragging_into_another_areas_top_moves_it_there() {
        let (mut b, work) = board_with_area("mac");
        let house = b.areas_sorted()[2].meta.id;
        let id = b.add_task(work, "task", None, None);
        b.move_in_top(id, house, 0);
        assert_eq!(b.tasks[&id].area, house);
        assert!(b.tasks[&id].pinned);
        assert_eq!(top_titles(&b, house), ["task"]);
    }

    #[test]
    fn arranged_tasks_come_before_unarranged_ones() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "soon", None, Some(date(2026, 10, 30)));
        let pinned = b.add_task(area, "pinned", None, None);
        b.move_in_top(pinned, area, 0);
        // A new due task appears after the arranged ones, though it's sooner.
        b.add_task(area, "today", None, Some(TODAY));
        assert_eq!(top_titles(&b, area), ["pinned", "soon", "today"]);
    }

    #[test]
    fn a_recurring_task_dragged_into_the_list_keeps_its_date() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "trash", None, Some(TODAY));
        weekly(&mut b, id);
        b.move_task(id, area, 0);
        assert_eq!(b.tasks[&id].deadline, Some(TODAY));
    }

    // ----- Deadlines and lead times ---------------------------------------

    #[test]
    fn a_lead_time_of_zero_means_only_on_the_day() {
        let (mut b, area) = board_with_area("mac");
        let tomorrow = b.add_task(area, "tomorrow", None, Some(date(2026, 10, 30)));
        b.update_task(tomorrow, |t| t.lead_days = Some(0));
        assert!(b.top_list(area).is_empty());
        b.today = date(2026, 10, 30);
        assert_eq!(top_titles(&b, area), ["tomorrow"]);
    }

    #[test]
    fn the_app_wide_horizon_applies_to_tasks_without_their_own() {
        let (mut b, area) = board_with_area("mac");
        b.add_task(area, "in 5 days", None, Some(date(2026, 11, 3)));
        b.add_task(area, "in 10 days", None, Some(date(2026, 11, 8)));
        b.add_task(area, "in 11 days", None, Some(date(2026, 11, 9)));
        assert_eq!(top_titles(&b, area), ["in 5 days", "in 10 days"]);
        b.horizon_days = 3;
        assert!(b.top_list(area).is_empty());
        b.horizon_days = 0;
        assert!(b.top_list(area).is_empty());
    }

    #[test]
    fn overdue_and_urgent() {
        let (mut b, area) = board_with_area("mac");
        let past = b.add_task(area, "past", None, Some(date(2026, 10, 28)));
        let today = b.add_task(area, "today", None, Some(TODAY));
        let later = b.add_task(area, "later", None, Some(date(2026, 10, 30)));
        let none = b.add_task(area, "none", None, None);
        let t = |id| &b.tasks[&id];
        assert!(b.overdue(t(past)) && b.urgent(t(past)));
        assert!(!b.overdue(t(today)) && b.urgent(t(today)));
        assert!(!b.overdue(t(later)) && !b.urgent(t(later)));
        assert!(!b.overdue(t(none)) && !b.urgent(t(none)));
    }

    // ----- Statistics -----------------------------------------------------

    #[test]
    fn stats_skip_deleted_tasks_and_areas() {
        let (mut b, work) = board_with_area("mac");
        let house = b.areas_sorted()[2].meta.id;
        b.add_task(work, "open", None, None);
        let gone = b.add_task(work, "deleted", None, Some(date(2026, 10, 1)));
        b.delete_task(gone);
        b.add_task(house, "in a deleted area", None, Some(date(2026, 10, 1)));
        b.delete_area(house);
        let stats = b.stats(TODAY);
        assert_eq!(stats.open, 1);
        assert!(stats.overdue.is_empty());
    }

    #[test]
    fn this_week_means_the_next_seven_days() {
        let (mut b, area) = board_with_area("mac");
        let today = b.add_task(area, "today", None, Some(TODAY));
        let seven = b.add_task(area, "in 7", None, Some(date(2026, 11, 5)));
        b.add_task(area, "in 8", None, Some(date(2026, 11, 6)));
        let late = b.add_task(area, "late b", None, Some(date(2026, 10, 20)));
        let later = b.add_task(area, "late a", None, Some(date(2026, 10, 20)));
        let earliest = b.add_task(area, "latest", None, Some(date(2026, 10, 1)));
        let stats = b.stats(TODAY);
        assert_eq!(stats.due_this_week, [today, seven]);
        // Earliest deadline first; same day by title.
        assert_eq!(stats.overdue, [earliest, later, late]);
        assert_eq!(stats.open, 6);
    }

    // ----- Areas ----------------------------------------------------------

    #[test]
    fn unchanged_area_settings_are_not_a_change() {
        let (mut b, area) = board_with_area("mac");
        let before = b.areas[&area].meta.clone();
        b.set_area_color(area, None);
        b.set_active_hours(area, None);
        assert_eq!(b.areas[&area].meta, before, "nothing to sync");
        b.set_area_color(area, Some([1, 2, 3]));
        assert!(b.areas[&area].meta.is_newer_than(&before));
        let colored = b.areas[&area].meta.clone();
        b.set_area_color(area, Some([1, 2, 3]));
        assert_eq!(b.areas[&area].meta, colored);
        b.set_active_hours(area, Some(crate::active::ActiveHours::default()));
        assert!(b.areas[&area].meta.is_newer_than(&colored));
    }

    #[test]
    fn area_names_are_trimmed_and_new_areas_go_last() {
        let (mut b, work) = board_with_area("mac");
        b.rename_area(work, "  Job  ");
        assert_eq!(b.areas[&work].name, "Job");
        let new = b.add_area(" Hobbies ");
        let names: Vec<&str> = b.areas_sorted().iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names.last(), Some(&"Hobbies"));
        assert_eq!(b.areas_sorted().last().unwrap().meta.id, new);
    }

    #[test]
    fn area_contents_count_what_a_delete_would_take() {
        let (mut b, area) = board_with_area("mac");
        b.add_group(area, "g", GROUP_COLORS[0]);
        b.add_task(area, "open", None, None);
        b.add_task(area, "due", None, Some(TODAY));
        let done = b.add_task(area, "done", None, None);
        b.set_done(done, true);
        let gone = b.add_task(area, "gone", None, None);
        b.delete_task(gone);
        assert_eq!(b.area_contents(area), (2, 1, 1));
    }

    #[test]
    fn a_deleted_area_disappears_on_the_other_device_too() {
        let (mut mac, area) = board_with_area("mac");
        mac.add_task(area, "task", None, None);
        let (mut linux, _) = board_with_area("linux");
        sync(&mac, &mut linux);
        mac.delete_area(area);
        sync(&mac, &mut linux);
        assert!(linux.areas_sorted().iter().all(|a| a.meta.id != area));
        assert_eq!(linux.stats(TODAY).open, 0);
    }

    #[test]
    fn shifting_past_the_edges_stays_at_the_edge() {
        let (mut b, work) = board_with_area("mac");
        b.shift_area(work, -1);
        assert_eq!(b.areas_sorted()[0].meta.id, work);
        let family = b.areas_sorted()[3].meta.id;
        b.shift_area(family, 5);
        assert_eq!(b.areas_sorted()[3].meta.id, family);
        b.move_area(work, 99);
        assert_eq!(b.areas_sorted()[3].meta.id, work);
    }

    #[test]
    fn moving_an_area_to_where_it_is_changes_nothing() {
        let (mut b, work) = board_with_area("mac");
        b.move_area(work, 0);
        let before = b.clone();
        b.move_area(work, 0);
        assert_eq!(b, before);
    }

    // ----- Groups ---------------------------------------------------------

    #[test]
    fn a_deleted_groups_tasks_stay_without_it() {
        let (mut b, area) = board_with_area("mac");
        let group = b.add_group(area, "Old", GROUP_COLORS[0]);
        let id = b.add_task(area, "task", Some(group), None);
        b.delete_group(group);
        assert!(b.group(group).is_none());
        assert!(b.groups_of(area).is_empty());
        assert_eq!(titles(&b, area), ["task"]);
        assert!(!b.tasks[&id].meta.deleted);
    }

    #[test]
    fn groups_are_listed_by_name_and_can_be_edited() {
        let (mut b, area) = board_with_area("mac");
        let z = b.add_group(area, "Zoo", GROUP_COLORS[0]);
        b.add_group(area, "Admin", GROUP_COLORS[1]);
        let names: Vec<&str> = b.groups_of(area).iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Admin", "Zoo"]);
        b.update_group(z, "  Aardvark ", [9, 9, 9]);
        assert_eq!(b.groups[&z].name, "Aardvark");
        assert_eq!(b.groups[&z].color, [9, 9, 9]);
        assert_eq!(b.groups_of(area)[0].meta.id, z);
    }

    #[test]
    fn group_colors_cycle_when_all_are_used() {
        let (mut b, area) = board_with_area("mac");
        for (i, color) in GROUP_COLORS.iter().enumerate() {
            assert_eq!(b.next_group_color(area), *color);
            b.add_group(area, &format!("g{i}"), *color);
        }
        assert_eq!(b.next_group_color(area), GROUP_COLORS[0]);
        // Other areas' colours don't count.
        let other = b.areas_sorted()[1].meta.id;
        assert_eq!(b.next_group_color(other), GROUP_COLORS[0]);
    }

    #[test]
    fn moving_a_group_to_its_own_or_an_unknown_area_does_nothing() {
        let (mut b, area) = board_with_area("mac");
        let group = b.add_group(area, "g", GROUP_COLORS[0]);
        b.add_task(area, "t", Some(group), None);
        let before = b.clone();
        b.move_group(group, area);
        b.move_group(group, Uuid::new_v4());
        assert_eq!(b, before);
    }

    // ----- Merging --------------------------------------------------------

    #[test]
    fn equal_timestamps_converge_on_both_devices() {
        let (mut mac, area) = board_with_area("mac");
        let id = mac.add_task(area, "task", None, None);
        let (mut linux, _) = board_with_area("linux");
        sync(&mac, &mut linux);
        let stamp = Timestamp::now();
        mac.update_task(id, |t| t.title = "mac".into());
        linux.update_task(id, |t| t.title = "linux".into());
        mac.tasks.get_mut(&id).unwrap().meta.updated_at = stamp;
        linux.tasks.get_mut(&id).unwrap().meta.updated_at = stamp;
        let mac_before = mac.clone();
        sync(&linux, &mut mac);
        sync(&mac_before, &mut linux);
        // "mac" > "linux", so mac's version wins everywhere.
        assert_eq!(mac.tasks[&id].title, "mac");
        assert_eq!(linux.tasks[&id].title, "mac");
    }

    /// Make `board`'s change to task `id` a second newer than `other`'s.
    fn later_than(board: &mut Board, other: &Board, id: Uuid) {
        let stamp = other.tasks[&id].meta.updated_at + jiff::SignedDuration::from_secs(1);
        board.tasks.get_mut(&id).unwrap().meta.updated_at = stamp;
    }

    #[test]
    fn an_edit_after_a_delete_brings_the_task_back() {
        let (mut mac, area) = board_with_area("mac");
        let id = mac.add_task(area, "task", None, None);
        let (mut linux, _) = board_with_area("linux");
        sync(&mac, &mut linux);
        mac.delete_task(id);
        linux.update_task(id, |t| t.title = "edited later".into());
        // Both can fall in the same clock tick; make the order explicit.
        later_than(&mut linux, &mac, id);
        sync(&linux, &mut mac);
        assert!(!mac.tasks[&id].meta.deleted);
        assert_eq!(titles(&mac, area), ["edited later"]);
    }

    #[test]
    fn a_delete_after_an_edit_wins() {
        let (mut mac, area) = board_with_area("mac");
        let id = mac.add_task(area, "task", None, None);
        let (mut linux, _) = board_with_area("linux");
        sync(&mac, &mut linux);
        linux.update_task(id, |t| t.title = "edited".into());
        mac.delete_task(id);
        later_than(&mut mac, &linux, id);
        sync(&mac, &mut linux);
        assert!(linux.tasks[&id].meta.deleted);
    }

    #[test]
    fn older_records_do_not_overwrite_newer_ones() {
        let (mut mac, area) = board_with_area("mac");
        let id = mac.add_task(area, "task", None, None);
        let old = mac.clone();
        mac.update_task(id, |t| t.title = "new".into());
        assert!(!sync(&old, &mut mac));
        assert_eq!(mac.tasks[&id].title, "new");
    }

    #[test]
    fn three_devices_end_up_the_same_in_any_order() {
        let mut devices: Vec<Board> = ["a", "b", "c"]
            .into_iter()
            .map(|d| board_with_area(d).0)
            .collect();
        let area = devices[0].areas_sorted()[0].meta.id;
        for (i, b) in devices.iter_mut().enumerate() {
            b.add_task(area, &format!("from {i}"), None, None);
            b.add_group(area, &format!("group {i}"), GROUP_COLORS[i]);
        }
        let shared = devices[0].add_task(area, "shared", None, None);
        let copy = devices[0].clone();
        sync(&copy, &mut devices[1]);
        sync(&copy, &mut devices[2]);
        devices[1].update_task(shared, |t| t.title = "b edits".into());
        devices[2].update_task(shared, |t| t.title = "c edits last".into());
        let b_edit = devices[1].clone();
        later_than(&mut devices[2], &b_edit, shared);

        let snapshots: Vec<Board> = devices.clone();
        for (i, b) in devices.iter_mut().enumerate() {
            // Each device reads the others in a different order.
            for j in [2, 0, 1].iter().map(|k| (k + i) % 3) {
                sync(&snapshots[j], b);
            }
        }
        for b in &devices[1..] {
            assert_eq!(b.tasks, devices[0].tasks);
            assert_eq!(b.groups, devices[0].groups);
            assert_eq!(b.areas, devices[0].areas);
        }
        assert_eq!(devices[0].tasks[&shared].title, "c edits last");
        assert_eq!(devices[0].priority_list(area).len(), 4);
        assert_eq!(devices[0].groups_of(area).len(), 3);
    }

    #[test]
    fn a_rename_beats_the_default_area_names() {
        let (mut mac, work) = board_with_area("mac");
        mac.rename_area(work, "Job");
        let (mut linux, _) = board_with_area("linux");
        sync(&mac, &mut linux);
        assert_eq!(linux.areas[&work].name, "Job");
        // A fresh device starting later doesn't undo it.
        let (fresh, _) = board_with_area("new");
        sync(&fresh, &mut linux);
        assert_eq!(linux.areas[&work].name, "Job");
    }

    #[test]
    fn undo_brings_back_a_deleted_task_and_group() {
        let (mut b, area) = board_with_area("mac");
        let group = b.add_group(area, "g", GROUP_COLORS[0]);
        let id = b.add_task(area, "task", Some(group), None);
        let snapshot = b.clone();
        b.delete_group(group);
        b.delete_task(id);
        b.restore(&snapshot);
        assert!(b.group(group).is_some());
        assert_eq!(titles(&b, area), ["task"]);
        // Restoring twice is harmless (it only records the restore again).
        b.restore(&snapshot);
        assert!(b.group(group).is_some());
        assert_eq!(titles(&b, area), ["task"]);
    }

    #[test]
    fn task_details_round_trip_through_the_editor() {
        let (mut b, area) = board_with_area("mac");
        let id = b.add_task(area, "task", None, Some(date(2026, 12, 1)));
        b.update_task(id, |t| {
            t.notes = "n".into();
            t.lead_days = Some(3);
            t.color = Some([1, 2, 3]);
        });
        let details = TaskDetails::from(&b.tasks[&id]);
        assert_eq!(details.title, "task");
        assert_eq!(details.notes, "n");
        assert_eq!(details.deadline, Some(date(2026, 12, 1)));
        assert_eq!(details.lead_days, Some(3));
        assert_eq!(details.color, Some([1, 2, 3]));
        let before = b.tasks[&id].clone();
        b.update_details(id, details);
        let mut after = b.tasks[&id].clone();
        after.meta = before.meta.clone();
        assert_eq!(after, before);
    }

    #[test]
    fn undo_only_restamps_records_that_really_change() {
        let mut b = Board::new("mac");
        b.today = TODAY;
        b.add_default_areas();
        let area = b.areas_sorted()[0].meta.id;
        let kept = b.add_task(area, "kept", None, None);
        let changed = b.add_task(area, "changed", None, None);
        let snapshot = b.clone();
        b.update_task(changed, |t| t.title = "renamed".into());
        let kept_stamp = b.tasks[&kept].meta.clone();

        b.restore(&snapshot);
        assert_eq!(b.tasks[&changed].title, "changed");
        assert_eq!(
            b.tasks[&kept].meta, kept_stamp,
            "unchanged: not synced again"
        );
        // Restoring the same state again changes nothing at all.
        let after = b.clone();
        b.restore(&snapshot);
        assert_eq!(b, after);
    }
}
