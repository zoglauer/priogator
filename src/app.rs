//! The user interface.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, Modifiers, Rect, RichText, Sense, UiBuilder};
use jiff::civil::Date;
use uuid::Uuid;

use crate::active::{ActiveHours, clock};
use crate::backup::{BackupFile, Backups};
use crate::board::{Board, TASK_COLORS, TaskDetails};
use crate::model::{Subtask, TaskItem};
use crate::repeat::{Repeat, Unit, WEEKDAY_BUTTONS};
use crate::storage::{ColorScheme, Config, SyncFolder, read_board_file};

pub const DEFAULT_COLUMN_WIDTH: f32 = 280.0;
const COLUMN_WIDTHS: std::ops::RangeInclusive<f32> = 200.0..=600.0;
/// Width of a folded area.
const STRIP_WIDTH: f32 = 34.0;
/// Area names, open (title) and folded (strip) alike.
const AREA_TITLE_SIZE: f32 = 17.0;
/// Widest the floating label of a dragged task gets.
const DRAG_GHOST_WIDTH: f32 = 240.0;
/// Room for the group bar at the bottom of an area: always one line.
const GROUP_BAR_HEIGHT: f32 = 34.0;
/// How often the sync folder is checked for changes from other devices.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
const UNDO_LIMIT: usize = 100;

const OVERDUE: Color32 = Color32::from_rgb(220, 70, 60);
const DUE_SOON_RGB: [u8; 3] = [230, 160, 40];
const DUE_SOON: Color32 = Color32::from_rgb(DUE_SOON_RGB[0], DUE_SOON_RGB[1], DUE_SOON_RGB[2]);
/// The standard orange's heading on a light background, picked by eye.
const DUE_SOON_HEADING_LIGHT: Color32 = Color32::from_rgb(170, 105, 0);
/// Subtask progress bar.
const PROGRESS: Color32 = Color32::from_rgb(52, 168, 83);

/// Drag-and-drop payload: the task being dragged.
struct DraggedTask(Uuid);

/// Drag-and-drop payload: the column being dragged by its header.
struct DraggedArea(Uuid);

#[derive(Clone, Copy)]
enum DropTarget {
    /// Into a priority list at this position (0 = top).
    List { area: Uuid, index: usize },
    /// Into an area's top section at this position (0 = first).
    Top { area: Uuid, index: usize },
}

/// Changes requested while drawing, applied afterwards.
enum Action {
    Select(Uuid),
    Edit(Uuid),
    SetDone(Uuid, bool),
    SetSubtaskDone(Uuid, Uuid, bool),
    Skip(Uuid),
    ToggleSubtasks(Uuid),
    MoveToTop(Uuid),
    Delete(Uuid),
    Drop(Uuid, DropTarget),
    Pin(Uuid),
    Unpin(Uuid),
    SetColor(Uuid, Option<[u8; 3]>),
    SetAreaColor(Uuid, Option<[u8; 3]>),
    SetFilter(Uuid, Option<Uuid>),
    NewGroup(Uuid),
    EditGroup(Uuid),
    NewArea,
    NewTask(Uuid),
    Unfold(Uuid),
    Fold(Uuid),
    RenameArea(Uuid),
    DeleteArea(Uuid),
    ShiftArea(Uuid, isize),
    MoveArea(Uuid, usize),
}

#[derive(Clone, Copy)]
enum History {
    Undo,
    Redo,
}

/// Where things were drawn in one column, for working out drop targets.
struct ColumnZone {
    area: Uuid,
    rect: Rect,
    /// Where the top section's cards start (below its heading).
    top_start: f32,
    /// Top of the priority section; above it is the top section.
    list_top: f32,
    /// The top section's cards as drawn (only the filtered ones, if filtered).
    top_cards: Vec<(Uuid, Rect)>,
    /// The priority cards as drawn (only the filtered ones, if filtered).
    cards: Vec<(Uuid, Rect)>,
}

/// Collected while drawing one frame.
#[derive(Default)]
struct Pass {
    actions: Vec<Action>,
    zones: Vec<ColumnZone>,
    /// The pointer is over a task card (or something on it, like its checkbox).
    pointer_on_card: bool,
}

struct TaskEditor {
    /// `None` for a new task.
    id: Option<Uuid>,
    details: TaskDetails,
    has_deadline: bool,
    deadline: Date,
    /// Text of the subtask being added.
    new_subtask: String,
    /// Put the cursor in the title field (once).
    focus_title: bool,
    /// Days before the deadline it moves to the top.
    lead_days: i64,
}

struct GroupEditor {
    /// `None` for a new group.
    id: Option<Uuid>,
    area: Uuid,
    name: String,
    color: [u8; 3],
}

struct AreaEditor {
    /// `None` for a new area.
    id: Option<Uuid>,
    name: String,
    active: Option<ActiveHours>,
    color: Option<[u8; 3]>,
}

struct BackupsDialog {
    /// Newest first, with the number of open tasks in each.
    entries: Vec<(BackupFile, Option<usize>)>,
    /// Action waiting for confirmation: what, which file, and how to call it.
    confirm: Option<(BackupAction, std::path::PathBuf, String)>,
    message: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum BackupAction {
    Restore,
    Delete,
}

pub struct PriogatorApp {
    board: Board,
    folder: SyncFolder,
    /// Local changes not yet written to disk.
    dirty: bool,
    errors: Vec<String>,
    last_scan: Instant,
    selected: Option<Uuid>,
    /// Group filter per column.
    filters: HashMap<Uuid, Uuid>,
    /// Tasks whose subtask list is unfolded on the card.
    expanded: HashSet<Uuid>,
    /// Areas opened by a click although outside their active hours; they fold
    /// again when the window loses focus.
    unfolded: HashSet<Uuid>,
    window_focused: bool,
    /// Board states before each change, newest last.
    undo: Vec<Board>,
    redo: Vec<Board>,
    /// Undo/redo requested by a button, done at the start of the next frame.
    history_request: Option<History>,
    task_editor: Option<TaskEditor>,
    group_editor: Option<GroupEditor>,
    area_editor: Option<AreaEditor>,
    /// Column waiting for delete confirmation.
    confirm_delete_area: Option<Uuid>,
    config: Config,
    backups: Backups,
    /// Day of the last daily backup check.
    last_backup: Option<Date>,
    backups_dialog: Option<BackupsDialog>,
    about_open: bool,
    about_page: AboutPage,
    settings_open: bool,
    shortcuts_open: bool,
    #[cfg(target_os = "macos")]
    updater: crate::update::Updater,
    #[cfg(target_os = "macos")]
    update_dialog: bool,
}

/// Back on the left, Close on the right.
fn about_footer(ui: &mut egui::Ui, page: &mut AboutPage, close: &mut bool) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("‹ Back").clicked() {
            *page = AboutPage::Main;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            *close = ui.button("Close").clicked();
        });
    });
}

/// What the About dialog shows.
#[derive(Clone, Copy, PartialEq)]
enum AboutPage {
    Main,
    License,
    Acknowledgements,
}

/// The projects Priogator is built on most directly, with the people behind
/// them as they name themselves.
const THANKS: [(&str, &str, &str); 10] = [
    (
        "egui & eframe",
        "Emil Ernerfeldt and contributors",
        "https://github.com/emilk/egui",
    ),
    (
        "jiff",
        "Andrew Gallant",
        "https://github.com/BurntSushi/jiff",
    ),
    (
        "serde",
        "Erick Tryzelaar, David Tolnay and contributors",
        "https://github.com/serde-rs/serde",
    ),
    (
        "winit",
        "The winit contributors",
        "https://github.com/rust-windowing/winit",
    ),
    (
        "wgpu & glow",
        "The gfx-rs developers, Joshua Groves, Dzmitry Malyshau",
        "https://github.com/gfx-rs/wgpu",
    ),
    (
        "fontique & parley",
        "The Linebender community",
        "https://github.com/linebender/parley",
    ),
    (
        "rfd",
        "Bartłomiej Maryńczak",
        "https://github.com/PolyMeilex/rfd",
    ),
    (
        "uuid",
        "Ashley Mannix, Dylan DPC, Hunar Roop Kahlon",
        "https://github.com/uuid-rs/uuid",
    ),
    (
        "directories",
        "Simon Ochsenreither",
        "https://github.com/soc/directories-rs",
    ),
    (
        "objc2",
        "Mads Marquart, Steven Sheldon",
        "https://github.com/madsmtm/objc2",
    ),
];

fn today() -> Date {
    jiff::Zoned::now().date()
}

/// A card background in the colour's hue: soft enough that group names,
/// dates and red "overdue" stay readable (saturation and brightness as they
/// appear on screen). Used by the rainbow scheme and for the preset task
/// colours.
fn soft_tint(dark: bool, color: Color32) -> Color32 {
    let mut hsva = egui::ecolor::HsvaGamma::from(color);
    if dark {
        hsva.s *= 0.5;
        hsva.v = 0.3;
    } else {
        hsva.s *= 0.2;
        hsva.v = 1.0;
    }
    hsva.into()
}

/// The preset task colours as stored and shown: soft versions of
/// [`TASK_COLORS`], for a light or dark window.
fn preset_colors(dark: bool) -> [[u8; 3]; 8] {
    TASK_COLORS.map(|c| {
        let soft = soft_tint(dark, rgb(c));
        [soft.r(), soft.g(), soft.b()]
    })
}

/// Text colour for a card whose own colour is too dark for a light window
/// (or too light for a dark one); `None` when the usual text reads fine.
fn contrast_text(dark: bool, fill: Color32) -> Option<Color32> {
    let light_fill = egui::Rgba::from(fill).intensity() > 0.25;
    match (dark, light_fill) {
        (false, false) => Some(Color32::from_gray(245)),
        (true, true) => Some(Color32::from_gray(25)),
        _ => None,
    }
}

/// Names of [`TASK_COLORS`], for tooltips and screen readers.
const TASK_COLOR_NAMES: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Teal", "Blue", "Purple", "Pink",
];

/// Pick a card colour: the preset dots, any colour (`other`), or none.
/// The colour is stored and shown exactly as picked. Returns whether it
/// changed.
fn color_choice(ui: &mut egui::Ui, color: &mut Option<[u8; 3]>, other: bool) -> bool {
    let mut changed = false;
    let outline = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let presets = preset_colors(ui.visuals().dark_mode);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (preset, name) in presets.into_iter().zip(TASK_COLOR_NAMES) {
            let selected = *color == Some(preset);
            let response = color_dot(ui, rgb(preset), outline, selected);
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, name)
            });
            if response
                .on_hover_text(name)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *color = Some(preset);
                changed = true;
            }
        }
        if other {
            ui.add_space(6.0);
            // A colour of your own: shown as a dot once chosen.
            if let Some(own) = color.filter(|c| !presets.contains(c)) {
                color_dot(ui, rgb(own), outline, true);
            }
            let button = ui.add(egui::Button::new("Other…").small());
            egui::Popup::from_toggle_button_response(&button)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    let mut picked = rgb(color.unwrap_or(presets[5]));
                    if egui::color_picker::color_picker_color32(
                        ui,
                        &mut picked,
                        egui::color_picker::Alpha::Opaque,
                    ) {
                        *color = Some([picked.r(), picked.g(), picked.b()]);
                        changed = true;
                    }
                });
        }
        ui.add_space(6.0);
        if ui
            .add_enabled(color.is_some(), egui::Button::new("None").small())
            .on_hover_text("Use the color scheme")
            .clicked()
        {
            *color = None;
            changed = true;
        }
    });
    changed
}

/// A round colour swatch with a thin outline (pale colours need it), and a
/// ring when selected or hovered.
fn color_dot(ui: &mut egui::Ui, fill: Color32, outline: Color32, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::click());
    let painter = ui.painter();
    painter.circle(rect.center(), 7.5, fill, egui::Stroke::new(1.0, outline));
    if selected {
        painter.circle_stroke(
            rect.center(),
            9.5,
            egui::Stroke::new(1.5, ui.visuals().strong_text_color()),
        );
    } else if response.hovered() {
        painter.circle_stroke(
            rect.center(),
            9.5,
            egui::Stroke::new(1.0, ui.visuals().weak_text_color()),
        );
    }
    response
}

