//! UI tests: the real app in a test window, driven like a user would.

use super::tests::{click_at, demo_config, drag, harness, right_click_at};
use super::*;
use egui_kittest::kittest::{By, NodeT as _, Queryable as _};

type H = egui_kittest::Harness<'static, PriogatorApp>;

/// The app with the example data, in a temporary folder.
fn app(dir: &tempfile::TempDir) -> H {
    let mut h = harness(demo_config(dir.path()), egui::Theme::Light);
    h.run_steps(2);
    h
}

/// The (only) task with this title.
fn id_of(h: &H, title: &str) -> Uuid {
    h.state()
        .board
        .tasks
        .values()
        .find(|t| t.title == title && !t.meta.deleted)
        .unwrap_or_else(|| panic!("no task {title:?}"))
        .meta
        .id
}

fn task<'a>(h: &'a H, title: &str) -> &'a TaskItem {
    &h.state().board.tasks[&id_of(h, title)]
}

/// The n-th area from the left: Work, Private ventures, House, Family.
fn area(h: &H, n: usize) -> Uuid {
    h.state().board.areas_sorted()[n].meta.id
}

fn titles(list: Vec<&TaskItem>) -> Vec<String> {
    list.iter().map(|t| t.title.clone()).collect()
}

fn priorities(h: &H, area: Uuid) -> Vec<String> {
    titles(h.state().board.priority_list(area))
}

fn top(h: &H, area: Uuid) -> Vec<String> {
    titles(h.state().board.top_list(area))
}

fn type_text(h: &mut H, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.run_steps(2);
}

fn press(h: &mut H, modifiers: egui::Modifiers, key: egui::Key) {
    h.key_press_modifiers(modifiers, key);
    h.run_steps(2);
}

/// Click a task's card (on its title).
fn select(h: &mut H, title: &str) {
    let pos = h.get_by_label(title).rect().center();
    click_at(h, pos);
}

/// Right-click a task's card and choose `item` from its menu.
fn card_menu(h: &mut H, title: &str, item: &str) {
    let pos = h.get_by_label(title).rect().center();
    right_click_at(h, pos);
    button(h, item).click();
    h.run_steps(2);
}

fn button<'a>(h: &'a H, label: &'a str) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(egui::Role::Button, label)
}

fn disabled(h: &H, label: &str) -> bool {
    button(h, label).accesskit_node().is_disabled()
}

fn editor(h: &H) -> &TaskEditor {
    h.state()
        .task_editor
        .as_ref()
        .expect("the task dialog is open")
}

// ----- New tasks -----------------------------------------------------------

#[test]
fn the_toolbar_button_adds_a_task_at_the_bottom_of_the_first_area() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    h.get_by_label("+ New task").click();
    h.run_steps(2);
    assert!(editor(&h).id.is_none(), "a new task");
    assert_eq!(editor(&h).details.area, work);

    type_text(&mut h, "Write minutes");
    press(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    assert!(h.state().task_editor.is_none(), "Enter saves and closes");
    assert_eq!(
        priorities(&h, work).last().map(String::as_str),
        Some("Write minutes")
    );
    assert!(
        h.query_by_label("Write minutes").is_some(),
        "shown on a card"
    );
}

#[test]
fn command_n_adds_a_task_in_the_selected_tasks_area() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let house = area(&h, 2);
    select(&mut h, "Fix dripping tap");
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::N);
    assert_eq!(editor(&h).details.area, house);
    type_text(&mut h, "Oil the hinges");
    button(&h, "Add task").click();
    h.run_steps(2);
    assert_eq!(
        priorities(&h, house).last().map(String::as_str),
        Some("Oil the hinges")
    );
}

#[test]
fn a_task_needs_a_title() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let before = h.state().board.tasks.len();
    h.get_by_label("+ New task").click();
    h.run_steps(2);
    assert!(disabled(&h, "Add task"));
    // Only spaces: still nothing to add.
    type_text(&mut h, "   ");
    assert!(disabled(&h, "Add task"));
    press(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    assert_eq!(h.state().board.tasks.len(), before);
    assert!(h.state().task_editor.is_some(), "the dialog stays open");
    // (Enter took the focus off the title; click back into it.)
    h.get(By::new().role(egui::Role::TextInput).value("   "))
        .click();
    h.run_steps(2);
    type_text(&mut h, "x");
    assert!(!disabled(&h, "Add task"));
}

#[test]
fn cancel_and_escape_discard_a_new_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let before = h.state().board.tasks.len();

    h.get_by_label("+ New task").click();
    h.run_steps(2);
    type_text(&mut h, "Never mind");
    button(&h, "Cancel").click();
    h.run_steps(2);
    assert!(h.state().task_editor.is_none());

    h.get_by_label("+ New task").click();
    h.run_steps(2);
    type_text(&mut h, "Never mind");
    press(&mut h, egui::Modifiers::NONE, egui::Key::Escape);
    assert!(h.state().task_editor.is_none());

    assert_eq!(h.state().board.tasks.len(), before);
    assert!(h.query_by_label("Never mind").is_none());
}

