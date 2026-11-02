//! Files on disk.
//!
//! The sync folder holds one file per device (`device-<name>.json`). A device
//! only ever writes its own file and merges everybody else's, so a folder
//! synced by Syncthing or copied by USB stick never has write conflicts.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::board::Board;
use crate::model::{Area, TaskGroup, TaskItem};

const FORMAT_VERSION: u32 = 1;
const FILE_PREFIX: &str = "device-";
const FILE_SUFFIX: &str = ".json";

/// Local settings of this device (not synced).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub device: String,
    pub sync_dir: PathBuf,
    /// Local daily backups (not synced).
    #[serde(default = "default_backup_dir")]
    pub backup_dir: PathBuf,
    /// Deadlines within this many days put a task in the top section.
    #[serde(default = "default_horizon_days")]
    pub deadline_horizon_days: i64,
    /// Completed tasks stay visible for this many days.
    #[serde(default = "default_done_days")]
    pub done_days: i64,
    /// Width of each column, in points.
    #[serde(default = "default_column_width")]
    pub column_width: f32,
    /// Areas folded by hand on this computer (they stay folded until opened).
    #[serde(default)]
    pub folded_areas: Vec<uuid::Uuid>,
    #[serde(default)]
    pub color_scheme: ColorScheme,
    /// Colour of the "Due soon · pinned" section; `None`: the standard orange.
    #[serde(default)]
    pub due_soon_color: Option<[u8; 3]>,
    /// The file it was loaded from and is saved to. `None` (e.g. in tests):
    /// never saved, so nothing can overwrite the real settings.
    #[serde(skip)]
    pub file: Option<PathBuf>,
}

/// How task cards are coloured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    /// Plain cards; only the group colours.
    #[default]
    Standard,
    /// Each area's open tasks from red (most important) to blue.
    Rainbow,
}

impl ColorScheme {
    pub const ALL: [ColorScheme; 2] = [ColorScheme::Standard, ColorScheme::Rainbow];

    pub fn name(self) -> &'static str {
        match self {
            ColorScheme::Standard => "Standard",
            ColorScheme::Rainbow => "Rainbow",
        }
    }
}

fn default_done_days() -> i64 {
    crate::board::DEFAULT_DONE_DAYS
}

fn default_column_width() -> f32 {
    crate::app::DEFAULT_COLUMN_WIDTH
}

fn default_backup_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.data_dir().join("backups"))
        .unwrap_or_else(|_| PathBuf::from("backups"))
}

fn default_horizon_days() -> i64 {
    crate::board::DEFAULT_HORIZON_DAYS
}

/// Contents of one device file.
#[derive(Debug, Serialize, Deserialize)]
struct DeviceFile {
    format: u32,
    device: String,
    saved_at: Timestamp,
    areas: Vec<Area>,
    groups: Vec<TaskGroup>,
    tasks: Vec<TaskItem>,
}

fn project_dirs() -> io::Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "Priogator")
        .ok_or_else(|| io::Error::other("no home directory found"))
}

/// The app used to be called Prioritizer: move its folders over, entry by
/// entry, and leave anything already in the new folder alone.
fn move_folder(old: &Path, new: &Path) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(old) else {
        return Ok(());
    };
    fs::create_dir_all(new)?;
    for entry in entries.flatten() {
        let target = new.join(entry.file_name());
        if !target.exists() {
            fs::rename(entry.path(), target)?;
        }
    }
    // Only goes if it is empty now.
    let _ = fs::remove_dir(old);
    Ok(())
}

/// `path` with the `old` folder at its start replaced by `new`.
fn rebase(path: &Path, old: &Path, new: &Path) -> Option<PathBuf> {
    path.strip_prefix(old).ok().map(|rest| new.join(rest))
}