/// The colour darkened to about two thirds of its brightness, for text in
/// that colour on a light background.
fn darker(color: Color32) -> Color32 {
    let mut hsva = egui::ecolor::HsvaGamma::from(color);
    hsva.v *= 0.72;
    hsva.into()
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

impl PriogatorApp {
    pub fn new(ctx: &egui::Context, config: Config) -> Self {
        // Labels would otherwise take the mouse for text selection,
        // which gets in the way of dragging cards.
        ctx.all_styles_mut(|s| s.interaction.selectable_labels = false);
        crate::fonts::install(ctx);
        crate::theme::install(ctx);

        let mut folder = SyncFolder::new(&config.sync_dir, &config.device);
        let (mut board, errors) = folder.load();
        board.horizon_days = config.deadline_horizon_days;
        board.done_days = config.done_days;
        let backups = Backups::new(&config.backup_dir);
        let mut app = Self {
            board,
            folder,
            dirty: false,
            errors,
            last_scan: Instant::now(),
            selected: None,
            filters: HashMap::new(),
            expanded: HashSet::new(),
            unfolded: HashSet::new(),
            window_focused: true,
            task_editor: None,
            group_editor: None,
            area_editor: None,
            confirm_delete_area: None,
            config,
            backups,
            last_backup: None,
            backups_dialog: None,
            about_open: false,
            about_page: AboutPage::Main,
            settings_open: false,
            shortcuts_open: false,
            #[cfg(target_os = "macos")]
            updater: crate::update::Updater::new(),
            #[cfg(target_os = "macos")]
            update_dialog: false,
            undo: Vec::new(),
            redo: Vec::new(),
            history_request: None,
        };
        app.daily_backup();
        app
    }

    /// Make today's backup if it doesn't exist yet (at start-up and when the
    /// date changes while running).
    fn daily_backup(&mut self) {
        let today = today();
        if self.last_backup == Some(today) {
            return;
        }
        self.last_backup = Some(today);
        if let Err(e) = self.backups.daily(&self.board, today) {
            self.errors.push(format!("Backup failed: {e}"));
        }
    }

    fn sync(&mut self) {
        if self.dirty {
            match self.folder.save(&self.board) {
                Ok(()) => self.dirty = false,
                Err(e) => self.errors = vec![format!("Saving failed: {e}")],
            }
        }
        if self.last_scan.elapsed() >= SCAN_INTERVAL {
            self.last_scan = Instant::now();
            self.daily_backup();
            let (changed, errors) = self.folder.reload_if_changed(&mut self.board);
            if changed {
                // Undoing past changes from the other device would revert them too.
                self.undo.clear();
                self.redo.clear();
            }
            if !errors.is_empty() {
                self.errors = errors;
            }
        }
    }

    fn history(&mut self, ctx: &egui::Context) {
        let mut request = self.history_request.take();
        if request.is_none() && !self.editing() && !ctx.egui_wants_keyboard_input() {
            ctx.input_mut(|i| {
                if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) {
                    request = Some(History::Redo);
                } else if i.consume_key(Modifiers::COMMAND, Key::Z) {
                    request = Some(History::Undo);
                }
            });
        }
        let (from, to) = match request {
            Some(History::Undo) => (&mut self.undo, &mut self.redo),
            Some(History::Redo) => (&mut self.redo, &mut self.undo),
            None => return,
        };
        if let Some(snapshot) = from.pop() {
            to.push(self.board.clone());
            self.board.restore(&snapshot);
            self.dirty = true;
        }
    }

    fn editing(&self) -> bool {
        self.task_editor.is_some()
            || self.group_editor.is_some()
            || self.area_editor.is_some()
            || self.confirm_delete_area.is_some()
            || self.backups_dialog.is_some()
            || self.about_open
            || self.settings_open
            || self.shortcuts_open
    }

    fn open_task_editor(&mut self, id: Uuid) {
        let Some(task) = self.board.tasks.get(&id) else {
            return;
        };
        self.task_editor = Some(TaskEditor {
            id: Some(id),
            details: TaskDetails::from(task),
            has_deadline: task.deadline.is_some(),
            deadline: task.deadline.unwrap_or_else(today),
            new_subtask: String::new(),
            focus_title: false,
            lead_days: task.lead_days.unwrap_or(self.config.deadline_horizon_days),
        });
    }

    /// The task dialog for a new task in `area`.
    fn open_new_task(&mut self, area: Uuid) {
        let group = self.filters.get(&area).copied();
        self.task_editor = Some(TaskEditor {
            id: None,
            details: TaskDetails {
                title: String::new(),
                notes: String::new(),
                area,
                group,
                deadline: None,
                pinned: false,
                subtasks: Vec::new(),
                lead_days: None,
                repeat: None,
                color: None,
            },
            has_deadline: false,
            deadline: today(),
            new_subtask: String::new(),
            focus_title: true,
            lead_days: self.config.deadline_horizon_days,
        });
    }

    fn apply(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Select(id) => {
                    self.selected = Some(id);
                    continue;
                }
                Action::Edit(id) => {
                    self.open_task_editor(id);
                    continue;
                }
                Action::SetDone(id, done) => self.board.set_done(id, done),
                Action::Skip(id) => self.board.skip(id),
                Action::SetSubtaskDone(id, subtask, done) => {
                    self.board.set_subtask_done(id, subtask, done);
                }
                Action::ToggleSubtasks(id) => {
                    if !self.expanded.remove(&id) {
                        self.expanded.insert(id);
                    }
                    continue;
                }
                Action::MoveToTop(id) => {
                    let task = &self.board.tasks[&id];
                    let area = task.area;
                    if self.board.in_top(task) {
                        self.board.move_in_top(id, area, 0);
                    } else {
                        self.board.move_task(id, area, 0);
                    }
                }
                Action::Delete(id) => self.board.delete_task(id),
                Action::Drop(id, DropTarget::List { area, index }) => {
                    self.board.move_task(id, area, index);
                }
                Action::Drop(id, DropTarget::Top { area, index }) => {
                    self.board.move_in_top(id, area, index);
                }
                Action::Pin(id) => {
                    let area = self.board.tasks[&id].area;
                    self.board.pin(id, area);
                }
                Action::SetAreaColor(id, color) => self.board.set_area_color(id, color),
                Action::SetColor(id, color) => {
                    self.board.update_task(id, |t| t.color = color);
                }
                Action::Unpin(id) => {
                    let mut details = TaskDetails::from(&self.board.tasks[&id]);
                    details.pinned = false;
                    self.board.update_details(id, details);
                }
                Action::SetFilter(area, group) => {
                    match group {
                        Some(g) => self.filters.insert(area, g),
                        None => self.filters.remove(&area),
                    };
                    continue;
                }
                Action::NewGroup(area) => {
                    self.group_editor = Some(GroupEditor {
                        id: None,
                        area,
                        name: String::new(),
                        color: self.board.next_group_color(area),
                    });
                    continue;
                }
                Action::EditGroup(id) => {
                    if let Some(g) = self.board.group(id) {
                        self.group_editor = Some(GroupEditor {
                            id: Some(id),
                            area: g.area,
                            name: g.name.clone(),
                            color: g.color,
                        });
                    }
                    continue;
                }
                Action::NewTask(area) => {
                    self.open_new_task(area);
                    continue;
                }
                Action::NewArea => {
                    self.area_editor = Some(AreaEditor {
                        id: None,
                        name: String::new(),
                        active: None,
                        color: None,
                    });
                    continue;
                }
                Action::RenameArea(id) => {
                    if let Some(a) = self.board.areas.get(&id) {
                        self.area_editor = Some(AreaEditor {
                            id: Some(id),
                            name: a.name.clone(),
                            active: a.active.clone(),
                            color: a.color,
                        });
                    }
                    continue;
                }
                Action::Unfold(id) => {
                    // Opens it for good if folded by hand; for a while if folded
                    // because it's outside its active hours.
                    self.config.folded_areas.retain(|a| *a != id);
                    self.unfolded.insert(id);
                    self.save_config();
                    continue;
                }
                Action::Fold(id) => {
                    if !self.config.folded_areas.contains(&id) {
                        self.config.folded_areas.push(id);
                    }
                    self.unfolded.remove(&id);
                    self.save_config();
                    continue;
                }
                Action::DeleteArea(id) => {
                    self.confirm_delete_area = Some(id);
                    continue;
                }
                Action::ShiftArea(id, delta) => self.board.shift_area(id, delta),
                Action::MoveArea(id, index) => self.board.move_area(id, index),
            }
            self.dirty = true;
        }
    }

    // ----- Keyboard ------------------------------------------------------

    fn keyboard(&mut self, ctx: &egui::Context) {
        if self.editing() || ctx.egui_wants_keyboard_input() {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::N)) {
            self.new_task_in_current_column();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Comma)) {
            self.settings_open = true;
        }
        let Some(id) = self.selected else {
            return;
        };
        let Some(task) = self.board.tasks.get(&id).filter(|t| !t.meta.deleted) else {
            self.selected = None;
            return;
        };
        let area = task.area;
        // Moves work within whichever section the task is in.
        let in_top = self.board.in_top(task);
        let section = if in_top {
            self.board.top_list(area)
        } else {
            self.board.priority_list(area)
        };
        let position = (!task.done)
            .then(|| section.iter().position(|t| t.meta.id == id))
            .flatten();
        let target = |index: usize| {
            if in_top {
                DropTarget::Top { area, index }
            } else {
                DropTarget::List { area, index }
            }
        };

        let (to_top, to_bottom, up, down, edit, delete, deselect) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::ALT | Modifiers::COMMAND, Key::ArrowUp),
                i.consume_key(Modifiers::ALT | Modifiers::COMMAND, Key::ArrowDown),
                i.consume_key(Modifiers::ALT, Key::ArrowUp),
                i.consume_key(Modifiers::ALT, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::COMMAND, Key::Backspace),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let mut actions = Vec::new();
        if let Some(pos) = position {
            if to_top {
                actions.push(Action::Drop(id, target(0)));
            } else if to_bottom {
                actions.push(Action::Drop(id, target(usize::MAX)));
            } else if up && pos > 0 {
                actions.push(Action::Drop(id, target(pos - 1)));
            } else if down {
                actions.push(Action::Drop(id, target(pos + 1)));
            }
        }
        if edit {
            actions.push(Action::Edit(id));
        }
        if delete {
            actions.push(Action::Delete(id));
        }
        if deselect {
            self.selected = None;
        }
        self.apply(actions);
    }

    // ----- Toolbar -------------------------------------------------------

    /// The New task dialog for the selected task's column, or the first one.
    fn new_task_in_current_column(&mut self) {
        let area = self
            .selected
            .and_then(|id| self.board.tasks.get(&id))
            .map(|t| t.area)
            .or_else(|| self.board.areas_sorted().first().map(|a| a.meta.id));
        if let Some(area) = area {
            self.open_new_task(area);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (logo, _) = ui.allocate_exact_size(egui::Vec2::splat(24.0), Sense::hover());
            paint_logo(ui.painter(), logo);
            ui.label(RichText::new("Priogator").font(crate::fonts::bold(18.0)));
            ui.add_space(8.0);
            if ui
                .button("+ New task")
                .on_hover_text("Add a task (⌘N). Or right-click an empty spot in an area.")
                .clicked()
            {
                self.new_task_in_current_column();
            }
            ui.add_space(12.0);
            self.statistics(ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.menu(ui);
                let redo = ui
                    .add_enabled(!self.redo.is_empty(), egui::Button::new("↷"))
                    .on_hover_text("Redo (⇧⌘Z)");
                let undo = ui
                    .add_enabled(!self.undo.is_empty(), egui::Button::new("↶"))
                    .on_hover_text("Undo (⌘Z)");
                #[cfg(target_os = "macos")]
                if let crate::update::State::Available(release) = self.updater.state() {
                    ui.add_space(8.0);
                    let button = egui::Button::new(
                        RichText::new(format!("Update to {}", release.version))
                            .color(Color32::WHITE),
                    )
                    .fill(ui.visuals().selection.bg_fill);
                    if ui
                        .add(button)
                        .on_hover_text("A new version of Priogator is available")
                        .clicked()
                    {
                        self.update_dialog = true;
                    }
                }
                if undo.clicked() {
                    self.history_request = Some(History::Undo);
                } else if redo.clicked() {
                    self.history_request = Some(History::Redo);
                }
                if self.history_request.is_some() {
                    ui.ctx().request_repaint();
                }
                if !self.errors.is_empty() {
                    ui.add_space(8.0);
                    let warning =
                        RichText::new(format!("⚠ {}", plural(self.errors.len(), "problem")))
                            .color(OVERDUE);
                    let response = ui
                        .add(egui::Button::new(warning).frame(false))
                        .on_hover_text(format!("{}\n\nClick to dismiss.", self.errors.join("\n")));
                    if response.clicked() {
                        self.errors.clear();
                    }
                }
            });
        });
    }

    /// "16 open · 1 overdue · 3 due within 7 days · 1 done in the last 7 days"
    fn statistics(&self, ui: &mut egui::Ui) {
        let stats = self.board.stats(today());
        let dot = || RichText::new("·").weak();
        ui.label(format!("{} open", stats.open))
            .on_hover_text("Tasks not done yet, in all areas");
        ui.label(dot());
        let overdue = RichText::new(format!("{} overdue", stats.overdue.len()));
        let size = egui::TextStyle::Body.resolve(ui.style()).size;
        ui.label(if stats.overdue.is_empty() {
            overdue
        } else {
            overdue.color(OVERDUE).font(crate::fonts::bold(size))
        })
        .on_hover_text(self.task_list_tip("Open tasks whose deadline has passed", &stats.overdue));
        ui.label(dot());
        ui.label(format!("{} due within 7 days", stats.due_this_week.len()))
            .on_hover_text(self.task_list_tip(
                "Open tasks due today or within the next 7 days",
                &stats.due_this_week,
            ));
        ui.label(dot());
        let done_days = match self.board.done_days {
            1 => "today".to_owned(),
            n => format!("in the last {n} days"),
        };
        ui.label(format!("{} done {done_days}", stats.done_recently))
            .on_hover_text("Tasks completed recently (see Settings)");
    }

    /// A tooltip: what's counted, then the tasks themselves, e.g.
    /// "Take trash out · House · Mon, Nov 2".
    fn task_list_tip(&self, what: &str, ids: &[Uuid]) -> String {
        let today = today();
        let mut tip = what.to_owned();
        if !ids.is_empty() {
            tip.push('\n');
        }
        for task in ids.iter().filter_map(|id| self.board.tasks.get(id)) {
            let area = self
                .board
                .areas
                .get(&task.area)
                .map_or("", |a| a.name.as_str());
            let when = task
                .deadline
                .map_or_else(String::new, |d| match (d - today).get_days() {
                    0 => "today".to_owned(),
                    1 => "tomorrow".to_owned(),
                    -1 => "yesterday".to_owned(),
                    _ => d.strftime("%a, %b %-d").to_string(),
                });
            tip.push_str(&format!("\n{} · {area} · {when}", task.title));
        }
        tip
    }

    fn save_config(&mut self) {
        if let Err(e) = self.config.save() {
            self.errors = vec![format!("Saving settings failed: {e}")];
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("☰", |ui| {
            if ui
                .add(egui::Button::new("New task…").shortcut_text("⌘N"))
                .clicked()
            {
                self.new_task_in_current_column();
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Settings…").shortcut_text("⌘,"))
                .clicked()
            {
                self.settings_open = true;
            }
            if ui.button("Backups…").clicked() {
                self.backups_dialog = Some(self.load_backups_dialog());
            }
            ui.separator();
            #[cfg(target_os = "macos")]
            if ui.button("Check for Updates…").clicked() {
                self.updater.check(ui.ctx());
                self.update_dialog = true;
            }
            if ui.button("Keyboard shortcuts").clicked() {
                self.shortcuts_open = true;
            }
            if ui.button("About Priogator").clicked() {
                self.about_open = true;
            }
        })
        .response
        .on_hover_text("Menu");
    }

    // ----- Columns -------------------------------------------------------

    fn columns(&self, ui: &mut egui::Ui, pass: &mut Pass) {
        let today = today();
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    let now = jiff::Zoned::now().datetime();
                    for area in self.board.areas_sorted() {
                        let folded = self.config.folded_areas.contains(&area.meta.id)
                            || area.active.as_ref().is_some_and(|a| !a.is_active(now))
                                && !self.unfolded.contains(&area.meta.id);
                        if folded {
                            self.folded_area(ui, area, pass);
                            continue;
                        }
                        // The column senses clicks below everything in it, so a
                        // right-click on an empty spot opens a new task there.
                        let column = ui
                            .scope_builder(
                                UiBuilder::new()
                                    .id_salt((area.meta.id, "column"))
                                    .sense(Sense::click()),
                                |ui| {
                                    let mut frame = egui::Frame::group(ui.style());
                                    if let Some(color) = area.color {
                                        let fill = rgb(color);
                                        // On a dark colour in a light window (or the
                                        // reverse) the column takes the other look, so
                                        // every text, button and card stays readable.
                                        let dark = ui.visuals().dark_mode;
                                        if contrast_text(dark, fill).is_some() {
                                            let other = if dark {
                                                egui::Theme::Light
                                            } else {
                                                egui::Theme::Dark
                                            };
                                            *ui.visuals_mut() =
                                                ui.ctx().style_of(other).visuals.clone();
                                        }
                                        frame = egui::Frame::group(ui.style()).fill(fill);
                                    }
                                    frame.show(ui, |ui| {
                                        let width = self.config.column_width;
                                        ui.set_width(
                                            width.clamp(
                                                *COLUMN_WIDTHS.start(),
                                                *COLUMN_WIDTHS.end(),
                                            ),
                                        );
                                        ui.set_height(ui.available_height());
                                        ui.vertical(|ui| {
                                            self.column(ui, area.meta.id, &area.name, today, pass);
                                        });
                                    })
                                },
                            )
                            .response;
                        if column.secondary_clicked() {
                            pass.actions.push(Action::NewTask(area.meta.id));
                        }
                        let rect = column.rect;
                        if let Some(zone) = pass.zones.last_mut() {
                            zone.rect = rect;
                        }
                    }
                    if ui
                        .button("+ Area")
                        .on_hover_text("Add an area, e.g. Work or Family")
                        .clicked()
                    {
                        pass.actions.push(Action::NewArea);
                    }
                });
            });
        self.drag_and_drop(ui, pass);
    }

    /// An area outside its active hours: a narrow strip with its name turned
    /// sideways and its task counts. Click to open it.
    fn folded_area(&self, ui: &mut egui::Ui, area: &crate::model::Area, pass: &mut Pass) {
        let id = area.meta.id;
        let size = egui::vec2(STRIP_WIDTH, ui.available_height());
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        // Text colours that read on the area's colour (see `column`).
        let dark = ui.visuals().dark_mode;
        let visuals = match area.color {
            Some(color) if contrast_text(dark, rgb(color)).is_some() => {
                let other = if dark {
                    egui::Theme::Light
                } else {
                    egui::Theme::Dark
                };
                ui.ctx().style_of(other).visuals.clone()
            }
            _ => ui.visuals().clone(),
        };
        let visuals = &visuals;
        let fill = match (area.color, response.hovered()) {
            (Some(color), false) => rgb(color),
            (Some(color), true) => rgb(color).gamma_multiply(0.85),
            (None, true) => visuals.widgets.hovered.weak_bg_fill,
            (None, false) => visuals.faint_bg_color,
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            6.0,
            fill,
            visuals.widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );

        // Counts at the top: open tasks, and overdue ones in red.
        let open = self.board.top_list(id).len() + self.board.priority_list(id).len();
        let overdue = self
            .board
            .tasks
            .values()
            .filter(|t| t.area == id && !t.meta.deleted && self.board.overdue(t))
            .count();
        let small = egui::TextStyle::Small.resolve(ui.style());
        let mut y = rect.top() + 12.0;
        painter.text(
            egui::pos2(rect.center().x, y),
            egui::Align2::CENTER_CENTER,
            open.to_string(),
            small.clone(),
            visuals.weak_text_color(),
        );
        if overdue > 0 {
            y += 16.0;
            painter.text(
                egui::pos2(rect.center().x, y),
                egui::Align2::CENTER_CENTER,
                overdue.to_string(),
                crate::fonts::bold(small.size),
                OVERDUE,
            );
        }

        // The name, turned 90° anticlockwise, reading upwards and ending just
        // below the counts. (The text is rotated about its top-left corner,
        // which becomes its bottom-left.)
        let galley = painter.layout_no_wrap(
            area.name.clone(),
            crate::fonts::bold(AREA_TITLE_SIZE),
            visuals.text_color(),
        );
        let size = galley.size();
        let pos = egui::pos2(rect.center().x - size.y / 2.0, y + 14.0 + size.x);
        painter.add(
            egui::epaint::TextShape::new(pos, galley, visuals.text_color())
                .with_angle(-std::f32::consts::FRAC_PI_2),
        );

        let mut tip = format!("{}: {open} open", area.name);
        if overdue > 0 {
            tip.push_str(&format!(", {overdue} overdue"));
        }
        if let Some(hours) = &area.active {
            tip.push_str(&format!("\nActive {}", hours.summary()));
        }
        if self.config.folded_areas.contains(&id) {
            tip.push_str("\nFolded by you");
        }
        tip.push_str("\nClick to open");
        let name = area.name.clone();
        response.widget_info(move || {
            egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{name} (folded)"))
        });
        let response = response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(tip);
        if response.clicked() {
            pass.actions.push(Action::Unfold(id));
        }
        let color = area.color;
        response.context_menu(|ui| area_menu(ui, id, color, true, pass));

        // Tasks dropped on the strip go to the bottom of the area's list.
        pass.zones.push(ColumnZone {
            area: id,
            rect,
            top_start: rect.top(),
            list_top: rect.top() - 1.0,
            top_cards: Vec::new(),
            cards: Vec::new(),
        });
    }

    fn column(&self, ui: &mut egui::Ui, area: Uuid, name: &str, today: Date, pass: &mut Pass) {
        let filter = self
            .filters
            .get(&area)
            .copied()
            .filter(|g| self.board.group(*g).is_some());
        let shown = |t: &&TaskItem| filter.is_none_or(|g| t.group == Some(g));
        let deadlines: Vec<_> = self
            .board
            .top_list(area)
            .into_iter()
            .filter(shown)
            .collect();
        let priorities: Vec<_> = self
            .board
            .priority_list(area)
            .into_iter()
            .filter(shown)
            .collect();
        let done: Vec<_> = self
            .board
            .done_list(area)
            .into_iter()
            .filter(shown)
            .collect();
        // The rainbow colours run over the open tasks, top section first.
        let open_count = deadlines.len() + priorities.len();

        // The header is the handle for dragging the column; it also takes
        // double-clicks (rename) and right-clicks (menu).
        let header = ui
            .scope_builder(
                UiBuilder::new()
                    .id_salt((area, "header"))
                    .sense(Sense::click_and_drag()),
                |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).font(crate::fonts::bold(AREA_TITLE_SIZE)));
                        let count = deadlines.len() + priorities.len();
                        ui.label(RichText::new(count.to_string()).weak());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            grip(ui);
                            if fold_button(ui)
                                .on_hover_text("Fold this area (click the strip to open it again)")
                                .clicked()
                            {
                                pass.actions.push(Action::Fold(area));
                            }
                        });
                    });
                },
            )
            .response
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text("Drag to move the area, double-click to edit it, right-click for more");
        if header.dragged() {
            header.dnd_set_drag_payload(DraggedArea(area));
            drag_ghost(ui.ctx(), name);
        }
        if header.double_clicked() {
            pass.actions.push(Action::RenameArea(area));
        }
        let color = self.board.areas.get(&area).and_then(|a| a.color);
        header.context_menu(|ui| area_menu(ui, area, color, false, pass));
        ui.separator();

        let mut zone = ColumnZone {
            area,
            rect: Rect::NOTHING,
            top_start: 0.0,
            list_top: 0.0,
            top_cards: Vec::new(),
            cards: Vec::new(),
        };
        let list_height = (ui.available_height() - GROUP_BAR_HEIGHT).max(0.0);
        egui::ScrollArea::vertical()
            .id_salt(area)
            .max_height(list_height)
            .min_scrolled_height(list_height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // The top section on a warm tint, so it stands apart from the
                // priority list below.
                let dark = ui.visuals().dark_mode;
                let color = self.config.due_soon_color.map_or(DUE_SOON, rgb);
                let tint = color.gamma_multiply(if dark { 0.14 } else { 0.10 });
                egui::Frame::new()
                    .fill(tint)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // (How soon is per task, so the heading names no number of days.)
                        let heading = match (dark, self.config.due_soon_color) {
                            (true, _) => color,
                            (false, None) => DUE_SOON_HEADING_LIGHT,
                            (false, Some(_)) => darker(color),
                        };
                        ui.label(RichText::new("DUE SOON · PINNED").small().color(heading));
                        zone.top_start = ui.cursor().top();
                        if deadlines.is_empty() {
                            ui.label(RichText::new("None").small().weak().italics());
                        }
                        for (i, task) in deadlines.iter().enumerate() {
                            let tint = self.card_fill(ui, task, i, open_count);
                            let rect = self.task_card(ui, task, None, tint, today, pass);
                            zone.top_cards.push((task.meta.id, rect));
                        }
                    });
                ui.add_space(10.0);
                zone.list_top = ui.cursor().top();
                ui.label(RichText::new("PRIORITIES").small().weak());
                if priorities.is_empty() {
                    ui.label(RichText::new("Drop tasks here").small().weak().italics());
                }
                for (i, task) in priorities.iter().enumerate() {
                    let tint = self.card_fill(ui, task, deadlines.len() + i, open_count);
                    let rect = self.task_card(ui, task, Some(i + 1), tint, today, pass);
                    zone.cards.push((task.meta.id, rect));
                }
                if !done.is_empty() {
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new(
                        RichText::new(done_heading(self.config.done_days, done.len()))
                            .small()
                            .weak(),
                    )
                    .id_salt((area, "done"))
                    .show(ui, |ui| {
                        for task in &done {
                            self.task_card(ui, task, None, None, today, pass);
                        }
                    });
                }
            });
        pass.zones.push(zone);

        ui.separator();
        self.group_bar(ui, area, filter, pass);
    }

    /// Group selector at the bottom of a column: filters the column.
    /// The bottom row of an area: "Groups", a dropdown for which group is
    /// visible (All, or one group), and "+ New" for a new group.
    fn group_bar(&self, ui: &mut egui::Ui, area: Uuid, filter: Option<Uuid>, pass: &mut Pass) {
        let groups = self.board.groups_of(area);
        let current = filter.and_then(|f| groups.iter().find(|g| g.meta.id == f));
        ui.horizontal(|ui| {
            ui.label(RichText::new("Groups").weak());
            let selected = match current {
                Some(g) => RichText::new(format!("● {}", g.name)).color(rgb(g.color)),
                None => RichText::new("All"),
            };
            // A menu rather than a combo box: it can hold the "Edit group" submenu.
            let mut selected = selected;
            selected = RichText::new(format!("{} ▾", selected.text()))
                .color(current.map_or(ui.visuals().text_color(), |g| rgb(g.color)));
            ui.menu_button(selected, |ui| {
                if ui.selectable_label(filter.is_none(), "All").clicked() {
                    pass.actions.push(Action::SetFilter(area, None));
                }
                for g in &groups {
                    let text = RichText::new(format!("● {}", g.name)).color(rgb(g.color));
                    if ui
                        .selectable_label(filter == Some(g.meta.id), text)
                        .clicked()
                    {
                        pass.actions.push(Action::SetFilter(area, Some(g.meta.id)));
                    }
                }
                if groups.is_empty() {
                    ui.label(RichText::new("No groups yet").weak().italics());
                } else {
                    ui.separator();
                    ui.menu_button("Edit group", |ui| {
                        for g in &groups {
                            let text = RichText::new(format!("● {}…", g.name)).color(rgb(g.color));
                            if ui.button(text).clicked() {
                                pass.actions.push(Action::EditGroup(g.meta.id));
                            }
                        }
                    });
                }
            })
            .response
            .on_hover_text("Show all tasks, or only one group's");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("+ New").on_hover_text("New group").clicked() {
                    pass.actions.push(Action::NewGroup(area));
                }
            });
        });
    }

    /// Draw one task. `position` is its place in the priority list (1 = top),
    /// `None` elsewhere. Returns the card's rectangle.
    /// The card colour for the open task at `index` (of `count`) in an area
    /// with the rainbow colours: red for the most important, through orange,
    /// yellow and green, to blue. `None` with the standard colours.
    fn rainbow(&self, ui: &egui::Ui, index: usize, count: usize) -> Option<Color32> {
        if self.config.color_scheme != ColorScheme::Rainbow {
            return None;
        }
        let t = if count > 1 {
            index as f32 / (count - 1) as f32
        } else {
            0.0
        };
        // Hue 0 is red, 0.62 blue.
        let hue = t * 0.62;
        Some(soft_tint(
            ui.visuals().dark_mode,
            egui::ecolor::HsvaGamma {
                h: hue,
                s: 1.0,
                v: 1.0,
                a: 1.0,
            }
            .into(),
        ))
    }

    /// A card's background: the task's own colour, else the colour scheme's.
    fn card_fill(
        &self,
        ui: &egui::Ui,
        task: &TaskItem,
        index: usize,
        count: usize,
    ) -> Option<Color32> {
        match task.color {
            Some(color) => Some(rgb(color)),
            None => self.rainbow(ui, index, count),
        }
    }

    fn task_card(
        &self,
        ui: &mut egui::Ui,
        task: &TaskItem,
        position: Option<usize>,
        tint: Option<Color32>,
        today: Date,
        pass: &mut Pass,
    ) -> Rect {
        let id = task.meta.id;
        let selected = self.selected == Some(id);
        let expanded = self.expanded.contains(&id);
        let group = task.group.and_then(|g| self.board.group(g));
        let stripe = group.map_or(Color32::TRANSPARENT, |g| rgb(g.color));
        let sense = if task.done {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let stroke = if selected {
            ui.visuals().selection.stroke
        } else {
            egui::Stroke::NONE
        };

        // The card senses clicks and drags *below* its checkbox.
        let response = ui
            .scope_builder(UiBuilder::new().id_salt(("task", id)).sense(sense), |ui| {
                egui::Frame::new()
                    // Solid, so cards look the same on the top section's tint.
                    .fill(tint.unwrap_or_else(|| {
                        ui.visuals().panel_fill.blend(ui.visuals().faint_bg_color)
                    }))
                    .stroke(stroke)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 6,
                        top: 4,
                        // Room for the subtask progress bar.
                        bottom: if task.subtasks.is_empty() || task.done {
                            4
                        } else {
                            8
                        },
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        // Readable text on a task's own dark (or light) colour.
                        // (Also undoes the area's text colour: cards have their own.)
                        ui.visuals_mut().override_text_color =
                            tint.and_then(|fill| contrast_text(ui.visuals().dark_mode, fill));
                        // A line for the group's name in small letters above the
                        // title; the name is drawn once the title's position is known.
                        let group_line = group.map(|_| {
                            let height = ui.text_style_height(&egui::TextStyle::Small);
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), height),
                                Sense::hover(),
                            );
                            ui.add_space(-ui.spacing().item_spacing.y);
                            rect
                        });
                        let mut title_left = None;
                        // With a deadline as well, the subtask count goes below the
                        // date, so the title keeps its width.
                        let own_subtask_row =
                            !task.subtasks.is_empty() && task.deadline.is_some() && !task.done;
                        // One row, everything centred on it: checkbox, title,
                        // and the labels on the right. With the subtask count below
                        // the date, the row is top-aligned instead, so the checkbox
                        // and the title's first line stay level with the date.
                        let align = if own_subtask_row {
                            egui::Align::Min
                        } else {
                            egui::Align::Center
                        };
                        let row = |ui: &mut egui::Ui| {
                            let mut done = task.done;
                            let checkbox = ui.add(egui::Checkbox::without_text(&mut done));
                            // Name it after the task, for VoiceOver.
                            checkbox.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::CheckBox,
                                    true,
                                    task.done,
                                    format!("Done: {}", task.title),
                                )
                            });
                            if checkbox.changed() {
                                pass.actions.push(Action::SetDone(id, done));
                            }
                            let title = RichText::new(&task.title);
                            let title = if task.done {
                                title.strikethrough().weak()
                            } else if self.board.urgent(task) {
                                let size = egui::TextStyle::Body.resolve(ui.style()).size;
                                title.font(crate::fonts::bold(size))
                            } else {
                                title
                            };
                            // Labels on the right first, so a long title wraps in
                            // the space left over instead of running under them.
                            ui.with_layout(egui::Layout::right_to_left(align), |ui| {
                                let flags = |ui: &mut egui::Ui| {
                                    if let Some(deadline) = task.deadline
                                        && !task.done
                                    {
                                        deadline_label(ui, deadline, today);
                                    }
                                    if let Some(repeat) = &task.repeat
                                        && !task.done
                                    {
                                        ui.label(RichText::new("↻").weak())
                                            .on_hover_text(repeat.summary());
                                    }
                                };
                                if own_subtask_row {
                                    // Date above the subtask count, both beside
                                    // the title.
                                    ui.with_layout(
                                        egui::Layout::top_down(egui::Align::Max),
                                        |ui| {
                                            ui.spacing_mut().item_spacing.y = 2.0;
                                            let line = egui::vec2(
                                                ui.available_width(),
                                                ui.spacing().interact_size.y,
                                            );
                                            ui.allocate_ui_with_layout(
                                                line,
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                flags,
                                            );
                                            if subtask_toggle(ui, &task.subtasks, expanded) {
                                                pass.actions.push(Action::ToggleSubtasks(id));
                                            }
                                        },
                                    );
                                } else {
                                    flags(ui);
                                    if !task.subtasks.is_empty()
                                        && subtask_toggle(ui, &task.subtasks, expanded)
                                    {
                                        pass.actions.push(Action::ToggleSubtasks(id));
                                    }
                                }
                                ui.with_layout(egui::Layout::left_to_right(align), |ui| {
                                    if own_subtask_row {
                                        // Centre the first line on the checkbox.
                                        let pad = (ui.spacing().interact_size.y
                                            - ui.text_style_height(&egui::TextStyle::Body))
                                            / 2.0;
                                        ui.vertical(|ui| {
                                            ui.add_space(pad.max(0.0));
                                            let title = ui.add(egui::Label::new(title).wrap());
                                            title_left = Some(title.rect.left());
                                        });
                                    } else {
                                        let title = ui.add(egui::Label::new(title).wrap());
                                        title_left = Some(title.rect.left());
                                    }
                                });
                            });
                        };
                        if own_subtask_row {
                            ui.horizontal_top(row);
                        } else {
                            ui.horizontal(row);
                        }
                        if let (Some(group), Some(line), Some(left)) =
                            (group, group_line, title_left)
                        {
                            ui.painter().text(
                                egui::pos2(left, line.center().y),
                                egui::Align2::LEFT_CENTER,
                                &group.name,
                                egui::TextStyle::Small.resolve(ui.style()),
                                rgb(group.color),
                            );
                        }
                        if expanded {
                            for subtask in &task.subtasks {
                                ui.horizontal(|ui| {
                                    ui.add_space(22.0);
                                    let mut done = subtask.done;
                                    let checkbox = ui.add(egui::Checkbox::without_text(&mut done));
                                    checkbox.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::Role::CheckBox,
                                            true,
                                            subtask.done,
                                            format!("Done: {}", subtask.title),
                                        )
                                    });
                                    if checkbox.changed() {
                                        pass.actions
                                            .push(Action::SetSubtaskDone(id, subtask.id, done));
                                    }
                                    let text = RichText::new(&subtask.title);
                                    let text = if subtask.done {
                                        text.strikethrough().weak()
                                    } else {
                                        text
                                    };
                                    ui.add(egui::Label::new(text).wrap());
                                });
                            }
                        }
                        if let Some(first) = task.notes.lines().find(|l| !l.trim().is_empty()) {
                            ui.add(
                                egui::Label::new(RichText::new(first).small().weak()).truncate(),
                            );
                        }
                    });
            })
            .response;
        let rect = response.rect;
        pass.pointer_on_card |= response.contains_pointer();

        // Colour stripe on the left edge for the task's group.
        ui.painter().rect_filled(
            Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.left() + 4.0, rect.bottom()),
            ),
            egui::CornerRadius {
                nw: 4,
                sw: 4,
                ne: 0,
                se: 0,
            },
            stripe,
        );
        if !task.subtasks.is_empty() && !task.done {
            paint_progress(ui, rect, &task.subtasks);
        }

        if response.dragged() {
            response.dnd_set_drag_payload(DraggedTask(id));
            // Dim the card in place and show its title at the pointer.
            ui.painter()
                .rect_filled(rect, 4.0, ui.visuals().panel_fill.gamma_multiply(0.7));
            drag_ghost(ui.ctx(), &task.title);
        }
        if response.clicked() {
            pass.actions.push(Action::Select(id));
        }
        if response.double_clicked() {
            pass.actions.push(Action::Edit(id));
        }
        response.context_menu(|ui| {
            if ui.button("Edit…").clicked() {
                pass.actions.push(Action::Edit(id));
            }
            if !task.subtasks.is_empty() {
                let label = if expanded {
                    "Hide subtasks"
                } else {
                    "Show subtasks"
                };
                if ui.button(label).clicked() {
                    pass.actions.push(Action::ToggleSubtasks(id));
                }
            }
            let first_on_top = position.is_none()
                && self
                    .board
                    .top_list(task.area)
                    .first()
                    .is_some_and(|t| t.meta.id == id);
            let can_move_up = position.map_or(!task.done && !first_on_top, |n| n > 1);
            if can_move_up && ui.button("Move to top").clicked() {
                pass.actions.push(Action::MoveToTop(id));
            }
            if task.repeat.is_some() && !task.done && ui.button("Skip this time").clicked() {
                pass.actions.push(Action::Skip(id));
            }
            if !task.done {
                if task.pinned {
                    if ui.button("Unpin").clicked() {
                        pass.actions.push(Action::Unpin(id));
                    }
                } else if !self.board.in_top(task) && ui.button("Pin to top").clicked() {
                    pass.actions.push(Action::Pin(id));
                }
            }
            ui.menu_button("Color", |ui| {
                let mut color = task.color;
                if color_choice(ui, &mut color, false) {
                    pass.actions.push(Action::SetColor(id, color));
                    ui.close();
                }
            });
            if task.done {
                if ui.button("Reopen").clicked() {
                    pass.actions.push(Action::SetDone(id, false));
                }
            } else if ui.button("Mark done").clicked() {
                pass.actions.push(Action::SetDone(id, true));
            }
            ui.separator();
            if ui.button("Delete").clicked() {
                pass.actions.push(Action::Delete(id));
            }
        });
        rect
    }

    // ----- Drag and drop -------------------------------------------------

    /// Show where a dragged task would land, and drop it on release.
    fn drag_and_drop(&self, ui: &egui::Ui, pass: &mut Pass) {
        let ctx = ui.ctx();
        if let Some(dragged) = egui::DragAndDrop::payload::<DraggedArea>(ctx) {
            Self::drop_column(ui, dragged.0, pass);
            return;
        }
        let Some(dragged) = egui::DragAndDrop::payload::<DraggedTask>(ctx) else {
            return;
        };
        let Some(pos) = ctx.pointer_interact_pos() else {
            return;
        };
        let Some(zone) = pass.zones.iter().find(|z| z.rect.contains(pos)) else {
            return;
        };
        let (target, marker) = self.drop_target(zone, dragged.0, pos);

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("drop_marker"),
        ));
        let color = ui.visuals().selection.stroke.color;
        painter.line_segment([marker.left_top(), marker.right_top()], (3.0, color));

        if ctx.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ctx);
            pass.actions.push(Action::Drop(dragged.0, target));
        }
    }

    /// Column reordering: show a vertical marker between columns and move the
    /// dragged column there on release.
    fn drop_column(ui: &egui::Ui, dragged: Uuid, pass: &mut Pass) {
        let ctx = ui.ctx();
        let Some(pos) = ctx.pointer_interact_pos() else {
            return;
        };
        let others: Vec<&ColumnZone> = pass.zones.iter().filter(|z| z.area != dragged).collect();
        let (Some(first), Some(last)) = (pass.zones.first(), others.last()) else {
            return;
        };
        let index = others
            .iter()
            .position(|z| pos.x < z.rect.center().x)
            .unwrap_or(others.len());
        let gap = ui.spacing().item_spacing.x / 2.0;
        let x = match others.get(index) {
            Some(z) => z.rect.left() - gap,
            None => last.rect.right() + gap,
        };
        let y = first.rect.y_range();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::unique("drop_marker"),
        ));
        let color = ui.visuals().selection.stroke.color;
        painter.line_segment([egui::pos2(x, y.min), egui::pos2(x, y.max)], (3.0, color));

        if ctx.input(|i| i.pointer.any_released()) {
            egui::DragAndDrop::clear_payload(ctx);
            pass.actions.push(Action::MoveArea(dragged, index));
        }
    }

    /// The drop target under `pos`, and where to draw its marker line.
    fn drop_target(&self, zone: &ColumnZone, dragged: Uuid, pos: egui::Pos2) -> (DropTarget, Rect) {
        let area = zone.area;
        let in_top = pos.y < zone.list_top;
        let (list, cards, empty_y) = if in_top {
            (
                self.board.top_list(area),
                &zone.top_cards,
                zone.top_start + 10.0,
            )
        } else {
            (
                self.board.priority_list(area),
                &zone.cards,
                zone.list_top + 20.0,
            )
        };
        // Positions are counted in the full list without the dragged task,
        // which is what `Board` expects, even when a filter hides some.
        let full: Vec<Uuid> = list
            .iter()
            .map(|t| t.meta.id)
            .filter(|t| *t != dragged)
            .collect();
        let index_of = |id: Uuid| full.iter().position(|t| *t == id).unwrap_or(full.len());
        let visible: Vec<_> = cards.iter().filter(|(t, _)| *t != dragged).collect();
        let (index, y) = match visible.iter().find(|(_, r)| pos.y < r.center().y) {
            Some((t, r)) => (index_of(*t), r.top() - 2.0),
            None => match visible.last() {
                Some((t, r)) => (index_of(*t) + 1, r.bottom() + 2.0),
                None => (full.len(), empty_y),
            },
        };
        let x = zone.rect.x_range().shrink(8.0);
        let marker = Rect::from_x_y_ranges(x, y..=y);
        let target = if in_top {
            DropTarget::Top { area, index }
        } else {
            DropTarget::List { area, index }
        };
        (target, marker)
    }

    // ----- Dialogs -------------------------------------------------------

    fn task_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.task_editor else {
            return;
        };
        let areas: Vec<(Uuid, String)> = self
            .board
            .areas_sorted()
            .iter()
            .map(|a| (a.meta.id, a.name.clone()))
            .collect();

        let mut save = false;
        let mut delete = false;
        let mut close = false;
        let is_new = editor.id.is_none();
        let modal = egui::Modal::new(egui::Id::unique("task_editor")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.horizontal(|ui| {
                ui.heading(if is_new { "New task" } else { "Edit task" });
                if !is_new {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Delete").color(OVERDUE)).clicked() {
                            delete = true;
                        }
                    });
                }
            });
            ui.add_space(6.0);
            let title = ui.add(
                egui::TextEdit::singleline(&mut editor.details.title)
                    .hint_text("What needs to be done?")
                    .font(egui::FontId::proportional(15.0))
                    .margin(egui::vec2(8.0, 6.0))
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut editor.focus_title) {
                title.request_focus();
            }
            if title.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                save = true;
            }
            ui.add_space(10.0);

            // The task's properties as a form: labels on the left, right-aligned.
            let label = |ui: &mut egui::Ui, text: &str| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(text).weak());
                });
            };
            egui::Grid::new("task_form")
                .num_columns(2)
                .spacing([12.0, 10.0])
                .min_col_width(60.0)
                .show(ui, |ui| {
                    label(ui, "Area");
                    ui.horizontal(|ui| {
                        let mut area = Some(editor.details.area);
                        area_combo(
                            ui,
                            "edit_area",
                            &areas,
                            &mut area,
                            &mut editor.details.group,
                        );
                        if let Some(area) = area {
                            editor.details.area = area;
                        }
                        if !self.board.groups_of(editor.details.area).is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Group").weak());
                            group_combo(
                                ui,
                                "edit_group",
                                &self.board,
                                editor.details.area,
                                &mut editor.details.group,
                            );
                        }
                    });
                    ui.end_row();

                    label(ui, "Deadline");
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut editor.has_deadline, "").changed()
                            && !editor.has_deadline
                        {
                            // A repeating task needs its date.
                            editor.details.repeat = None;
                        }
                        if editor.has_deadline {
                            ui.add(
                                egui_extras::DatePickerButton::new(&mut editor.deadline)
                                    .id_salt("edit_deadline"),
                            );
                            ui.add_space(8.0);
                            ui.label(RichText::new("on top").weak());
                            ui.add(egui::DragValue::new(&mut editor.lead_days).range(0..=365))
                                .on_hover_text(
                                    "How long before its deadline the task moves to the top \
                                     of its area",
                                );
                            ui.label(
                                RichText::new(if editor.lead_days == 1 {
                                    "day before"
                                } else {
                                    "days before"
                                })
                                .weak(),
                            );
                        } else {
                            ui.label(RichText::new("None").weak());
                        }
                    });
                    ui.end_row();

                    label(ui, "Repeat");
                    if repeat_kind(ui, &mut editor.details.repeat, editor.deadline)
                        && !editor.has_deadline
                    {
                        // Repeating needs a first due date.
                        editor.has_deadline = true;
                    }
                    ui.end_row();
                    if editor.details.repeat.is_some() {
                        ui.label("");
                        ui.vertical(|ui| repeat_details(ui, &mut editor.details.repeat));
                        ui.end_row();
                    }

                    label(ui, "Pin");
                    ui.checkbox(
                        &mut editor.details.pinned,
                        "Keep on top, whatever the deadline",
                    );
                    ui.end_row();

                    label(ui, "Color");
                    color_choice(ui, &mut editor.details.color, true);
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(RichText::new("NOTES").small().weak());
            ui.add(
                egui::TextEdit::multiline(&mut editor.details.notes)
                    .hint_text("Add note")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("SUBTASKS").small().weak());
            subtask_editor(ui, &mut editor.details.subtasks, &mut editor.new_subtask);

            ui.add_space(12.0);
            // One row high, buttons on the right.
            let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(
                row,
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let can_save = !editor.details.title.trim().is_empty();
                    if ui
                        .add_enabled(
                            can_save,
                            egui::Button::new(if is_new { "Add task" } else { "Save" }),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                },
            );
        });

        if save && !editor.details.title.trim().is_empty() {
            let mut details = editor.details.clone();
            details.deadline = editor.has_deadline.then_some(editor.deadline);
            // Only remember a lead time that differs from the app-wide one.
            details.lead_days = (editor.has_deadline
                && editor.lead_days != self.config.deadline_horizon_days)
                .then_some(editor.lead_days);
            let id = editor.id.unwrap_or_else(|| {
                self.board.add_task(
                    details.area,
                    &details.title,
                    details.group,
                    details.deadline,
                )
            });
            self.board.update_details(id, details);
            self.dirty = true;
            close = true;
        } else if delete && let Some(id) = editor.id {
            self.board.delete_task(id);
            self.dirty = true;
            close = true;
        }
        if close || modal.should_close() {
            self.task_editor = None;
        }
    }

    fn group_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.group_editor else {
            return;
        };
        let areas: Vec<(Uuid, String)> = self
            .board
            .areas_sorted()
            .iter()
            .map(|a| (a.meta.id, a.name.clone()))
            .collect();
        let mut save = false;
        let mut delete = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("group_editor")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.horizontal(|ui| {
                ui.heading(if editor.id.is_some() {
                    "Edit group"
                } else {
                    "New group"
                });
                if editor.id.is_some() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new("Delete").color(OVERDUE))
                            .on_hover_text("Its tasks stay, without a group")
                            .clicked()
                        {
                            delete = true;
                        }
                    });
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.color_edit_button_srgb(&mut editor.color);
                let name = ui.add(
                    egui::TextEdit::singleline(&mut editor.name)
                        .hint_text("Group name")
                        .desired_width(f32::INFINITY),
                );
                if editor.id.is_none() && editor.name.is_empty() {
                    name.request_focus();
                }
                if name.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    save = true;
                }
            });
            if editor.id.is_some() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Area").weak());
                    let mut area = Some(editor.area);
                    let mut no_group = None;
                    area_combo(ui, "group_area", &areas, &mut area, &mut no_group);
                    if let Some(area) = area {
                        editor.area = area;
                    }
                })
                .response
                .on_hover_text("Moving the group takes its tasks along");
            }
            ui.add_space(10.0);
            let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(
                row,
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui
                        .add_enabled(!editor.name.trim().is_empty(), egui::Button::new("Save"))
                        .clicked()
                    {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                },
            );
        });
        if save && !editor.name.trim().is_empty() {
            match editor.id {
                Some(id) => {
                    self.board.update_group(id, &editor.name, editor.color);
                    if self.board.group(id).is_some_and(|g| g.area != editor.area) {
                        self.filters.retain(|_, g| *g != id);
                        self.board.move_group(id, editor.area);
                    }
                }
                None => {
                    self.board
                        .add_group(editor.area, &editor.name, editor.color);
                }
            }
            self.dirty = true;
            close = true;
        } else if delete && let Some(id) = editor.id {
            self.filters.retain(|_, g| *g != id);
            self.board.delete_group(id);
            self.dirty = true;
            close = true;
        }
        if close || modal.should_close() {
            self.group_editor = None;
        }
    }

    fn confirm_delete_area(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete_area else {
            return;
        };
        let Some(area) = self.board.areas.get(&id) else {
            self.confirm_delete_area = None;
            return;
        };
        let name = area.name.clone();
        let (open, done, groups) = self.board.area_contents(id);

        let mut delete = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("confirm_delete_area")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading(format!("Delete area \"{name}\"?"));
            ui.add_space(4.0);
            let mut parts = Vec::new();
            if open > 0 {
                parts.push(plural(open, "open task"));
            }
            if done > 0 {
                parts.push(plural(done, "done task"));
            }
            if groups > 0 {
                parts.push(plural(groups, "group"));
            }
            if parts.is_empty() {
                ui.label("The area is empty.");
            } else {
                ui.label(format!("This also deletes {}.", parts.join(", ")));
            }
            ui.label(RichText::new("You can undo this with ⌘Z.").weak());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Delete").color(OVERDUE)).clicked() {
                    delete = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if delete {
            self.board.delete_area(id);
            self.filters.remove(&id);
            self.dirty = true;
            close = true;
        }
        if close || modal.should_close() {
            self.confirm_delete_area = None;
        }
    }

    // ----- Backups and About ---------------------------------------------

    fn load_backups_dialog(&self) -> BackupsDialog {
        let device = &self.board.device;
        let entries = self
            .backups
            .list()
            .into_iter()
            .map(|file| {
                let open = read_board_file(&file.path, device).ok().map(|b| {
                    b.tasks
                        .values()
                        .filter(|t| !t.done && !t.meta.deleted)
                        .count()
                });
                (file, open)
            })
            .collect();
        BackupsDialog {
            entries,
            confirm: None,
            message: None,
        }
    }

    /// Replace the board with a backup's state, after backing up the current one.
    /// Goes through `Board::restore`, so it also reaches the other device and can
    /// be undone.
    fn restore_backup(&mut self, path: &std::path::Path) -> Result<(), String> {
        let snapshot = read_board_file(path, &self.board.device)
            .map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        self.backups
            .now(&self.board)
            .map_err(|e| format!("Can't back up the current state first: {e}"))?;
        self.board.restore(&snapshot);
        self.dirty = true;
        self.selected = None;
        self.filters.clear();
        Ok(())
    }

    fn backups_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.backups_dialog else {
            return;
        };
        let mut close = false;
        let mut back_up_now = false;
        let mut delete = None;
        let mut restore = None;
        let modal = egui::Modal::new(egui::Id::unique("backups")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading("Backups");
            ui.add_space(4.0);
            ui.label(
                "A backup is made every day the app runs. Kept are all backups of \
                 the last week, then one per week for 2 months, one per month for \
                 2 years, and one per year after that.",
            );
            ui.add(
                egui::Label::new(RichText::new(short_path(self.backups.dir())).small().weak())
                    .truncate(),
            )
            .on_hover_text(self.backups.dir().display().to_string());
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(280.0)
                .show(ui, |ui| {
                    if dialog.entries.is_empty() {
                        ui.label(RichText::new("No backups yet.").weak().italics());
                    }
                    egui::Grid::new("backup_list")
                        .num_columns(4)
                        .spacing([16.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (file, open) in &dialog.entries {
                                let label = backup_label(file);
                                ui.label(&label);
                                ui.label(
                                    RichText::new(match open {
                                        Some(n) => plural(*n, "open task"),
                                        None => "unreadable".to_owned(),
                                    })
                                    .weak(),
                                );
                                if ui
                                    .add_enabled(open.is_some(), egui::Button::new("Restore…"))
                                    .clicked()
                                {
                                    dialog.confirm = Some((
                                        BackupAction::Restore,
                                        file.path.clone(),
                                        label.clone(),
                                    ));
                                }
                                if ui.button("Delete…").clicked() {
                                    dialog.confirm =
                                        Some((BackupAction::Delete, file.path.clone(), label));
                                }
                                ui.end_row();
                            }
                        });
                });
            ui.add_space(8.0);
            if let Some(message) = &dialog.message {
                ui.label(RichText::new(message).color(DUE_SOON));
            }
            ui.horizontal(|ui| {
                if ui.button("Back up now").clicked() {
                    back_up_now = true;
                }
                if ui.button("Restore from file…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("Restore Priogator backup")
                        .add_filter("Priogator data", &["json"])
                        .set_directory(self.backups.dir())
                        .pick_file()
                {
                    let label = path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    dialog.confirm = Some((BackupAction::Restore, path, label));
                }
                if ui.button("Show folder").clicked() {
                    show_folder(self.backups.dir());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        });

        if let Some((action, path, label)) = dialog.confirm.clone() {
            let (title, text, button) = match action {
                BackupAction::Restore => (
                    "Restore backup?",
                    format!(
                        "All areas and tasks will be replaced by the backup \"{label}\". \
                         The current state is saved as a backup first, and ⌘Z undoes \
                         the restore."
                    ),
                    "Restore",
                ),
                BackupAction::Delete => (
                    "Delete backup?",
                    format!("The backup \"{label}\" will be deleted. This can't be undone."),
                    "Delete",
                ),
            };
            let confirm = egui::Modal::new(egui::Id::unique("confirm_backup")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading(title);
                ui.add_space(4.0);
                ui.label(text);
                ui.add_space(8.0);
                let mut answer = None;
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(button).color(OVERDUE)).clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(false);
                    }
                });
                answer
            });
            match confirm.inner {
                Some(true) if action == BackupAction::Restore => restore = Some(path),
                Some(true) => delete = Some(path),
                Some(false) => dialog.confirm = None,
                None if confirm.should_close() => dialog.confirm = None,
                None => {}
            }
        } else if modal.should_close() {
            close = true;
        }

        if back_up_now {
            let message = match self.backups.now(&self.board) {
                Ok(_) => "Backup saved.".to_owned(),
                Err(e) => format!("Backup failed: {e}"),
            };
            self.backups_dialog = Some(BackupsDialog {
                message: Some(message),
                ..self.load_backups_dialog()
            });
        } else if let Some(path) = delete {
            let message = match self.backups.delete(&path) {
                Ok(()) => "Backup deleted.".to_owned(),
                Err(e) => format!("Deleting failed: {e}"),
            };
            self.backups_dialog = Some(BackupsDialog {
                message: Some(message),
                ..self.load_backups_dialog()
            });
        } else if let Some(path) = restore {
            match self.restore_backup(&path) {
                Ok(()) => self.backups_dialog = None,
                Err(e) => {
                    self.backups_dialog = Some(BackupsDialog {
                        message: Some(e),
                        ..self.load_backups_dialog()
                    });
                }
            }
        } else if close {
            self.backups_dialog = None;
        }
    }

    fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut changed = false;
        let mut open_backups = false;
        let mut close = false;
        let config = &mut self.config;
        let modal = egui::Modal::new(egui::Id::unique("settings")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.set_max_width(420.0);
            ui.heading("Settings");
            ui.add_space(8.0);

            ui.label(RichText::new("Display").strong());
            egui::Grid::new("settings_tasks")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Deadlines move to the top");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.deadline_horizon_days)
                                .range(0..=365)
                                .suffix(" days ahead"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Done tasks stay visible for");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.done_days)
                                .range(1..=365)
                                .suffix(" days"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Area width");
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut config.column_width)
                                    .range(COLUMN_WIDTHS)
                                    .speed(2.0)
                                    .fixed_decimals(0)
                                    .suffix(" px"),
                            )
                            .on_hover_text("Click and type, or drag sideways")
                            .changed();
                        if ui
                            .add_enabled(
                                config.column_width != DEFAULT_COLUMN_WIDTH,
                                egui::Button::new("Default").small(),
                            )
                            .clicked()
                        {
                            config.column_width = DEFAULT_COLUMN_WIDTH;
                            changed = true;
                        }
                    });
                    ui.end_row();
                    ui.label("Colors");
                    egui::ComboBox::from_id_salt("color_scheme")
                        .selected_text(config.color_scheme.name())
                        .show_ui(ui, |ui| {
                            for scheme in ColorScheme::ALL {
                                changed |= ui
                                    .selectable_value(
                                        &mut config.color_scheme,
                                        scheme,
                                        scheme.name(),
                                    )
                                    .changed();
                            }
                        })
                        .response
                        .on_hover_text(
                            "Rainbow: each area's tasks from red (most important) to blue",
                        );
                    ui.end_row();
                    ui.label("Due soon color");
                    ui.horizontal(|ui| {
                        let mut color = config.due_soon_color.unwrap_or(DUE_SOON_RGB);
                        if ui
                            .color_edit_button_srgb(&mut color)
                            .on_hover_text(
                                "Background and heading of the Due soon · pinned section",
                            )
                            .changed()
                        {
                            config.due_soon_color = Some(color);
                            changed = true;
                        }
                        if ui
                            .add_enabled(
                                config.due_soon_color.is_some(),
                                egui::Button::new("Default").small(),
                            )
                            .clicked()
                        {
                            config.due_soon_color = None;
                            changed = true;
                        }
                    });
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "Tasks due later stay in the priority list until their deadline \
                     is that close. This is the default; each task can have its own \
                     in its dialog. Older done tasks are hidden but kept in your data \
                     and backups.",
                )
                .small()
                .weak(),
            );

            ui.add_space(12.0);
            #[cfg(target_os = "macos")]
            {
                ui.label(RichText::new("Updates").strong());
                changed |= ui
                    .checkbox(
                        &mut config.check_updates,
                        "Check for updates automatically (once a day)",
                    )
                    .on_hover_text(
                        "Asks GitHub for the latest version. Nothing is installed \
                         without asking you.",
                    )
                    .changed();
                ui.add_space(12.0);
            }
            ui.label(RichText::new("Backups").strong());
            let count = self.backups.list().len();
            ui.label(format!(
                "{} kept. A backup is made every day the app runs.",
                plural(count, "backup")
            ));
            if ui.button("Manage backups…").clicked() {
                open_backups = true;
            }

            ui.add_space(12.0);
            ui.label(RichText::new("This computer").strong());
            egui::Grid::new("settings_device")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Device name");
                    ui.label(&config.device);
                    ui.end_row();
                    ui.label("Sync folder");
                    let link = RichText::new(short_path(&config.sync_dir))
                        .color(ui.visuals().hyperlink_color);
                    if ui
                        .add(egui::Label::new(link).truncate().sense(Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(format!(
                            "{}\nClick to show in the file manager",
                            config.sync_dir.display()
                        ))
                        .clicked()
                    {
                        show_folder(&config.sync_dir);
                    }
                    ui.end_row();
                });
            ui.label(
                RichText::new(
                    "Keep the sync folder identical on both computers (Syncthing or \
                     USB stick) to sync. Change these in config.json.",
                )
                .small()
                .weak(),
            );

            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Done").clicked() {
                    close = true;
                }
            });
        });
        if changed && let Err(e) = self.config.save() {
            self.errors = vec![format!("Saving settings failed: {e}")];
        }
        if open_backups {
            self.settings_open = false;
            self.backups_dialog = Some(self.load_backups_dialog());
        } else if close || modal.should_close() {
            self.settings_open = false;
        }
    }

    /// Updates: the daily check, the dialog, and the restart once a new
    /// version is installed.
    #[cfg(target_os = "macos")]
    fn update(&mut self, ctx: &egui::Context) {
        use crate::update::{RELEASES_PAGE, State};
        if self.config.check_updates {
            self.updater.check_daily(ctx);
        }
        let state = self.updater.state();
        if let State::Installed(bundle) = &state {
            // Quit (saving, see on_exit) and start the new version.
            crate::update::relaunch_after_exit(bundle);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if !self.update_dialog {
            return;
        }
        let current = env!("CARGO_PKG_VERSION");
        let mut install = None;
        let mut close = false;
        let busy = matches!(state, State::Checking | State::Installing(_));
        let modal = egui::Modal::new(egui::Id::unique("update")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Updates");
            ui.add_space(8.0);
            match &state {
                State::Idle => close = true,
                State::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Checking for updates…");
                    });
                }
                State::UpToDate => {
                    ui.label(format!("Priogator {current} is the latest version."));
                }
                State::Available(release) => {
                    ui.label(format!(
                        "Priogator {} is available. You have {current}.",
                        release.version
                    ));
                    if !release.notes.is_empty() {
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical()
                            .max_height(160.0)
                            .show(ui, |ui| ui.label(RichText::new(&release.notes).weak()));
                    }
                    ui.add_space(4.0);
                    ui.hyperlink_to("Release page", &release.page_url);
                }
                State::Installing(release) => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Downloading and installing {}…", release.version));
                    });
                    ui.label(
                        RichText::new("Priogator restarts by itself when it's done.")
                            .small()
                            .weak(),
                    );
                }
                State::Installed(_) => {
                    ui.label("Restarting…");
                }
                State::Failed(why) => {
                    ui.label("Couldn't update.");
                    ui.label(RichText::new(why).weak());
                    ui.add_space(4.0);
                    ui.hyperlink_to("Download it from the releases page", RELEASES_PAGE);
                }
            }
            ui.add_space(10.0);
            let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(
                row,
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| match &state {
                    State::Available(release) => {
                        if ui.button("Install and Restart").clicked() {
                            install = Some(release.clone());
                        }
                        if ui.button("Later").clicked() {
                            close = true;
                        }
                    }
                    State::Checking | State::Installing(_) | State::Installed(_) => {}
                    _ => {
                        if ui.button("OK").clicked() {
                            close = true;
                        }
                    }
                },
            );
        });
        if let Some(release) = install {
            let bundle = std::env::current_exe()
                .ok()
                .and_then(|exe| crate::update::bundle_of(&exe));
            match bundle {
                Some(bundle) => self.updater.install(ctx, release, bundle),
                None => self.updater.fail(
                    "This copy isn't an installed Priogator app (e.g. a development build), \
                     so it can't replace itself.",
                ),
            }
        }
        if close || (modal.should_close() && !busy) {
            self.update_dialog = false;
            // A found update stays offered in the toolbar.
            if !matches!(state, State::Available(_)) {
                self.updater.dismiss();
            }
        }
    }

    fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        const SHORTCUTS: [(&str, &str); 10] = [
            ("⌘N", "New task"),
            ("Right-click an area", "New task in that area"),
            ("Click a task", "Select it"),
            ("⌥↑ / ⌥↓", "Move the selected task up / down"),
            ("⌥⌘↑ / ⌥⌘↓", "Move it to the top / bottom"),
            ("Enter", "Edit the selected task"),
            ("⌘⌫", "Delete the selected task"),
            ("Esc", "Deselect, close a dialog, cancel a drag"),
            ("⌘Z / ⇧⌘Z", "Undo / redo"),
            ("⌘,", "Settings"),
        ];
        let modal = egui::Modal::new(egui::Id::unique("shortcuts")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Keyboard shortcuts");
            ui.add_space(8.0);
            egui::Grid::new("shortcut_list")
                .num_columns(2)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    for (keys, action) in SHORTCUTS {
                        ui.label(RichText::new(keys).strong());
                        ui.label(action);
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
            ui.label(
                RichText::new("On Linux, use Ctrl instead of ⌘.")
                    .small()
                    .weak(),
            );
            ui.add_space(8.0);
            let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(
                row,
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| ui.button("Close").clicked(),
            )
            .inner
        });
        if modal.inner || modal.should_close() {
            self.shortcuts_open = false;
        }
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut close = false;
        let mut page = self.about_page;
        let modal = egui::Modal::new(egui::Id::unique("about")).show(ctx, |ui| match page {
            AboutPage::Main => {
                ui.set_width(300.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(4.0);
                    let (logo, _) = ui.allocate_exact_size(egui::Vec2::splat(64.0), Sense::hover());
                    paint_logo(ui.painter(), logo);
                    ui.add_space(4.0);
                    ui.label(RichText::new("Priogator").font(crate::fonts::bold(24.0)));
                    ui.label("Priority tracker");
                    ui.label(
                        RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                            .small()
                            .weak(),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        "A to-do list for every area of your life, with easy re-prioritizing.",
                    );
                    ui.add_space(8.0);
                    ui.label("Designed by Andreas Zoglauer");
                    ui.label("Implemented by Claude");
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Free software under the Apache License 2.0")
                            .small()
                            .weak(),
                    );
                    ui.horizontal(|ui| {
                        // Center the two links.
                        let width = 190.0;
                        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                        if ui.link("License").clicked() {
                            page = AboutPage::License;
                        }
                        ui.label(RichText::new("·").weak());
                        if ui.link("Acknowledgements").clicked() {
                            page = AboutPage::Acknowledgements;
                        }
                    });
                    ui.add_space(8.0);
                    close = ui.button("Close").clicked();
                });
            }
            AboutPage::License => {
                ui.set_width(560.0);
                ui.heading("License");
                ui.add_space(4.0);
                ui.label(format!(
                    "Priogator {}, copyright 2026 Andreas Zoglauer, is licensed under the \
                     Apache License, Version 2.0:",
                    env!("CARGO_PKG_VERSION")
                ));
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(RichText::new(crate::licenses::OWN).monospace().small());
                    });
                about_footer(ui, &mut page, &mut close);
            }
            AboutPage::Acknowledgements => {
                ui.set_width(560.0);
                ui.heading("Acknowledgements");
                ui.add_space(4.0);
                let licenses = crate::licenses::third_party();
                egui::ScrollArea::vertical()
                    .max_height(460.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(
                            "Priogator stands on the shoulders of open-source software. \
                             Thank you to everyone who wrote and shares it, especially:",
                        );
                        ui.add_space(6.0);
                        egui::Grid::new("thanks")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                for (project, people, url) in THANKS {
                                    ui.hyperlink_to(project, url);
                                    ui.label(people);
                                    ui.end_row();
                                }
                            });
                        ui.add_space(6.0);
                        ui.label("…and the Rust project, which makes all of this possible.");
                        ui.add_space(10.0);
                        ui.label(format!(
                            "Priogator includes {} open-source libraries. Their authors ask \
                             that their licenses come with the app; here they are, each with \
                             the libraries that use it.",
                            crate::licenses::library_count(licenses)
                        ));
                        ui.add_space(6.0);
                        for (name, texts) in crate::licenses::by_name(licenses) {
                            let count: usize = texts.iter().map(|l| l.libraries.len()).sum();
                            let title = format!(
                                "{name} · {count} {}",
                                if count == 1 { "library" } else { "libraries" }
                            );
                            egui::CollapsingHeader::new(title)
                                .id_salt(name)
                                .show(ui, |ui| {
                                    for (i, license) in texts.iter().enumerate() {
                                        if i > 0 {
                                            ui.separator();
                                        }
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing.x = 0.0;
                                            ui.label(RichText::new("Used by ").weak());
                                            for (j, lib) in license.libraries.iter().enumerate() {
                                                if j > 0 {
                                                    ui.label(", ");
                                                }
                                                let label = format!("{} {}", lib.name, lib.version);
                                                if lib.repository.is_empty() {
                                                    ui.label(label);
                                                } else {
                                                    ui.hyperlink_to(label, &lib.repository);
                                                }
                                            }
                                        });
                                        ui.add_space(4.0);
                                        ui.label(RichText::new(&license.text).monospace().small());
                                    }
                                });
                        }
                    });
                about_footer(ui, &mut page, &mut close);
            }
        });
        if close || (modal.should_close() && page == self.about_page) {
            self.about_open = false;
            page = AboutPage::Main;
        }
        self.about_page = page;
    }

    fn area_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.area_editor else {
            return;
        };
        let mut save = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("area_editor")).show(ctx, |ui| {
            ui.set_width(300.0);
            ui.heading(if editor.id.is_some() {
                "Edit area"
            } else {
                "New area"
            });
            ui.add_space(4.0);
            let name = ui.add(
                egui::TextEdit::singleline(&mut editor.name)
                    .hint_text("Area name, e.g. Work")
                    .desired_width(f32::INFINITY),
            );
            if editor.id.is_none() && editor.name.is_empty() {
                name.request_focus();
            }
            if name.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                save = true;
            }
            ui.add_space(8.0);
            active_hours_editor(ui, &mut editor.active);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Color");
                color_choice(ui, &mut editor.color, true);
            });
            ui.add_space(8.0);
            let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
            ui.allocate_ui_with_layout(
                row,
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui
                        .add_enabled(!editor.name.trim().is_empty(), egui::Button::new("Save"))
                        .clicked()
                    {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                },
            );
        });
        if save && !editor.name.trim().is_empty() {
            let id = match editor.id {
                Some(id) => {
                    self.board.rename_area(id, &editor.name);
                    id
                }
                None => self.board.add_area(&editor.name),
            };
            self.board.set_active_hours(id, editor.active.clone());
            self.board.set_area_color(id, editor.color);
            self.dirty = true;
            close = true;
        }
        if close || modal.should_close() {
            self.area_editor = None;
        }
    }
}