#[test]
fn a_new_task_gets_the_group_the_area_is_filtered_to() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    // Work's groups menu: show only Clients.
    h.get_all(By::new().role(egui::Role::Button).label("All ▾"))
        .next()
        .unwrap()
        .click();
    h.run_steps(4);
    h.get_by_label("● Clients").click();
    h.run_steps(2);

    h.get_by_label("+ New task").click();
    h.run_steps(2);
    let clients = h
        .state()
        .board
        .groups
        .values()
        .find(|g| g.name == "Clients")
        .unwrap()
        .meta
        .id;
    assert_eq!(editor(&h).details.group, Some(clients));
    type_text(&mut h, "Send offer");
    press(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    assert_eq!(task(&h, "Send offer").group, Some(clients));
    assert_eq!(task(&h, "Send offer").area, work);
}

// ----- Editing -------------------------------------------------------------

/// Open a task's dialog by double-clicking its card.
fn open(h: &mut H, title: &str) {
    let pos = h.get_by_label(title).rect().center();
    // Each test step is a quarter second, slower than a real double-click
    // (and a second apart from any earlier click, so it's not a triple one).
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 0.6);
    h.hover_at(pos);
    h.run_steps(4);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(1);
        }
    }
    h.run_steps(2);
    assert!(
        h.state().task_editor.is_some(),
        "double-click opens {title:?}"
    );
}

fn save(h: &mut H) {
    // Let the dialog settle first: it re-centres when its content changes.
    h.run_steps(3);
    button(h, "Save").click();
    h.run_steps(2);
    assert!(h.state().task_editor.is_none(), "Save closes the dialog");
}

#[test]
fn a_long_checklist_can_be_edited_in_the_smallest_window() {
    let dir = tempfile::tempdir().unwrap();
    // The window can't be made smaller than this (see main.rs).
    let mut h =
        super::tests::harness_sized(demo_config(dir.path()), egui::Theme::Light, [700.0, 400.0]);
    h.run_steps(2);
    // (On top, so in view in the small window.)
    open(&mut h, "Send quarterly report");
    let subtasks = &mut h.state_mut().task_editor.as_mut().unwrap().details.subtasks;
    for i in 1..=40 {
        subtasks.push(crate::model::Subtask::new(&format!("Step {i}")));
    }
    h.run_steps(8);
    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 400.0));
    assert!(window.contains_rect(button(&h, "Save").rect()));
    assert!(window.contains_rect(button(&h, "Cancel").rect()));

    // Scroll down to the last subtask and change it.
    let last_field = By::new()
        .role(egui::accesskit::Role::TextInput)
        .value("Step 40");
    let last = |h: &H| h.get(last_field.clone()).rect();
    let save_top = button(&h, "Save").rect().top();
    assert!(last(&h).top() > save_top, "out of view at first");
    let inside = egui::pos2(350.0, save_top - 60.0);
    h.hover_at(inside);
    for _ in 0..40 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -200.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(4);
    let field = last(&h);
    assert!(field.bottom() < save_top, "scrolled into view: {field:?}");
    h.get(last_field.clone()).click();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("Last step".into()));
    h.run_steps(2);
    save(&mut h);
    let task = task(&h, "Send quarterly report");
    assert_eq!(task.subtasks.len(), 40);
    assert_eq!(task.subtasks[39].title, "Last step");
}

#[test]
fn a_task_deleted_on_another_computer_while_its_dialog_is_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let id = id_of(&h, "Read team feedback");
    open(&mut h, "Read team feedback");
    h.state_mut().task_editor.as_mut().unwrap().details.notes = "Ask Anna too".into();

    let mut laptop = h.state().board.clone();
    laptop.device = "laptop".into();
    laptop.delete_task(id);
    SyncFolder::new(&dir.path().join("sync"), "laptop")
        .save(&mut laptop)
        .unwrap();
    h.state_mut().last_scan = Instant::now() - SCAN_INTERVAL;
    h.run_steps(4);
    assert!(h.state().board.tasks[&id].meta.deleted);
    assert!(h.state().task_editor.is_some(), "the dialog stays open");

    // Saving keeps what was typed: the task comes back with it.
    save(&mut h);
    let task = &h.state().board.tasks[&id];
    assert!(!task.meta.deleted);
    assert_eq!(task.notes, "Ask Anna too");
    assert!(h.query_by_label("Read team feedback").is_some());
}

#[test]
fn a_long_checklist_scrolls_and_save_stays_in_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Read team feedback");
    let subtasks = &mut h.state_mut().task_editor.as_mut().unwrap().details.subtasks;
    for i in 1..=40 {
        subtasks.push(crate::model::Subtask::new(&format!("Step {i}")));
    }
    // (The dialog re-centres for a few frames after growing.)
    h.run_steps(8);
    let save_button = button(&h, "Save").rect();
    assert!(save_button.bottom() <= 800.0, "{save_button:?}");
    assert!(button(&h, "Cancel").rect().bottom() <= 800.0);
    save(&mut h);
    assert_eq!(task(&h, "Read team feedback").subtasks.len(), 40);
}