impl Config {
    /// Load the config, creating it with defaults on first start.
    pub fn load_or_create() -> io::Result<Self> {
        let dirs = project_dirs()?;
        let file = dirs.config_dir().join("config.json");
        let old = directories::ProjectDirs::from("", "", "Prioritizer");
        if let Some(old) = &old {
            move_folder(old.config_dir(), dirs.config_dir())?;
            move_folder(old.data_dir(), dirs.data_dir())?;
        }
        if let Ok(text) = fs::read_to_string(&file) {
            let mut config: Config = serde_json::from_str(&text).map_err(io::Error::other)?;
            config.file = Some(file);
            if let Some(old) = &old {
                let moved = |p: &Path| rebase(p, old.data_dir(), dirs.data_dir());
                let sync = moved(&config.sync_dir);
                let backup = moved(&config.backup_dir);
                if sync.is_some() || backup.is_some() {
                    config.sync_dir = sync.unwrap_or(config.sync_dir);
                    config.backup_dir = backup.unwrap_or(config.backup_dir);
                    config.save()?;
                }
            }
            return Ok(config);
        }
        let config = Config {
            device: default_device_name(),
            sync_dir: dirs.data_dir().join("sync"),
            backup_dir: default_backup_dir(),
            deadline_horizon_days: default_horizon_days(),
            done_days: default_done_days(),
            column_width: default_column_width(),
            folded_areas: Vec::new(),
            color_scheme: ColorScheme::default(),
            due_soon_color: None,
            file: Some(file),
        };
        config.save()?;
        Ok(config)
    }

    /// Write to the file it came from; does nothing without one.
    pub fn save(&self) -> io::Result<()> {
        match &self.file {
            Some(file) => write_atomic(file, &serde_json::to_vec_pretty(self)?),
            None => Ok(()),
        }
    }
}

/// Host name plus a random suffix, e.g. `macbook-3f2a`.
fn default_device_name() -> String {
    let host = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let host: String = host
        .split('.')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase();
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..4];
    if host.is_empty() {
        format!("device-{suffix}")
    } else {
        format!("{host}-{suffix}")
    }
}

/// Reads and writes the device files in the sync folder.
pub struct SyncFolder {
    dir: PathBuf,
    device: String,
    /// Modification times of the device files at the last scan.
    seen: Vec<(PathBuf, SystemTime)>,
}

impl SyncFolder {
    pub fn new(dir: &Path, device: &str) -> Self {
        Self {
            dir: dir.to_path_buf(),
            device: device.to_owned(),
            seen: Vec::new(),
        }
    }

    fn own_file(&self) -> PathBuf {
        self.dir
            .join(format!("{FILE_PREFIX}{}{FILE_SUFFIX}", self.device))
    }