/// "Only active at certain times": weekdays, from–to, during or outside.
fn active_hours_editor(ui: &mut egui::Ui, active: &mut Option<ActiveHours>) {
    let mut limited = active.is_some();
    if ui
        .checkbox(&mut limited, "Only active at certain times")
        .on_hover_text("Outside these times the area is folded to a narrow strip")
        .changed()
    {
        *active = limited.then(ActiveHours::default);
    }
    let Some(hours) = active else {
        return;
    };
    let indent = ui.spacing().icon_width + ui.spacing().item_spacing.x;
    ui.horizontal(|ui| {
        ui.add_space(indent);
        for (i, name) in WEEKDAY_BUTTONS.iter().enumerate() {
            let day = i as i8 + 1;
            let on = hours.weekdays.contains(&day);
            if ui.selectable_label(on, *name).clicked() {
                if on {
                    hours.weekdays.retain(|d| *d != day);
                } else {
                    hours.weekdays.push(day);
                    hours.weekdays.sort_unstable();
                }
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.label("from");
        time_field(ui, &mut hours.from);
        ui.label("to");
        time_field(ui, &mut hours.to);
    });
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.radio_value(&mut hours.outside, false, "Active during these times");
    });
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.radio_value(&mut hours.outside, true, "Active outside them");
    });
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.label(RichText::new(format!("Active {}", hours.summary())).weak());
    });
}

