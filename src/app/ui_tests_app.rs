//! UI tests: the real app in a test window, driven like a user would.
//! Areas, groups, settings, backups, About, the menu and syncing.

use super::tests::{demo_config, drag, harness, right_click_at};
use super::*;
use egui::accesskit::Role;
use egui_kittest::kittest::{By, NodeT as _, Queryable as _};

type Harness = egui_kittest::Harness<'static, PriogatorApp>;

/// The app with the example data in a temporary folder (kept alive with it).
fn app() -> (tempfile::TempDir, Harness) {
    app_in(egui::Theme::Light)
}

fn app_in(theme: egui::Theme) -> (tempfile::TempDir, Harness) {
    let dir = tempfile::tempdir().unwrap();
    let mut harness = harness(demo_config(dir.path()), theme);
    harness.run_steps(4);
    (dir, harness)
}

fn area_names(h: &Harness) -> Vec<String> {
    h.state()
        .board
        .areas_sorted()
        .iter()
        .map(|a| a.name.clone())
        .collect()
}

fn area_id(h: &Harness, name: &str) -> Uuid {
    h.state()
        .board
        .areas_sorted()
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("no area {name}"))
        .meta
        .id
}

fn task_id(h: &Harness, title: &str) -> Uuid {
    h.state()
        .board
        .tasks
        .values()
        .find(|t| t.title == title && !t.meta.deleted)
        .unwrap_or_else(|| panic!("no task {title}"))
        .meta
        .id
}

/// A button by its exact label (several widgets may share a text).
fn button<'a>(h: &'a Harness, label: &'a str) -> egui_kittest::Node<'a> {
    h.get_by_role_and_label(Role::Button, label)
}

/// Right-click an area's title bar (next to its name) to open its menu.
fn area_menu(h: &mut Harness, name: &str) {
    let rect = h.get_all_by_label(name).next().unwrap().rect();
    let pos = if name.ends_with("(folded)") {
        rect.center()
    } else {
        egui::pos2(rect.right() + 60.0, rect.center().y)
    };
    right_click_at(h, pos);
}

/// Open the ☰ menu and click one of its entries.
fn menu(h: &mut Harness, entry: &str) {
    h.get_by_label("☰").click();
    h.run_steps(4);
    // Entries with a shortcut have it in their label too.
    h.get_all(By::new().role(Role::Button).label_contains(entry))
        .next()
        .unwrap_or_else(|| panic!("no menu entry {entry}"))
        .click();
    h.run_steps(4);
}

/// Replace the text of the focused text field.
fn type_into_focused(h: &mut Harness, text: &str) {
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.event(egui::Event::Text(text.to_owned()));
    h.run_steps(2);
}

// ----- Areas ------------------------------------------------------------

#[test]
fn plus_area_adds_a_named_area_at_the_end() {
    let (_dir, mut h) = app();
    button(&h, "+ Area").click();
    h.run_steps(4);
    assert!(h.state().area_editor.is_some());
    assert!(h.query_by_label("New area").is_some());
    h.event(egui::Event::Text("Garden".into()));
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(4);
    assert!(h.state().area_editor.is_none());
    assert_eq!(
        area_names(&h),
        ["Work", "Private ventures", "House", "Family", "Garden"]
    );
    assert!(h.query_all_by_label("Garden").next().is_some(), "shown");
}

#[test]
fn an_area_needs_a_name() {
    let (_dir, mut h) = app();
    button(&h, "+ Area").click();
    h.run_steps(4);
    let save = button(&h, "Save");
    assert!(save.accesskit_node().is_disabled(), "Save is disabled");
    save.click();
    h.run_steps(2);
    assert!(h.state().area_editor.is_some(), "still open");
    // Spaces only don't count either.
    h.event(egui::Event::Text("   ".into()));
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(2);
    assert_eq!(area_names(&h).len(), 4);
    button(&h, "Cancel").click();
    h.run_steps(2);
    assert!(h.state().area_editor.is_none());
    assert_eq!(area_names(&h).len(), 4);
}

#[test]
fn new_area_saves_with_enter() {
    let (_dir, mut h) = app();
    button(&h, "+ Area").click();
    h.run_steps(4);
    h.event(egui::Event::Text("Hobbies".into()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert!(h.state().area_editor.is_none());
    assert_eq!(area_names(&h).last().map(String::as_str), Some("Hobbies"));
}

#[test]
fn double_clicking_a_title_edits_the_area() {
    let dir = tempfile::tempdir().unwrap();
    let config = demo_config(dir.path());
    // Short steps, so two clicks can come within the double-click time.
    let mut h = egui_kittest::Harness::builder()
        .with_size([1280.0, 800.0])
        .with_step_dt(0.05)
        .build_eframe(|cc| PriogatorApp::new(&cc.egui_ctx, config));
    h.run_steps(4);
    let rect = h.get_all_by_label("House").next().unwrap().rect();
    let pos = egui::pos2(rect.right() + 40.0, rect.center().y);
    h.hover_at(pos);
    h.run_steps(2);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
        }
    }
    h.run_steps(4);
    let editor = h.state().area_editor.as_ref().expect("dialog opens");
    assert_eq!(editor.name, "House");
    assert!(h.query_by_label("Edit area").is_some());
}

#[test]
fn edit_area_renames_it() {
    let (_dir, mut h) = app();
    let house = area_id(&h, "House");
    area_menu(&mut h, "House");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    assert_eq!(h.state().area_editor.as_ref().unwrap().id, Some(house));
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(2);
    type_into_focused(&mut h, "Home");
    button(&h, "Save").click();
    h.run_steps(4);
    assert_eq!(h.state().board.areas[&house].name, "Home");
    assert!(h.query_all_by_label("Home").next().is_some());
    // Its tasks stay with it.
    assert!(
        h.state()
            .board
            .priority_list(house)
            .iter()
            .any(|t| t.title == "Fix dripping tap")
    );
}

#[test]
fn cancelling_edit_area_keeps_the_name() {
    let (_dir, mut h) = app();
    let house = area_id(&h, "House");
    area_menu(&mut h, "House");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(2);
    type_into_focused(&mut h, "Nope");
    button(&h, "Cancel").click();
    h.run_steps(4);
    assert!(h.state().area_editor.is_none());
    assert_eq!(h.state().board.areas[&house].name, "House");
}

