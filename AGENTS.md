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
| `src/fonts.rs`, `src/theme.rs` | system fonts via fontique, macOS label colours, dark mode |
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
  number (`FORMAT_VERSION` in `storage.rs`); bump it only for incompatible
  changes, and keep reading old files. New fields need `#[serde(default)]`.
- Recurring tasks: completing one creates the next occurrence with a
  deterministic id `Uuid::new_v5(&id, b"next occurrence")`, so two
  computers completing the same task don't create duplicates; reopening
  deletes that successor.
- Undo/redo and backup restore go through `Board::restore`.
- Backups: `backup-YYYY-MM-DD-HHMMSS.json`, one automatic per day, kept:
  all from the last 7 days, then weekly up to 2 months, monthly up to 2
  years, yearly after that.
- Rename migration (`Config::load_or_create`): contents of the old
  `Prioritizer` config/data folders move into the `Priogator` ones, and
  `sync_dir`/`backup_dir` paths in the config are rebased.

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

~280 tests, all run by `cargo test` in a few seconds (plus the two preview
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

## Licenses (keep up to date)

The About dialog shows the app's license and **every third-party library's
license** (required by MIT/BSD/Apache/zlib/OFL…). After adding, removing or
updating dependencies:

1. `scripts/licenses.sh` (needs `cargo install --locked cargo-about --features cli`)
2. If it fails on a new license, check it is OK for an Apache-2.0 app before
   adding it to `accepted` in `about.toml`. Avoid GPL-only dependencies.
3. Update `THANKS` in `src/app.rs` if a major direct dependency was added.

`licenses::tests::list_is_up_to_date` fails if direct dependencies changed
version without regenerating.

## Releasing

The owner runs `scripts/release.sh` (`minor` / `major` as argument for
bigger steps). It checks the branch and remote, runs tests and clippy,
bumps `version` in `Cargo.toml` (and `Cargo.lock`), refreshes the license
list, commits "Priogator X.Y.Z", tags `vX.Y.Z` and, after asking, pushes.
Agents don't run it unless asked.

The tag push starts GitHub Actions (`release.yml`, macOS runner): tests,
universal `Priogator-X.Y.Z.dmg`, attached to the release with
`.github/release-notes.md`.

The app is ad-hoc signed, not notarized: downloaders need "Open Anyway" or
`xattr -dr com.apple.quarantine`. The DMG contains "Read Me First.txt"
(from `assets/dmg-read-me-first.txt`) explaining install and first start;
keep it and `.github/release-notes.md` in line. Notarization would need an Apple
Developer ID. Bundle id: `com.andreaszoglauer.priogator`.

## Open ideas (not started)

- Test on Linux (fonts, file dialog, launcher entry).
- Protect against a damaged own sync file.
- Linux build in the release workflow.
