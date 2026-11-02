//! Recurring tasks: when the next occurrence is due.

use jiff::Span;
use jiff::civil::{Date, Weekday};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Days,
    Weeks,
    Months,
    Years,
}

impl Unit {
    pub const ALL: [Unit; 4] = [Unit::Days, Unit::Weeks, Unit::Months, Unit::Years];

    /// "day" / "days" etc.
    pub fn name(self, count: u32) -> &'static str {
        let one = count == 1;
        match self {
            Unit::Days if one => "day",
            Unit::Days => "days",
            Unit::Weeks if one => "week",
            Unit::Weeks => "weeks",
            Unit::Months if one => "month",
            Unit::Months => "months",
            Unit::Years if one => "year",
            Unit::Years => "years",
        }
    }

    fn span(self, count: u32) -> Span {
        let n = i64::from(count);
        match self {
            Unit::Days => Span::new().days(n),
            Unit::Weeks => Span::new().weeks(n),
            Unit::Months => Span::new().months(n),
            Unit::Years => Span::new().years(n),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Repeat {
    /// On the calendar: every `every` units from the due date; for weeks
    /// optionally on given weekdays (1 = Monday … 7 = Sunday).
    Schedule {
        every: u32,
        unit: Unit,
        #[serde(default)]
        weekdays: Vec<i8>,
    },
    /// `every` units after the day it was done.
    AfterDone { every: u32, unit: Unit },
}

impl Repeat {
    /// The next due date, after the occurrence due on `due` was done (or
    /// skipped) on `today`. A schedule moves on to its next date after today,
    /// so missed occurrences don't pile up.
    pub fn next(&self, due: Date, today: Date) -> Date {
        match self {
            Repeat::AfterDone { every, unit } => today.saturating_add(unit.span((*every).max(1))),
            Repeat::Schedule { .. } => {
                let mut date = self.step(due);
                while date <= today {
                    date = self.step(date);
                }
                date
            }
        }
    }

    /// The scheduled date after `from`.
    fn step(&self, from: Date) -> Date {
        let Repeat::Schedule {
            every,
            unit,
            weekdays,
        } = self
        else {
            return from;
        };
        let every = (*every).max(1);
        if *unit != Unit::Weeks || weekdays.is_empty() {
            return from.saturating_add(unit.span(every));
        }
        let on = |d: Date| weekdays.contains(&d.weekday().to_monday_one_offset());
        // Later in the same week?
        let mut d = from;
        while d.weekday() != Weekday::Sunday {
            d = d.tomorrow().unwrap_or(d);
            if on(d) {
                return d;
            }
        }
        // Otherwise the first chosen weekday `every` weeks on.
        let monday = from
            .saturating_sub(Span::new().days(i64::from(from.weekday().to_monday_zero_offset())))
            .saturating_add(Span::new().weeks(i64::from(every)));
        (0..7)
            .map(|i| monday.saturating_add(Span::new().days(i)))
            .find(|d| on(*d))
            .unwrap_or(monday)
    }

    /// E.g. "Every Friday", "Every 2 weeks on Mon, Thu", "2 months after done".
    pub fn summary(&self) -> String {
        match self {
            Repeat::AfterDone { every, unit } => {
                format!("{every} {} after done", unit.name(*every))
            }
            Repeat::Schedule {
                every,
                unit,
                weekdays,
            } => {
                let days = weekday_names(weekdays);
                match (every, *unit, days.is_empty()) {
                    (1, Unit::Weeks, false) => format!("Every {days}"),
                    (n, Unit::Weeks, false) => format!("Every {n} weeks on {days}"),
                    (1, unit, _) => format!("Every {}", unit.name(1)),
                    (n, unit, _) => format!("Every {n} {}", unit.name(*n)),
                }
            }
        }
    }
}

const WEEKDAY_SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAY_LONG: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Two-letter labels for the weekday buttons, Monday first.
pub const WEEKDAY_BUTTONS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// "Friday", or "Mon, Thu" for several.
fn weekday_names(weekdays: &[i8]) -> String {
    let mut days: Vec<i8> = weekdays
        .iter()
        .copied()
        .filter(|d| (1..=7).contains(d))
        .collect();
    days.sort_unstable();
    days.dedup();
    match days.as_slice() {
        [one] => WEEKDAY_LONG[(*one - 1) as usize].to_owned(),
        _ => days
            .iter()
            .map(|d| WEEKDAY_SHORT[(*d - 1) as usize])
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn weekly(weekdays: &[i8]) -> Repeat {
        Repeat::Schedule {
            every: 1,
            unit: Unit::Weeks,
            weekdays: weekdays.to_vec(),
        }
    }

    #[test]
    fn every_friday() {
        let fridays = weekly(&[5]);
        let friday = date(2026, 10, 30);
        // Done on the day: next Friday.
        assert_eq!(fridays.next(friday, friday), date(2026, 11, 6));
        // Done early (Thursday): still next Friday.
        assert_eq!(fridays.next(friday, date(2026, 10, 29)), date(2026, 11, 6));
        // Done late (Monday): the coming Friday, missed ones don't pile up.
        assert_eq!(fridays.next(friday, date(2026, 11, 2)), date(2026, 11, 6));
        // Away for three weeks: the next Friday after today.
        assert_eq!(fridays.next(friday, date(2026, 11, 18)), date(2026, 11, 20));
        assert_eq!(fridays.summary(), "Every Friday");
    }

    #[test]
    fn several_weekdays_and_every_other_week() {
        let mon_thu = weekly(&[4, 1]);
        let monday = date(2026, 11, 2);
        assert_eq!(mon_thu.next(monday, monday), date(2026, 11, 5));
        assert_eq!(
            mon_thu.next(date(2026, 11, 5), date(2026, 11, 5)),
            date(2026, 11, 9)
        );
        assert_eq!(mon_thu.summary(), "Every Mon, Thu");

        let other_tuesday = Repeat::Schedule {
            every: 2,
            unit: Unit::Weeks,
            weekdays: vec![2],
        };
        assert_eq!(
            other_tuesday.next(date(2026, 11, 3), date(2026, 11, 3)),
            date(2026, 11, 17)
        );
        assert_eq!(other_tuesday.summary(), "Every 2 weeks on Tuesday");
    }

    #[test]
    fn months_and_years() {
        let monthly = Repeat::Schedule {
            every: 1,
            unit: Unit::Months,
            weekdays: vec![],
        };
        assert_eq!(
            monthly.next(date(2026, 11, 1), date(2026, 11, 1)),
            date(2026, 12, 1)
        );
        // The 31st in a short month becomes its last day.
        assert_eq!(
            monthly.next(date(2027, 1, 31), date(2027, 1, 31)),
            date(2027, 2, 28)
        );
        assert_eq!(monthly.summary(), "Every month");

        let yearly = Repeat::Schedule {
            every: 1,
            unit: Unit::Years,
            weekdays: vec![],
        };
        assert_eq!(
            yearly.next(date(2026, 12, 24), date(2026, 12, 24)),
            date(2027, 12, 24)
        );
    }

    #[test]
    fn after_done_counts_from_the_day_it_was_done() {
        let tires = Repeat::AfterDone {
            every: 2,
            unit: Unit::Months,
        };
        // Due Nov 1, but checked on Nov 10: next one two months after that.
        assert_eq!(
            tires.next(date(2026, 11, 1), date(2026, 11, 10)),
            date(2027, 1, 10)
        );
        assert_eq!(tires.summary(), "2 months after done");
    }

    #[test]
    fn stored_as_readable_json() {
        let json = serde_json::to_string(&weekly(&[5])).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"schedule","every":1,"unit":"weeks","weekdays":[5]}"#
        );
        let back: Repeat = serde_json::from_str(&json).unwrap();
        assert_eq!(back, weekly(&[5]));
    }

    fn schedule(every: u32, unit: Unit) -> Repeat {
        Repeat::Schedule {
            every,
            unit,
            weekdays: vec![],
        }
    }

    #[test]
    fn every_zero_counts_as_every_one() {
        let day = date(2026, 11, 2);
        assert_eq!(schedule(0, Unit::Days).next(day, day), date(2026, 11, 3));
        let zero = Repeat::AfterDone {
            every: 0,
            unit: Unit::Weeks,
        };
        assert_eq!(zero.next(day, day), date(2026, 11, 9));
    }

    #[test]
    fn leap_days_and_month_ends() {
        // Jan 31 in a leap year: Feb 29.
        let monthly = schedule(1, Unit::Months);
        assert_eq!(
            monthly.next(date(2028, 1, 31), date(2028, 1, 31)),
            date(2028, 2, 29)
        );
        // A yearly task on Feb 29 lands on Feb 28 in other years.
        let yearly = schedule(1, Unit::Years);
        assert_eq!(
            yearly.next(date(2028, 2, 29), date(2028, 2, 29)),
            date(2029, 2, 28)
        );
        // Every 3 months from the 30th of November.
        let quarterly = schedule(3, Unit::Months);
        assert_eq!(
            quarterly.next(date(2026, 11, 30), date(2026, 11, 30)),
            date(2027, 2, 28)
        );
    }

    #[test]
    fn missed_occurrences_are_skipped_for_every_unit() {
        let today = date(2026, 10, 29);
        let every_three_days = schedule(3, Unit::Days);
        // Oct 1 + 30 days is the first date after today.
        assert_eq!(
            every_three_days.next(date(2026, 10, 1), today),
            date(2026, 10, 31)
        );
        let monthly = schedule(1, Unit::Months);
        assert_eq!(monthly.next(date(2026, 1, 15), today), date(2026, 11, 15));
        let yearly = schedule(1, Unit::Years);
        assert_eq!(yearly.next(date(2020, 3, 1), today), date(2027, 3, 1));
    }

    #[test]
    fn a_future_due_date_moves_one_step_on() {
        // Done early, long before the due date: the next date after that.
        let weekly = schedule(1, Unit::Weeks);
        assert_eq!(
            weekly.next(date(2026, 12, 4), date(2026, 10, 29)),
            date(2026, 12, 11)
        );
    }

    #[test]
    fn after_done_ignores_the_due_date() {
        let today = date(2026, 10, 29);
        let after = Repeat::AfterDone {
            every: 10,
            unit: Unit::Days,
        };
        for due in [date(2026, 1, 1), today, date(2027, 6, 1)] {
            assert_eq!(after.next(due, today), date(2026, 11, 8));
        }
        let yearly = Repeat::AfterDone {
            every: 1,
            unit: Unit::Years,
        };
        assert_eq!(yearly.next(today, today), date(2027, 10, 29));
    }

    #[test]
    fn weekdays_may_come_unsorted_duplicated_or_invalid() {
        let messy = weekly(&[5, 1, 1, 9, 0]);
        assert_eq!(messy.summary(), "Every Mon, Fri");
        let monday = date(2026, 11, 2);
        assert_eq!(messy.next(monday, monday), date(2026, 11, 6));
        let friday = date(2026, 11, 6);
        assert_eq!(messy.next(friday, friday), date(2026, 11, 9));
    }

    #[test]
    fn only_invalid_weekdays_still_moves_on() {
        let nonsense = weekly(&[9]);
        let monday = date(2026, 11, 2);
        let next = nonsense.next(monday, monday);
        assert!(next > monday, "{next}");
        assert_eq!(nonsense.summary(), "Every week");
    }

    #[test]
    fn sundays_end_the_week() {
        let sunday = date(2026, 11, 1);
        assert_eq!(weekly(&[7]).next(sunday, sunday), date(2026, 11, 8));
        assert_eq!(weekly(&[1, 7]).next(sunday, sunday), date(2026, 11, 2));
        // From Saturday, Sunday is still this week.
        let saturday = date(2026, 10, 31);
        assert_eq!(weekly(&[7]).next(saturday, saturday), sunday);
    }

    #[test]
    fn every_other_week_with_several_days() {
        let mon_thu = Repeat::Schedule {
            every: 2,
            unit: Unit::Weeks,
            weekdays: vec![1, 4],
        };
        let monday = date(2026, 11, 2);
        // Thursday of the same week, then Monday two weeks on.
        assert_eq!(mon_thu.next(monday, monday), date(2026, 11, 5));
        let thursday = date(2026, 11, 5);
        assert_eq!(mon_thu.next(thursday, thursday), date(2026, 11, 16));
        assert_eq!(mon_thu.summary(), "Every 2 weeks on Mon, Thu");
    }

    #[test]
    fn summaries_for_every_kind() {
        let cases = [
            (schedule(1, Unit::Days), "Every day"),
            (schedule(3, Unit::Days), "Every 3 days"),
            (schedule(1, Unit::Weeks), "Every week"),
            (schedule(2, Unit::Weeks), "Every 2 weeks"),
            (schedule(6, Unit::Months), "Every 6 months"),
            (schedule(1, Unit::Years), "Every year"),
            (weekly(&[1, 2, 3, 4, 5]), "Every Mon, Tue, Wed, Thu, Fri"),
            (
                Repeat::AfterDone {
                    every: 1,
                    unit: Unit::Weeks,
                },
                "1 week after done",
            ),
            (
                Repeat::AfterDone {
                    every: 3,
                    unit: Unit::Days,
                },
                "3 days after done",
            ),
            (
                Repeat::AfterDone {
                    every: 1,
                    unit: Unit::Years,
                },
                "1 year after done",
            ),
        ];
        for (repeat, text) in cases {
            assert_eq!(repeat.summary(), text);
        }
    }

    #[test]
    fn unit_names_are_singular_for_one() {
        for unit in Unit::ALL {
            assert!(!unit.name(1).ends_with('s'), "{:?}", unit);
            assert!(unit.name(2).ends_with('s'), "{:?}", unit);
            assert!(unit.name(0).ends_with('s'), "{:?}", unit);
        }
    }

    #[test]
    fn after_done_and_old_schedules_load_from_json() {
        let after: Repeat =
            serde_json::from_str(r#"{"kind":"after_done","every":2,"unit":"months"}"#).unwrap();
        assert_eq!(
            after,
            Repeat::AfterDone {
                every: 2,
                unit: Unit::Months
            }
        );
        // Written without weekdays.
        let plain: Repeat =
            serde_json::from_str(r#"{"kind":"schedule","every":1,"unit":"days"}"#).unwrap();
        assert_eq!(plain, schedule(1, Unit::Days));
        assert!(serde_json::from_str::<Repeat>(r#"{"kind":"hourly","every":1}"#).is_err());
    }
}