/// Replace the text of the field currently showing `old`.
fn retype(h: &mut H, old: &str, new: &str) {
    h.get(By::new().role(egui::Role::TextInput).value(old))
        .click();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    type_text(h, new);
}

#[test]
fn double_click_and_enter_open_the_task_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Read team feedback");
    assert_eq!(editor(&h).id, Some(id_of(&h, "Read team feedback")));
    press(&mut h, egui::Modifiers::NONE, egui::Key::Escape);
    assert!(h.state().task_editor.is_none());

    select(&mut h, "Fix dripping tap");
    press(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    assert_eq!(editor(&h).id, Some(id_of(&h, "Fix dripping tap")));
    assert_eq!(editor(&h).details.title, "Fix dripping tap");
}

#[test]
fn renaming_a_task_saves_and_cancel_keeps_the_old_title() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let id = id_of(&h, "Read team feedback");
    open(&mut h, "Read team feedback");
    retype(&mut h, "Read team feedback", "Read the survey");
    save(&mut h);
    assert_eq!(h.state().board.tasks[&id].title, "Read the survey");
    assert!(h.query_by_label("Read the survey").is_some());

    open(&mut h, "Read the survey");
    retype(&mut h, "Read the survey", "Something else");
    button(&h, "Cancel").click();
    h.run_steps(2);
    assert_eq!(h.state().board.tasks[&id].title, "Read the survey");
}

#[test]
fn deleting_from_the_dialog_removes_the_task() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let id = id_of(&h, "Sketch landing page");
    open(&mut h, "Sketch landing page");
    button(&h, "Delete").click();
    h.run_steps(2);
    assert!(h.state().task_editor.is_none());
    assert!(h.state().board.tasks[&id].meta.deleted);
    assert!(h.query_by_label("Sketch landing page").is_none());
}

#[test]
fn pinning_in_the_dialog_puts_the_task_on_top() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    open(&mut h, "Read team feedback");
    h.get_by_label("Keep on top, whatever the deadline").click();
    h.run_steps(2);
    save(&mut h);
    assert!(task(&h, "Read team feedback").pinned);
    assert!(top(&h, work).contains(&"Read team feedback".to_owned()));
    assert!(!priorities(&h, work).contains(&"Read team feedback".to_owned()));
}

#[test]
fn notes_are_saved_and_the_first_line_shows_on_the_card() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Read team feedback");
    h.get_by_role(egui::Role::MultilineTextInput).click();
    h.run_steps(2);
    type_text(&mut h, "Ask Kim first");
    save(&mut h);
    assert_eq!(task(&h, "Read team feedback").notes, "Ask Kim first");
    assert!(
        h.query_by_label("Ask Kim first").is_some(),
        "shown on the card"
    );
}

/// Open the combo box currently showing `current` and pick `option`.
fn choose(h: &mut H, current: &str, option: &str) {
    h.run_steps(3);
    h.get(By::new().role(egui::Role::ComboBox).value(current))
        .click();
    h.run_steps(2);
    h.get_by_role(egui::Role::ListBox)
        .get_by_label(option)
        .click();
    h.run_steps(2);
}

/// The dialog's deadline checkbox (it has no text of its own).
fn deadline_checkbox(h: &H) -> egui_kittest::Node<'_> {
    h.get_all_by_role(egui::Role::CheckBox)
        .find(|n| n.accesskit_node().label().is_none_or(|l| l.is_empty()))
        .expect("the deadline checkbox")
}

#[test]
fn moving_a_task_to_another_area_in_the_dialog_drops_its_group() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let family = area(&h, 3);
    open(&mut h, "Clean up shared drive");
    choose(&mut h, "Work", "Family");
    assert_eq!(
        editor(&h).details.group,
        None,
        "Work's group can't come along"
    );
    save(&mut h);
    let t = task(&h, "Clean up shared drive");
    assert_eq!(t.area, family);
    assert_eq!(t.group, None);
    assert_eq!(
        priorities(&h, family).last().map(String::as_str),
        Some("Clean up shared drive")
    );
}

#[test]
fn the_group_can_be_changed_in_the_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let garden = h
        .state()
        .board
        .groups
        .values()
        .find(|g| g.name == "Garden")
        .unwrap()
        .meta
        .id;
    open(&mut h, "Fix dripping tap");
    choose(&mut h, "● Repairs", "● Garden");
    save(&mut h);
    assert_eq!(task(&h, "Fix dripping tap").group, Some(garden));

    open(&mut h, "Fix dripping tap");
    choose(&mut h, "● Garden", "None");
    save(&mut h);
    assert_eq!(task(&h, "Fix dripping tap").group, None);
}

#[test]
fn a_deadline_moves_the_task_on_top_and_removing_it_moves_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    trash_out_of_the_way(&mut h);
    let work = area(&h, 0);
    open(&mut h, "Read team feedback");
    deadline_checkbox(&h).click();
    h.run_steps(2);
    assert!(editor(&h).has_deadline);
    save(&mut h);
    // Due today (the date picker starts there): on top, and urgent.
    assert_eq!(task(&h, "Read team feedback").deadline, Some(today()));
    assert!(top(&h, work).contains(&"Read team feedback".to_owned()));
    // (The chimney sweep is due today as well.)
    assert_eq!(h.get_all_by_label("today").count(), 2);

    open(&mut h, "Read team feedback");
    deadline_checkbox(&h).click();
    h.run_steps(2);
    save(&mut h);
    assert_eq!(task(&h, "Read team feedback").deadline, None);
    assert_eq!(
        priorities(&h, work).last().map(String::as_str),
        Some("Read team feedback"),
        "back at the bottom of the list"
    );
}

