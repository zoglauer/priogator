# AGENTS.md

Notes for anyone (human or AI agent) working on Priogator. Read this first.

## What it is

Priogator ("Priority tracker") is a personal, local-only to-do app for macOS
and Linux, written in Rust with egui/eframe 0.37. Tasks live in **areas**
(the columns: Work, Private ventures, House, Family, …). Each area has a
**Due soon · Pinned** section on top (deadline within its lead days, or
pinned) and a freely re-orderable **Priorities** list below, plus a
**Done** section.

- Designed by Andreas Zoglauer, implemented by Claude. Keep both lines in
  the About dialog exactly like that.
- Licensed Apache-2.0 (`LICENSE`). Repo: https://github.com/zoglauer/priogator
- Formerly called **Prioritizer**. The app migrates old data on first start
  (see Storage). Don't rename things back.
- `README.md` starts with the owner's original design notes. Leave them
  alone apart from the product name.

## Hard constraints

- **No online services, no server, no accounts.** Data moves between
  computers only through files in a sync folder (Syncthing on the local
  network, or a USB stick).
- Must work on **macOS and Linux**. Mac-only code goes behind
  `#[cfg(target_os = "macos")]` (see `src/macos_menu.rs`).
- **Tests must never touch real user data.** `Config` has a
  `#[serde(skip)] file: Option<PathBuf>`; `save()` does nothing when it is
  `None`, and test configs leave it `None`. Never call
  `Config::load_or_create()` from tests. Real data lives in
  `~/Library/Application Support/Priogator/` (Linux: `~/.config/priogator`,
  `~/.local/share/priogator`).
- Don't commit, tag or push unless the owner asks.

## Commands

```sh
cargo run                      # start the app (uses the real settings)
cargo run -- --demo            # add example tasks if there are none
cargo test                     # all tests (unit + UI tests with egui_kittest)
cargo clippy --all-targets     # must be warning-free
cargo fmt
APP_PREVIEW=/some/dir/prev cargo test app_preview -- --ignored
                               # PNG screenshots of the UI with demo data, light
                               # + dark, dialogs included: look at them after UI changes
FONT_PREVIEW=/some/file.png cargo test font_preview -- --ignored
scripts/bundle-macos.sh        # build target/release/Priogator.app
scripts/bundle-macos.sh --install   # …and copy it to ~/Applications
scripts/bundle-macos.sh --release   # universal (arm64 + x86_64) app + DMG
scripts/install-linux.sh       # ~/.local/bin/priogator + launcher entry
scripts/licenses.sh            # regenerate third-party licenses (see below)
```

If `cargo` is not on PATH: `source ~/.cargo/env`.

After a change the owner will look at, rebuild and reinstall the app:
`pkill -x Priogator; scripts/bundle-macos.sh --install && open ~/Applications/Priogator.app`.

## Code layout

| File | What |
|---|---|
| `src/main.rs` | startup, window options, `--demo` |
| `src/model.rs` | data types: `Meta`, `Area`, `TaskGroup`, `TaskItem`, `Subtask` |
| `src/board.rs` | all tasks in memory: lists, ordering, moves, done/reopen, stats, merge, restore |
| `src/repeat.rs` | recurring tasks (`Repeat::Schedule` / `AfterDone`), next due date |
| `src/active.rs` | active hours per area (areas fold outside them) |
| `src/storage.rs` | `Config` (settings), `SyncFolder` (per-device files), migration from Prioritizer |
| `src/backup.rs` | daily backups, retention, restore/delete |
| `src/app.rs` | the whole UI, dialogs, and the UI tests (`mod tests` at the end) |
| `src/fonts.rs`, `src/theme.rs` | system fonts via fontique (bold: a variable font's weight axis, else the family's bold face), macOS label colours, dark mode |
| `src/licenses.rs` | parses `assets/third-party-licenses.txt` for the About dialog |
| `src/macos_menu.rs` | macOS app menu "About Priogator" opens our dialog (objc2) |
| `src/demo.rs` | example data |
| `HELP.md` | user guide; keep it in sync with every user-visible change |
| `scripts/` | packaging, `make_icon.py` (draws `assets/icon.png`), `licenses.sh` |
| `.github/workflows/release.yml` | builds and publishes the DMG on tags `vX.Y.Z` |

## Data and sync model

- Every computer writes **only its own** file `device-<name>.json` in the
  sync folder and reads everyone else's. Never write another device's file.
- Records (areas, groups, tasks) carry `Meta { id, updated_at, device,
  deleted }`. Merging is last-writer-wins per record; deletions are
  tombstones (`deleted: true`), never removed outright.