#[test]
fn active_hours_are_set_in_the_area_dialog() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    area_menu(&mut h, "Work");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    h.get_by_label("Only active at certain times").click();
    h.run_steps(2);
    // Default: weekdays 08:00–17:00, during.
    assert_eq!(
        h.state().area_editor.as_ref().unwrap().active,
        Some(ActiveHours::default())
    );
    assert!(
        h.query_by_label_contains("Active Mo–Fr 08:00–17:00")
            .is_some()
    );
    // Add Saturday, drop Monday, and make it "outside".
    h.get_by_label("Sa").click();
    h.run_steps(2);
    h.get_by_label("Mo").click();
    h.run_steps(2);
    h.get_by_label("Active outside them").click();
    h.run_steps(2);
    assert!(
        h.query_by_label_contains("Active outside Tu–Sa 08:00–17:00")
            .is_some()
    );
    button(&h, "Save").click();
    h.run_steps(4);
    let active = h.state().board.areas[&work].active.clone().unwrap();
    assert_eq!(active.weekdays, [2, 3, 4, 5, 6]);
    assert!(active.outside);
    assert_eq!((active.from, active.to), (8 * 60, 17 * 60));
}

#[test]
fn active_hours_times_can_be_typed() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    area_menu(&mut h, "Work");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    h.get_by_label("Only active at certain times").click();
    h.run_steps(2);
    // The two time fields, from and to.
    let from = h.get_by_value("08:00");
    from.set_value_accesskit(9.0 * 60.0 + 30.0);
    h.run_steps(2);
    let to = h.get_by_value("17:00");
    to.set_value_accesskit(18.0 * 60.0);
    h.run_steps(2);
    assert!(h.query_by_label_contains("09:30–18:00").is_some());
    button(&h, "Save").click();
    h.run_steps(4);
    let active = h.state().board.areas[&work].active.clone().unwrap();
    assert_eq!((active.from, active.to), (9 * 60 + 30, 18 * 60));
}

#[test]
fn unticking_active_hours_makes_the_area_always_active() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    let never = ActiveHours {
        weekdays: Vec::new(),
        ..ActiveHours::default()
    };
    h.state_mut().board.set_active_hours(work, Some(never));
    h.run_steps(4);
    assert!(h.query_by_label("Work (folded)").is_some());
    area_menu(&mut h, "Work (folded)");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    h.get_by_label("Only active at certain times").click();
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(4);
    assert_eq!(h.state().board.areas[&work].active, None);
    assert!(h.query_by_label("Work (folded)").is_none());
    assert!(h.query_by_label("Send quarterly report").is_some());
}

#[test]
fn an_area_outside_its_hours_shows_counts_on_its_strip() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    let never = ActiveHours {
        weekdays: Vec::new(),
        ..ActiveHours::default()
    };
    h.state_mut().board.set_active_hours(work, Some(never));
    h.run_steps(4);
    let strip = h.get_by_label("Work (folded)");
    strip.hover();
    h.run_steps(4);
    // The tooltip names the hours and how to open it.
    assert!(h.query_by_label_contains("Click to open").is_some());
}

#[test]
fn move_left_and_right_reorder_areas() {
    let (_dir, mut h) = app();
    area_menu(&mut h, "House");
    h.get_by_label("Move left").click();
    h.run_steps(4);
    assert_eq!(
        area_names(&h),
        ["Work", "House", "Private ventures", "Family"]
    );
    area_menu(&mut h, "Work");
    h.get_by_label("Move right").click();
    h.run_steps(4);
    assert_eq!(
        area_names(&h),
        ["House", "Work", "Private ventures", "Family"]
    );
    // The last one can't go further right.
    area_menu(&mut h, "Family");
    h.get_by_label("Move right").click();
    h.run_steps(4);
    assert_eq!(
        area_names(&h),
        ["House", "Work", "Private ventures", "Family"]
    );
}

#[test]
fn dragging_an_area_title_moves_the_area() {
    let (_dir, mut h) = app();
    let work = h.get_all_by_label("Work").next().unwrap().rect();
    let family = h.get_all_by_label("Family").next().unwrap().rect();
    let from = egui::pos2(work.right() + 60.0, work.center().y);
    // Past the middle of the last area.
    let to = egui::pos2(family.right() + 120.0, family.center().y + 200.0);
    drag(&mut h, from, to);
    h.run_steps(4);
    assert_eq!(
        area_names(&h),
        ["Private ventures", "House", "Family", "Work"]
    );
}

#[test]
fn deleting_an_area_asks_first_and_can_be_cancelled() {
    let (_dir, mut h) = app();
    area_menu(&mut h, "Work");
    h.get_by_label("Delete area…").click();
    h.run_steps(4);
    assert!(h.query_by_label("Delete area \"Work\"?").is_some());
    // Work has 6 open tasks, 2 groups, no done tasks.
    assert!(
        h.query_by_label("This also deletes 6 open tasks, 2 groups.")
            .is_some()
    );
    button(&h, "Cancel").click();
    h.run_steps(4);
    assert!(h.state().confirm_delete_area.is_none());
    assert_eq!(area_names(&h).len(), 4);
}

#[test]
fn deleting_an_area_removes_it_with_its_tasks_and_undo_brings_it_back() {
    let (_dir, mut h) = app();
    let family = area_id(&h, "Family");
    area_menu(&mut h, "Family");
    h.get_by_label("Delete area…").click();
    h.run_steps(4);
    // Family: 3 open tasks and 1 done.
    assert!(
        h.query_by_label("This also deletes 3 open tasks, 1 done task.")
            .is_some()
    );
    button(&h, "Delete").click();
    h.run_steps(4);
    assert_eq!(area_names(&h), ["Work", "Private ventures", "House"]);
    assert!(h.query_by_label("Plan weekend trip").is_none());
    assert!(
        h.state()
            .board
            .tasks
            .values()
            .filter(|t| t.area == family)
            .all(|t| t.meta.deleted)
    );

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(4);
    assert_eq!(area_names(&h).len(), 4);
    assert!(h.query_by_label("Plan weekend trip").is_some());
}

#[test]
fn an_empty_area_says_so_when_deleted() {
    let (_dir, mut h) = app();
    // Narrow areas, so a fifth one fits in the window.
    h.state_mut().config.column_width = 200.0;
    h.state_mut().board.add_area("Empty");
    h.run_steps(4);
    area_menu(&mut h, "Empty");
    h.get_by_label("Delete area…").click();
    h.run_steps(4);
    assert!(h.query_by_label("The area is empty.").is_some());
    button(&h, "Delete").click();
    h.run_steps(4);
    assert_eq!(area_names(&h).len(), 4);
}

