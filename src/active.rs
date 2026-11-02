//! When an area is active, e.g. Work on weekdays 8–17. Outside that time
//! the area is shown folded.

use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};

use crate::repeat::WEEKDAY_BUTTONS;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveHours {
    /// 1 = Monday … 7 = Sunday.
    pub weekdays: Vec<i8>,
    /// Minutes after midnight; the window is `from..to` on each weekday.
    pub from: u16,
    pub to: u16,
    /// Active *outside* the window instead (e.g. Family: not during work hours).
    #[serde(default)]
    pub outside: bool,
}

impl Default for ActiveHours {
    /// Weekdays 8:00–17:00.
    fn default() -> Self {
        Self {
            weekdays: vec![1, 2, 3, 4, 5],
            from: 8 * 60,
            to: 17 * 60,
            outside: false,
        }
    }
}

impl ActiveHours {
    pub fn is_active(&self, now: DateTime) -> bool {
        let day = now.date().weekday().to_monday_one_offset();
        let minute = now.hour() as u16 * 60 + now.minute() as u16;
        let on = |d: i8| self.weekdays.contains(&d);
        let in_window = if self.from <= self.to {
            on(day) && (self.from..self.to).contains(&minute)
        } else {
            // Past midnight, e.g. Monday 22:00–02:00 runs into Tuesday morning.
            let yesterday = if day == 1 { 7 } else { day - 1 };
            (on(day) && minute >= self.from) || (on(yesterday) && minute < self.to)
        };
        in_window != self.outside
    }

    /// E.g. "Mo–Fr 08:00–17:00", or "outside Mo–Fr 08:00–17:00".
    pub fn summary(&self) -> String {
        let window = format!(
            "{} {}–{}",
            days(&self.weekdays),
            clock(self.from),
            clock(self.to)
        );
        if self.outside {
            format!("outside {window}")
        } else {
            window
        }
    }
}

/// "08:30"
pub fn clock(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60 % 24, minutes % 60)
}