#[test]
fn a_new_task_due_today_goes_first_on_top() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    let before = top(&h, work);
    assert_eq!(before[0], "Send quarterly report", "overdue, first so far");
    h.get_by_label("+ New task").click();
    h.run_steps(2);
    type_text(&mut h, "Call the plumber");
    deadline_checkbox(&h).click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::Role::Button, "Add task")
        .click();
    h.run_steps(4);
    assert_eq!(task(&h, "Call the plumber").deadline, Some(today()));
    let after = top(&h, work);
    assert_eq!(after[0], "Call the plumber", "{after:?}");
    assert_eq!(after[1..], before[..], "the others keep their order");
}

#[test]
fn an_existing_task_given_todays_deadline_keeps_the_usual_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    trash_out_of_the_way(&mut h);
    let work = area(&h, 0);
    open(&mut h, "Read team feedback");
    deadline_checkbox(&h).click();
    h.run_steps(2);
    save(&mut h);
    // Overdue still comes first; only new tasks jump ahead.
    assert_eq!(top(&h, work)[0], "Send quarterly report");
    assert_eq!(task(&h, "Read team feedback").top_rank, None);
}

#[test]
fn a_longer_lead_time_brings_a_far_deadline_on_top() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let ventures = area(&h, 1);
    // Due in 12 days, the default lead time is 10: still in the list.
    assert!(priorities(&h, ventures).contains(&"File VAT return".to_owned()));
    open(&mut h, "File VAT return");
    h.get(By::new().value("10")).set_value_accesskit(20.0);
    h.run_steps(2);
    save(&mut h);
    assert_eq!(task(&h, "File VAT return").lead_days, Some(20));
    assert!(top(&h, ventures).contains(&"File VAT return".to_owned()));

    // Back to the default: stored as "no lead time of its own".
    open(&mut h, "File VAT return");
    h.get(By::new().value("20")).set_value_accesskit(10.0);
    h.run_steps(2);
    save(&mut h);
    assert_eq!(task(&h, "File VAT return").lead_days, None);
    assert!(priorities(&h, ventures).contains(&"File VAT return".to_owned()));
}

#[test]
fn a_task_can_repeat_on_chosen_weekdays() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Read team feedback");
    choose(&mut h, "Never", "On a schedule");
    assert!(editor(&h).has_deadline, "repeating needs a due date");
    // Weekly on today's weekday to start with; add another day.
    let today_wd = today().weekday().to_monday_one_offset();
    let other = if today_wd == 3 { 5 } else { 3 };
    let name = crate::repeat::WEEKDAY_BUTTONS[(other - 1) as usize];
    h.get_by_label(name).click();
    h.run_steps(2);
    save(&mut h);
    let mut days = vec![today_wd, other];
    days.sort_unstable();
    assert_eq!(
        task(&h, "Read team feedback").repeat,
        Some(Repeat::Schedule {
            every: 1,
            unit: crate::repeat::Unit::Weeks,
            weekdays: days,
        })
    );
    // The card shows it repeats (as the two example ones do).
    assert_eq!(h.get_all_by_label("↻").count(), 3);
}

#[test]
fn a_task_can_repeat_some_time_after_it_is_done() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Read team feedback");
    choose(&mut h, "Never", "After it's done");
    h.get(By::new().value("1")).set_value_accesskit(3.0);
    h.run_steps(2);
    save(&mut h);
    assert_eq!(
        task(&h, "Read team feedback").repeat,
        Some(Repeat::AfterDone {
            every: 3,
            unit: crate::repeat::Unit::Months,
        })
    );

    // And off again.
    open(&mut h, "Read team feedback");
    choose(&mut h, "After it's done", "Never");
    save(&mut h);
    assert_eq!(task(&h, "Read team feedback").repeat, None);
}

#[test]
fn subtasks_can_be_added_renamed_ticked_and_removed_in_the_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Plan weekend trip");
    // Add one: type into the empty field and press Enter.
    h.get(By::new().role(egui::Role::TextInput).value(""))
        .click();
    h.run_steps(2);
    type_text(&mut h, "Pack bags");
    press(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    retype(&mut h, "Book a hotel", "Book a cabin");
    h.get_by_label("Done: Book a cabin").click();
    h.run_steps(2);
    // Remove the first one.
    h.get_all_by_label("×").next().unwrap().click();
    h.run_steps(2);
    save(&mut h);

    let subtasks: Vec<(String, bool)> = task(&h, "Plan weekend trip")
        .subtasks
        .iter()
        .map(|s| (s.title.clone(), s.done))
        .collect();
    assert_eq!(
        subtasks,
        [
            ("Book a cabin".to_owned(), true),
            ("Pack bags".to_owned(), false)
        ]
    );
}