- Files are written atomically (temp file + rename). Files have a `format`
  number (`FORMAT_VERSION` in `storage.rs`). **Bump it whenever a record's
  serialized fields change** (new field, rename, skip rules): an app that
  sees a newer format anywhere in the sync folder stops saving
  (`Waiting::NewerFormat`) until it is updated, so it can't drop fields it
  doesn't know. Keep reading old formats; new fields need `#[serde(default)]`.
- From format 2 every file has a `checksum` (FNV-1a 64 of its compact JSON
  without the checksum, recomputed by re-serializing what was read, so
  serialization must round-trip exactly; another reason to bump the format
  on any change). A mismatch means damaged: not read; for the own file the
  "⚠ Not saved" dialog offers a backup or continuing without it.
- One instance per user: `storage::single_instance` locks
  `.priogator.lock` in the settings folder (main.rs); a second start says so
  and quits.
- Recurring tasks: completing one creates the next occurrence with a
  deterministic id `Uuid::new_v5(&id, b"next occurrence")`, so two
  computers completing the same task don't create duplicates; reopening
  deletes that successor.
- `SyncFolder::not_saving()` says when saving waits; the toolbar shows it
  as "⚠ Not saved", deliberately apart from the dismissable problems, so
  dismissing those never allows overwriting:
  - own file unreadable or failing its checksum (at load, a scan, or
    before overwriting): a copy `damaged-…-device-<name>.json` is kept (not
    scanned as a device file); clicking "⚠ Not saved" opens a dialog:
    restore a backup (which ends the wait), or continue (`save_anyway`);
  - sync folder can't be listed (`scan` returns the error; only a missing
    folder counts as empty): waits until it can, then reads everything;
  - another copy of the app holds `.device-<name>.lock` (`File::try_lock`,
    released when the process ends; a second guard besides the single
    instance lock, e.g. two settings folders with one device name): waits,
    takes over when it quits, and reads its file first;
  - a device file has a newer format: waits until this app is updated
    (clicking the warning checks for updates).
  Daily backups wait too (the board may be incomplete).
- Before writing, an own file that changed since our last read/write is
  merged in (`save` takes `&mut Board`), never just overwritten.
- After a merge, a task whose group is now in another area follows the
  group (`Board::follow_moved_groups`), the same in either merge order.
  And an open recurring task whose open successor exists was completed
  elsewhere: it is marked done again (`keep_completions`), keeping edits,
  unless it was reopened by hand (`TaskItem.reopened`).
- Reopening a recurring task deletes its open successor only if that wasn't
  edited (stamped more than a second after its creation); else both stay
  open (the owner's choice).
- Active hours with equal from/to mean all day on the chosen days.
- Editing a record deleted elsewhere brings it back: tasks (dialog save),
  areas (`update_area`, or a task added/moved into it), groups
  (`update_group`).
- Monthly/yearly series keep their day of the month in
  `TaskItem.repeat_day` (Jan 31 → Feb 28 → Mar 31); a deadline set by hand
  resets it. Day counts from files are clamped (`MAX_DAYS`), and repeat
  spans and loops are limited, so damaged values can't panic or hang.
- Settings: only a missing `config.json` means first start. Unreadable or
  invalid settings are reported (a dialog via rfd) and left alone.
- Undo/redo and backup restore go through `Board::restore`.
- The Log (☰ → Log…, or a click on the toolbar's done count) lists every
  completed task by its `completed_at`, by local day (`Board::achieved`,
  `log_dialog`). Nothing extra is stored: it relies on done tasks never
  being purged, so keep it that way.
- Backups: `backup-YYYY-MM-DD-HHMMSS.json`, one automatic per day, kept:
  all from the last 7 days, then weekly up to 2 months, monthly up to 2
  years, yearly after that. `prune` deletes at most `MAX_PRUNED` (3) per
  run, newest first, so a wrong clock can't wipe the history. No daily
  backup while saving waits (the board may be incomplete). Quitting with
  unsaved changes that can't be saved writes a rescue backup.
- Files are fsynced before the rename (`write_atomic`; temp names are
  unique per write). On macOS `sync_all` is a full flush, so don't save
  every frame: the settings dialog saves once a drag is over.
- Rename migration (`Config::load_or_create`): contents of the old
  `Prioritizer` config/data folders move into the `Priogator` ones, and
  `sync_dir`/`backup_dir` paths in the config are rebased. The work is in
  `Config::load_from(new, old)`, which tests call with temporary folders.

## UI conventions and owner preferences

- Words: **area** (never "column" in the UI), **group**, **task**,
  **subtask**. Buttons say **+ New**, not "Add". Task title placeholder:
  "What needs to be done?"; notes placeholder: "Add note".
- Top section heading "DUE SOON · PINNED" on a warm tint; pinned tasks
  don't say "pinned". No position numbers on cards.
- Overdue and due-today tasks are **bold**; overdue stays visible.
- Group name is drawn small above the task title, aligned with it.
- Bottom row of each area: `Groups  [All ▾]  … [+ New]`, always one fixed
  height row. The dropdown filters; its "Edit group ▸" submenu opens the
  group dialog (name, colour, area, Delete).
- Dialogs: Delete top right (red), Cancel/Save bottom right.
- Areas fold to a narrow strip with the name rotated (reads bottom to top),
  bold like the headers. Folding: ‹ button, or automatically outside active
  hours once the window loses focus.
- Colors setting (`ColorScheme` in `storage.rs`): Standard, or Rainbow,
  which tints each area's open cards red → blue by position (top section
  first). Tints are set in on-screen values (`HsvaGamma`), soft enough that
  group names, dates and red "overdue" stay readable in light and dark.
- Per-task colour (`TaskItem.color`, synced): overrides the scheme and is
  shown **exactly as stored** — what the picker shows is what the card
  shows (the owner insisted). Preset dots are soft tints of `TASK_COLORS`
  for the current light/dark mode (`preset_colors`, same tint as Rainbow,
  stored as such); "Other…" is egui's full picker. On a colour too dark
  for a light window (or too light for a dark one) the card's text flips
  (`contrast_text`). Keep the tints soft; the owner preferred them.
- Area colour (`Area.color`, synced): the column's background (and folded
  strip), exactly as stored, chosen with the same `color_choice`. If it
  contrasts with the window's mode (`contrast_text`), the column uses the
  other theme's visuals (`ctx.style_of(other)`), so labels, buttons and
  cards stay readable.