/// A time of day as "08:30"; drag sideways or click and type it.
fn time_field(ui: &mut egui::Ui, minutes: &mut u16) {
    let mut value = f64::from(*minutes);
    ui.add(
        egui::DragValue::new(&mut value)
            .range(0..=24 * 60 - 1)
            .speed(1.0)
            .custom_formatter(|v, _| clock(v as u16))
            .custom_parser(|text| {
                let (h, m) = text.trim().split_once(':').unwrap_or((text.trim(), "0"));
                let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
                (h < 24 && m < 60).then_some(f64::from(h * 60 + m))
            }),
    );
    *minutes = (value.round() as u16).min(24 * 60 - 1);
}

/// The ⠿ handle of an area's title bar: six dots, centred in the row (the
/// font's ⠿ sits high).
fn grip(ui: &mut egui::Ui) {
    let size = egui::vec2(10.0, ui.spacing().interact_size.y);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let color = ui.visuals().weak_text_color();
    let c = rect.center();
    for dx in [-2.5, 2.5] {
        for dy in [-5.0, 0.0, 5.0] {
            ui.painter()
                .circle_filled(c + egui::vec2(dx, dy), 1.3, color);
        }
    }
}

/// A small "‹" for folding an area, drawn so it sits exactly in the middle
/// of its row (the font's ‹ sits low).
fn fold_button(ui: &mut egui::Ui) -> egui::Response {
    let size = egui::Vec2::splat(ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Fold area"));
    let visuals = ui.style().interact(&response);
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, visuals.weak_bg_fill);
    }
    let c = rect.center();
    let (w, h) = (rect.width() * 0.14, rect.height() * 0.24);
    // As faint as the ⠿ next to it; full strength while hovered. Mixed with
    // the background into a solid colour, since a see-through one would show
    // darker where the two strokes overlap at the tip.
    let solid = |c: Color32| ui.visuals().panel_fill.blend(c);
    let color = if response.hovered() {
        solid(visuals.fg_stroke.color)
    } else {
        solid(ui.visuals().weak_text_color())
    };
    let stroke = egui::Stroke::new(2.0, color);
    ui.painter().line(
        vec![
            egui::pos2(c.x + w, c.y - h),
            egui::pos2(c.x - w, c.y),
            egui::pos2(c.x + w, c.y + h),
        ],
        stroke,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The right-click menu of an area (its title or its folded strip).
fn area_menu(ui: &mut egui::Ui, area: Uuid, color: Option<[u8; 3]>, folded: bool, pass: &mut Pass) {
    if ui.button("New task…").clicked() {
        pass.actions.push(Action::NewTask(area));
    }
    ui.separator();
    if folded {
        if ui.button("Open").clicked() {
            pass.actions.push(Action::Unfold(area));
        }
    } else if ui.button("Fold").clicked() {
        pass.actions.push(Action::Fold(area));
    }
    if ui.button("Edit area…").clicked() {
        pass.actions.push(Action::RenameArea(area));
    }
    ui.menu_button("Color", |ui| {
        let mut color = color;
        if color_choice(ui, &mut color, false) {
            pass.actions.push(Action::SetAreaColor(area, color));
            ui.close();
        }
    });
    if ui.button("Move left").clicked() {
        pass.actions.push(Action::ShiftArea(area, -1));
    }
    if ui.button("Move right").clicked() {
        pass.actions.push(Action::ShiftArea(area, 1));
    }
    ui.separator();
    if ui.button("Delete area…").clicked() {
        pass.actions.push(Action::DeleteArea(area));
    }
}

/// The "▶ 1/3" pill on a card: subtask progress, click to fold / unfold.
/// Drawn by hand, as fonts draw ▸ ▾ too small at this size.
/// Returns whether it was clicked.
fn subtask_toggle(ui: &mut egui::Ui, subtasks: &[Subtask], expanded: bool) -> bool {
    let done = subtasks.iter().filter(|s| s.done).count();
    let text_color = ui.visuals().text_color();
    let galley = ui.painter().layout_no_wrap(
        format!("{done}/{}", subtasks.len()),
        egui::TextStyle::Small.resolve(ui.style()),
        text_color,
    );
    let icon = galley.size().y * 0.6;
    let padding = egui::vec2(7.0, 2.0);
    let size = egui::vec2(icon + 4.0 + galley.size().x, galley.size().y) + padding * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let accent = ui.visuals().selection.bg_fill;
    let fill = accent.gamma_multiply(if response.hovered() { 0.6 } else { 0.35 });
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
    let icon_rect = Rect::from_center_size(
        egui::pos2(rect.left() + padding.x + icon / 2.0, rect.center().y),
        egui::Vec2::splat(icon),
    );
    let rotation = if expanded {
        0.0
    } else {
        -std::f32::consts::FRAC_PI_2
    };
    ui.painter().add(egui::Shape::rotated_triangle(
        icon_rect, rotation, text_color,
    ));
    let text_pos = egui::pos2(
        icon_rect.right() + 4.0,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter().galley(text_pos, galley, text_color);

    // Named for screen readers (and tests), e.g. "Subtasks 1/3".
    let label = format!("Subtasks {done}/{}", subtasks.len());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if expanded {
            "Hide subtasks"
        } else {
            "Show subtasks"
        })
        .clicked()
}

/// Thin bar along the bottom of a card showing how many subtasks are done.
fn paint_progress(ui: &egui::Ui, card: Rect, subtasks: &[Subtask]) {
    let done = subtasks.iter().filter(|s| s.done).count();
    let fraction = done as f32 / subtasks.len() as f32;
    let track = Rect::from_min_max(
        egui::pos2(card.left() + 10.0, card.bottom() - 4.0),
        egui::pos2(card.right() - 6.0, card.bottom() - 2.0),
    );
    let painter = ui.painter();
    painter.rect_filled(track, 1.0, ui.visuals().widgets.inactive.bg_fill);
    if done > 0 {
        let mut bar = track;
        bar.set_right(track.left() + track.width() * fraction);
        painter.rect_filled(bar, 1.0, PROGRESS);
    }
}

#[derive(PartialEq)]
enum RepeatKind {
    Never,
    Schedule,
    AfterDone,
}

/// The Repeat choice (Never / On a schedule / After it's done) and its
/// summary. Returns true when repeating was just switched on.
fn repeat_kind(ui: &mut egui::Ui, repeat: &mut Option<Repeat>, deadline: Date) -> bool {
    let kind = match repeat {
        None => RepeatKind::Never,
        Some(Repeat::Schedule { .. }) => RepeatKind::Schedule,
        Some(Repeat::AfterDone { .. }) => RepeatKind::AfterDone,
    };
    let mut new_kind = None;
    ui.horizontal(|ui| {
        let label = match kind {
            RepeatKind::Never => "Never",
            RepeatKind::Schedule => "On a schedule",
            RepeatKind::AfterDone => "After it's done",
        };
        egui::ComboBox::from_id_salt("repeat_kind")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (k, text) in [
                    (RepeatKind::Never, "Never"),
                    (RepeatKind::Schedule, "On a schedule"),
                    (RepeatKind::AfterDone, "After it's done"),
                ] {
                    if ui.selectable_label(kind == k, text).clicked() && kind != k {
                        new_kind = Some(k);
                    }
                }
            });
        if let Some(repeat) = repeat {
            ui.label(RichText::new(repeat.summary()).weak());
        }
    });
    let switched_on =
        kind == RepeatKind::Never && new_kind.as_ref().is_some_and(|k| *k != RepeatKind::Never);
    if let Some(k) = new_kind {
        *repeat = match k {
            RepeatKind::Never => None,
            RepeatKind::Schedule => Some(Repeat::Schedule {
                every: 1,
                unit: Unit::Weeks,
                weekdays: vec![deadline.weekday().to_monday_one_offset()],
            }),
            RepeatKind::AfterDone => Some(Repeat::AfterDone {
                every: 1,
                unit: Unit::Months,
            }),
        };
    }
    switched_on
}