#[test]
fn a_subtask_typed_but_not_added_is_kept_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Plan weekend trip");
    h.get(By::new().role(egui::Role::TextInput).value(""))
        .click();
    h.run_steps(2);
    type_text(&mut h, "Pack bags");
    // Straight to Save, without Add or Enter.
    save(&mut h);
    let subtasks: Vec<String> = task(&h, "Plan weekend trip")
        .subtasks
        .iter()
        .map(|s| s.title.clone())
        .collect();
    assert_eq!(subtasks.last().map(String::as_str), Some("Pack bags"));
    assert_eq!(subtasks.len(), 3, "{subtasks:?}");
}

#[test]
fn a_subtask_typed_but_not_added_is_dropped_on_cancel() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let before = task(&h, "Plan weekend trip").subtasks.clone();
    open(&mut h, "Plan weekend trip");
    h.get(By::new().role(egui::Role::TextInput).value(""))
        .click();
    h.run_steps(2);
    type_text(&mut h, "Pack bags");
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().task_editor.is_none());
    assert_eq!(task(&h, "Plan weekend trip").subtasks, before);
}

#[test]
fn cancelling_the_dialog_keeps_everything_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let before = task(&h, "Plan weekend trip").clone();
    open(&mut h, "Plan weekend trip");
    h.get_by_label("Keep on top, whatever the deadline").click();
    h.run_steps(2);
    h.get_all_by_label("×").next().unwrap().click();
    h.run_steps(2);
    choose(&mut h, "Never", "After it's done");
    button(&h, "Cancel").click();
    h.run_steps(2);
    assert_eq!(task(&h, "Plan weekend trip"), &before);
}

// ----- Cards ---------------------------------------------------------------

fn tick(h: &mut H, title: &str) {
    h.get_by_label(&format!("Done: {title}")).click();
    h.run_steps(2);
}

#[test]
fn ticking_a_task_moves_it_to_done_and_unticking_brings_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    tick(&mut h, "Read team feedback");
    assert!(task(&h, "Read team feedback").done);
    assert!(!priorities(&h, work).contains(&"Read team feedback".to_owned()));
    // The DONE section of Work (the first of two), folded; open it.
    h.get_all_by_label("DONE IN THE LAST 7 DAYS (1)")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(h.query_by_label("Read team feedback").is_some());

    tick(&mut h, "Read team feedback");
    assert!(!task(&h, "Read team feedback").done);
    assert_eq!(
        priorities(&h, work).last().map(String::as_str),
        Some("Read team feedback")
    );
}

#[test]
fn done_tasks_can_be_reopened_from_their_menu() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    // The example's one done task, in Family.
    h.get_by_label("DONE IN THE LAST 7 DAYS (1)").click();
    h.run_steps(2);
    card_menu(&mut h, "Book dentist appointment", "Reopen");
    assert!(!task(&h, "Book dentist appointment").done);
    assert!(
        h.query_by_label("DONE IN THE LAST 7 DAYS (1)").is_none(),
        "nothing done any more"
    );
}

#[test]
fn finishing_a_recurring_task_shows_its_next_occurrence() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let due = task(&h, "Take out the trash").deadline.unwrap();
    let id = id_of(&h, "Take out the trash");
    tick(&mut h, "Take out the trash");
    assert!(h.state().board.tasks[&id].done);
    let next = h
        .state()
        .board
        .tasks
        .values()
        .find(|t| t.title == "Take out the trash" && !t.done && !t.meta.deleted)
        .expect("the next one");
    let next_due = next.deadline.unwrap();
    assert!(next_due > due && next_due > today());
    assert_eq!(next_due.weekday(), jiff::civil::Weekday::Friday);
    // Its card shows the new date.
    assert!(h.query_by_label("Done: Take out the trash").is_some());
}

#[test]
fn subtasks_unfold_on_the_card_and_can_be_ticked_there() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    assert!(
        h.query_by_label("Done: Check the weather").is_none(),
        "folded"
    );
    card_menu(&mut h, "Plan weekend trip", "Show subtasks");
    assert!(h.query_by_label("Check the weather").is_some());
    tick(&mut h, "Check the weather");
    let trip = task(&h, "Plan weekend trip");
    assert!(trip.subtasks[0].done);
    assert!(!trip.done, "ticking a subtask doesn't finish the task");

    card_menu(&mut h, "Plan weekend trip", "Hide subtasks");
    assert!(h.query_by_label("Check the weather").is_none());
}

/// The demo's trash is due next Friday, so depending on the weekday its
/// date reads "today", "tomorrow", "in 3 d"… like the dates under test.
fn trash_out_of_the_way(h: &mut H) {
    let trash = id_of(h, "Take out the trash");
    let day = today().saturating_add(jiff::Span::new().days(10));
    h.state_mut()
        .board
        .update_task(trash, |t| t.deadline = Some(day));
    h.run_steps(2);
}