#[test]
fn the_area_menu_folds_and_opens_an_area() {
    let (_dir, mut h) = app();
    let house = area_id(&h, "House");
    area_menu(&mut h, "House");
    h.get_by_label("Fold").click();
    h.run_steps(4);
    assert_eq!(h.state().config.folded_areas, [house]);
    assert!(h.query_by_label("House (folded)").is_some());
    // The strip's menu opens it again.
    let strip = h.get_by_label("House (folded)").rect().center();
    right_click_at(&mut h, strip);
    h.get_by_label("Open").click();
    h.run_steps(4);
    assert!(h.state().config.folded_areas.is_empty());
    assert!(h.query_by_label("Fix dripping tap").is_some());
}

#[test]
fn new_task_from_the_area_menu_opens_the_dialog_for_that_area() {
    let (_dir, mut h) = app();
    let family = area_id(&h, "Family");
    area_menu(&mut h, "Family");
    h.get_by_label("New task…").click();
    h.run_steps(4);
    let editor = h.state().task_editor.as_ref().expect("dialog opens");
    assert!(editor.id.is_none());
    assert_eq!(editor.details.area, family);
}

#[test]
fn several_areas_can_be_folded_at_once() {
    let (_dir, mut h) = app();
    for _ in 0..2 {
        h.get_all_by_label("Fold area").next().unwrap().click();
        h.run_steps(4);
    }
    assert_eq!(h.state().config.folded_areas.len(), 2);
    assert!(h.query_by_label("Work (folded)").is_some());
    assert!(h.query_by_label("Private ventures (folded)").is_some());
    assert_eq!(h.get_all_by_label("Fold area").count(), 2);
}

#[test]
fn area_color_from_the_dialog_and_back_to_none() {
    let (_dir, mut h) = app();
    let house = area_id(&h, "House");
    area_menu(&mut h, "House");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    h.get_by_label("Blue").click();
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(4);
    assert_eq!(
        h.state().board.areas[&house].color,
        Some(preset_colors(false)[5])
    );

    area_menu(&mut h, "House");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    assert_eq!(
        h.get_by_label("Blue").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "the current color is marked"
    );
    button(&h, "None").click();
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(4);
    assert_eq!(h.state().board.areas[&house].color, None);
}

#[test]
fn a_dark_area_color_renders_in_both_themes() {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let (_dir, mut h) = app_in(theme);
        let family = area_id(&h, "Family");
        h.state_mut()
            .board
            .set_area_color(family, Some([40, 50, 90]));
        h.run_steps(4);
        // Everything in it is still there and drawn.
        assert!(h.query_by_label("Plan weekend trip").is_some());
        h.render().expect("renders");
        // A very light one too.
        h.state_mut()
            .board
            .set_area_color(family, Some([250, 250, 240]));
        h.run_steps(4);
        h.render().expect("renders");
        // And folded.
        h.state_mut().config.folded_areas.push(family);
        h.run_steps(4);
        assert!(h.query_by_label("Family (folded)").is_some());
        h.render().expect("renders");
    }
}

// ----- Groups -----------------------------------------------------------

/// Open an area's Groups menu (`index`: which area, left to right).
fn groups_menu(h: &mut Harness, index: usize) {
    h.get_all(By::new().role(Role::Button).label_contains(" ▾"))
        .nth(index)
        .expect("groups button")
        .click();
    h.run_steps(4);
}

fn visible(h: &Harness, title: &str) -> bool {
    h.query_by_label(title).is_some()
}

#[test]
fn plus_new_adds_a_group_to_that_area() {
    let (_dir, mut h) = app();
    let house = area_id(&h, "House");
    h.get_all(By::new().role(Role::Button).label("+ New"))
        .nth(2)
        .unwrap()
        .click();
    h.run_steps(4);
    let editor = h.state().group_editor.as_ref().expect("dialog opens");
    assert_eq!(editor.area, house);
    assert!(editor.id.is_none());
    assert!(h.query_by_label("New group").is_some());
    // A new group gets a colour not used in the area yet.
    let colour = editor.color;
    assert!(
        h.state()
            .board
            .groups_of(house)
            .iter()
            .all(|g| g.color != colour)
    );
    h.event(egui::Event::Text("Cellar".into()));
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(4);
    let groups: Vec<String> = h
        .state()
        .board
        .groups_of(house)
        .iter()
        .map(|g| g.name.clone())
        .collect();
    assert!(groups.contains(&"Cellar".to_owned()), "{groups:?}");
}