/// How often: "Every [1] [week] on Mo … Su", or "[2] [months] after it's done".
fn repeat_details(ui: &mut egui::Ui, repeat: &mut Option<Repeat>) {
    match repeat {
        None => {}
        Some(Repeat::Schedule {
            every,
            unit,
            weekdays,
        }) => {
            ui.horizontal(|ui| {
                ui.label("Every");
                ui.add(egui::DragValue::new(every).range(1..=365));
                unit_combo(ui, "repeat_unit", unit, *every);
            });
            if *unit == Unit::Weeks {
                ui.horizontal(|ui| {
                    ui.label("on");
                    for (i, name) in WEEKDAY_BUTTONS.iter().enumerate() {
                        let day = i as i8 + 1;
                        let on = weekdays.contains(&day);
                        if ui.selectable_label(on, *name).clicked() {
                            if on {
                                weekdays.retain(|d| *d != day);
                            } else {
                                weekdays.push(day);
                                weekdays.sort_unstable();
                            }
                        }
                    }
                });
            }
        }
        Some(Repeat::AfterDone { every, unit }) => {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(every).range(1..=365));
                unit_combo(ui, "repeat_unit", unit, *every);
                ui.label("after it's done");
            });
        }
    }
}

fn unit_combo(ui: &mut egui::Ui, id_salt: &str, unit: &mut Unit, count: u32) {
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(unit.name(count))
        .width(80.0)
        .show_ui(ui, |ui| {
            for u in Unit::ALL {
                ui.selectable_value(unit, u, u.name(count));
            }
        });
}