#[test]
fn deadlines_read_as_days_from_today() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    trash_out_of_the_way(&mut h);
    // Read team feedback: due tomorrow; Clean up shared drive: in 30 days.
    let in_days = |n: i64| today().saturating_add(jiff::Span::new().days(n));
    let read = id_of(&h, "Read team feedback");
    let clean = id_of(&h, "Clean up shared drive");
    h.state_mut()
        .board
        .update_task(read, |t| t.deadline = Some(in_days(1)));
    h.state_mut()
        .board
        .update_task(clean, |t| t.deadline = Some(in_days(30)));
    h.run_steps(2);
    assert!(h.query_by_label("1 d overdue").is_some());
    assert!(h.query_by_label("tomorrow").is_some());
    assert!(h.query_by_label("in 3 d").is_some());
    assert!(h.query_by_label("in 6 d").is_some());
    let far = in_days(30).strftime("%b %-d").to_string();
    assert!(h.query_by_label(&far).is_some(), "{far}");
}

// ----- Card menu -----------------------------------------------------------

#[test]
fn edit_in_the_card_menu_opens_the_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    card_menu(&mut h, "Order tulip bulbs", "Edit…");
    assert_eq!(editor(&h).id, Some(id_of(&h, "Order tulip bulbs")));
}

#[test]
fn move_to_top_puts_the_task_first_in_its_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    card_menu(&mut h, "Read team feedback", "Move to top");
    assert_eq!(priorities(&h, work)[0], "Read team feedback");
    // The first one has nothing to move above.
    let pos = h.get_by_label("Read team feedback").rect().center();
    right_click_at(&mut h, pos);
    assert!(h.query_by_label("Move to top").is_none());
}

#[test]
fn pin_and_unpin_from_the_card_menu() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let house = area(&h, 2);
    card_menu(&mut h, "Cut back the hedge", "Pin to top");
    assert!(task(&h, "Cut back the hedge").pinned);
    assert!(top(&h, house).contains(&"Cut back the hedge".to_owned()));

    card_menu(&mut h, "Cut back the hedge", "Unpin");
    assert!(!task(&h, "Cut back the hedge").pinned);
    assert_eq!(
        priorities(&h, house).last().map(String::as_str),
        Some("Cut back the hedge"),
        "back at the bottom of the list"
    );
}

#[test]
fn skipping_a_recurring_task_moves_it_on_without_marking_it_done() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let due = task(&h, "Take out the trash").deadline.unwrap();
    let done_before = h.state().board.stats(today()).done_recently;
    card_menu(&mut h, "Take out the trash", "Skip this time");
    let trash: Vec<&TaskItem> = h
        .state()
        .board
        .tasks
        .values()
        .filter(|t| t.title == "Take out the trash" && !t.meta.deleted)
        .collect();
    assert!(trash.iter().all(|t| !t.done));
    assert!(trash.iter().any(|t| t.deadline.unwrap() > due));
    assert_eq!(h.state().board.stats(today()).done_recently, done_before);
}

#[test]
fn only_recurring_tasks_can_be_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let pos = h.get_by_label("Order tulip bulbs").rect().center();
    right_click_at(&mut h, pos);
    assert!(h.query_by_label("Skip this time").is_none());
}

#[test]
fn mark_done_and_delete_from_the_card_menu() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    card_menu(&mut h, "Order tulip bulbs", "Mark done");
    assert!(task(&h, "Order tulip bulbs").done);

    let id = id_of(&h, "Cut back the hedge");
    card_menu(&mut h, "Cut back the hedge", "Delete");
    assert!(h.state().board.tasks[&id].meta.deleted);
    assert!(h.query_by_label("Cut back the hedge").is_none());
}

// ----- Keyboard ------------------------------------------------------------

const ALT: egui::Modifiers = egui::Modifiers::ALT;
const ALT_COMMAND: egui::Modifiers = egui::Modifiers {
    alt: true,
    command: true,
    mac_cmd: false,
    ctrl: false,
    shift: false,
};

#[test]
fn option_arrows_move_the_selected_task_in_its_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    assert_eq!(
        priorities(&h, work),
        [
            "Prepare kick-off workshop",
            "Clean up shared drive",
            "Read team feedback"
        ]
    );
    select(&mut h, "Clean up shared drive");
    press(&mut h, ALT, egui::Key::ArrowDown);
    assert_eq!(priorities(&h, work)[2], "Clean up shared drive");
    press(&mut h, ALT, egui::Key::ArrowUp);
    press(&mut h, ALT, egui::Key::ArrowUp);
    assert_eq!(priorities(&h, work)[0], "Clean up shared drive");
    // Already first: stays.
    press(&mut h, ALT, egui::Key::ArrowUp);
    assert_eq!(priorities(&h, work)[0], "Clean up shared drive");
}

#[test]
fn option_command_arrows_move_to_the_top_or_bottom() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    select(&mut h, "Read team feedback");
    press(&mut h, ALT_COMMAND, egui::Key::ArrowUp);
    assert_eq!(priorities(&h, work)[0], "Read team feedback");
    press(&mut h, ALT_COMMAND, egui::Key::ArrowDown);
    assert_eq!(
        priorities(&h, work).last().map(String::as_str),
        Some("Read team feedback")
    );
}

