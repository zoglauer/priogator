//! Example data for trying the app out (`cargo run -- --demo`).

use jiff::civil::Date;

use crate::board::{Board, GROUP_COLORS, TaskDetails};
use crate::repeat::{Repeat, Unit};

/// Add example groups and tasks to the default columns.
/// Does nothing if the board already has tasks.
pub fn fill(board: &mut Board, today: Date) {
    if board.tasks.values().any(|t| !t.meta.deleted) {
        return;
    }
    let areas: Vec<_> = board.areas_sorted().iter().map(|a| a.meta.id).collect();
    let [work, ventures, house, family, ..] = areas[..] else {
        return;
    };
    let in_days = |n: i64| Some(today.saturating_add(jiff::Span::new().days(n)));

    let clients = board.add_group(work, "Clients", GROUP_COLORS[0]);
    let admin = board.add_group(work, "Admin", GROUP_COLORS[1]);
    let garden = board.add_group(house, "Garden", GROUP_COLORS[2]);
    let repairs = board.add_group(house, "Repairs", GROUP_COLORS[3]);

    board.add_task(work, "Send quarterly report", Some(admin), in_days(-1));
    board.add_task(work, "Proposal for Müller GmbH", Some(clients), in_days(3));
    let workshop = board.add_task(
        work,
        "Prepare kick-off workshop",
        Some(clients),
        in_days(24),
    );
    let room = board.add_subtask(workshop, "Book a room");
    board.set_subtask_done(workshop, room, true);
    board.add_subtask(workshop, "Send invitations");
    board.add_subtask(workshop, "Prepare slides");
    board.add_task(work, "Clean up shared drive", Some(admin), None);
    board.add_task(work, "Read team feedback", None, None);
    let bank = board.add_task(work, "Call back the bank", None, None);
    board.pin(bank, work);

    board.add_task(ventures, "File VAT return", None, in_days(12));
    board.add_task(ventures, "Sketch landing page", None, None);
    board.add_task(ventures, "Ask Tom about domain names", None, None);

    board.add_task(
        house,
        "Chimney sweep appointment",
        Some(repairs),
        in_days(0),
    );
    board.add_task(house, "Fix dripping tap", Some(repairs), None);
    board.add_task(house, "Cut back the hedge", Some(garden), None);
    board.add_task(house, "Order tulip bulbs", Some(garden), None);

    // Recurring: the trash every Friday, shown on top from the day before;
    // the tire pressure two months after it was last checked.
    let friday = (0..7)
        .filter_map(in_days)
        .find(|d| d.weekday() == jiff::civil::Weekday::Friday)
        .unwrap_or(today);
    let trash = board.add_task(house, "Take out the trash", None, Some(friday));
    let mut details = TaskDetails::from(&board.tasks[&trash]);
    details.lead_days = Some(1);
    details.repeat = Some(Repeat::Schedule {
        every: 1,
        unit: Unit::Weeks,
        weekdays: vec![5],
    });
    board.update_details(trash, details);
    let tires = board.add_task(house, "Check tire pressure", Some(repairs), in_days(45));
    let mut details = TaskDetails::from(&board.tasks[&tires]);
    details.repeat = Some(Repeat::AfterDone {
        every: 2,
        unit: Unit::Months,
    });
    board.update_details(tires, details);

    board.add_task(family, "Birthday present for Lena", None, in_days(6));
    let trip = board.add_task(family, "Plan weekend trip", None, None);
    board.add_subtask(trip, "Check the weather");
    board.add_subtask(trip, "Book a hotel");
    board.add_task(family, "Renew passports", None, in_days(40));
    let done = board.add_task(family, "Book dentist appointment", None, None);
    board.set_done(done, true);
}
