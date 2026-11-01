//! Local backups of the whole board: `backup-YYYY-MM-DD-HHMMSS.json`.
//! One is made automatically each day; extra ones on request or right before
//! restoring a backup. (Older versions named the daily one
//! `backup-YYYY-MM-DD.json`; those are still read.)
//!
//! Old backups are thinned out: everything from the last 7 days is kept,
//! then one per week up to 2 months, one per month up to 2 years, and one
//! per year beyond that. Within each week/month/year the *oldest* backup is
//! the one kept, so a kept backup stays kept as it ages into the next tier.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::Span;
use jiff::civil::Date;

use crate::board::Board;
use crate::storage::write_board_file;

const PREFIX: &str = "backup-";
const SUFFIX: &str = ".json";

pub struct BackupFile {
    pub path: PathBuf,
    pub date: Date,
    /// Time of day as `HH:MM:SS`: from the name, or for old names without a
    /// time, from when the file was written.
    pub time: String,
}

pub struct Backups {
    dir: PathBuf,
}

impl Backups {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// All backups, newest first.
    pub fn list(&self) -> Vec<BackupFile> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let (date, time) = parse_name(&e.file_name().to_string_lossy())?;
                let time = time.unwrap_or_else(|| modified_time(&e.path()));
                Some(BackupFile {
                    path: e.path(),
                    date,
                    time,
                })
            })
            .collect();
        files.sort_by(|a, b| (b.date, &b.time).cmp(&(a.date, &a.time)));
        files
    }

    /// Make today's backup unless there already is one, then thin out old ones.
    /// Returns whether a backup was written.
    pub fn daily(&self, board: &Board, today: Date) -> io::Result<bool> {
        let written = !self.list().iter().any(|f| f.date == today);
        if written {
            self.write(
                board,
                today,
                &jiff::Zoned::now().strftime("%H%M%S").to_string(),
            )?;
        }
        self.prune(today)?;
        Ok(written)
    }

    pub fn delete(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    /// A backup right now.
    pub fn now(&self, board: &Board) -> io::Result<PathBuf> {
        let now = jiff::Zoned::now();
        self.write(board, now.date(), &now.strftime("%H%M%S").to_string())
    }

    /// `time` as `HHMMSS`.
    fn write(&self, board: &Board, date: Date, time: &str) -> io::Result<PathBuf> {
        let path = self.dir.join(format!("{PREFIX}{date}-{time}{SUFFIX}"));
        write_board_file(&path, board)?;
        Ok(path)
    }

    /// Delete the backups the retention rules no longer keep.
    pub fn prune(&self, today: Date) -> io::Result<()> {
        let mut files = self.list();
        files.reverse(); // oldest first
        let dates: Vec<Date> = files.iter().map(|f| f.date).collect();
        for (file, keep) in files.iter().zip(to_keep(&dates, today)) {
            if !keep {
                fs::remove_file(&file.path)?;
            }
        }
        Ok(())
    }
}

/// Local time a file was last written, as `HH:MM:SS` ("" if unknown).
fn modified_time(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| jiff::Timestamp::try_from(t).ok())
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

/// `backup-2026-10-29.json` → (date, None); `backup-2026-10-29-153012.json`
/// → (date, Some("15:30:12")). Anything else → None.
fn parse_name(name: &str) -> Option<(Date, Option<String>)> {
    let stem = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    let date: Date = stem.get(..10)?.parse().ok()?;
    match stem.get(10..)? {
        "" => Some((date, None)),
        rest => {
            let t = rest.strip_prefix('-')?;
            (t.len() == 6 && t.bytes().all(|b| b.is_ascii_digit()))
                .then(|| Some((date, Some(format!("{}:{}:{}", &t[..2], &t[2..4], &t[4..])))))?
        }
    }
}