#[test]
fn option_arrows_work_in_the_top_section_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    select(&mut h, "Call back the bank");
    press(&mut h, ALT_COMMAND, egui::Key::ArrowUp);
    assert_eq!(top(&h, work)[0], "Call back the bank");
    assert!(
        !priorities(&h, work).contains(&"Call back the bank".to_owned()),
        "it stays on top"
    );
}

#[test]
fn keys_dont_act_on_a_task_in_an_area_that_folded() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let family = area(&h, 3);
    let id = id_of(&h, "Plan weekend trip");
    select(&mut h, "Plan weekend trip");
    assert_eq!(h.state().selected, Some(id));
    // Family folds without a click (e.g. its active hours end).
    h.state_mut().config.folded_areas.push(family);
    h.run_steps(2);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Backspace);
    assert!(!h.state().board.tasks[&id].meta.deleted);
    assert!(h.state().selected.is_none());
}

#[test]
fn keys_dont_act_on_a_task_a_filter_hides() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    let id = id_of(&h, "Read team feedback");
    select(&mut h, "Read team feedback");
    let clients = h
        .state()
        .board
        .groups_of(work)
        .iter()
        .find(|g| g.name == "Clients")
        .unwrap()
        .meta
        .id;
    h.state_mut().filters.insert(work, clients);
    h.run_steps(2);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Backspace);
    assert!(!h.state().board.tasks[&id].meta.deleted);
}

#[test]
fn command_backspace_deletes_and_escape_deselects() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let id = id_of(&h, "Ask Tom about domain names");
    select(&mut h, "Ask Tom about domain names");
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Backspace);
    assert!(h.state().board.tasks[&id].meta.deleted);

    select(&mut h, "Sketch landing page");
    assert!(h.state().selected.is_some());
    press(&mut h, egui::Modifiers::NONE, egui::Key::Escape);
    assert!(h.state().selected.is_none());
    // Without a selection the keys do nothing.
    let before = h.state().board.clone();
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Backspace);
    press(&mut h, ALT, egui::Key::ArrowUp);
    assert_eq!(h.state().board, before);
}

#[test]
fn task_keys_are_ignored_while_a_dialog_is_open() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    select(&mut h, "Read team feedback");
    h.get_by_label("+ New task").click();
    h.run_steps(2);
    let before = priorities(&h, work);
    press(&mut h, ALT, egui::Key::ArrowUp);
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Backspace);
    assert_eq!(priorities(&h, work), before);
    assert!(h.state().task_editor.is_some());
}

// ----- Undo / redo ---------------------------------------------------------

#[test]
fn command_z_undoes_step_by_step_and_shift_command_z_redoes() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let read = id_of(&h, "Read team feedback");
    let tulips = id_of(&h, "Order tulip bulbs");
    tick(&mut h, "Read team feedback");
    card_menu(&mut h, "Order tulip bulbs", "Delete");
    assert!(h.state().board.tasks[&tulips].meta.deleted);

    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert!(
        !h.state().board.tasks[&tulips].meta.deleted,
        "delete undone"
    );
    assert!(h.state().board.tasks[&read].done, "only one step");
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert!(!h.state().board.tasks[&read].done, "tick undone");

    let shift_command = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    press(&mut h, shift_command, egui::Key::Z);
    assert!(h.state().board.tasks[&read].done, "tick redone");
    assert!(!h.state().board.tasks[&tulips].meta.deleted);
    press(&mut h, shift_command, egui::Key::Z);
    assert!(h.state().board.tasks[&tulips].meta.deleted, "delete redone");
}

#[test]
fn a_new_change_clears_redo() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    assert!(disabled(&h, "↶"), "nothing to undo yet");
    tick(&mut h, "Read team feedback");
    assert!(!disabled(&h, "↶"));
    h.get_by_label("↶").click();
    h.run_steps(2);
    assert!(!task(&h, "Read team feedback").done);
    assert!(!disabled(&h, "↷"), "can redo");

    tick(&mut h, "Order tulip bulbs");
    assert!(disabled(&h, "↷"), "a new change clears redo");
    h.get_by_label("↶").click();
    h.run_steps(2);
    assert!(!task(&h, "Order tulip bulbs").done);
    assert!(!task(&h, "Read team feedback").done);
}

#[test]
fn undo_brings_back_a_task_deleted_in_its_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    open(&mut h, "Sketch landing page");
    button(&h, "Delete").click();
    h.run_steps(2);
    assert!(h.query_by_label("Sketch landing page").is_none());
    press(&mut h, egui::Modifiers::COMMAND, egui::Key::Z);
    assert!(h.query_by_label("Sketch landing page").is_some());
}

// ----- Drag and drop -------------------------------------------------------

fn center(h: &H, title: &str) -> egui::Pos2 {
    h.get_by_label(title).rect().center()
}

/// Just above the card with this title (its group name sits above the title).
fn above(h: &H, title: &str) -> egui::Pos2 {
    let r = h.get_by_label(title).rect();
    egui::pos2(r.center().x, r.top() - 6.0)
}