/// The checklist in the edit dialog: tick, rename, move up, remove, add.
/// Each row is laid out from the right, so the text field takes exactly the
/// room the buttons leave.
fn subtask_editor(ui: &mut egui::Ui, subtasks: &mut Vec<Subtask>, new: &mut String) {
    let mut remove = None;
    let mut move_up = None;
    let count = subtasks.len();
    for (i, subtask) in subtasks.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let title = subtask.title.clone();
            let checkbox = ui.add(egui::Checkbox::without_text(&mut subtask.done));
            checkbox.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::CheckBox,
                    true,
                    subtask.done,
                    format!("Done: {title}"),
                )
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").on_hover_text("Remove").clicked() {
                    remove = Some(i);
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("↑").small())
                    .on_hover_text("Move up")
                    .clicked()
                {
                    move_up = Some(i);
                }
                ui.add(
                    egui::TextEdit::singleline(&mut subtask.title)
                        .desired_width(ui.available_width()),
                );
            });
        });
    }
    if let Some(i) = move_up {
        subtasks.swap(i - 1, i);
    }
    if let Some(i) = remove {
        subtasks.remove(i);
    }
    // One row high (a right-to-left layout would otherwise take all the height).
    let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(
        row,
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            let add = ui
                .add_enabled(!new.trim().is_empty(), egui::Button::new("Add").small())
                .clicked();
            let field = ui.add(
                egui::TextEdit::singleline(new)
                    .hint_text(if count == 0 {
                        "Add a subtask…"
                    } else {
                        "Add another…"
                    })
                    .desired_width(ui.available_width()),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if (enter || add) && !new.trim().is_empty() {
                subtasks.push(Subtask::new(new));
                new.clear();
                field.request_focus();
            }
        },
    );
}