#[test]
fn a_group_needs_a_name_and_enter_saves() {
    let (_dir, mut h) = app();
    let family = area_id(&h, "Family");
    h.get_all(By::new().role(Role::Button).label("+ New"))
        .nth(3)
        .unwrap()
        .click();
    h.run_steps(4);
    button(&h, "Save").click();
    h.run_steps(2);
    assert!(h.state().group_editor.is_some(), "no name, no save");
    h.event(egui::Event::Text("Kids".into()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert!(h.state().group_editor.is_none());
    assert_eq!(h.state().board.groups_of(family).len(), 1);
}

#[test]
fn the_groups_menu_filters_the_cards() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    groups_menu(&mut h, 0);
    // All groups of the area are listed, with All first.
    assert!(h.query_by_label("● Clients").is_some());
    assert!(h.query_by_label("● Admin").is_some());
    h.get_by_label("● Clients").click();
    h.run_steps(4);
    let clients = h
        .state()
        .board
        .groups_of(work)
        .iter()
        .find(|g| g.name == "Clients")
        .unwrap()
        .meta
        .id;
    assert_eq!(h.state().filters.get(&work), Some(&clients));
    assert!(visible(&h, "Proposal for Müller GmbH"));
    assert!(visible(&h, "Prepare kick-off workshop"));
    assert!(!visible(&h, "Send quarterly report"), "an Admin task");
    assert!(!visible(&h, "Read team feedback"), "no group");
    // The button now names the group.
    assert!(
        h.query(By::new().role(Role::Button).label("● Clients ▾"))
            .is_some()
    );
    // Other areas aren't filtered.
    assert!(visible(&h, "Fix dripping tap"));

    // All shows everything again.
    h.get_by_role_and_label(Role::Button, "● Clients ▾").click();
    h.run_steps(4);
    h.get_by_label("All").click();
    h.run_steps(4);
    assert!(!h.state().filters.contains_key(&work));
    assert!(visible(&h, "Send quarterly report"));
}

#[test]
fn an_area_without_groups_says_so() {
    let (_dir, mut h) = app();
    groups_menu(&mut h, 3);
    assert!(h.query_by_label("No groups yet").is_some());
    assert!(h.query_by_label("Edit group").is_none());
}

/// Groups menu of area `index` → Edit group → `name`.
fn edit_group(h: &mut Harness, index: usize, name: &str) {
    groups_menu(h, index);
    h.get_by_label("Edit group").hover();
    h.run_steps(4);
    h.get_by_label(&format!("● {name}…")).click();
    h.run_steps(4);
}

#[test]
fn edit_group_renames_it() {
    let (_dir, mut h) = app();
    edit_group(&mut h, 2, "Garden");
    let editor = h.state().group_editor.as_ref().expect("dialog opens");
    assert_eq!(editor.name, "Garden");
    assert!(h.query_by_label("Edit group").is_some());
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(2);
    type_into_focused(&mut h, "Yard");
    button(&h, "Save").click();
    h.run_steps(4);
    let board = &h.state().board;
    assert!(
        board
            .groups
            .values()
            .any(|g| g.name == "Yard" && !g.meta.deleted)
    );
    assert!(
        !board
            .groups
            .values()
            .any(|g| g.name == "Garden" && !g.meta.deleted)
    );
    // The Groups menu lists it under the new name.
    groups_menu(&mut h, 2);
    assert!(h.query_by_label("● Yard").is_some());
}

#[test]
fn edit_group_changes_its_color() {
    let (_dir, mut h) = app();
    edit_group(&mut h, 2, "Garden");
    h.state_mut().group_editor.as_mut().unwrap().color = [10, 20, 30];
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(4);
    let garden = h
        .state()
        .board
        .groups
        .values()
        .find(|g| g.name == "Garden")
        .unwrap();
    assert_eq!(garden.color, [10, 20, 30]);
}

#[test]
fn deleting_a_group_keeps_its_tasks_and_clears_the_filter() {
    let (_dir, mut h) = app();
    let house = area_id(&h, "House");
    // Filter House by Garden first.
    groups_menu(&mut h, 2);
    h.get_by_label("● Garden").click();
    h.run_steps(4);
    assert!(h.state().filters.contains_key(&house));

    h.get_by_role_and_label(Role::Button, "● Garden ▾").click();
    h.run_steps(4);
    h.get_by_label("Edit group").hover();
    h.run_steps(4);
    h.get_by_label("● Garden…").click();
    h.run_steps(4);
    button(&h, "Delete").click();
    h.run_steps(4);
    assert!(h.state().group_editor.is_none());
    assert!(!h.state().filters.contains_key(&house), "filter cleared");
    let hedge = task_id(&h, "Cut back the hedge");
    let group = h.state().board.tasks[&hedge].group;
    assert!(
        group.and_then(|g| h.state().board.group(g)).is_none(),
        "the task is without a group now"
    );
    assert!(
        h.state()
            .board
            .groups_of(house)
            .iter()
            .all(|g| g.name != "Garden")
    );
    assert!(visible(&h, "Cut back the hedge"));
    assert!(visible(&h, "Fix dripping tap"), "all of House shown again");
    // Undo brings the group back.
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(4);
    assert!(
        h.state()
            .board
            .groups_of(house)
            .iter()
            .any(|g| g.name == "Garden")
    );
}

#[test]
fn moving_a_filtered_group_away_clears_the_filter() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    let family = area_id(&h, "Family");
    groups_menu(&mut h, 0);
    h.get_by_label("● Admin").click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "● Admin ▾").click();
    h.run_steps(4);
    h.get_by_label("Edit group").hover();
    h.run_steps(4);
    h.get_by_label("● Admin…").click();
    h.run_steps(4);
    h.state_mut().group_editor.as_mut().unwrap().area = family;
    h.run_steps(2);
    button(&h, "Save").click();
    h.run_steps(4);
    assert!(!h.state().filters.contains_key(&work));
    let board = &h.state().board;
    let in_family: Vec<&str> = board
        .priority_list(family)
        .iter()
        .chain(board.top_list(family).iter())
        .map(|t| t.title.as_str())
        .collect();
    assert!(
        in_family.contains(&"Clean up shared drive"),
        "{in_family:?}"
    );
    assert!(
        in_family.contains(&"Send quarterly report"),
        "{in_family:?}"
    );
}

#[test]
fn cancelling_the_group_dialog_changes_nothing() {
    let (_dir, mut h) = app();
    let before = h.state().board.groups.clone();
    edit_group(&mut h, 0, "Clients");
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(2);
    type_into_focused(&mut h, "Customers");
    button(&h, "Cancel").click();
    h.run_steps(4);
    assert!(h.state().group_editor.is_none());
    assert_eq!(h.state().board.groups, before);
}

// ----- Settings ---------------------------------------------------------

#[test]
fn settings_open_from_the_menu_and_with_command_comma() {
    let (_dir, mut h) = app();
    menu(&mut h, "Settings…");
    assert!(h.state().settings_open);
    assert!(h.query_by_label("Settings").is_some());
    button(&h, "Done").click();
    h.run_steps(4);
    assert!(!h.state().settings_open);

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Comma);
    h.run_steps(4);
    assert!(h.state().settings_open);
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(!h.state().settings_open, "Esc closes it");
}

#[test]
fn settings_show_this_computer() {
    let (_dir, mut h) = app();
    menu(&mut h, "Settings…");
    assert!(h.query_by_label("preview").is_some(), "device name");
    assert!(
        h.query_by_label_contains("kept. A backup is made every day")
            .is_some()
    );
}

/// The settings dialog's number field showing `value`.
fn settings_number<'a>(h: &'a Harness, value: &'a str) -> egui_kittest::Node<'a> {
    h.get_by_value(value)
}