/// Which of the backups (dates sorted oldest first) to keep.
fn to_keep(dates: &[Date], today: Date) -> Vec<bool> {
    let daily_from = today.saturating_sub(Span::new().days(6));
    let weekly_from = today.saturating_sub(Span::new().months(2));
    let monthly_from = today.saturating_sub(Span::new().years(2));
    let mut buckets = HashSet::new();
    dates
        .iter()
        .map(|&d| {
            if d >= daily_from {
                return true;
            }
            let bucket = if d >= weekly_from {
                let week = d.iso_week_date();
                (1, week.year(), week.week())
            } else if d >= monthly_from {
                (2, d.year(), d.month())
            } else {
                (3, d.year(), 0)
            };
            buckets.insert(bucket)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn names_are_parsed() {
        assert_eq!(
            parse_name("backup-2026-10-29.json"),
            Some((date(2026, 10, 29), None))
        );
        assert_eq!(
            parse_name("backup-2026-10-29-153012.json"),
            Some((date(2026, 10, 29), Some("15:30:12".into())))
        );
        assert_eq!(parse_name("backup-2026-10-29-x.json"), None);
        assert_eq!(parse_name(".backup-2026-10-29.json.tmp"), None);
        assert_eq!(parse_name("device-mac.json"), None);
    }

    /// Run the app every day for three years, pruning each day like the app does.
    #[test]
    fn three_years_of_daily_backups() {
        let start = date(2024, 1, 1);
        let today = date(2026, 12, 31);
        let mut kept: Vec<Date> = Vec::new();
        let mut day = start;
        while day <= today {
            kept.push(day);
            let keep = to_keep(&kept, day);
            kept = kept
                .into_iter()
                .zip(keep)
                .filter(|(_, k)| *k)
                .map(|(d, _)| d)
                .collect();
            day = day.tomorrow().unwrap();
        }

        let age = |d: &Date| (today - *d).get_days();
        let daily: Vec<_> = kept.iter().filter(|d| age(d) < 7).collect();
        assert_eq!(daily.len(), 7, "every backup of the last week");

        let weekly_from = today.saturating_sub(Span::new().months(2));
        let weekly: Vec<_> = kept
            .iter()
            .filter(|d| age(d) >= 7 && **d >= weekly_from)
            .collect();
        let weeks: HashSet<_> = weekly.iter().map(|d| d.iso_week_date().week()).collect();
        assert_eq!(weeks.len(), weekly.len(), "at most one per week");
        assert!((7..=10).contains(&weekly.len()), "{} weekly", weekly.len());

        let monthly_from = today.saturating_sub(Span::new().years(2));
        let monthly: Vec<_> = kept
            .iter()
            .filter(|d| **d < weekly_from && **d >= monthly_from)
            .collect();
        let months: HashSet<_> = monthly.iter().map(|d| (d.year(), d.month())).collect();
        assert_eq!(months.len(), monthly.len(), "at most one per month");
        assert!(
            (20..=23).contains(&monthly.len()),
            "{} monthly",
            monthly.len()
        );

        let yearly: Vec<_> = kept.iter().filter(|d| **d < monthly_from).collect();
        assert!(!yearly.is_empty() && yearly.len() <= 2, "{yearly:?}");
        assert_eq!(
            kept[0], start,
            "the very first backup survives as the yearly one"
        );
    }

    #[test]
    fn daily_writes_once_per_day_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let board = Board::new("mac");
        // Old leftovers: two in the same month, a year ago.
        for name in ["backup-2025-06-03.json", "backup-2025-06-20.json"] {
            write_board_file(&dir.path().join(name), &board).unwrap();
        }
        let today = date(2026, 10, 29);
        assert!(backups.daily(&board, today).unwrap());
        assert!(!backups.daily(&board, today).unwrap());
        let list = backups.list();
        let dates: Vec<Date> = list.iter().map(|b| b.date).collect();
        assert_eq!(dates, [today, date(2025, 6, 3)]);
        // Every backup has a time: from its name, or from the file for old names.
        assert!(
            list.iter().all(|b| b.time.len() == 8),
            "{:?}",
            list.iter().map(|b| &b.time).collect::<Vec<_>>()
        );
        assert!(
            list[0]
                .path
                .to_string_lossy()
                .contains(&format!("backup-{today}-"))
        );

        backups.delete(&list[1].path).unwrap();
        assert_eq!(backups.list().len(), 1);
    }

    #[test]
    fn restoring_a_backup_brings_back_its_state() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let mut board = Board::new("mac");
        board.add_default_areas();
        let area = board.areas_sorted()[0].meta.id;
        let kept = board.add_task(area, "kept", None, None);
        let path = backups.now(&board).unwrap();

        board.update_task(kept, |t| t.title = "renamed later".into());
        board.add_task(area, "added later", None, None);
        // A board that never had the backup's records (e.g. a fresh install).
        let mut fresh = Board::new("linux");

        let snapshot = crate::storage::read_board_file(&path, "mac").unwrap();
        board.restore(&snapshot);
        fresh.restore(&snapshot);
        for b in [&board, &fresh] {
            let titles: Vec<_> = b
                .priority_list(area)
                .iter()
                .map(|t| t.title.clone())
                .collect();
            assert_eq!(titles, ["kept"]);
        }
    }
}