/// Area picker; resets `group` when the area changes.
fn area_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    areas: &[(Uuid, String)],
    area: &mut Option<Uuid>,
    group: &mut Option<Uuid>,
) {
    let name = areas
        .iter()
        .find(|(id, _)| Some(*id) == *area)
        .map_or("", |(_, name)| name.as_str());
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(name)
        .show_ui(ui, |ui| {
            for (id, name) in areas {
                if ui.selectable_value(area, Some(*id), name).changed() {
                    *group = None;
                }
            }
        });
}

/// Group picker for the groups of `area`; hidden if it has none.
fn group_combo(
    ui: &mut egui::Ui,
    id_salt: &str,
    board: &Board,
    area: Uuid,
    group: &mut Option<Uuid>,
) {
    let groups = board.groups_of(area);
    if groups.is_empty() {
        return;
    }
    let selected = groups
        .iter()
        .find(|g| Some(g.meta.id) == *group)
        .map_or_else(
            || RichText::new("None"),
            |g| RichText::new(format!("● {}", g.name)).color(rgb(g.color)),
        );
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected)
        .show_ui(ui, |ui| {
            ui.selectable_value(group, None, "None");
            for g in &groups {
                let text = RichText::new(format!("● {}", g.name)).color(rgb(g.color));
                ui.selectable_value(group, Some(g.meta.id), text);
            }
        });
}

/// "Wed, Oct 29, 2026 · 16:41".
fn backup_label(file: &BackupFile) -> String {
    let date = file.date.strftime("%a, %b %-d, %Y");
    match file.time.get(..5) {
        Some(time) => format!("{date} · {time}"),
        None => date.to_string(),
    }
}

/// A path for display, with the home folder shown as `~`.
fn short_path(path: &std::path::Path) -> String {
    let home = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf());
    match home.and_then(|home| path.strip_prefix(home).ok().map(|p| p.to_path_buf())) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Open a folder in Finder / the Linux file manager.
fn show_folder(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    open_with_system(dir.as_os_str());
}

/// Open a folder or web address with the system's default app.
fn open_with_system(target: &std::ffi::OsStr) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(target).spawn();
}

/// Open the web links clicked this frame in the browser. (eframe could do
/// this itself, but its "links" feature brings ~30 libraries along.)
fn open_links(ctx: &egui::Context) {
    ctx.output_mut(|output| {
        output.commands.retain(|command| match command {
            egui::OutputCommand::OpenUrl(open) if open.url.starts_with("https://") => {
                open_with_system(open.url.as_ref());
                false
            }
            _ => true,
        });
    });
}

/// The app icon (see `scripts/make_icon.py`), drawn as shapes so it's sharp
/// at any size: three columns, a red deadline bar on top of each, then
/// coloured task bars.
fn paint_logo(painter: &egui::Painter, rect: Rect) {
    // The icon's layout, in its 1024-unit coordinates; the body spans 100..924.
    let scale = rect.width() / 824.0;
    let at = |x: f32, y: f32| rect.min + egui::vec2(x - 100.0, y - 100.0) * scale;
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
    let radius = |units: f32| units * scale;

    painter.rect_filled(rect, radius(185.0), Color32::from_rgb(46, 59, 89));
    let deadline = Color32::from_rgb(235, 87, 74);
    let colours = [
        Color32::from_rgb(86, 150, 255),
        Color32::from_rgb(72, 190, 120),
        Color32::from_rgb(250, 170, 60),
        Color32::from_rgb(176, 110, 220),
    ];
    let bars: [&[usize]; 3] = [&[0, 1, 0, 2], &[3, 0, 1], &[2, 3, 1, 0]];
    let (col_w, gap) = (196.0, 34.0);
    let left = (1024.0 - (3.0 * col_w + 2.0 * gap)) / 2.0;
    for (c, column) in bars.iter().enumerate() {
        let x0 = left + c as f32 * (col_w + gap);
        let x1 = x0 + col_w;
        painter.rect_filled(
            r(x0, 230.0, x1, 800.0),
            radius(30.0),
            Color32::from_white_alpha(26),
        );
        painter.rect_filled(
            r(x0 + 18.0, 252.0, x1 - 18.0, 316.0),
            radius(16.0),
            deadline,
        );
        let mut y = 366.0;
        for &i in *column {
            painter.rect_filled(
                r(x0 + 18.0, y, x1 - 18.0, y + 64.0),
                radius(16.0),
                colours[i],
            );
            y += 86.0;
        }
    }
}

/// While dragging: a small floating label with `text` at the pointer.
fn drag_ghost(ctx: &egui::Context, text: &str) {
    let Some(pos) = ctx.pointer_interact_pos() else {
        return;
    };
    egui::Area::new(egui::Id::unique("drag_ghost"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(12.0, 4.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(DRAG_GHOST_WIDTH);
                ui.label(text);
            });
        });
}