/// "Mo–Fr", "Sa, Su", "Mo, We, Fr"; runs of three or more days are joined.
fn days(weekdays: &[i8]) -> String {
    let mut days: Vec<i8> = weekdays
        .iter()
        .copied()
        .filter(|d| (1..=7).contains(d))
        .collect();
    days.sort_unstable();
    days.dedup();
    let name = |d: i8| WEEKDAY_BUTTONS[(d - 1) as usize];
    let mut parts = Vec::new();
    let mut i = 0;
    while i < days.len() {
        let mut j = i;
        while j + 1 < days.len() && days[j + 1] == days[j] + 1 {
            j += 1;
        }
        if j - i >= 2 {
            parts.push(format!("{}–{}", name(days[i]), name(days[j])));
        } else {
            parts.extend(days[i..=j].iter().map(|d| name(*d).to_owned()));
        }
        i = j + 1;
    }
    if parts.is_empty() {
        "no days".to_owned()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn at(day: i8, hour: i8, minute: i8) -> DateTime {
        // Nov 2, 2026 is a Monday.
        date(2026, 11, 1 + day).at(hour, minute, 0, 0)
    }

    #[test]
    fn work_hours() {
        let work = ActiveHours::default();
        assert!(work.is_active(at(1, 8, 0)), "Monday 8:00");
        assert!(work.is_active(at(5, 16, 59)), "Friday 16:59");
        assert!(!work.is_active(at(5, 17, 0)), "Friday 17:00");
        assert!(!work.is_active(at(3, 7, 59)), "Wednesday before 8");
        assert!(!work.is_active(at(6, 10, 0)), "Saturday");
        assert_eq!(work.summary(), "Mo–Fr 08:00–17:00");
    }

    #[test]
    fn family_is_the_reverse() {
        let family = ActiveHours {
            outside: true,
            ..ActiveHours::default()
        };
        assert!(!family.is_active(at(2, 10, 0)), "Tuesday morning");
        assert!(family.is_active(at(2, 19, 0)), "Tuesday evening");
        assert!(family.is_active(at(7, 10, 0)), "Sunday");
        assert_eq!(family.summary(), "outside Mo–Fr 08:00–17:00");
    }

    #[test]
    fn past_midnight() {
        let late = ActiveHours {
            weekdays: vec![1, 2, 3, 4, 5, 6, 7],
            from: 22 * 60,
            to: 2 * 60,
            outside: false,
        };
        assert!(late.is_active(at(3, 23, 0)));
        assert!(late.is_active(at(3, 1, 0)));
        assert!(!late.is_active(at(3, 12, 0)));
        assert_eq!(late.summary(), "Mo–Su 22:00–02:00");
    }

    #[test]
    fn day_lists() {
        assert_eq!(days(&[6, 7]), "Sa, Su");
        assert_eq!(days(&[1, 3, 5]), "Mo, We, Fr");
        assert_eq!(days(&[1, 2, 3, 6]), "Mo–We, Sa");
    }

    #[test]
    fn an_empty_window_is_never_active() {
        let empty = ActiveHours {
            from: 9 * 60,
            to: 9 * 60,
            ..ActiveHours::default()
        };
        assert!(!empty.is_active(at(1, 9, 0)));
        assert!(!empty.is_active(at(1, 12, 0)));
        // ... and outside of it, always.
        let always = ActiveHours {
            outside: true,
            ..empty
        };
        assert!(always.is_active(at(1, 9, 0)));
        assert!(always.is_active(at(6, 3, 0)));
    }

    #[test]
    fn no_weekdays_means_never_or_always() {
        let none = ActiveHours {
            weekdays: vec![],
            ..ActiveHours::default()
        };
        for day in 1..=7 {
            assert!(!none.is_active(at(day, 10, 0)));
        }
        assert_eq!(none.summary(), "no days 08:00–17:00");
        let outside = ActiveHours {
            outside: true,
            ..none
        };
        assert!(outside.is_active(at(3, 10, 0)));
    }

    #[test]
    fn outside_a_window_past_midnight() {
        let night = ActiveHours {
            weekdays: vec![1, 2, 3, 4, 5, 6, 7],
            from: 22 * 60,
            to: 2 * 60,
            outside: true,
        };
        assert!(night.is_active(at(3, 12, 0)));
        assert!(night.is_active(at(3, 2, 0)), "02:00 is the end");
        assert!(!night.is_active(at(3, 22, 0)));
        assert!(!night.is_active(at(3, 1, 59)));
    }

    #[test]
    fn a_night_window_continues_into_the_next_morning() {
        let monday_night = ActiveHours {
            weekdays: vec![1],
            from: 22 * 60,
            to: 2 * 60,
            outside: false,
        };
        assert!(monday_night.is_active(at(1, 23, 0)));
        assert!(monday_night.is_active(at(2, 1, 0)), "Tuesday 01:00");
    }

    #[test]
    fn every_minute_at_the_edges() {
        let work = ActiveHours::default();
        assert!(!work.is_active(at(2, 7, 59)));
        assert!(work.is_active(at(2, 8, 0)));
        assert!(work.is_active(at(2, 16, 59)));
        assert!(!work.is_active(at(2, 17, 0)));
        assert!(!work.is_active(at(7, 12, 0)), "Sunday");
    }

    #[test]
    fn clock_times() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(8 * 60 + 5), "08:05");
        assert_eq!(clock(23 * 60 + 59), "23:59");
        assert_eq!(clock(24 * 60), "00:00");
    }

    #[test]
    fn more_day_lists() {
        assert_eq!(days(&[1, 2, 3, 4, 5, 6, 7]), "Mo–Su");
        assert_eq!(days(&[1, 2]), "Mo, Tu");
        assert_eq!(days(&[7, 1]), "Mo, Su");
        assert_eq!(days(&[3, 3, 0, 8]), "We");
        assert_eq!(days(&[5, 6, 7, 1]), "Mo, Fr–Su");
        assert_eq!(days(&[]), "no days");
    }

    #[test]
    fn stored_without_outside_means_during() {
        let hours: ActiveHours =
            serde_json::from_str(r#"{"weekdays":[1,2],"from":480,"to":600}"#).unwrap();
        assert!(!hours.outside);
        assert_eq!(hours.summary(), "Mo, Tu 08:00–10:00");
        let json = serde_json::to_string(&hours).unwrap();
        let back: ActiveHours = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hours);
    }
}
