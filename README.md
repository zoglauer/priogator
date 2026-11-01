Priogator

Key design features

Priogator is a special to-do list
It has a multi-column format for e.g. work, private-enterpices, house, family etc.

Key differentiator is that tasks can easily re-priorized
Tasks with fixed dead-lines are pinned on top of its list and are seperated from the non-deadline priority listsin the bottom section

Tasks in the columns have color codes for further sub-group ordering - there is a selector for groups at the bottom of each column

On the left is a column with statistics and the option to create new tasks


## Installing

macOS:

    scripts/bundle-macos.sh --install   # builds Priogator.app into ~/Applications

Linux (first install Rust and the system libraries listed below):

    scripts/install-linux.sh            # ~/.local/bin/priogator + app-launcher entry

Libraries egui needs on Linux (Ubuntu/Debian names):

    sudo apt install build-essential libxkbcommon-dev libgl1-mesa-dev \
        libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev

## Using it

The full user guide is in [HELP.md](HELP.md). In short:

- Drag tasks to reorder them or move them to another area.
- Right-click an empty spot in an area to add a task there.
- The top of each area holds tasks due within the next 10 days (change it
  in ☰ → *Settings…*) and pinned tasks. Tasks due later
  stay in the priority list with their date and move up by themselves once
  they're due soon.
- Done tasks show under *DONE* for 7 days, then they're hidden (still kept in
  the data). The number of days is in Settings too.
- Pin any task to the top by dragging it there or with right-click → *Pin to
  top*; drag it back down or choose *Unpin* to undo. Dragging a task that's due
  soon down into the priority list removes its deadline.
- Double-click a task (or select it and press Enter) to edit title, notes,
  subtasks, area, group and deadline. Right-click for more.
- Recurring tasks ("every Friday", "2 months after done"): set *Repeat* in the
  task dialog. Each task can also move to the top its own number of days
  before its deadline. Overdue tasks are shown in bold.
- Tasks can have subtasks: click the `▶ 1/3` counter on a card to unfold the
  checklist and tick items off.
- The bottom row of an area is about groups: *All ▾* shows only one group or
  all of them, and holds *Edit group*; *+ New* adds a group.
- Areas can have active hours (e.g. Work Mo–Fr 08:00–17:00, Family outside
  them). Outside that time they fold to a narrow strip; click it to peek in.
- Drag an area by its title to reorder the areas. Double-click the title
  to edit it (name, active hours); right-click for that, move left/right and delete (with a
  confirmation; the area's tasks and groups are deleted too, ⌘Z undoes it).

| Keys | Action |
| --- | --- |
| ⌥↑ / ⌥↓ | Move selected task up / down |
| ⌥⌘↑ / ⌥⌘↓ | Move selected task to top / bottom |
| Enter | Edit selected task |
| ⌘⌫ | Delete selected task |
| ⌘Z / ⇧⌘Z | Undo / redo |
| ⌘N | New task |
| Esc | Deselect |

On Linux use Ctrl instead of ⌘.

## Development

Written in Rust with egui; runs on macOS and Linux.

    cargo run            # start the app
    cargo run -- --demo  # add example tasks first (only if there are none yet)
    cargo test           # run the tests
    FONT_PREVIEW=/tmp/fonts.png cargo test font_preview -- --ignored
                         # render sample text, to compare with native text
    APP_PREVIEW=/tmp/app cargo test app_preview -- --ignored
                         # screenshots of the window with example data (light + dark)
    cargo build --release
    scripts/licenses.sh  # after changing dependencies: update the list of
                         # library licenses shown under About → Acknowledgements
                         # (needs: cargo install --locked cargo-about --features cli)

Code layout:

- `src/model.rs`   – data types (areas, groups, tasks) with version info for syncing
- `src/board.rs`   – all tasks in memory: ordering, changes, statistics, merging
- `src/storage.rs` – settings and the files in the sync folder
- `src/app.rs`     – the user interface
- `src/fonts.rs`, `src/theme.rs` – system fonts and macOS text colours
- `src/backup.rs`  – daily backups and which of them to keep
- `src/demo.rs`    – example data
- `src/licenses.rs` – the licenses shown in the About dialog
- `src/macos_menu.rs` – makes the macOS menu's About open the app's own dialog
- `scripts/`        – packaging, and `make_icon.py` which draws `assets/icon.png`

## Data and syncing between computers

Settings live in `config.json` (macOS: `~/Library/Application Support/Priogator/`,
Linux: `~/.config/priogator/`). It holds this computer's device name and the
sync folder.

The sync folder (default: `sync/` inside the data folder) holds one file per
computer, `device-<name>.json`. Each computer writes only its own file and
merges the others: the newest change to each task wins, deletions are kept
as markers so they reach the other computer. To sync, keep this folder
identical on both laptops with Syncthing (local network, no cloud) or a USB
stick, or point `sync_dir` in `config.json` at such a folder. Changes from the
other computer appear within a few seconds while the app is running.

### Backups

Every day the app runs it saves a backup of everything to `backups/` in the
data folder (macOS: `~/Library/Application Support/Priogator/backups`, Linux:
`~/.local/share/priogator/backups`). These stay on each computer and are not
synced. All backups of the last 7 days are kept, then one per week for 2
months, one per month for 2 years, and one per year after that.

☰ → *Backups…* lists them (with date and time), deletes one, or restores one (or any backup or
`device-*.json` file via *Restore from file…*). Before restoring, the current
state is saved as an extra backup; ⌘Z undoes a restore, and the restored state
syncs to the other computer like any other change.

### Setting up Syncthing (local network only)

1. Install on both laptops: `brew install syncthing && brew services start syncthing`
   on macOS, `sudo apt install syncthing && systemctl --user enable --now syncthing`
   on Linux.
2. Open http://127.0.0.1:8384 on each. Under *Actions → Settings → Connections*
   untick *Global Discovery*, *Enable Relaying* and *Enable NAT traversal*, so
   Syncthing only talks to computers on your own network.
3. On the Mac: *Add Remote Device* and enter the Linux laptop's device ID
   (*Actions → Show ID* there). Accept it on the Linux side.
4. On the Mac: *Add Folder* with path
   `~/Library/Application Support/Priogator/sync`, share it with the Linux
   laptop. On Linux accept it with path `~/.local/share/priogator/sync`.

Without a network, copy the `device-*.json` files both ways with a USB stick;
because each computer only writes its own file, copying can't overwrite
anything the other one needs.

