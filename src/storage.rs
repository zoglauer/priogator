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

/// The file format. Bump it whenever records get new fields: an older
/// Priogator that finds a newer format stops saving until it is updated, so
/// it can never write back records without the fields it doesn't know.
/// 2: checksum; tasks' `repeat_day` and `reopened`.
const FORMAT_VERSION: u32 = 2;
/// From this format on, files carry a checksum and must match it.
const CHECKSUM_FORMAT: u32 = 2;
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
    /// Look for a new version once a day (macOS).
    #[serde(default = "yes")]
    pub check_updates: bool,
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

fn yes() -> bool {
    true
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
    /// FNV-1a 64 of the file's compact JSON without this field: a file
    /// changed by anything but Priogator (cut short, a bit flipped, edited
    /// by hand) is noticed and not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checksum: Option<String>,
    areas: Vec<Area>,
    groups: Vec<TaskGroup>,
    tasks: Vec<TaskItem>,
}

impl DeviceFile {
    fn compute_checksum(&self) -> io::Result<String> {
        let bytes = serde_json::to_vec(&DeviceFile {
            checksum: None,
            format: self.format,
            device: self.device.clone(),
            saved_at: self.saved_at,
            areas: self.areas.clone(),
            groups: self.groups.clone(),
            tasks: self.tasks.clone(),
        })?;
        Ok(format!("{:016x}", fnv1a64(&bytes)))
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Why a device file (or backup) isn't read.
#[derive(Debug)]
enum FileProblem {
    /// Written by a newer Priogator: this one must be updated.
    Newer(u32),
    /// Can't be read, isn't valid, or doesn't match its checksum.
    Damaged(String),
}

impl std::fmt::Display for FileProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileProblem::Newer(format) => write!(
                f,
                "written by a newer Priogator (format {format}); update this one"
            ),
            FileProblem::Damaged(why) => write!(f, "damaged ({why})"),
        }
    }
}

fn project_dirs() -> io::Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "Priogator")
        .ok_or_else(|| io::Error::other("no home directory found"))
}