- macOS look: system fonts, macOS label colours, dark mode follows the system.
- The owner refines visuals a lot. When they describe a layout, implement
  exactly that rather than an alternative of your own; render previews and
  look at them before reporting.

## Tests

~330 tests, all run by `cargo test` in a few seconds (plus the two preview
renderers, `--ignored`). Every feature gets a test; add one with each change.

- Data: `mod tests` in each module (board, storage, backup, model, repeat,
  active, demo, licenses).
- UI, the real app driven like a user: `src/app/ui_tests_tasks.rs` (task
  dialog, cards, menus, keyboard, drag & drop, undo, stats) and
  `src/app/ui_tests_app.rs` (areas, groups, settings, backups, About, menu,
  sync). Shared helpers (`harness`, `demo_config`, `click_at`,
  `right_click_at`, `drag`) live in `app::tests`.
- Drive the action under test through the UI; setting state directly is
  only for setup. Assert on `harness.state()` and on what is shown.
- Hand-drawn widgets need `widget_info` labels to be findable (e.g. the
  subtask counter is "Subtasks 1/3", colour dots "Red", "Purple"…).
- Kittest steps advance 0.25 s: double-clicks need a widened delay (see the
  helper in ui_tests_tasks), and dialogs re-centre for a few frames after
  their content changes — let them settle before clicking.
- Never rely on two `Timestamp::now()` calls differing (same tick → tie →
  device name decides); set `updated_at` explicitly when order matters.

## egui 0.37 / kittest gotchas

- `App::ui(&mut self, ui, frame)` (not `update`); `egui::Panel`, `Modal`,
  `menu_button`; use `Id::unique`/`id_salt`, `Id::new` is deprecated here.
- Right-to-left layouts take all available height; give them one row via
  `allocate_ui_with_layout` with an explicit size.
- Right-click on items inside a ComboBox popup doesn't work (the popup
  closes first); use `menu_button` with submenus.
- kittest: import `Queryable`; labels must be unique for `get_by_label`,
  otherwise use `get_all_by_label(..).next()` or role filters. Clicking a
  widget that is scrolled out of view clicks outside the modal and closes it.

## Dependencies: ship only what's needed

The owner wants as few libraries in the app as possible. Count them with
`cargo tree -e normal --target aarch64-apple-darwin --prefix none --format '{p}' | sort -u | wc -l`
(and `--target x86_64-unknown-linux-gnu`); at 1.0 it was 164 / 168.

- eframe has `default-features = false`, with only `clipboard`,
  `default_fonts`, `system_fonts`, `wayland`, `x11`. Renderer per platform: `wgpu` (Metal)
  on macOS, `glow` (OpenGL) elsewhere, set in the target sections of
  `Cargo.toml`. Because of that, `App::on_exit` has two cfg'd signatures.
- Not included on purpose: `accesskit` (screen readers; ~60 libraries on
  Linux), `links` (we open URLs ourselves in `open_links`, like folders
  via `open` / `xdg-open`), `web_screen_reader`, egui_extras' default
  file loaders.
