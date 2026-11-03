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
use jiff::civil::{Date, DateTime};

use crate::board::Board;
use crate::storage::write_board_file;

const PREFIX: &str = "backup-";
/// At most this many backups are deleted per day (see `Backups::prune`).
const MAX_PRUNED: usize = 3;
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
            self.write(board, today.to_datetime(jiff::Zoned::now().time()))?;
        }
        self.prune(today)
            .map_err(|e| io::Error::new(e.kind(), format!("removing old backups: {e}")))?;
        Ok(written)
    }

    pub fn delete(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    /// A backup right now.
    pub fn now(&self, board: &Board) -> io::Result<PathBuf> {
        self.write(board, jiff::Zoned::now().datetime())
    }

    /// Never replaces a backup: if one has this second's name already (two
    /// backups within a second), the next free second is used.
    fn write(&self, board: &Board, mut at: DateTime) -> io::Result<PathBuf> {
        let path = loop {
            let path = self.dir.join(format!(
                "{PREFIX}{}{SUFFIX}",
                at.strftime("%Y-%m-%d-%H%M%S")
            ));
            if !path.try_exists()? {
                break path;
            }
            at = at
                .checked_add(Span::new().seconds(1))
                .map_err(io::Error::other)?;
        };
        write_board_file(&path, board)?;
        Ok(path)
    }

    /// Delete the backups the retention rules no longer keep, but at most
    /// `MAX_PRUNED` at a time, the newest of them first. Day by day that is
    /// all there is; after a long break it catches up over a few days. A
    /// clock that is wrong for a while (set years ahead by hand, a flat
    /// battery) can then only take a few backups, not the weekly and
    /// monthly ones all at once.
    pub fn prune(&self, today: Date) -> io::Result<()> {
        let mut files = self.list();
        files.reverse(); // oldest first
        let dates: Vec<Date> = files.iter().map(|f| f.date).collect();
        let mut doomed: Vec<&BackupFile> = files
            .iter()
            .zip(to_keep(&dates, today))
            .filter(|(_, keep)| !keep)
            .map(|(file, _)| file)
            .collect();
        doomed.reverse(); // newest first
        for file in doomed.into_iter().take(MAX_PRUNED) {
            // (Gone already, e.g. deleted by hand: fine.)
            match fs::remove_file(&file.path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
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

    fn board() -> Board {
        let mut board = Board::new("mac");
        board.add_default_areas();
        board
    }

    #[test]
    fn several_backups_a_day_are_listed_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let b = board();
        for name in [
            "backup-2026-10-28-090000.json",
            "backup-2026-10-29-080000.json",
            "backup-2026-10-29-173000.json",
            "backup-2026-10-29-120000.json",
        ] {
            write_board_file(&dir.path().join(name), &b).unwrap();
        }
        let list: Vec<(Date, String)> = backups
            .list()
            .into_iter()
            .map(|f| (f.date, f.time))
            .collect();
        assert_eq!(
            list,
            [
                (date(2026, 10, 29), "17:30:00".to_owned()),
                (date(2026, 10, 29), "12:00:00".to_owned()),
                (date(2026, 10, 29), "08:00:00".to_owned()),
                (date(2026, 10, 28), "09:00:00".to_owned()),
            ]
        );
    }

    #[test]
    fn other_files_and_a_missing_folder_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.txt"), "x").unwrap();
        fs::write(dir.path().join("device-mac.json"), "{}").unwrap();
        fs::write(dir.path().join("backup-yesterday.json"), "{}").unwrap();
        assert!(Backups::new(dir.path()).list().is_empty());
        assert!(Backups::new(&dir.path().join("missing")).list().is_empty());
    }

    #[test]
    fn everything_from_the_last_week_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let b = board();
        let today = date(2026, 10, 29);
        // Three a day for a week.
        for day in 0..7 {
            let d = today.saturating_sub(Span::new().days(day));
            for time in ["080000", "120000", "200000"] {
                write_board_file(&dir.path().join(format!("backup-{d}-{time}.json")), &b).unwrap();
            }
        }
        backups.prune(today).unwrap();
        assert_eq!(backups.list().len(), 21);
    }

    #[test]
    fn within_a_week_the_oldest_backup_is_kept() {
        let today = date(2026, 10, 29);
        // Three weeks ago: Monday, Wednesday, Friday of one ISO week.
        let dates = [date(2026, 10, 5), date(2026, 10, 7), date(2026, 10, 9)];
        assert_eq!(to_keep(&dates, today), [true, false, false]);
        // The day a week ago is the first one thinned out.
        let week_ago = today.saturating_sub(Span::new().days(7));
        let six_days = today.saturating_sub(Span::new().days(6));
        assert_eq!(to_keep(&[week_ago, six_days], today), [true, true]);
    }

    #[test]
    fn an_old_style_backup_from_today_counts_as_todays() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let today = date(2026, 10, 29);
        write_board_file(&dir.path().join("backup-2026-10-29.json"), &board()).unwrap();
        assert!(!backups.daily(&board(), today).unwrap());
        assert_eq!(backups.list().len(), 1);
    }

    #[test]
    fn daily_creates_the_backup_folder() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b/backups");
        let backups = Backups::new(&nested);
        assert!(backups.daily(&board(), date(2026, 10, 29)).unwrap());
        assert_eq!(backups.list().len(), 1);
        assert_eq!(backups.dir(), nested);
    }

    #[test]
    fn a_manual_backup_is_named_with_the_time() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let path = backups.now(&board()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let (day, time) = parse_name(&name).expect("a backup name");
        assert_eq!(day, jiff::Zoned::now().date());
        assert!(time.is_some());
    }

    #[test]
    fn backups_within_one_second_dont_replace_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let mut b = board();
        let mut write_at = |at: DateTime, device: &str| {
            b.device = device.to_owned();
            let path = backups.write(&b, at).unwrap();
            path.file_name().unwrap().to_string_lossy().into_owned()
        };
        let noon = date(2026, 10, 29).at(12, 0, 0, 0);
        let names = [
            write_at(noon, "a"),
            write_at(noon, "b"),
            write_at(noon, "c"),
        ];
        assert_eq!(
            names,
            [
                "backup-2026-10-29-120000.json",
                "backup-2026-10-29-120001.json",
                "backup-2026-10-29-120002.json",
            ]
        );
        // The last second of a day moves on into the next one.
        let late = date(2026, 10, 29).at(23, 59, 59, 0);
        assert_eq!(write_at(late, "d"), "backup-2026-10-29-235959.json");
        assert_eq!(write_at(late, "e"), "backup-2026-10-30-000000.json");

        // Each kept its own content.
        let devices: Vec<String> = backups
            .list()
            .iter()
            .map(|f| {
                let file: serde_json::Value =
                    serde_json::from_slice(&fs::read(&f.path).unwrap()).unwrap();
                file["device"].as_str().unwrap().to_owned()
            })
            .collect();
        assert_eq!(devices, ["e", "d", "c", "b", "a"]);
    }

    #[test]
    fn deleting_a_missing_backup_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        assert!(
            backups
                .delete(&dir.path().join("backup-2026-10-29.json"))
                .is_err()
        );
    }

    #[test]
    fn restoring_removes_areas_added_since() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let mut b = board();
        let path = backups.now(&b).unwrap();
        let new_area = b.add_area("Hobbies");
        b.add_task(new_area, "paint", None, None);
        let snapshot = crate::storage::read_board_file(&path, "mac").unwrap();
        b.restore(&snapshot);
        let names: Vec<&str> = b.areas_sorted().iter().map(|a| a.name.as_str()).collect();
        assert!(!names.contains(&"Hobbies"), "{names:?}");
        assert_eq!(names.len(), 4);
        assert!(b.tasks.values().all(|t| t.meta.deleted));
    }

    #[test]
    fn backups_contain_deleted_records_too() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let mut b = board();
        let area = b.areas_sorted()[0].meta.id;
        let gone = b.add_task(area, "gone", None, None);
        b.delete_task(gone);
        let path = backups.now(&b).unwrap();
        let back = crate::storage::read_board_file(&path, "mac").unwrap();
        assert!(back.tasks[&gone].meta.deleted);
    }

    #[test]
    fn a_clock_years_ahead_takes_only_a_few_backups() {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path());
        let b = board();
        let mut day = date(2026, 1, 1);
        while day <= date(2026, 11, 2) {
            backups.daily(&b, day).unwrap();
            day = day.tomorrow().unwrap();
        }
        let before = backups.list().len();
        // One start with the clock three years ahead.
        backups.daily(&b, date(2029, 11, 2)).unwrap();
        assert_eq!(backups.list().len(), before + 1 - MAX_PRUNED);
        // Back to the right date: everything else is still there.
        backups.daily(&b, date(2026, 11, 3)).unwrap();
        assert!(
            backups.list().len() >= before - MAX_PRUNED,
            "{}",
            backups.list().len()
        );
        let dates: Vec<Date> = backups.list().iter().map(|f| f.date).collect();
        assert!(dates.contains(&date(2026, 1, 1)), "the oldest one stays");
        assert!(
            dates
                .iter()
                .filter(|d| d.year() == 2026 && d.month() < 9)
                .count()
                >= 8,
            "the monthly ones stay: {dates:?}"
        );
    }
}