/// Where the settings and the data (sync folder, backups) are kept. The same
/// folder on macOS; two on Linux.
struct Folders {
    config: PathBuf,
    data: PathBuf,
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
        let folders = |d: &directories::ProjectDirs| Folders {
            config: d.config_dir().to_path_buf(),
            data: d.data_dir().to_path_buf(),
        };
        let old = directories::ProjectDirs::from("", "", "Prioritizer");
        Self::load_from(&folders(&dirs), old.as_ref().map(folders).as_ref())
    }

    /// `load_or_create` with the folders given, moving over the ones of the
    /// app's old name (`old`) first. Tests pass temporary folders.
    fn load_from(new: &Folders, old: Option<&Folders>) -> io::Result<Self> {
        let file = new.config.join("config.json");
        if let Some(old) = old {
            move_folder(&old.config, &new.config)?;
            move_folder(&old.data, &new.data)?;
        }
        // Only a missing file means a first start; one that can't be read
        // must not be replaced with defaults (device name, folders, …).
        let text = match fs::read_to_string(&file) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(io::Error::new(e.kind(), format!("{}: {e}", file.display()))),
        };
        if let Some(text) = text {
            let mut config: Config = serde_json::from_str(&text)
                .map_err(|e| io::Error::other(format!("{}: {e}", file.display())))?;
            config.file = Some(file);
            if let Some(old) = old {
                let moved = |p: &Path| rebase(p, &old.data, &new.data);
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
            sync_dir: new.data.join("sync"),
            backup_dir: new.data.join("backups"),
            deadline_horizon_days: default_horizon_days(),
            done_days: default_done_days(),
            column_width: default_column_width(),
            folded_areas: Vec::new(),
            color_scheme: ColorScheme::default(),
            due_soon_color: None,
            check_updates: true,
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
///
/// Saving waits (see `not_saving`) while it could destroy something: our
/// own file can't be read (`damaged`, until the user agrees), the folder
/// can't be listed so we don't know what's in it (`unlisted`, until it can),
/// or another copy of the app has our file open (`lock`, until it quits).
pub struct SyncFolder {
    dir: PathBuf,
    device: String,
    /// Modification times of the device files at the last scan.
    seen: Vec<(PathBuf, SystemTime)>,
    damaged: Option<String>,
    unlisted: Option<String>,
    /// A device file from a newer Priogator: this one must be updated first.
    newer: Option<String>,
    /// Held while this copy of the app owns our file; locks go away with the
    /// process, also when it crashes.
    lock: Option<fs::File>,
    /// Another copy of the app holds the lock.
    open_elsewhere: bool,
}

/// Why saving waits, for the toolbar's warning.
pub struct NotSaving {
    pub message: String,
    pub kind: Waiting,
}

/// What the user can do about it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Waiting {
    /// Our own file is damaged: restore a backup, or continue without it
    /// (`save_anyway`).
    Damaged,
    /// A file from a newer Priogator: update this one.
    NewerFormat,
    /// Nothing but wait (folder unreadable, app open twice).
    Other,
}

impl SyncFolder {
    pub fn new(dir: &Path, device: &str) -> Self {
        Self {
            dir: dir.to_path_buf(),
            device: device.to_owned(),
            seen: Vec::new(),
            damaged: None,
            unlisted: None,
            newer: None,
            lock: None,
            open_elsewhere: false,
        }
    }

    pub fn not_saving(&self) -> Option<NotSaving> {
        let waits = |message: &str| NotSaving {
            message: message.to_owned(),
            kind: Waiting::Other,
        };
        if let Some(why) = &self.unlisted {
            return Some(waits(why));
        }
        if let Some(why) = &self.newer {
            return Some(NotSaving {
                message: why.clone(),
                kind: Waiting::NewerFormat,
            });
        }
        if self.open_elsewhere {
            return Some(waits(&format!(
                "Priogator is already open with these settings, and it saves {}. \
                 Changes made here are not saved until that one is closed.",
                self.own_file().display()
            )));
        }
        self.damaged.as_ref().map(|message| NotSaving {
            message: message.clone(),
            kind: Waiting::Damaged,
        })
    }

    /// Save again although our own file could not be read (its copy stays).
    pub fn save_anyway(&mut self) {
        self.damaged = None;
        // The file as it is now may be replaced; only a later change is checked.
        let own = self.own_file();
        if let Ok(modified) = fs::metadata(&own).and_then(|m| m.modified()) {
            self.seen.retain(|(path, _)| *path != own);
            self.seen.push((own, modified));
            self.seen.sort();
        }
    }

    /// Own our file, unless another copy of the app does. Returns whether we
    /// have it. (Without a lock file at all, e.g. in a read-only folder,
    /// saving fails on its own.)
    fn take_lock(&mut self) -> bool {
        if self.lock.is_some() {
            return true;
        }
        let path = self.dir.join(format!(".{FILE_PREFIX}{}.lock", self.device));
        let file = fs::create_dir_all(&self.dir).and_then(|()| {
            fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)
        });
        let Ok(file) = file else {
            return true;
        };
        self.open_elsewhere = match file.try_lock() {
            Ok(()) => {
                self.lock = Some(file);
                false
            }
            Err(fs::TryLockError::WouldBlock) => true,
            Err(fs::TryLockError::Error(_)) => false,
        };
        !self.open_elsewhere
    }

    /// Our own file can't be read: copy it aside, so that saving can't
    /// destroy what might still be recovered from it, and stop saving.
    /// Once is enough while waiting for the user.
    fn note_damage(&mut self, own: &Path, error: &dyn std::fmt::Display) {
        if self.damaged.is_some() {
            return;
        }
        let stamp = jiff::Zoned::now().strftime("%Y-%m-%d-%H%M%S").to_string();
        let name = own.file_name().unwrap_or_default().to_string_lossy();
        let copy = self.dir.join(format!("damaged-{stamp}-{name}"));
        self.damaged = Some(match fs::copy(own, &copy) {
            Ok(_) => format!(
                "This computer's file {} is {error}, so it isn't read. A copy is kept \
                 as {}. Changes are not saved until you restore a backup or carry on \
                 without it.",
                own.display(),
                copy.display()
            ),
            Err(copy_error) => format!(
                "This computer's file {} is {error}, so it isn't read, nor could it be \
                 copied ({copy_error}). Changes are not saved until you restore a \
                 backup or carry on without it.",
                own.display()
            ),
        });
    }

    fn note_newer(&mut self, path: &Path, format: u32) {
        self.newer = Some(format!(
            "{} was written by a newer Priogator (file format {format}, this one \
             reads up to {FORMAT_VERSION}). Update Priogator (☰ → Check for \
             Updates…); until then, changes here are not saved.",
            path.display()
        ));
    }

    fn own_file(&self) -> PathBuf {
        self.dir
            .join(format!("{FILE_PREFIX}{}{FILE_SUFFIX}", self.device))
    }

    /// All device files currently in the folder, with modification times.
    /// A folder that doesn't exist yet is empty; one that can't be listed
    /// is an error, not empty.
    fn scan(&self) -> io::Result<Vec<(PathBuf, SystemTime)>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX) {
                files.push((entry.path(), entry.metadata()?.modified()?));
            }
        }
        files.sort();
        Ok(files)
    }

    /// Load the board by merging every device file in the folder.
    /// Returns the board and a list of files that could not be read.
    pub fn load(&mut self) -> (Board, Vec<String>) {
        self.take_lock();
        let mut board = Board::new(&self.device);
        let errors = self.merge_all(&mut board);
        if board.areas.is_empty() {
            board.add_default_areas();
        }
        (board, errors)
    }

    /// Merge all device files that changed since the last scan (and try
    /// again what made saving wait). Returns (anything changed, errors).
    pub fn reload_if_changed(&mut self, board: &mut Board) -> (bool, Vec<String>) {
        if self.open_elsewhere && self.take_lock() {
            // The other copy has quit: read everything it saved, before we save.
            self.seen.clear();
        }
        if self.unlisted.is_none() && self.scan().is_ok_and(|files| files == self.seen) {
            return (false, Vec::new());
        }
        let before = board.clone();
        let errors = self.merge_all(board);
        (*board != before, errors)
    }

    fn merge_all(&mut self, board: &mut Board) -> Vec<String> {
        let files = match self.scan() {
            Ok(files) => files,
            Err(e) => {
                self.unlisted = Some(format!(
                    "The sync folder {} can't be read ({e}). Changes are not saved \
                     until it can be.",
                    self.dir.display()
                ));
                return Vec::new();
            }
        };
        self.unlisted = None;
        self.newer = None;
        let own = self.own_file();
        let mut errors = Vec::new();
        for (path, _) in &files {
            match read_device_file(path) {
                Ok(file) => {
                    board.merge(file.areas, file.groups, file.tasks);
                }
                // Not problems the user dismisses: they stop saving.
                Err(FileProblem::Newer(format)) => self.note_newer(path, format),
                Err(e) if *path == own => self.note_damage(path, &e),
                Err(e) => errors.push(format!(
                    "{} is {e}, so it isn't read. On that computer, restore a backup \
                     (☰ → Backups…).",
                    path.display()
                )),
            }
        }
        self.seen = files;
        errors
    }

    /// Write this device's file with the full current state. If the file
    /// changed since we last read or wrote it, that is merged in first
    /// (or, if it can't be read, kept and not overwritten). Returns whether
    /// that merge changed the board.
    pub fn save(&mut self, board: &mut Board) -> io::Result<bool> {
        if !self.take_lock() || self.not_saving().is_some() {
            return Err(io::Error::other("saving waits, see the warning"));
        }
        let own = self.own_file();
        let seen = self.seen.iter().find(|(p, _)| *p == own).map(|(_, t)| *t);
        let mut merged = false;
        if let Ok(modified) = fs::metadata(&own).and_then(|m| m.modified())
            && Some(modified) != seen
        {
            match read_device_file(&own) {
                Ok(file) => {
                    let before = board.clone();
                    board.merge(file.areas, file.groups, file.tasks);
                    merged = *board != before;
                }
                Err(FileProblem::Newer(format)) => {
                    self.note_newer(&own, format);
                    return Err(io::Error::other("written by a newer Priogator"));
                }
                Err(e) => {
                    self.note_damage(&own, &e);
                    return Err(io::Error::other("this computer's file can't be read"));
                }
            }
        }
        write_board_file(&own, board)?;
        // Our own write is not a change from another device.
        let modified = fs::metadata(&own)?.modified()?;
        self.seen.retain(|(path, _)| *path != own);
        self.seen.push((own, modified));
        self.seen.sort();
        Ok(merged)
    }
}

