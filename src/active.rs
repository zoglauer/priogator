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
        let in_window = self.weekdays.contains(&day)
            && if self.from <= self.to {
                (self.from..self.to).contains(&minute)
            } else {
                // Past midnight, e.g. 22:00–02:00.
                minute >= self.from || minute < self.to
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
}