#[test]
fn the_deadline_window_moves_tasks_in_and_out_of_the_top() {
    let (_dir, mut h) = app();
    let ventures = area_id(&h, "Private ventures");
    // "File VAT return" is due in 12 days: in the list with the 10-day default.
    let vat = task_id(&h, "File VAT return");
    assert!(
        !h.state()
            .board
            .top_list(ventures)
            .iter()
            .any(|t| t.meta.id == vat)
    );
    menu(&mut h, "Settings…");
    settings_number(&h, "10 days ahead").set_value_accesskit(20.0);
    h.run_steps(4);
    assert_eq!(h.state().config.deadline_horizon_days, 20);
    button(&h, "Done").click();
    h.run_steps(4);
    assert!(
        h.state()
            .board
            .top_list(ventures)
            .iter()
            .any(|t| t.meta.id == vat)
    );

    menu(&mut h, "Settings…");
    settings_number(&h, "20 days ahead").set_value_accesskit(0.0);
    h.run_steps(4);
    button(&h, "Done").click();
    h.run_steps(4);
    let work = area_id(&h, "Work");
    // Only overdue / due today stay on top (plus pinned ones).
    let top: Vec<String> = h
        .state()
        .board
        .top_list(work)
        .iter()
        .map(|t| t.title.clone())
        .collect();
    assert!(
        !top.contains(&"Proposal for Müller GmbH".to_owned()),
        "{top:?}"
    );
    assert!(top.contains(&"Send quarterly report".to_owned()), "{top:?}");
}

#[test]
fn done_days_control_how_long_done_tasks_show() {
    let (_dir, mut h) = app();
    let family = area_id(&h, "Family");
    assert_eq!(h.state().board.done_list(family).len(), 1);
    assert!(h.query_by_label("DONE IN THE LAST 7 DAYS (1)").is_some());
    // Mark the done task as done 3 days ago.
    let done = h.state().board.done_list(family)[0].meta.id;
    let three_days_ago = jiff::Timestamp::now() - jiff::SignedDuration::from_hours(72);
    h.state_mut()
        .board
        .update_task(done, |t| t.completed_at = Some(three_days_ago));
    h.run_steps(4);
    menu(&mut h, "Settings…");
    settings_number(&h, "7 days").set_value_accesskit(1.0);
    h.run_steps(4);
    button(&h, "Done").click();
    h.run_steps(4);
    assert_eq!(h.state().config.done_days, 1);
    // Done 3 days ago: not "today" any more, so the section is gone.
    assert!(h.query_by_label_contains("DONE").is_none());
    // The toolbar follows: "done today".
    assert!(h.query_by_label_contains("done today").is_some());
}

#[test]
fn area_width_changes_and_default_restores_it() {
    let (_dir, mut h) = app();
    menu(&mut h, "Settings…");
    assert!(
        h.get_all(By::new().role(Role::Button).label("Default"))
            .next()
            .unwrap()
            .accesskit_node()
            .is_disabled(),
        "already the default"
    );
    settings_number(&h, "280 px").set_value_accesskit(400.0);
    h.run_steps(4);
    assert_eq!(h.state().config.column_width, 400.0);
    // Wider areas: the next area starts further right.
    button(&h, "Done").click();
    h.run_steps(4);
    let folds: Vec<f32> = h
        .get_all_by_label("Fold area")
        .map(|n| n.rect().center().x)
        .collect();
    assert!(folds[1] - folds[0] > 390.0, "{folds:?}");

    menu(&mut h, "Settings…");
    h.get_all(By::new().role(Role::Button).label("Default"))
        .next()
        .unwrap()
        .click();
    h.run_steps(4);
    assert_eq!(h.state().config.column_width, DEFAULT_COLUMN_WIDTH);
}

#[test]
fn area_width_stays_within_limits() {
    let (_dir, mut h) = app();
    menu(&mut h, "Settings…");
    settings_number(&h, "280 px").set_value_accesskit(5000.0);
    h.run_steps(4);
    assert_eq!(h.state().config.column_width, *COLUMN_WIDTHS.end());
}

#[test]
fn the_rainbow_scheme_colors_cards_red_to_blue() {
    let (_dir, mut h) = app();
    menu(&mut h, "Settings…");
    h.get_by_role(Role::ComboBox).click();
    h.run_steps(4);
    h.get_by_label("Rainbow").click();
    h.run_steps(4);
    assert_eq!(h.state().config.color_scheme, ColorScheme::Rainbow);
    button(&h, "Done").click();
    h.run_steps(4);
    // The first card is red-ish, the last blue-ish.
    let state = h.state();
    let mut first = None;
    let mut last = None;
    egui::__run_test_ui(|ui| {
        first = state.rainbow(ui, 0, 5);
        last = state.rainbow(ui, 4, 5);
    });
    let (first, last) = (first.unwrap(), last.unwrap());
    assert!(first.r() > first.b(), "{first:?}");
    assert!(last.b() > last.r(), "{last:?}");

    // A task's own colour beats the scheme.
    let id = task_id(&h, "Read team feedback");
    h.state_mut()
        .board
        .update_task(id, |t| t.color = Some([1, 2, 3]));
    let task = h.state().board.tasks[&id].clone();
    let mut fill = None;
    egui::__run_test_ui(|ui| fill = h.state().card_fill(ui, &task, 0, 5));
    assert_eq!(fill, Some(Color32::from_rgb(1, 2, 3)));
}

#[test]
fn the_standard_scheme_leaves_cards_plain() {
    let (_dir, h) = app();
    let task = h.state().board.tasks[&task_id(&h, "Read team feedback")].clone();
    let mut fill = Some(Color32::RED);
    egui::__run_test_ui(|ui| fill = h.state().card_fill(ui, &task, 0, 5));
    assert_eq!(fill, None);
}

#[test]
fn due_soon_color_default_button() {
    let (_dir, mut h) = app();
    h.state_mut().config.due_soon_color = Some([10, 100, 200]);
    h.run_steps(4);
    menu(&mut h, "Settings…");
    // Area width's Default is first, the colour's second.
    h.get_all(By::new().role(Role::Button).label("Default"))
        .nth(1)
        .unwrap()
        .click();
    h.run_steps(4);
    assert_eq!(h.state().config.due_soon_color, None);
}

#[test]
fn color_settings_render_in_both_themes() {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let (_dir, mut h) = app_in(theme);
        h.state_mut().config.color_scheme = ColorScheme::Rainbow;
        h.state_mut().config.due_soon_color = Some([70, 130, 230]);
        h.run_steps(4);
        assert!(h.query_all_by_label("DUE SOON · PINNED").next().is_some());
        h.render().expect("renders");
    }
}

#[test]
fn manage_backups_in_settings_opens_the_backups_dialog() {
    let (_dir, mut h) = app();
    menu(&mut h, "Settings…");
    button(&h, "Manage backups…").click();
    h.run_steps(4);
    assert!(!h.state().settings_open);
    assert!(h.state().backups_dialog.is_some());
}

// ----- Backups ----------------------------------------------------------

fn backup_count(h: &Harness) -> usize {
    h.state().backups.list().len()
}