/// Only one Priogator at a time per user: the lock on `.priogator.lock` in
/// the settings folder is held while it runs (the returned file), and goes
/// with the process, also when it crashes. `None`: another one runs.
pub fn single_instance(dir: &Path) -> io::Result<Option<fs::File>> {
    fs::create_dir_all(dir)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".priogator.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Write the full state of `board` to `path` (sync files and backups alike).
pub fn write_board_file(path: &Path, board: &Board) -> io::Result<()> {
    let mut file = DeviceFile {
        format: FORMAT_VERSION,
        device: board.device.clone(),
        saved_at: Timestamp::now(),
        checksum: None,
        areas: board.areas.values().cloned().collect(),
        groups: board.groups.values().cloned().collect(),
        tasks: board.tasks.values().cloned().collect(),
    };
    file.checksum = Some(file.compute_checksum()?);
    write_atomic(path, &serde_json::to_vec_pretty(&file)?)
}

/// Read a sync file or backup as a board of its own (for restoring).
pub fn read_board_file(path: &Path, device: &str) -> io::Result<Board> {
    let file = read_device_file(path).map_err(|e| io::Error::other(e.to_string()))?;
    let mut board = Board::new(device);
    board.merge(file.areas, file.groups, file.tasks);
    Ok(board)
}

fn read_device_file(path: &Path) -> Result<DeviceFile, FileProblem> {
    let damaged = |e: &dyn std::fmt::Display| FileProblem::Damaged(e.to_string());
    let bytes = fs::read(path).map_err(|e| damaged(&e))?;
    // The format first: a newer file may not even parse as ours.
    #[derive(Deserialize)]
    struct Header {
        format: u32,
    }
    let header: Header = serde_json::from_slice(&bytes).map_err(|e| damaged(&e))?;
    if header.format > FORMAT_VERSION {
        return Err(FileProblem::Newer(header.format));
    }
    let file: DeviceFile = serde_json::from_slice(&bytes).map_err(|e| damaged(&e))?;
    if file.format >= CHECKSUM_FORMAT {
        let expected = file.compute_checksum().map_err(|e| damaged(&e))?;
        if file.checksum.as_deref() != Some(expected.as_str()) {
            return Err(FileProblem::Damaged("its checksum doesn't match".into()));
        }
    }
    Ok(file)
}

/// Write via a hidden temp file and rename, so a crash or a sync tool never
/// sees a half-written file.
fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    // Its own name per write, so two writers never share a temp file.
    let tmp = dir.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents)?;
        // On disk before it replaces the old file: after a power cut the
        // file is either the old or the new one, not empty.
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written?;
    // The rename itself, too.
    if let Ok(dir) = fs::File::open(dir) {
        let _ = dir.sync_all();
    }
    Ok(())
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
            check_updates: true,
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
        mac_folder.save(&mut mac).unwrap();

        let mut linux_folder = SyncFolder::new(dir.path(), "linux");
        let (mut linux, errors) = linux_folder.load();
        assert!(errors.is_empty());
        assert_eq!(linux.priority_list(area).len(), 1);
        assert_eq!(linux.areas_sorted().len(), mac.areas_sorted().len());

        linux.add_task(area, "from linux", None, None);
        linux_folder.save(&mut linux).unwrap();

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

    /// The copies of damaged own files in `dir`.
    fn damaged_copies(dir: &Path) -> Vec<PathBuf> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("damaged-")
            })
            .collect()
    }

    #[test]
    fn a_damaged_own_file_is_kept_and_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("device-mac.json");
        fs::write(&own, "{ half a file").unwrap();
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, errors) = folder.load();
        assert!(errors.is_empty(), "not a dismissable problem: {errors:?}");
        let message = folder.not_saving().unwrap().message;
        assert!(message.contains("isn't read"), "{message}");

        // A copy is kept (not read as a device file), and saving waits.
        let copies = damaged_copies(dir.path());
        assert_eq!(copies.len(), 1);
        assert_eq!(fs::read_to_string(&copies[0]).unwrap(), "{ half a file");
        assert!(folder.save(&mut board).is_err());
        assert_eq!(fs::read_to_string(&own).unwrap(), "{ half a file");
        assert_eq!(folder.scan().unwrap().len(), 1);

        // Scanning again doesn't make more copies or report it again.
        let (_, again) = folder.load();
        assert!(again.is_empty(), "{again:?}");
        assert_eq!(damaged_copies(dir.path()), copies);

        // Once the user agrees, saving works; the copy stays.
        folder.save_anyway();
        folder.save(&mut board).unwrap();
        assert!(read_device_file(&own).is_ok());
        assert!(copies[0].exists());
    }

    #[test]
    fn a_file_damaged_while_running_is_kept_before_saving() {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("device-mac.json");
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, _) = folder.load();
        folder.save(&mut board).unwrap();
        // Damaged after it was loaded and saved, e.g. by a sync tool.
        fs::write(&own, "{ cut short").unwrap();
        let area = board.areas_sorted()[0].meta.id;
        board.add_task(area, "New", None, None);

        assert!(
            folder.save(&mut board).is_err(),
            "checked before overwriting"
        );
        assert_eq!(fs::read_to_string(&own).unwrap(), "{ cut short");
        assert!(folder.not_saving().is_some());
        let copies = damaged_copies(dir.path());
        assert_eq!(copies.len(), 1);
        assert_eq!(fs::read_to_string(&copies[0]).unwrap(), "{ cut short");

        // Our own saves don't count as changes to check.
        folder.save_anyway();
        folder.save(&mut board).unwrap();
        folder.save(&mut board).unwrap();
        assert!(folder.not_saving().is_none());
    }

    #[test]
    fn another_devices_damaged_file_doesnt_stop_saving() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("device-pc.json"), "junk").unwrap();
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, _) = folder.load();
        assert!(folder.not_saving().is_none());
        folder.save(&mut board).unwrap();
    }

    #[test]
    fn writes_leave_no_temp_files_and_dont_collide() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-mac.json");
        std::thread::scope(|scope| {
            for i in 0..8 {
                let path = &path;
                scope.spawn(move || write_atomic(path, format!("{{\"n\":{i}}}").as_bytes()));
            }
        });
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(names.len(), 1, "only the file itself");
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            serde_json::from_str::<serde_json::Value>(&text).is_ok(),
            "{text}"
        );
    }

    #[test]
    fn only_one_instance_runs_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let first = single_instance(dir.path()).unwrap();
        assert!(first.is_some());
        assert!(
            single_instance(dir.path()).unwrap().is_none(),
            "a second one"
        );
        drop(first);
        assert!(
            single_instance(dir.path()).unwrap().is_some(),
            "after the first quit"
        );
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

    /// The app's folders under a temporary home: one folder (macOS) or
    /// separate config and data folders (Linux).
    fn folders(home: &Path, name: &str, separate: bool) -> Folders {
        if separate {
            Folders {
                config: home.join(".config").join(name),
                data: home.join(".local/share").join(name),
            }
        } else {
            Folders {
                config: home.join(name),
                data: home.join(name),
            }
        }
    }

    #[test]
    fn a_first_start_creates_the_settings_in_the_new_folder() {
        let home = tempfile::tempdir().unwrap();
        let new = folders(home.path(), "Priogator", false);
        let old = folders(home.path(), "Prioritizer", false);
        let config = Config::load_from(&new, Some(&old)).unwrap();
        assert_eq!(config.sync_dir, new.data.join("sync"));
        assert_eq!(config.backup_dir, new.data.join("backups"));
        assert!(new.config.join("config.json").exists());
        assert!(!old.config.exists(), "no old folder is made");
        // The second start reads the same settings.
        let again = Config::load_from(&new, Some(&old)).unwrap();
        assert_eq!(again.device, config.device);
    }

    #[test]
    fn everything_from_prioritizer_moves_over() {
        for separate in [false, true] {
            let home = tempfile::tempdir().unwrap();
            let old = folders(home.path(), "Prioritizer", separate);
            let new = folders(home.path(), "Priogator", separate);

            // Prioritizer's settings, tasks and a backup.
            let mut config = Config::load_from(&old, None).unwrap();
            config.device = "macbook-3f2a".into();
            config.done_days = 14;
            config.color_scheme = ColorScheme::Rainbow;
            config.save().unwrap();
            let mut board = Board::new("macbook-3f2a");
            board.add_default_areas();
            let work = board.areas_sorted()[0].meta.id;
            board.add_task(work, "From the old app", None, None);
            SyncFolder::new(&config.sync_dir, &config.device)
                .save(&mut board)
                .unwrap();
            let backups = crate::backup::Backups::new(&config.backup_dir);
            backups.now(&board).unwrap();

            for start in 0..2 {
                let config = Config::load_from(&new, Some(&old)).unwrap();
                let what = format!("separate folders: {separate}, start {start}");
                assert_eq!(config.device, "macbook-3f2a", "{what}");
                assert_eq!(config.done_days, 14, "{what}");
                assert_eq!(config.color_scheme, ColorScheme::Rainbow, "{what}");
                assert_eq!(config.sync_dir, new.data.join("sync"), "{what}");
                assert_eq!(config.backup_dir, new.data.join("backups"), "{what}");
                let (board, errors) = SyncFolder::new(&config.sync_dir, &config.device).load();
                assert!(errors.is_empty(), "{what}: {errors:?}");
                assert!(
                    board.tasks.values().any(|t| t.title == "From the old app"),
                    "{what}"
                );
                let backups = crate::backup::Backups::new(&config.backup_dir);
                assert_eq!(backups.list().len(), 1, "{what}");
                // The rebased paths are saved, too.
                let saved: Config = serde_json::from_str(
                    &fs::read_to_string(new.config.join("config.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(saved.sync_dir, config.sync_dir, "{what}");
            }
            assert!(
                !old.config.exists() && !old.data.exists(),
                "emptied and removed"
            );
        }
    }

    #[test]
    fn unreadable_settings_are_reported_not_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let new = folders(home.path(), "Priogator", false);
        let mut config = Config::load_from(&new, None).unwrap();
        config.device = "macbook-3f2a".into();
        config.save().unwrap();
        let file = new.config.join("config.json");
        let saved = fs::read_to_string(&file).unwrap();

        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        let error = Config::load_from(&new, None).map(|_| ()).unwrap_err();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(error.to_string().contains("config.json"), "{error}");
        assert_eq!(fs::read_to_string(&file).unwrap(), saved, "left alone");

        // Not valid JSON: the same.
        fs::write(&file, "{ cut short").unwrap();
        let error = Config::load_from(&new, None).map(|_| ()).unwrap_err();
        assert!(error.to_string().contains("config.json"), "{error}");
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ cut short");
    }

    #[test]
    fn a_sync_folder_elsewhere_stays_where_it_is() {
        let home = tempfile::tempdir().unwrap();
        let old = folders(home.path(), "Prioritizer", false);
        let new = folders(home.path(), "Priogator", false);
        let mut config = Config::load_from(&old, None).unwrap();
        config.sync_dir = home.path().join("Syncthing/priogator");
        config.save().unwrap();
        let config = Config::load_from(&new, Some(&old)).unwrap();
        assert_eq!(config.sync_dir, home.path().join("Syncthing/priogator"));
        assert_eq!(config.backup_dir, new.data.join("backups"));
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
        // Another computer runs a newer version: this one must be updated
        // before it saves again, so it can't drop fields it doesn't know.
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, errors) = folder.load();
        assert!(errors.is_empty(), "{errors:?}");
        let why = folder.not_saving().expect("saving waits");
        assert_eq!(why.kind, Waiting::NewerFormat);
        assert!(
            why.message.contains("device-future.json"),
            "{}",
            why.message
        );
        assert!(why.message.contains("Check for Updates"), "{}", why.message);
        assert!(folder.save(&mut board).is_err());
        assert!(!dir.path().join("device-mac.json").exists());

        // Once that file is gone (or this app updated), saving works again.
        fs::remove_file(&path).unwrap();
        folder.reload_if_changed(&mut board);
        assert!(folder.not_saving().is_none());
        folder.save(&mut board).unwrap();
    }

    #[test]
    fn files_carry_a_checksum_and_a_changed_one_isnt_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-pc.json");
        let mut board = Board::new("pc");
        board.add_default_areas();
        let work = board.areas_sorted()[0].meta.id;
        board.add_task(work, "Some task", None, None);
        write_board_file(&path, &board).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"checksum\""));
        assert!(read_board_file(&path, "mac").is_ok());

        // One value changed, the file still valid JSON: noticed.
        fs::write(&path, text.replacen("Some task", "Some tusk", 1)).unwrap();
        let error = read_board_file(&path, "mac").unwrap_err().to_string();
        assert!(error.contains("checksum"), "{error}");
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (board, errors) = folder.load();
        assert_eq!(errors.len(), 1, "another computer's file: a problem");
        assert!(errors[0].contains("restore a backup"), "{errors:?}");
        assert!(!board.tasks.values().any(|t| t.title == "Some tusk"));

        // Missing the checksum in a format that has one: not read either.
        let stripped: String = text
            .lines()
            .filter(|l| !l.contains("\"checksum\""))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, stripped).unwrap();
        assert!(read_board_file(&path, "mac").is_err());
    }

    #[test]
    fn files_of_the_first_format_are_still_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device-old.json");
        let mut board = Board::new("old");
        board.add_default_areas();
        write_board_file(&path, &board).unwrap();
        // As Priogator 1.0 wrote it: format 1, no checksum.
        let old: String = fs::read_to_string(&path)
            .unwrap()
            .replacen(&format!("\"format\": {FORMAT_VERSION}"), "\"format\": 1", 1)
            .lines()
            .filter(|l| !l.contains("\"checksum\""))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, old).unwrap();
        assert_eq!(read_board_file(&path, "mac").unwrap().areas.len(), 4);
    }

    #[test]
    fn the_own_file_is_named_after_the_device() {
        let dir = tempfile::tempdir().unwrap();
        let mut folder = SyncFolder::new(dir.path(), "macbook-3f2a");
        let (mut board, _) = folder.load();
        folder.save(&mut board).unwrap();
        let mut names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [".device-macbook-3f2a.lock", "device-macbook-3f2a.json"],
            "the file, its lock, and no temp file left"
        );
    }

    /// A task in Work, saved by `folder`.
    fn save_with_task(folder: &mut SyncFolder, board: &mut Board, title: &str) {
        let work = board.areas_sorted()[0].meta.id;
        board.add_task(work, title, None, None);
        folder.save(board).unwrap();
    }

    fn titles_in(path: &Path) -> Vec<String> {
        let mut titles: Vec<String> = read_board_file(path, "x")
            .unwrap()
            .tasks
            .into_values()
            .map(|t| t.title)
            .collect();
        titles.sort();
        titles
    }

    #[test]
    fn a_second_copy_of_the_app_waits_until_the_first_quits() {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("device-mac.json");
        let mut first = SyncFolder::new(dir.path(), "mac");
        let (mut first_board, _) = first.load();
        let mut second = SyncFolder::new(dir.path(), "mac");
        let (mut second_board, _) = second.load();
        let why = second.not_saving().expect("the second one waits");
        assert!(why.message.contains("already open"), "{}", why.message);
        assert_eq!(why.kind, Waiting::Other);
        assert!(first.not_saving().is_none());

        save_with_task(&mut first, &mut first_board, "from the first");
        let work = second_board.areas_sorted()[0].meta.id;
        second_board.add_task(work, "from the second", None, None);
        assert!(second.save(&mut second_board).is_err());
        assert_eq!(titles_in(&own), ["from the first"]);

        // The first quits: the second takes over, reads what it saved first,
        // and then saves both.
        drop(first);
        second.reload_if_changed(&mut second_board);
        assert!(second.not_saving().is_none());
        second.save(&mut second_board).unwrap();
        assert_eq!(titles_in(&own), ["from the first", "from the second"]);
    }

    #[test]
    fn a_file_changed_from_outside_is_merged_before_saving() {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("device-mac.json");
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, _) = folder.load();
        save_with_task(&mut folder, &mut board, "ours");
        // Something else (no lock: e.g. an older Priogator) writes the file.
        let mut other = read_board_file(&own, "mac").unwrap();
        let work = other.areas_sorted()[0].meta.id;
        other.add_task(work, "written elsewhere", None, None);
        std::thread::sleep(std::time::Duration::from_millis(10));
        write_board_file(&own, &other).unwrap();

        let work = board.areas_sorted()[0].meta.id;
        board.add_task(work, "ours too", None, None);
        assert!(folder.save(&mut board).unwrap(), "merged something");
        assert_eq!(titles_in(&own), ["ours", "ours too", "written elsewhere"]);
        assert!(
            !folder.save(&mut board).unwrap(),
            "nothing new the next time"
        );
    }

    #[test]
    fn a_folder_that_cant_be_listed_isnt_taken_for_empty() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("device-mac.json");
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, _) = folder.load();
        save_with_task(&mut folder, &mut board, "saved before");
        drop(folder);
        let mode = |m| fs::set_permissions(dir.path(), fs::Permissions::from_mode(m)).unwrap();

        // Files can be written and opened, but not listed.
        mode(0o300);
        let mut folder = SyncFolder::new(dir.path(), "mac");
        let (mut board, _) = folder.load();
        let why = folder.not_saving().map(|w| (w.message, w.kind));
        let failed_save = folder.save(&mut board).is_err();
        let reload = folder.reload_if_changed(&mut board);
        let still = folder.not_saving().is_some();
        mode(0o755);
        let (message, kind) = why.expect("saving waits");
        assert!(message.contains("can't be read"), "{message}");
        assert_eq!(kind, Waiting::Other);
        assert!(failed_save);
        assert_eq!(reload, (false, Vec::new()));
        assert!(still, "and keeps waiting while it can't be listed");
        assert_eq!(titles_in(&own), ["saved before"], "not overwritten");

        // Readable again: everything is read, then saving works.
        let (changed, _) = folder.reload_if_changed(&mut board);
        assert!(changed);
        assert!(folder.not_saving().is_none());
        assert!(board.tasks.values().any(|t| t.title == "saved before"));
        save_with_task(&mut folder, &mut board, "after");
        assert_eq!(titles_in(&own), ["after", "saved before"]);
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
        folder.save(&mut board).unwrap();
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
        let (mut board, _) = folder.load();
        folder.save(&mut board).unwrap();
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
        aa.tasks.get_mut(&id).unwrap().meta.updated_at = stamp + jiff::SignedDuration::from_secs(1);
        newer.save(&mut aa).unwrap();
        SyncFolder::new(dir.path(), "zz").save(&mut zz).unwrap();

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
        mac_folder.save(&mut mac).unwrap();

        let mut linux_folder = SyncFolder::new(dir.path(), "linux");
        let (mut linux, _) = linux_folder.load();
        linux.delete_task(id);
        linux_folder.save(&mut linux).unwrap();

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
        linux_folder.save(&mut linux).unwrap();
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