- Fallback fonts for characters the UI font lacks (☰, emoji, CJK…) come
  from `egui_system_font_provider`, added in `fonts::install` (eframe's own
  `system_fonts` feature and `system_font_fallback` are off). Without it
  those characters render wrong.
- **Tests and previews must use the same libraries and features as the
  app**, or they show something the app doesn't. `scripts/check-test-parity.sh`
  compares the two builds (macOS; only accesskit may differ, kittest needs
  it to find widgets). Run it after changing dependencies or features;
  `release.sh` runs it too. Set things up in shared code (as
  `fonts::install`), not in the test harness.
- Before adding a dependency or feature, check what it pulls in, and say
  so. Check that Linux still compiles:
  `cargo check --target x86_64-unknown-linux-gnu`.

## Updates (macOS only)

`src/update.rs`: once a day (if `config.check_updates`) and via ☰ → Check
for Updates…, `curl` fetches GitHub's latest release (no HTTP library, on
purpose). A newer version shows an "Update to x.y.z" toolbar button; the
dialog installs only on the user's click: download the DMG, verify (codesign
intact, TeamIdentifier Z86A9UVWNP, spctl/notarized), replace the bundle
(stage, swap, keep old until done), restart via a small shell waiting for
our pid. Development/test builds (not in a .app) refuse to install. Tests
never touch the network: `demo_config` has `check_updates: false` and UI
tests inject `Updater::with_fetch`. Installing goes through a `System` trait (commands
and renames); the tests use a fake one to try each failure (download,
mount, damaged, other team, not notarized, copy, swap) and check the old
app is untouched. `cargo test real_update -- --ignored` does the real
download + install into a temp folder.

## Licenses (keep up to date)

The About dialog shows the app's license and **every third-party library's
license** (required by MIT/BSD/Apache/zlib/OFL…). After adding, removing or
updating dependencies:

1. `scripts/licenses.sh` (needs `cargo install --locked cargo-about --features cli`)
2. If it fails on a new license, check it is OK for an Apache-2.0 app before
   adding it to `accepted` in `about.toml`. Avoid GPL-only dependencies.
3. Update `THANKS` in `src/app.rs` if a major direct dependency was added.

`licenses::tests::list_is_up_to_date` compares the list with `cargo tree`
for all three release targets and fails if any shipped library is missing.

## Releasing

The owner runs `scripts/release.sh` (`minor` / `major` as argument for
bigger steps). It checks the branch and remote, runs tests and clippy,
bumps `version` in `Cargo.toml` (and `Cargo.lock`), refreshes the license
list, commits "Priogator X.Y.Z", tags `vX.Y.Z` and, after asking, pushes.
Agents don't run it unless asked.

The tag push starts GitHub Actions (`release.yml`): tests on macOS and
Linux, the universal `Priogator-X.Y.Z.dmg`, and `priogator_X.Y.Z_amd64.deb`
/ `_arm64.deb` (`cargo deb`, metadata in `Cargo.toml`, built in an Ubuntu
22.04 container so they need only glibc 2.35). A last job attaches them all
to the release with `.github/release-notes.md`, only if everything passed.

Linux package: the program, `assets/priogator.desktop` (also used by
`install-linux.sh`), the icon, README/HELP/licenses. Libraries opened at run
time (GL, EGL, Wayland, X11, fontconfig) are listed in `depends` by hand;
check with a fresh install after changing renderer or windowing features.
Locally on the Mac, build and try it in a **podman** container (Ubuntu
22.04/24.04, Xvfb for a screenshot), not Docker. Run the tests there as an
ordinary user: as root, the unreadable-folder tests fail. Install
`fonts-noto-color-emoji`, or kittest stops at the date picker's 📆.

Signing: `bundle-macos.sh` signs with the owner's "Developer ID Application:
ANDREAS CHRISTIAN ZOGLAUER (Z86A9UVWNP)" (hardened runtime, no
entitlements needed) when it's in the keychain, else ad hoc. `--release`
also signs the DMG, notarizes it (`notarytool`, keychain profile
`priogator` locally; APPLE_ID / APPLE_TEAM_ID / APPLE_APP_PASSWORD secrets on
GitHub) and staples it. The workflow imports MACOS_CERTIFICATE (+ password)
into a temporary keychain and sets REQUIRE_SIGNING=1, so an unsigned DMG is
never released. Each build is notarized anew (automatic, a few minutes).
The DMG's "Read Me First.txt" (`assets/dmg-read-me-first.txt`) and
`.github/release-notes.md` say how to install; keep them in line.

## Open ideas (not started)

- Try the .deb on a real Linux desktop (file dialog, launcher entry, Wayland;
  so far only Xvfb in a container).