#[test]
fn a_backup_is_made_at_startup() {
    let (dir, h) = app();
    let files: Vec<_> = std::fs::read_dir(dir.path().join("backups"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(
        files[0].starts_with(&format!("backup-{}-", today())),
        "{files:?}"
    );
    assert_eq!(backup_count(&h), 1);
}

#[test]
fn the_backups_dialog_lists_backups_with_their_time() {
    let (_dir, mut h) = app();
    menu(&mut h, "Backups…");
    assert!(h.state().backups_dialog.is_some());
    assert!(h.query_by_label("Backups").is_some());
    let label = backup_label(&h.state().backups.list()[0]);
    assert!(label.contains(" · "), "has a time: {label}");
    assert!(h.query_by_label(&label).is_some());
    // The demo data: 18 open tasks.
    assert!(h.query_by_label("18 open tasks").is_some());
    button(&h, "Close").click();
    h.run_steps(4);
    assert!(h.state().backups_dialog.is_none());
}

#[test]
fn back_up_now_adds_a_backup() {
    let (_dir, mut h) = app();
    menu(&mut h, "Backups…");
    // Backup names have the time to the second; wait for a new one.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    button(&h, "Back up now").click();
    h.run_steps(4);
    assert_eq!(backup_count(&h), 2);
    assert!(h.query_by_label("Backup saved.").is_some());
    assert_eq!(h.state().backups_dialog.as_ref().unwrap().entries.len(), 2);
}

#[test]
fn deleting_a_backup_asks_first() {
    let (_dir, mut h) = app();
    menu(&mut h, "Backups…");
    button(&h, "Delete…").click();
    h.run_steps(4);
    assert!(h.query_by_label("Delete backup?").is_some());
    // Cancel keeps it.
    button(&h, "Cancel").click();
    h.run_steps(4);
    assert_eq!(backup_count(&h), 1);
    assert!(h.query_by_label("Delete backup?").is_none());
    // Delete removes it.
    button(&h, "Delete…").click();
    h.run_steps(4);
    button(&h, "Delete").click();
    h.run_steps(4);
    assert_eq!(backup_count(&h), 0);
    assert!(h.query_by_label("Backup deleted.").is_some());
    assert!(h.query_by_label("No backups yet.").is_some());
}

#[test]
fn restoring_a_backup_brings_back_its_state_and_can_be_undone() {
    let (_dir, mut h) = app();
    // A change after the startup backup.
    let work = area_id(&h, "Work");
    h.state_mut()
        .board
        .add_task(work, "Added later", None, None);
    h.run_steps(4);
    assert!(visible(&h, "Added later"));

    std::thread::sleep(std::time::Duration::from_millis(1100));
    menu(&mut h, "Backups…");
    button(&h, "Restore…").click();
    h.run_steps(4);
    assert!(h.query_by_label("Restore backup?").is_some());
    button(&h, "Restore").click();
    h.run_steps(4);
    assert!(h.state().backups_dialog.is_none(), "closes after restoring");
    assert!(!visible(&h, "Added later"));
    assert!(visible(&h, "Send quarterly report"));
    // The state before was backed up first.
    assert_eq!(backup_count(&h), 2);

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(4);
    assert!(visible(&h, "Added later"), "⌘Z undoes the restore");
}

#[test]
fn cancelling_a_restore_changes_nothing() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    h.state_mut()
        .board
        .add_task(work, "Added later", None, None);
    h.run_steps(4);
    menu(&mut h, "Backups…");
    button(&h, "Restore…").click();
    h.run_steps(4);
    button(&h, "Cancel").click();
    h.run_steps(4);
    assert!(h.state().backups_dialog.is_some(), "back in the list");
    button(&h, "Close").click();
    h.run_steps(4);
    assert!(visible(&h, "Added later"));
    assert_eq!(backup_count(&h), 1);
}

#[test]
fn an_unreadable_backup_cant_be_restored() {
    let (dir, mut h) = app();
    std::fs::write(
        dir.path()
            .join("backups")
            .join("backup-2026-01-01-120000.json"),
        "not json",
    )
    .unwrap();
    menu(&mut h, "Backups…");
    assert!(h.query_by_label("unreadable").is_some());
    let restores: Vec<bool> = h
        .get_all(By::new().role(Role::Button).label("Restore…"))
        .map(|n| n.accesskit_node().is_disabled())
        .collect();
    // Newest first: today's works, the broken old one doesn't.
    assert_eq!(restores, [false, true]);
}

// ----- About, shortcuts, menu, toolbar ----------------------------------

#[test]
fn about_shows_name_version_and_credits() {
    let (_dir, mut h) = app();
    menu(&mut h, "About Priogator");
    assert!(h.state().about_open);
    for text in [
        "Priority tracker",
        "Designed by Andreas Zoglauer",
        "Implemented by Claude",
        "Free software under the Apache License 2.0",
    ] {
        assert!(h.query_by_label(text).is_some(), "{text}");
    }
    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
    assert!(h.query_by_label(&version).is_some());
    button(&h, "Close").click();
    h.run_steps(4);
    assert!(!h.state().about_open);
}

#[test]
fn about_license_page_shows_the_apache_license() {
    let (_dir, mut h) = app();
    menu(&mut h, "About Priogator");
    h.get_by_label("License").click();
    // The dialog grows and re-centres over a few frames.
    h.run_steps(12);
    assert!(h.state().about_page == AboutPage::License);
    assert!(
        h.query_by_label_contains("copyright 2026 Andreas Zoglauer")
            .is_some()
    );
    assert!(h.query_by_label_contains("TERMS AND CONDITIONS").is_some());
    button(&h, "‹ Back").click();
    h.run_steps(4);
    assert!(h.state().about_page == AboutPage::Main);
    assert!(h.query_by_label("Priority tracker").is_some());
}

#[test]
fn about_acknowledgements_thank_the_libraries() {
    let (_dir, mut h) = app();
    menu(&mut h, "About Priogator");
    h.get_by_label("Acknowledgements").click();
    h.run_steps(12);
    assert!(h.state().about_page == AboutPage::Acknowledgements);
    for (project, people, _) in THANKS {
        assert!(h.query_by_label(project).is_some(), "{project}");
        assert!(h.query_by_label(people).is_some(), "{people}");
    }
    let count = crate::licenses::library_count(crate::licenses::third_party());
    assert!(
        h.query_by_label_contains(&format!("Priogator includes {count} open-source libraries"))
            .is_some()
    );
    // Licenses are grouped; opening one shows who uses it.
    let mit = h.get_by_label_contains("MIT License · ");
    mit.scroll_to_me();
    h.run_steps(4);
    h.get_by_label_contains("MIT License · ").click();
    h.run_steps(4);
    assert!(h.query_all_by_label("Used by ").next().is_some());
    // Close from a sub-page closes, and About opens on its main page again.
    button(&h, "Close").click();
    h.run_steps(4);
    assert!(!h.state().about_open);
    menu(&mut h, "About Priogator");
    assert!(h.state().about_page == AboutPage::Main);
}

#[test]
fn keyboard_shortcuts_are_listed() {
    let (_dir, mut h) = app();
    menu(&mut h, "Keyboard shortcuts");
    assert!(h.state().shortcuts_open);
    for text in [
        "⌘N",
        "⌘Z / ⇧⌘Z",
        "Undo / redo",
        "Settings",
        "On Linux, use Ctrl instead of ⌘.",
    ] {
        assert!(h.query_by_label(text).is_some(), "{text}");
    }
    button(&h, "Close").click();
    h.run_steps(4);
    assert!(!h.state().shortcuts_open);
}

#[test]
fn new_task_in_the_menu_opens_the_dialog_for_the_first_area() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    menu(&mut h, "New task…");
    let editor = h.state().task_editor.as_ref().expect("dialog opens");
    assert_eq!(editor.details.area, work);
}

