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
}