    /// All device files currently in the folder, with modification times.
    fn scan(&self) -> Vec<(PathBuf, SystemTime)> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut files: Vec<_> = entries
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX)
            })
            .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
            .collect();
        files.sort();
        files
    }

    /// Load the board by merging every device file in the folder.
    /// Returns the board and a list of files that could not be read.
    pub fn load(&mut self) -> (Board, Vec<String>) {
        let mut board = Board::new(&self.device);
        let errors = self.merge_all(&mut board);
        if board.areas.is_empty() {
            board.add_default_areas();
        }
        (board, errors)
    }

    /// Merge all device files that changed since the last scan.
    /// Returns (anything changed, errors).
    pub fn reload_if_changed(&mut self, board: &mut Board) -> (bool, Vec<String>) {
        if self.scan() == self.seen {
            return (false, Vec::new());
        }
        let before = board.clone();
        let errors = self.merge_all(board);
        (*board != before, errors)
    }

    fn merge_all(&mut self, board: &mut Board) -> Vec<String> {
        let files = self.scan();
        let mut errors = Vec::new();
        for (path, _) in &files {
            match read_device_file(path) {
                Ok(file) => {
                    board.merge(file.areas, file.groups, file.tasks);
                }
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        self.seen = files;
        errors
    }

    /// Write this device's file with the full current state.
    pub fn save(&mut self, board: &Board) -> io::Result<()> {
        let own = self.own_file();
        write_board_file(&own, board)?;
        // Our own write is not a change from another device.
        let modified = fs::metadata(&own)?.modified()?;
        self.seen.retain(|(path, _)| *path != own);
        self.seen.push((own, modified));
        self.seen.sort();
        Ok(())
    }
}

/// Write the full state of `board` to `path` (sync files and backups alike).
pub fn write_board_file(path: &Path, board: &Board) -> io::Result<()> {
    let file = DeviceFile {
        format: FORMAT_VERSION,
        device: board.device.clone(),
        saved_at: Timestamp::now(),
        areas: board.areas.values().cloned().collect(),
        groups: board.groups.values().cloned().collect(),
        tasks: board.tasks.values().cloned().collect(),
    };
    write_atomic(path, &serde_json::to_vec_pretty(&file)?)
}

/// Read a sync file or backup as a board of its own (for restoring).
pub fn read_board_file(path: &Path, device: &str) -> io::Result<Board> {
    let file = read_device_file(path)?;
    let mut board = Board::new(device);
    board.merge(file.areas, file.groups, file.tasks);
    Ok(board)
}

fn read_device_file(path: &Path) -> io::Result<DeviceFile> {
    let file: DeviceFile = serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
    if file.format > FORMAT_VERSION {
        return Err(io::Error::other(format!(
            "written by a newer Priogator (format {})",
            file.format
        )));
    }
    Ok(file)
}

/// Write via a hidden temp file and rename, so a crash or a sync tool never
/// sees a half-written file.
fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.tmp"));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_save_only_to_the_file_they_came_from() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        let mut config = Config {
            device: "test".into(),
            sync_dir: dir.path().join("sync"),
            backup_dir: dir.path().join("backups"),
            deadline_horizon_days: 10,
            done_days: 7,
            column_width: 280.0,
            folded_areas: Vec::new(),
            color_scheme: ColorScheme::default(),
            due_soon_color: None,
            file: None,
        };
        config.save().unwrap();
        assert!(!file.exists(), "without a file, saving writes nothing");

        config.file = Some(file.clone());
        config.column_width = 320.0;
        config.save().unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("320"));
        assert!(
            !text.contains("\"file\""),
            "the location itself isn't stored"
        );
    }

    #[test]
    fn two_devices_share_a_folder() {
        let dir = tempfile::tempdir().unwrap();

        let mut mac_folder = SyncFolder::new(dir.path(), "mac");
        let (mut mac, _) = mac_folder.load();
        let area = mac.areas_sorted()[0].meta.id;
        mac.add_task(area, "from mac", None, None);
        mac_folder.save(&mac).unwrap();

        let mut linux_folder = SyncFolder::new(dir.path(), "linux");
        let (mut linux, errors) = linux_folder.load();
        assert!(errors.is_empty());
        assert_eq!(linux.priority_list(area).len(), 1);
        assert_eq!(linux.areas_sorted().len(), mac.areas_sorted().len());

        linux.add_task(area, "from linux", None, None);
        linux_folder.save(&linux).unwrap();

        let (changed, errors) = mac_folder.reload_if_changed(&mut mac);
        assert!(changed && errors.is_empty());
        assert_eq!(mac.priority_list(area).len(), 2);

        // Nothing new: no change reported.
        let (changed, _) = mac_folder.reload_if_changed(&mut mac);
        assert!(!changed);
    }

    #[test]
    fn broken_file_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("device-broken.json"), "not json").unwrap();
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (board, errors) = folder.load();
        assert_eq!(errors.len(), 1);
        assert!(!board.areas.is_empty());
    }

    #[test]
    fn temp_files_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".device-x.json.tmp"), "junk").unwrap();
        fs::write(dir.path().join(".syncthing.device-x.json.tmp"), "junk").unwrap();
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (_, errors) = folder.load();
        assert!(errors.is_empty());
    }

    #[test]
    fn old_folders_move_over() {
        let home = tempfile::tempdir().unwrap();
        let old = home.path().join("Prioritizer");
        let new = home.path().join("Priogator");
        fs::create_dir_all(old.join("sync")).unwrap();
        fs::write(old.join("sync/device-mac.json"), "{}").unwrap();
        fs::write(old.join("config.json"), "old").unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(new.join("config.json"), "new").unwrap();

        move_folder(&old, &new).unwrap();
        assert!(new.join("sync/device-mac.json").exists());
        assert_eq!(fs::read_to_string(new.join("config.json")).unwrap(), "new");
        // The old config was not moved, so the old folder stays.
        assert!(old.join("config.json").exists());

        assert_eq!(
            rebase(&old.join("sync"), &old, &new),
            Some(new.join("sync"))
        );
        assert_eq!(rebase(Path::new("/elsewhere"), &old, &new), None);
    }

    #[test]
    fn settings_from_an_older_version_get_defaults() {
        let config: Config =
            serde_json::from_str(r#"{"device":"mac","sync_dir":"/tmp/sync"}"#).unwrap();
        assert_eq!(config.device, "mac");
        assert_eq!(
            config.deadline_horizon_days,
            crate::board::DEFAULT_HORIZON_DAYS
        );
        assert_eq!(config.done_days, crate::board::DEFAULT_DONE_DAYS);
        assert_eq!(config.column_width, crate::app::DEFAULT_COLUMN_WIDTH);
        assert!(config.folded_areas.is_empty());
        assert_eq!(config.color_scheme, ColorScheme::Standard);
        assert_eq!(config.due_soon_color, None);
        assert_eq!(config.file, None, "never saved unless loaded from a file");
        assert!(config.backup_dir.ends_with("backups"));
    }

    #[test]
    fn colors_are_saved_readably() {
        let mut config: Config =
            serde_json::from_str(r#"{"device":"mac","sync_dir":"/tmp/sync"}"#).unwrap();
        config.color_scheme = ColorScheme::Rainbow;
        config.due_soon_color = Some([10, 20, 30]);
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains(r#""color_scheme":"rainbow""#), "{json}");
        assert!(json.contains(r#""due_soon_color":[10,20,30]"#), "{json}");
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.color_scheme, ColorScheme::Rainbow);
        assert_eq!(back.due_soon_color, Some([10, 20, 30]));
        assert!(serde_json::from_str::<ColorScheme>(r#""plaid""#).is_err());
    }

    #[test]
    fn color_scheme_names() {
        assert_eq!(
            ColorScheme::ALL.map(ColorScheme::name),
            ["Standard", "Rainbow"]
        );
    }

    #[test]
    fn a_file_from_a_newer_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-future.json");
        let mut board = Board::new("future");
        board.add_default_areas();
        write_board_file(&path, &board).unwrap();
        let text = fs::read_to_string(&path).unwrap().replacen(
            &format!("\"format\": {FORMAT_VERSION}"),
            "\"format\": 99",
            1,
        );
        fs::write(&path, text).unwrap();

        let error = read_board_file(&path, "mac").unwrap_err().to_string();
        assert!(error.contains("newer Priogator"), "{error}");
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (_, errors) = folder.load();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("device-future.json"), "{errors:?}");
    }

    #[test]
    fn the_own_file_is_named_after_the_device() {
        let dir = tempfile::tempdir().unwrap();
        let mut folder = SyncFolder::new(dir.path(), "macbook-3f2a");
        let (board, _) = folder.load();
        folder.save(&board).unwrap();
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["device-macbook-3f2a.json"], "no temp file left");
    }

    #[test]
    fn writing_replaces_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/folder/file.json");
        write_atomic(&path, b"a long first version").unwrap();
        write_atomic(&path, b"short").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"short");
        assert!(!dir.path().join("nested/folder/.file.json.tmp").exists());
    }

    #[test]
    fn saving_our_own_file_is_not_a_change_from_elsewhere() {
        let dir = tempfile::tempdir().unwrap();
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, _) = folder.load();
        let area = board.areas_sorted()[0].meta.id;
        board.add_task(area, "mine", None, None);
        folder.save(&board).unwrap();
        let (changed, errors) = folder.reload_if_changed(&mut board);
        assert!(!changed && errors.is_empty());
    }

    #[test]
    fn an_empty_or_missing_folder_starts_with_the_default_areas() {
        let dir = tempfile::tempdir().unwrap();
        for path in [dir.path().to_path_buf(), dir.path().join("not/there")] {
            let mut folder = SyncFolder::new(&path, "mac");
            let (board, errors) = folder.load();
            assert!(errors.is_empty());
            let names: Vec<&str> = board
                .areas_sorted()
                .iter()
                .map(|a| a.name.as_str())
                .collect();
            assert_eq!(names, ["Work", "Private ventures", "House", "Family"]);
            assert!(board.tasks.is_empty());
        }
    }

    #[test]
    fn saving_creates_a_missing_sync_folder() {
        let dir = tempfile::tempdir().unwrap();
        let sync = dir.path().join("new/sync");
        let mut folder = SyncFolder::new(&sync, "mac");
        let (board, _) = folder.load();
        folder.save(&board).unwrap();
        assert!(sync.join("device-mac.json").exists());
    }

    #[test]
    fn the_newest_edit_wins_whatever_the_file_order() {
        let dir = tempfile::tempdir().unwrap();
        // "zz" sorts after "aa" but has the older change.
        let mut newer = SyncFolder::new(dir.path(), "aa");
        let (mut aa, _) = newer.load();
        let area = aa.areas_sorted()[0].meta.id;
        let id = aa.add_task(area, "first", None, None);
        let mut zz = aa.clone();
        zz.device = "zz".into();
        zz.update_task(id, |t| t.title = "older".into());
        aa.update_task(id, |t| t.title = "newer".into());
        // Both edits can fall in the same clock tick; make the order explicit.
        let stamp = zz.tasks[&id].meta.updated_at;
        aa.tasks.get_mut(&id).unwrap().meta.updated_at =
            stamp + jiff::SignedDuration::from_secs(1);
        newer.save(&aa).unwrap();
        SyncFolder::new(dir.path(), "zz").save(&zz).unwrap();

        let (third, _) = SyncFolder::new(dir.path(), "third").load();
        assert_eq!(third.tasks[&id].title, "newer");
    }

    #[test]
    fn a_deletion_on_another_device_arrives() {
        let dir = tempfile::tempdir().unwrap();
        let mut mac_folder = SyncFolder::new(dir.path(), "mac");
        let (mut mac, _) = mac_folder.load();
        let area = mac.areas_sorted()[0].meta.id;
        let id = mac.add_task(area, "shared", None, None);
        mac_folder.save(&mac).unwrap();

        let mut linux_folder = SyncFolder::new(dir.path(), "linux");
        let (mut linux, _) = linux_folder.load();
        linux.delete_task(id);
        linux_folder.save(&linux).unwrap();

        let (changed, _) = mac_folder.reload_if_changed(&mut mac);
        assert!(changed);
        assert!(mac.priority_list(area).is_empty());
    }

    #[test]
    fn one_broken_file_does_not_stop_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let mut linux_folder = SyncFolder::new(dir.path(), "linux");
        let (mut linux, _) = linux_folder.load();
        let area = linux.areas_sorted()[0].meta.id;
        linux.add_task(area, "from linux", None, None);
        linux_folder.save(&linux).unwrap();
        fs::write(dir.path().join("device-broken.json"), "{ half a file").unwrap();

        let (mac, errors) = SyncFolder::new(dir.path(), "mac").load();
        assert_eq!(errors.len(), 1);
        assert_eq!(mac.priority_list(area).len(), 1);
    }

    #[test]
    fn files_that_are_not_device_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "notes.json",
            "backup-2026-10-29.json",
            "device-x.json.bak",
            "README",
        ] {
            fs::write(dir.path().join(name), "junk").unwrap();
        }
        let (_, errors) = SyncFolder::new(dir.path(), "mac").load();
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn reading_a_missing_board_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_board_file(&dir.path().join("nope.json"), "mac").is_err());
    }

    #[test]
    fn a_board_file_keeps_everything() {
        let dir = tempfile::tempdir().unwrap();
        let mut board = Board::new("mac");
        board.add_default_areas();
        let area = board.areas_sorted()[0].meta.id;
        let group = board.add_group(area, "Clients", [1, 2, 3]);
        let task = board.add_task(area, "task", Some(group), None);
        board.add_subtask(task, "step");
        board.set_area_color(area, Some([9, 9, 9]));
        let path = dir.path().join("file.json");
        write_board_file(&path, &board).unwrap();
        let back = read_board_file(&path, "mac").unwrap();
        assert_eq!(back.areas, board.areas);
        assert_eq!(back.groups, board.groups);
        assert_eq!(back.tasks, board.tasks);
    }

    #[test]
    fn moving_a_missing_old_folder_does_nothing() {
        let home = tempfile::tempdir().unwrap();
        let new = home.path().join("Priogator");
        move_folder(&home.path().join("Prioritizer"), &new).unwrap();
        assert!(!new.exists());
    }

    #[test]
    fn a_fully_moved_old_folder_is_removed() {
        let home = tempfile::tempdir().unwrap();
        let old = home.path().join("Prioritizer");
        let new = home.path().join("Priogator");
        fs::create_dir_all(old.join("backups/deep")).unwrap();
        fs::write(old.join("backups/deep/b.json"), "b").unwrap();
        fs::write(old.join("config.json"), "c").unwrap();
        move_folder(&old, &new).unwrap();
        assert!(!old.exists(), "empty afterwards, so removed");
        assert_eq!(
            fs::read_to_string(new.join("backups/deep/b.json")).unwrap(),
            "b"
        );
        assert_eq!(fs::read_to_string(new.join("config.json")).unwrap(), "c");
    }

    #[test]
    fn paths_inside_the_old_folder_are_rebased() {
        let old = Path::new("/home/a/Prioritizer");
        let new = Path::new("/home/a/Priogator");
        assert_eq!(rebase(old, old, new), Some(new.to_path_buf()));
        assert_eq!(
            rebase(&old.join("sync/sub"), old, new),
            Some(new.join("sync/sub"))
        );
        // A sibling whose name only starts the same isn't inside it.
        assert_eq!(rebase(Path::new("/home/a/Prioritizer2/x"), old, new), None);
    }

    #[test]
    fn device_names_are_safe_file_names() {
        let name = default_device_name();
        assert!(!name.is_empty());
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{name}"
        );
        assert_ne!(name, default_device_name(), "a random suffix each time");
    }
}