#[test]
fn the_toolbar_shows_name_buttons_and_statistics() {
    let (_dir, h) = app();
    assert!(h.query_all_by_label("Priogator").next().is_some());
    assert!(
        h.query(By::new().role(Role::Button).label("+ New task"))
            .is_some()
    );
    assert!(h.query_by_label("18 open").is_some());
    assert!(h.query_by_label("1 overdue").is_some());
    // Undo/redo start disabled.
    assert!(button(&h, "↶").accesskit_node().is_disabled());
    assert!(button(&h, "↷").accesskit_node().is_disabled());
}

#[test]
fn dialogs_block_keyboard_shortcuts() {
    let (_dir, mut h) = app();
    menu(&mut h, "About Priogator");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
    h.run_steps(4);
    assert!(
        h.state().task_editor.is_none(),
        "⌘N waits while a dialog is open"
    );
}

// ----- Syncing ----------------------------------------------------------

/// Let the app look at the sync folder now instead of in two seconds.
fn scan_now(h: &mut Harness) {
    h.state_mut().last_scan = Instant::now() - SCAN_INTERVAL;
    h.run_steps(4);
}

#[test]
fn tasks_from_another_computer_appear() {
    let (dir, mut h) = app();
    let sync = dir.path().join("sync");
    // The laptop has the same areas (default ids) and adds a task to Work.
    let mut laptop = crate::board::Board::new("laptop");
    laptop.add_default_areas();
    let work = laptop.areas_sorted()[0].meta.id;
    laptop.add_task(work, "Written on the laptop", None, None);
    let mut folder = SyncFolder::new(&sync, "laptop");
    folder.save(&laptop).unwrap();

    scan_now(&mut h);
    assert!(visible(&h, "Written on the laptop"));
    assert!(
        h.state()
            .board
            .priority_list(work)
            .iter()
            .any(|t| t.title == "Written on the laptop")
    );
    // Undo can't take back the other computer's changes.
    assert!(h.state().undo.is_empty());
}

#[test]
fn changes_are_saved_to_this_computers_file() {
    let (dir, mut h) = app();
    button(&h, "+ Area").click();
    h.run_steps(4);
    h.event(egui::Event::Text("Saved area".into()));
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    let own = crate::storage::read_board_file(
        &dir.path().join("sync").join("device-preview.json"),
        "preview",
    )
    .unwrap();
    assert!(own.areas.values().any(|a| a.name == "Saved area"));
}

#[test]
fn a_broken_file_shows_a_problem_that_can_be_dismissed() {
    let (dir, mut h) = app();
    std::fs::write(dir.path().join("sync").join("device-broken.json"), "{ nope").unwrap();
    scan_now(&mut h);
    assert_eq!(h.state().errors.len(), 1, "{:?}", h.state().errors);
    let warning = h.get_by_label("⚠ 1 problem");
    warning.click();
    h.run_steps(4);
    assert!(h.state().errors.is_empty());
    assert!(h.query_by_label("⚠ 1 problem").is_none());
    // The rest still works.
    assert!(visible(&h, "Send quarterly report"));
}

// ----- Undo / redo of area and group changes ----------------------------

#[test]
fn renaming_an_area_is_undone_and_redone_with_the_toolbar_buttons() {
    let (_dir, mut h) = app();
    let house = area_id(&h, "House");
    area_menu(&mut h, "House");
    h.get_by_label("Edit area…").click();
    h.run_steps(4);
    h.get_by_role(Role::TextInput).focus();
    h.run_steps(2);
    type_into_focused(&mut h, "Home");
    button(&h, "Save").click();
    h.run_steps(4);
    assert_eq!(h.state().board.areas[&house].name, "Home");

    button(&h, "↶").click();
    h.run_steps(4);
    assert_eq!(h.state().board.areas[&house].name, "House");
    button(&h, "↷").click();
    h.run_steps(4);
    assert_eq!(h.state().board.areas[&house].name, "Home");
}

#[test]
fn a_deleted_area_comes_back_with_undo_and_goes_again_with_redo() {
    let (_dir, mut h) = app();
    area_menu(&mut h, "Private ventures");
    h.get_by_label("Delete area…").click();
    h.run_steps(4);
    button(&h, "Delete").click();
    h.run_steps(4);
    assert_eq!(area_names(&h), ["Work", "House", "Family"]);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run_steps(4);
    assert_eq!(area_names(&h).len(), 4);
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::Z,
    );
    h.run_steps(4);
    assert_eq!(area_names(&h), ["Work", "House", "Family"]);
}