/// "1 group", "3 groups".
/// "DONE IN THE LAST 7 DAYS (2)", or "DONE TODAY (2)" for a one-day window
/// (as the toolbar says it).
fn done_heading(days: i64, count: usize) -> String {
    if days == 1 {
        format!("DONE TODAY ({count})")
    } else {
        format!("DONE IN THE LAST {days} DAYS ({count})")
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn deadline_label(ui: &mut egui::Ui, deadline: Date, today: Date) {
    let days = (deadline - today).get_days();
    let (text, color) = match days {
        d if d < 0 => (format!("{} d overdue", -d), Some(OVERDUE)),
        0 => ("today".to_owned(), Some(DUE_SOON)),
        1 => ("tomorrow".to_owned(), Some(DUE_SOON)),
        d if d <= 7 => (format!("in {d} d"), Some(DUE_SOON)),
        _ => (deadline.strftime("%b %-d").to_string(), None),
    };
    let mut text = RichText::new(text).small();
    if days <= 0 {
        // Due today or overdue.
        text = text.font(crate::fonts::bold(
            ui.style().text_styles[&egui::TextStyle::Small].size,
        ));
    }
    ui.label(match color {
        Some(c) => text.color(c),
        None => text.weak(),
    })
    .on_hover_text(deadline.strftime("%A, %B %-d, %Y").to_string());
}

impl eframe::App for PriogatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.sync();
        // Areas opened outside their active hours fold again once the window
        // loses focus.
        let focused = ui.ctx().input(|i| i.focused);
        if self.window_focused && !focused {
            self.unfolded.clear();
        }
        self.window_focused = focused;
        #[cfg(target_os = "macos")]
        if crate::macos_menu::take_request() {
            self.about_open = true;
        }
        // Which deadlines count as close; set before the undo snapshot below,
        // as this isn't a change of its own.
        self.board.today = today();
        self.board.horizon_days = self.config.deadline_horizon_days;
        self.board.done_days = self.config.done_days;
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(SCAN_INTERVAL);
        self.history(&ctx);

        // Any change made while drawing this frame becomes one undo step.
        let before = self.board.clone();
        self.keyboard(&ctx);

        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(ui, |ui| self.toolbar(ui));

        let mut pass = Pass::default();
        egui::CentralPanel::default().show(ui, |ui| self.columns(ui, &mut pass));
        // A click anywhere but on a task deselects (dialogs keep the selection).
        if !pass.pointer_on_card && !self.editing() && ctx.input(|i| i.pointer.primary_pressed()) {
            self.selected = None;
        }
        self.apply(pass.actions);

        self.task_editor(&ctx);
        self.group_editor(&ctx);
        self.area_editor(&ctx);
        self.confirm_delete_area(&ctx);
        self.settings_dialog(&ctx);
        self.backups_dialog(&ctx);
        self.about_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        #[cfg(target_os = "macos")]
        self.update(&ctx);
        open_links(&ctx);

        if self.board != before {
            self.undo.push(before);
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    // eframe passes the OpenGL context along when it draws with OpenGL, which
    // it does everywhere but on macOS (see Cargo.toml).
    #[cfg(target_os = "macos")]
    fn on_exit(&mut self) {
        self.save_on_exit();
    }

    #[cfg(not(target_os = "macos"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_on_exit();
    }
}

impl PriogatorApp {
    fn save_on_exit(&mut self) {
        if self.dirty {
            let _ = self.folder.save(&self.board);
        }
    }
}

#[cfg(test)]
mod ui_tests_app;
#[cfg(test)]
mod ui_tests_tasks;

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable as _;

    /// Settings for a temporary folder holding the example data.
    pub(super) fn demo_config(dir: &std::path::Path) -> Config {
        let config = Config {
            device: "preview".into(),
            sync_dir: dir.join("sync"),
            backup_dir: dir.join("backups"),
            deadline_horizon_days: crate::board::DEFAULT_HORIZON_DAYS,
            done_days: crate::board::DEFAULT_DONE_DAYS,
            column_width: DEFAULT_COLUMN_WIDTH,
            folded_areas: Vec::new(),
            color_scheme: ColorScheme::Standard,
            due_soon_color: None,
            // No network in tests; the update tests bring their own answer.
            check_updates: false,
            // Not saved anywhere: tests must never touch the real settings.
            file: None,
        };
        let mut folder = SyncFolder::new(&config.sync_dir, &config.device);
        let (mut board, _) = folder.load();
        crate::demo::fill(&mut board, today());
        folder.save(&board).unwrap();
        config
    }

    /// The real app in a 1280×800 test window.
    pub(super) fn harness(
        config: Config,
        theme: egui::Theme,
    ) -> egui_kittest::Harness<'static, PriogatorApp> {
        egui_kittest::Harness::builder()
            .with_size([1280.0, 800.0])
            .with_pixels_per_point(2.0)
            .with_theme(theme)
            // Same setup as the real app, fonts included (see fonts::install).
            .build_eframe(|cc| PriogatorApp::new(&cc.egui_ctx, config))
    }

    pub(super) fn click_at(harness: &mut egui_kittest::Harness<'_, PriogatorApp>, pos: egui::Pos2) {
        harness.hover_at(pos);
        harness.drag_at(pos);
        harness.drop_at(pos);
        harness.run_steps(2);
    }

    pub(super) fn right_click_at(
        harness: &mut egui_kittest::Harness<'_, PriogatorApp>,
        pos: egui::Pos2,
    ) {
        harness.hover_at(pos);
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        harness.run_steps(2);
    }

    /// Press at `from`, move to `to` in a few steps, release there.
    pub(super) fn drag(
        harness: &mut egui_kittest::Harness<'_, PriogatorApp>,
        from: egui::Pos2,
        to: egui::Pos2,
    ) {
        harness.hover_at(from);
        harness.run_steps(1);
        harness.drag_at(from);
        harness.run_steps(1);
        for step in 1..=6 {
            harness.hover_at(from + (to - from) * (step as f32 / 6.0));
            harness.run_steps(1);
        }
        harness.event(egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run_steps(2);
    }

    #[test]
    fn tasks_in_the_top_section_can_be_dragged_into_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        harness.run_steps(2);
        let work = harness.state().board.areas_sorted()[0].meta.id;
        let top = |h: &egui_kittest::Harness<'_, PriogatorApp>| -> Vec<String> {
            h.state()
                .board
                .top_list(work)
                .iter()
                .map(|t| t.title.clone())
                .collect()
        };
        assert_eq!(
            top(&harness),
            [
                "Send quarterly report",
                "Proposal for Müller GmbH",
                "Call back the bank"
            ]
        );

        let bank = harness.get_by_label("Call back the bank").rect().center();
        // The upper half of the first card (its group name sits above the title).
        let first = harness.get_by_label("Send quarterly report").rect();
        drag(&mut harness, bank, egui::pos2(bank.x, first.top() - 6.0));
        assert_eq!(
            top(&harness),
            [
                "Call back the bank",
                "Send quarterly report",
                "Proposal for Müller GmbH"
            ]
        );
    }

    #[test]
    fn areas_fold_outside_their_hours_and_after_losing_focus() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        // Work: never active, so it's folded whatever the time.
        let app = harness.state_mut();
        let work = app.board.areas_sorted()[0].meta.id;
        let never = ActiveHours {
            weekdays: Vec::new(),
            ..ActiveHours::default()
        };
        app.board.set_active_hours(work, Some(never));
        harness.run_steps(2);
        assert!(harness.query_by_label("Send quarterly report").is_none());

        harness.get_by_label("Work (folded)").click();
        harness.run_steps(2);
        assert!(
            harness.query_by_label("Send quarterly report").is_some(),
            "click opens it"
        );

        // Switching to another app folds it again.
        harness.input_mut().focused = false;
        harness.run_steps(2);
        assert!(harness.query_by_label("Send quarterly report").is_none());
        assert!(harness.query_by_label("Work (folded)").is_some());
    }

    #[test]
    fn a_group_moves_to_another_area_from_its_dialog() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        harness.run_steps(2);
        let house = harness.state().board.areas_sorted()[2].meta.id;

        // Work's group menu (the first area's) → Edit group → Clients…
        harness
            .get_all(
                egui_kittest::kittest::By::new()
                    .role(egui::accesskit::Role::Button)
                    .label("All ▾"),
            )
            .next()
            .unwrap()
            .click();
        harness.run_steps(4);
        harness.get_by_label("Edit group").hover();
        harness.run_steps(4);
        harness.get_by_label("● Clients…").click();
        harness.run_steps(2);
        assert!(harness.state().group_editor.is_some(), "the dialog opens");

        // Pick House as its area and save.
        harness.state_mut().group_editor.as_mut().unwrap().area = house;
        harness.run_steps(2);
        harness.get_by_label("Save").click();
        harness.run_steps(2);

        let board = &harness.state().board;
        let clients = board.groups.values().find(|g| g.name == "Clients").unwrap();
        assert_eq!(clients.area, house);
        assert!(
            board
                .priority_list(house)
                .iter()
                .any(|t| t.title == "Prepare kick-off workshop")
        );
    }

    #[test]
    fn any_area_can_be_folded_by_hand_and_stays_folded() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        harness.run_steps(2);
        let fold_buttons = harness.get_all_by_label("Fold area").count();
        assert_eq!(fold_buttons, 4, "one per open area");

        // Fold Work (no active hours) with its ‹ button.
        harness
            .get_all_by_label("Fold area")
            .next()
            .unwrap()
            .click();
        harness.run_steps(2);
        assert!(harness.query_by_label("Send quarterly report").is_none());
        assert!(harness.query_by_label("Work (folded)").is_some());

        // Losing focus doesn't open it; clicking the strip does.
        harness.input_mut().focused = false;
        harness.run_steps(2);
        harness.input_mut().focused = true;
        harness.run_steps(2);
        assert!(harness.query_by_label("Work (folded)").is_some());
        harness.get_by_label("Work (folded)").click();
        harness.run_steps(2);
        assert!(harness.query_by_label("Send quarterly report").is_some());
        assert!(harness.state().config.folded_areas.is_empty());
    }

    #[test]
    fn right_click_on_a_column_opens_a_new_task_there() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        harness.run_steps(2);
        let house = harness.state().board.areas_sorted()[2].meta.id;

        // On a task: its own menu, not the new-task dialog.
        let card = harness.get_by_label("Fix dripping tap").rect().center();
        right_click_at(&mut harness, card);
        assert!(harness.state().task_editor.is_none());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);

        // Below the tasks, in the same column.
        right_click_at(&mut harness, egui::pos2(card.x, 700.0));
        let editor = harness.state().task_editor.as_ref().expect("dialog opens");
        assert!(editor.id.is_none(), "it's a new task");
        assert_eq!(editor.details.area, house);

        // Typing a title and pressing Enter creates the task in that column.
        harness.event(egui::Event::Text("Paint fence".into()));
        harness.run_steps(2);
        harness.key_press(egui::Key::Enter);
        harness.run_steps(2);
        assert!(harness.state().task_editor.is_none());
        let titles: Vec<String> = harness
            .state()
            .board
            .priority_list(house)
            .iter()
            .map(|t| t.title.clone())
            .collect();
        assert_eq!(titles.last().map(String::as_str), Some("Paint fence"));
    }

    #[test]
    fn clicking_elsewhere_deselects_the_task() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        harness.run_steps(2);

        let card = harness.get_by_label("Read team feedback").rect().center();
        click_at(&mut harness, card);
        assert!(
            harness.state().selected.is_some(),
            "clicking a task selects it"
        );

        // Its checkbox is part of the card: the selection stays.
        let checkbox = harness
            .get_by_label("Done: Read team feedback")
            .rect()
            .center();
        click_at(&mut harness, checkbox);
        assert!(harness.state().selected.is_some());
        // (Ticking it moved the task to DONE; put it back where it was.)
        let id = harness.state().selected.unwrap();
        harness.state_mut().board.set_done(id, false);
        harness.run_steps(2);

        // The toolbar's statistics, then empty space in a column.
        let stats = harness.get_by_label_contains(" open").rect().center();
        click_at(&mut harness, stats);
        assert!(
            harness.state().selected.is_none(),
            "clicking the toolbar deselects"
        );
        click_at(&mut harness, card);
        click_at(&mut harness, egui::pos2(card.x, 700.0));
        assert!(
            harness.state().selected.is_none(),
            "clicking an empty column deselects"
        );
    }

    /// Screenshots of the whole window with example data, in light and dark mode.
    /// `APP_PREVIEW=/tmp/app cargo test app_preview -- --ignored` writes
    /// `/tmp/app-light.png` and `/tmp/app-dark.png`.
    #[test]
    #[ignore]
    fn app_preview() {
        let prefix = std::env::var("APP_PREVIEW").expect("set APP_PREVIEW to an output prefix");
        let dir = tempfile::tempdir().unwrap();
        let config = demo_config(dir.path());
        // A backup with an old-style name (no time), next to today's.
        let old = crate::storage::read_board_file(
            &config.sync_dir.join("device-preview.json"),
            "preview",
        )
        .unwrap();
        crate::storage::write_board_file(&config.backup_dir.join("backup-2026-10-20.json"), &old)
            .unwrap();

        for (theme, name) in [(egui::Theme::Light, "light"), (egui::Theme::Dark, "dark")] {
            let mut harness = harness(config.clone(), theme);
            harness.run_steps(4);
            let image = harness.render().expect("render");
            image.save(format!("{prefix}-{name}.png")).expect("save");

            harness.state_mut().config.color_scheme = ColorScheme::Rainbow;
            harness.run_steps(4);
            let image = harness.render().expect("render");
            image
                .save(format!("{prefix}-{name}-rainbow.png"))
                .expect("save");
            harness.state_mut().config.color_scheme = ColorScheme::Standard;
            harness.state_mut().config.due_soon_color = Some([70, 130, 230]);
            harness.run_steps(4);
            let image = harness.render().expect("render");
            image
                .save(format!("{prefix}-{name}-due-color.png"))
                .expect("save");
            harness.state_mut().config.due_soon_color = None;
            harness.run_steps(4);

            // A few tasks with their own colour, over the rainbow.
            let app = harness.state_mut();
            app.config.color_scheme = ColorScheme::Rainbow;
            let colored: Vec<(Uuid, [u8; 3])> = [
                (
                    "Sketch landing page",
                    preset_colors(theme == egui::Theme::Dark)[6],
                ),
                (
                    "Fix dripping tap",
                    preset_colors(theme == egui::Theme::Dark)[5],
                ),
                // A strong colour of one's own, with light text on it.
                ("Call back the bank", [30, 70, 160]),
            ]
            .iter()
            .map(|(title, color)| {
                let id = app
                    .board
                    .tasks
                    .values()
                    .find(|t| t.title == *title)
                    .unwrap()
                    .meta
                    .id;
                (id, *color)
            })
            .collect();
            for (id, color) in &colored {
                app.board.update_task(*id, |t| t.color = Some(*color));
            }
            harness.run_steps(4);
            let image = harness.render().expect("render");
            image
                .save(format!("{prefix}-{name}-task-colors.png"))
                .expect("save");
            if theme == egui::Theme::Light {
                let card = harness.get_by_label("Sketch landing page").rect().center();
                right_click_at(&mut harness, card);
                harness.get_by_label("Color").hover();
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-color-menu.png"))
                    .expect("save");
                harness.key_press(egui::Key::Escape);
                harness.run_steps(2);
                let (id, _) = colored[0];
                harness.state_mut().open_task_editor(id);
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-color-dialog.png"))
                    .expect("save");
                harness.get_by_label("Other…").click();
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-color-other.png"))
                    .expect("save");
                harness.key_press(egui::Key::Escape);
                harness.run_steps(2);
                harness.state_mut().task_editor = None;
            }
            // Areas with their own colour: a soft one, and a dark one with
            // light text (cards keep theirs).
            let app = harness.state_mut();
            let areas: Vec<Uuid> = app.board.areas_sorted().iter().map(|a| a.meta.id).collect();
            app.board
                .set_area_color(areas[1], Some(preset_colors(theme == egui::Theme::Dark)[3]));
            app.board.set_area_color(areas[3], Some([40, 50, 90]));
            harness.run_steps(4);
            let image = harness.render().expect("render");
            image
                .save(format!("{prefix}-{name}-area-colors.png"))
                .expect("save");
            harness.state_mut().config.folded_areas.push(areas[3]);
            harness.run_steps(4);
            let image = harness.render().expect("render");
            image
                .save(format!("{prefix}-{name}-area-folded.png"))
                .expect("save");
            harness.state_mut().config.folded_areas.clear();
            let app = harness.state_mut();
            app.board.set_area_color(areas[1], None);
            app.board.set_area_color(areas[3], None);
            let app = harness.state_mut();
            for (id, _) in &colored {
                app.board.update_task(*id, |t| t.color = None);
            }
            app.config.color_scheme = ColorScheme::Standard;
            harness.run_steps(4);

            if theme == egui::Theme::Light {
                let due = harness.get_by_label_contains("due within").rect().center();
                harness.hover_at(due);
                harness.run_steps(60);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-due-tip.png")).expect("save");
                harness.hover_at(egui::pos2(640.0, 700.0));
                harness.run_steps(2);
            }

            if theme == egui::Theme::Light {
                let app = harness.state_mut();
                let workshop = app
                    .board
                    .tasks
                    .values()
                    .find(|t| t.title == "Prepare kick-off workshop")
                    .map(|t| t.meta.id)
                    .unwrap();
                app.expanded.insert(workshop);
                app.open_task_editor(workshop);
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-subtasks.png")).expect("save");

                let app = harness.state_mut();
                app.expanded.clear();
                let trash = app
                    .board
                    .tasks
                    .values()
                    .find(|t| t.title == "Take out the trash")
                    .map(|t| t.meta.id)
                    .unwrap();
                app.open_task_editor(trash);
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-repeat.png")).expect("save");

                let app = harness.state_mut();
                let house = app.board.areas_sorted()[2].meta.id;
                app.open_new_task(house);
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-new-task.png")).expect("save");

                // An update found: the toolbar button and the dialog.
                #[cfg(target_os = "macos")]
                {
                    let json = r#"{"tag_name": "v1.1.0", "html_url": "https://example.com",
                        "body": "Area colours and a new Done heading.",
                        "assets": [{"browser_download_url": "https://example.com/P.dmg"}]}"#;
                    let ctx = harness.ctx.clone();
                    let app = harness.state_mut();
                    app.task_editor = None;
                    app.updater =
                        crate::update::Updater::with_fetch(std::sync::Arc::new(move || {
                            Ok(json.to_owned())
                        }));
                    app.updater.check(&ctx);
                    app.update_dialog = true;
                    for _ in 0..100 {
                        harness.run_steps(1);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    let image = harness.render().expect("render");
                    image.save(format!("{prefix}-update.png")).expect("save");
                    harness.state_mut().update_dialog = false;
                    harness.run_steps(4);
                }

                let app = harness.state_mut();
                app.task_editor = None;
                app.about_open = true;
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-about.png")).expect("save");
                for (page, name) in [
                    (AboutPage::License, "license"),
                    (AboutPage::Acknowledgements, "thanks"),
                ] {
                    harness.state_mut().about_page = page;
                    harness.run_steps(4);
                    let image = harness.render().expect("render");
                    image
                        .save(format!("{prefix}-about-{name}.png"))
                        .expect("save");
                }
                harness.get_by_label_contains("MIT License ·").click();
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-about-thanks-open.png"))
                    .expect("save");
                harness.state_mut().about_page = AboutPage::Main;

                // An area with more groups than fit on one line.
                let app = harness.state_mut();
                app.about_open = false;
                let house = app.board.areas_sorted()[2].meta.id;
                for name in ["Kitchen", "Cellar", "Windows", "Heating"] {
                    let color = app.board.next_group_color(house);
                    app.board.add_group(house, name, color);
                }
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-groups.png")).expect("save");
                // House's group menu, opened.
                harness
                    .get_all(
                        egui_kittest::kittest::By::new()
                            .role(egui::accesskit::Role::Button)
                            .label("All ▾"),
                    )
                    .nth(2)
                    .unwrap()
                    .click();
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-groups-open.png"))
                    .expect("save");
                harness.key_press(egui::Key::Escape);
                harness.run_steps(2);
                let app = harness.state_mut();
                let windows = app
                    .board
                    .groups
                    .values()
                    .find(|g| g.name == "Windows")
                    .map(|g| g.meta.id)
                    .unwrap();
                app.filters.insert(house, windows);
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-groups-filtered.png"))
                    .expect("save");
                harness.state_mut().filters.clear();
                harness.run_steps(2);
                let app = harness.state_mut();
                let clients = app
                    .board
                    .groups
                    .values()
                    .find(|g| g.name == "Clients")
                    .map(|g| g.meta.id)
                    .unwrap();
                app.apply(vec![Action::EditGroup(clients)]);
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-group-dialog.png"))
                    .expect("save");
                harness.state_mut().group_editor = None;
                harness.run_steps(2);

                // Two areas folded, and the dialog for active hours.
                let app = harness.state_mut();
                app.about_open = false;
                let areas: Vec<Uuid> = app.board.areas_sorted().iter().map(|a| a.meta.id).collect();
                let never = ActiveHours {
                    weekdays: Vec::new(),
                    ..ActiveHours::default()
                };
                app.board.set_active_hours(areas[1], Some(never.clone()));
                app.board.set_active_hours(areas[3], Some(never));
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-folded.png")).expect("save");
                let app = harness.state_mut();
                app.area_editor = Some(AreaEditor {
                    id: Some(areas[0]),
                    name: "Work".into(),
                    active: Some(ActiveHours::default()),
                    color: None,
                });
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-area-hours.png"))
                    .expect("save");
                let app = harness.state_mut();
                app.area_editor = None;
                app.board.set_active_hours(areas[1], None);
                app.board.set_active_hours(areas[3], None);

                let app = harness.state_mut();
                app.shortcuts_open = true;
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-shortcuts.png")).expect("save");

                let app = harness.state_mut();
                app.shortcuts_open = false;
                app.settings_open = true;
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-settings.png")).expect("save");

                harness.state_mut().settings_open = false;
                harness.run_steps(2);
                harness.get_by_label("☰").click();
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-menu.png")).expect("save");
                harness.key_press(egui::Key::Escape);
                harness.run_steps(2);

                let app = harness.state_mut();
                app.backups_dialog = Some(app.load_backups_dialog());
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image.save(format!("{prefix}-backups.png")).expect("save");

                let dialog = harness.state_mut().backups_dialog.as_mut().unwrap();
                let (file, _) = &dialog.entries[0];
                dialog.confirm =
                    Some((BackupAction::Delete, file.path.clone(), backup_label(file)));
                harness.run_steps(4);
                let image = harness.render().expect("render");
                image
                    .save(format!("{prefix}-backup-delete.png"))
                    .expect("save");
            }
        }
    }

    #[test]
    fn a_task_gets_its_colour_from_the_context_menu() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        harness.run_steps(4);
        let find = |h: &egui_kittest::Harness<'_, PriogatorApp>| {
            h.state()
                .board
                .tasks
                .values()
                .find(|t| t.title == "Sketch landing page")
                .map(|t| (t.meta.id, t.color))
                .unwrap()
        };
        let card = harness.get_by_label("Sketch landing page").rect().center();
        right_click_at(&mut harness, card);
        harness.get_by_label("Color").hover();
        harness.run_steps(4);
        harness.get_by_label("Purple").click();
        harness.run_steps(4);
        assert_eq!(find(&harness).1, Some(preset_colors(false)[6]));
        assert!(harness.query_by_label("Purple").is_none(), "menu closed");

        // And back to none in the task dialog.
        let (id, _) = find(&harness);
        harness.state_mut().open_task_editor(id);
        harness.run_steps(4);
        harness
            .get_by_role_and_label(egui::Role::Button, "None")
            .click();
        harness.run_steps(2);
        harness.get_by_label("Save").click();
        harness.run_steps(4);
        assert_eq!(find(&harness).1, None);
    }

    #[test]
    fn an_area_gets_its_colour_from_its_menu() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
        harness.run_steps(4);
        let work = harness.state().board.areas_sorted()[0].meta.id;
        // Right-click the area's header, next to its name.
        let name = harness.get_all_by_label("Work").next().unwrap().rect();
        right_click_at(
            &mut harness,
            egui::pos2(name.right() + 60.0, name.center().y),
        );
        harness.get_by_label("Color").hover();
        harness.run_steps(4);
        harness.get_by_label("Green").click();
        harness.run_steps(4);
        assert_eq!(
            harness.state().board.areas[&work].color,
            Some(preset_colors(false)[3])
        );
        assert!(harness.query_by_label("Green").is_none(), "menu closed");
    }
}