#[test]
fn dragging_in_a_filtered_area_keeps_the_hidden_tasks_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let house = area(&h, 2);
    let garden = h
        .state()
        .board
        .groups_of(house)
        .iter()
        .find(|g| g.name == "Garden")
        .unwrap()
        .meta
        .id;
    let before = priorities(&h, house);
    h.state_mut().filters.insert(house, garden);
    h.run_steps(2);
    assert!(h.query_by_label("Fix dripping tap").is_none(), "hidden");

    let from = center(&h, "Order tulip bulbs");
    let to = above(&h, "Cut back the hedge");
    drag(&mut h, from, to);
    let mut expected = before.clone();
    expected.retain(|t| t != "Order tulip bulbs");
    let hedge = expected
        .iter()
        .position(|t| t == "Cut back the hedge")
        .unwrap();
    expected.insert(hedge, "Order tulip bulbs".to_owned());
    assert_eq!(priorities(&h, house), expected, "was {before:?}");
}

#[test]
fn dragging_reorders_the_priority_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let work = area(&h, 0);
    let from = center(&h, "Read team feedback");
    let to = above(&h, "Prepare kick-off workshop");
    drag(&mut h, from, to);
    assert_eq!(
        priorities(&h, work),
        [
            "Read team feedback",
            "Prepare kick-off workshop",
            "Clean up shared drive"
        ]
    );
}

#[test]
fn dragging_into_another_areas_list_moves_the_task_there() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let house = area(&h, 2);
    let family = area(&h, 3);
    let from = center(&h, "Order tulip bulbs");
    let to = above(&h, "Renew passports");
    drag(&mut h, from, to);
    let t = task(&h, "Order tulip bulbs");
    assert_eq!(t.area, family);
    assert_eq!(t.group, None, "House's Garden group stays behind");
    assert_eq!(
        priorities(&h, family),
        ["Plan weekend trip", "Order tulip bulbs", "Renew passports"]
    );
    assert!(!priorities(&h, house).contains(&"Order tulip bulbs".to_owned()));
}

#[test]
fn dragging_into_another_areas_top_section_pins_it_there() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let family = area(&h, 3);
    let from = center(&h, "Ask Tom about domain names");
    let to = above(&h, "Birthday present for Lena");
    drag(&mut h, from, to);
    let t = task(&h, "Ask Tom about domain names");
    assert_eq!(t.area, family);
    assert!(t.pinned);
    assert_eq!(top(&h, family)[0], "Ask Tom about domain names");
}

#[test]
fn dropping_on_a_folded_area_puts_the_task_at_the_bottom_of_its_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let family = area(&h, 3);
    h.state_mut().config.folded_areas.push(family);
    h.run_steps(2);
    let from = center(&h, "Read team feedback");
    let to = h.get_by_label("Family (folded)").rect().center();
    drag(&mut h, from, to);
    assert_eq!(task(&h, "Read team feedback").area, family);
    assert_eq!(
        priorities(&h, family).last().map(String::as_str),
        Some("Read team feedback")
    );
    assert!(
        h.query_by_label("Family (folded)").is_some(),
        "stays folded"
    );
}

// ----- Toolbar statistics --------------------------------------------------

#[test]
fn the_toolbar_counts_match_the_tasks_and_follow_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = app(&dir);
    let stats = h.state().board.stats(today());
    assert_eq!(stats.overdue.len(), 1, "the quarterly report");
    assert!(h.query_by_label(&format!("{} open", stats.open)).is_some());
    assert!(h.query_by_label("1 overdue").is_some());
    let soon = format!("{} due within 7 days", stats.due_this_week.len());
    assert!(h.query_by_label(&soon).is_some(), "{soon}");
    assert!(h.query_by_label("1 done in the last 7 days").is_some());

    // Finishing the overdue one: one less open and overdue, one more done.
    tick(&mut h, "Send quarterly report");
    assert!(
        h.query_by_label(&format!("{} open", stats.open - 1))
            .is_some()
    );
    assert!(h.query_by_label("0 overdue").is_some());
    assert!(h.query_by_label("2 done in the last 7 days").is_some());

    // A new task counts as open.
    h.get_by_label("+ New task").click();
    h.run_steps(2);
    type_text(&mut h, "One more");
    press(&mut h, egui::Modifiers::NONE, egui::Key::Enter);
    assert!(h.query_by_label(&format!("{} open", stats.open)).is_some());
}

#[test]
fn the_subtask_count_on_a_card_opens_and_closes_its_subtasks() {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness(demo_config(dir.path()), egui::Theme::Light);
    harness.run_steps(4);
    // "Prepare kick-off workshop" has three subtasks, one done.
    harness.get_by_label("Subtasks 1/3").click();
    harness.run_steps(4);
    assert!(harness.query_by_label("Send invitations").is_some(), "open");
    harness.get_by_label("Done: Send invitations").click();
    harness.run_steps(4);
    assert!(
        harness.query_by_label("Subtasks 2/3").is_some(),
        "count follows"
    );
    harness.get_by_label("Subtasks 2/3").click();
    harness.run_steps(4);
    assert!(
        harness.query_by_label("Send invitations").is_none(),
        "closed"
    );
}