#[test]
fn escape_closes_dialogs_without_saving() {
    let (_dir, mut h) = app();
    // Area dialog.
    button(&h, "+ Area").click();
    h.run_steps(4);
    h.event(egui::Event::Text("Not saved".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(h.state().area_editor.is_none());
    assert_eq!(area_names(&h).len(), 4);
    // Group dialog.
    h.get_all(By::new().role(Role::Button).label("+ New"))
        .next()
        .unwrap()
        .click();
    h.run_steps(4);
    h.event(egui::Event::Text("Not saved".into()));
    h.run_steps(2);
    h.key_press(egui::Key::Escape);
    h.run_steps(4);
    assert!(h.state().group_editor.is_none());
    assert!(
        !h.state()
            .board
            .groups
            .values()
            .any(|g| g.name == "Not saved")
    );
    // About, shortcuts, backups.
    for entry in ["About Priogator", "Keyboard shortcuts", "Backups…"] {
        menu(&mut h, entry);
        assert!(h.state().editing(), "{entry} opened");
        h.key_press(egui::Key::Escape);
        h.run_steps(4);
        assert!(!h.state().editing(), "{entry} closed with Esc");
    }
}

#[test]
fn a_group_filter_belongs_to_one_area() {
    let (_dir, mut h) = app();
    let work = area_id(&h, "Work");
    let house = area_id(&h, "House");
    groups_menu(&mut h, 0);
    h.get_by_label("● Admin").click();
    h.run_steps(4);
    // House's menu is the third "▾" button now that Work names its group.
    groups_menu(&mut h, 2);
    h.get_by_label("● Repairs").click();
    h.run_steps(4);
    assert_eq!(h.state().filters.len(), 2);
    assert!(h.state().filters.contains_key(&work));
    assert!(h.state().filters.contains_key(&house));
    assert!(visible(&h, "Clean up shared drive"));
    assert!(!visible(&h, "Proposal for Müller GmbH"));
    assert!(visible(&h, "Fix dripping tap"));
    assert!(!visible(&h, "Cut back the hedge"));
    // Untouched areas show everything.
    assert!(visible(&h, "Plan weekend trip"));
}

#[test]
fn the_done_heading_names_the_time_window() {
    let (_dir, mut h) = app();
    assert!(h.query_by_label("DONE IN THE LAST 7 DAYS (1)").is_some());
    menu(&mut h, "Settings…");
    settings_number(&h, "7 days").set_value_accesskit(14.0);
    h.run_steps(4);
    button(&h, "Done").click();
    h.run_steps(4);
    assert!(h.query_by_label("DONE IN THE LAST 14 DAYS (1)").is_some());
    // A one-day window reads like the toolbar: "today".
    h.state_mut().config.done_days = 1;
    h.run_steps(4);
    assert!(h.query_by_label("DONE TODAY (1)").is_some());
}

// ----- Updates (macOS) ---------------------------------------------------

/// GitHub's answer naming `tag` as the latest release.
#[cfg(target_os = "macos")]
fn release_answer(tag: &str) -> crate::update::Fetch {
    let json = format!(
        r#"{{"tag_name": "{tag}", "html_url": "https://example.com/{tag}",
            "body": "Nicer colours.",
            "assets": [{{"browser_download_url": "https://example.com/Priogator.dmg"}}]}}"#
    );
    std::sync::Arc::new(move || Ok(json.clone()))
}

/// Let the background check finish.
#[cfg(target_os = "macos")]
fn wait_for_update_check(h: &mut Harness) {
    for _ in 0..300 {
        h.run_steps(1);
        if h.state().updater.state() != crate::update::State::Checking {
            h.run_steps(4);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the update check didn't finish");
}

#[cfg(target_os = "macos")]
#[test]
fn checking_for_updates_says_when_this_is_the_latest_version() {
    let (_dir, mut h) = app();
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    h.state_mut().updater = crate::update::Updater::with_fetch(release_answer(&current));
    menu(&mut h, "Check for Updates…");
    wait_for_update_check(&mut h);
    assert!(h.query_by_label_contains("is the latest version").is_some());
    assert!(h.query_by_label_contains("Update to").is_none());
    button(&h, "OK").click();
    h.run_steps(4);
    assert!(h.query_by_label("Updates").is_none(), "dialog closed");
}

#[cfg(target_os = "macos")]
#[test]
fn a_newer_version_is_offered_in_the_toolbar_until_installed() {
    let (_dir, mut h) = app();
    h.state_mut().updater = crate::update::Updater::with_fetch(release_answer("v99.0.0"));
    menu(&mut h, "Check for Updates…");
    wait_for_update_check(&mut h);
    assert!(
        h.query_by_label_contains("Priogator 99.0.0 is available")
            .is_some()
    );
    assert!(
        h.query_by_label("Nicer colours.").is_some(),
        "release notes shown"
    );
    button(&h, "Later").click();
    h.run_steps(4);
    // Still offered, and the toolbar button opens the dialog again.
    button(&h, "Update to 99.0.0").click();
    h.run_steps(4);
    assert!(h.query_by_label("Install and Restart").is_some());
}

#[cfg(target_os = "macos")]
#[test]
fn a_test_or_development_build_never_replaces_itself() {
    let (_dir, mut h) = app();
    h.state_mut().updater = crate::update::Updater::with_fetch(release_answer("v99.0.0"));
    menu(&mut h, "Check for Updates…");
    wait_for_update_check(&mut h);
    button(&h, "Install and Restart").click();
    h.run_steps(4);
    assert!(h.query_by_label("Couldn't update.").is_some());
    assert!(
        h.query_by_label_contains("isn't an installed Priogator app")
            .is_some()
    );
    assert!(matches!(
        h.state().updater.state(),
        crate::update::State::Failed(_)
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn a_failed_check_explains_itself() {
    let (_dir, mut h) = app();
    h.state_mut().updater = crate::update::Updater::with_fetch(std::sync::Arc::new(|| {
        Err("Couldn't reach GitHub.".into())
    }));
    menu(&mut h, "Check for Updates…");
    wait_for_update_check(&mut h);
    assert!(h.query_by_label("Couldn't update.").is_some());
    assert!(h.query_by_label("Couldn't reach GitHub.").is_some());
    assert!(h.query_by_label_contains("releases page").is_some());
    button(&h, "OK").click();
    h.run_steps(4);
    assert_eq!(h.state().updater.state(), crate::update::State::Idle);
}

#[cfg(target_os = "macos")]
#[test]
fn the_automatic_check_follows_the_setting() {
    let (_dir, mut h) = app();
    h.state_mut().updater = crate::update::Updater::with_fetch(release_answer("v99.0.0"));
    // Off (as in all tests): nothing is checked.
    h.run_steps(4);
    assert_eq!(h.state().updater.state(), crate::update::State::Idle);
    menu(&mut h, "Settings…");
    h.get_by_label("Check for updates automatically (once a day)")
        .click();
    h.run_steps(2);
    assert!(h.state().config.check_updates);
    button(&h, "Done").click();
    wait_for_update_check(&mut h);
    assert!(
        h.query_by_label("Update to 99.0.0").is_some(),
        "found by itself"
    );
}
