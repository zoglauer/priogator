# Prioritizer Help

Prioritizer is a to-do list organised in **areas**, one for each part of
your life: work, private ventures, house, family, or whatever you need. Each
area is shown as a column. Within an area you
decide the order of your tasks and change it whenever priorities shift.
Tasks with deadlines coming up, and tasks you pin, sit at the top so they
can't be missed.

On Linux, use **Ctrl** wherever this guide says **⌘**.

---

## The window

**Toolbar (top)**

- **+ New task** opens the New task dialog.
- **Statistics** for all areas, e.g. *16 open · 1 overdue · 3 due within 7
  days · 1 done in the last 7 days*. Hover over one for its meaning;
  *overdue* and *due within 7 days* also list the tasks they count, with
  area and day. (A task due soon can still be in the priority list if it's
  set to move to the top later.)
  - **Open:** tasks not done yet.
  - **Overdue:** open tasks whose deadline has passed.
  - **Due within 7 days:** open tasks due today or within the next 7 days.
  - **Done:** tasks completed within the last 7 days (the number of days
    follows the *Done tasks stay visible* setting).
- **⚠ 1 problem** (red) appears only when something went wrong, e.g. a sync
  file couldn't be read. Hover for details; click to dismiss.
- **Undo ↶ / redo ↷** and the **menu ☰** on the right. The menu has
  **New task…** (⌘N), **Settings…** (⌘,), **Backups…**, **Keyboard
  shortcuts** and **About Prioritizer**.

**Areas (below the toolbar)**

Each area has, from top to bottom:

1. **Title** with the number of open tasks. It's also the handle for moving
   the area.
2. **Top section** (*DUE SOON · PINNED*, on a light orange panel): tasks due soon and pinned
   tasks, by date until you drag them into an order of your own.
3. **Priorities:** all other open tasks, in the order you've set (most
   important at the top).
4. **DONE (n):** tasks completed in the last 7 days, folded away. Click to
   open.
5. **Groups row:** which group is shown (**All ▾**) and **+ New**.

If there are more areas than fit the window, scroll sideways. The area
width can be changed in Settings.

**Deadline labels** on a task:

| Label | Meaning |
| --- | --- |
| `3 d overdue` (red) | the deadline was 3 days ago |
| `today`, `tomorrow`, `in 5 d` (orange) | due within a week |
| `Nov 10` (grey) | due later; hover for the full date |
| **Bold title** | the task is due today or overdue |
| `↻` | a recurring task; hover to see how often |
| `▶ 1/3` (blue pill) | 1 of 3 subtasks done; click to show the checklist |

A task's group is shown by the coloured stripe on its left edge and by the
group's name in small coloured letters above the title. A thin green bar
along the bottom of a card shows how much of its checklist is done.

---

## Adding a task

Open the **New task** dialog in one of these ways:

- **Right-click an empty spot in an area.** That area is chosen already,
  and its group too if the area is filtered by one.
- Right-click an area's title → **New task…**.
- Click **+ New task** in the toolbar, or press **⌘N**. This uses the area
  of the selected task, or the first area.

Then:

1. Type the title. The cursor is already in the field.
2. Optionally change the **Area** or choose a **Group**. The group picker
   only appears if the area has groups.
3. Optionally set a **Deadline** (tick it and pick a date), a **Repeat**, or
   **Pin** it; below, add **Notes** or **Subtasks**.
4. Press **Enter** or click **Add task**.

New tasks go to the bottom of the priority list. A task with a deadline
within the deadline window (see below) goes to the top section instead.

---

## Re-prioritizing

**With the mouse**

- **Drag a task** up or down. A blue line shows where it will land.
- **Drag it into another area** to move it there. It loses its group,
  because groups belong to one area.
- Right-click → **Move to top** puts it first in its priority list.

**With the keyboard.** First click a task to select it; it gets an outline.
Clicking anywhere outside a task, or pressing Esc, deselects it.

| Keys | Action |
| --- | --- |
| ⌥↑ / ⌥↓ | move one place up / down |
| ⌥⌘↑ / ⌥⌘↓ | move to the top / bottom of the list |
| Enter | edit |
| ⌘⌫ | delete |
| Esc | deselect |

These keys move a task within its section: the top section or the priority
list. They do nothing while you're typing in a text field.

---

## Deadlines and the top section

The top section shows tasks whose deadline is close: within **10 days** by
default, or what you set for that task (see below),
earliest first unless you've arranged them yourself (see *Pinning* below for
the order). Overdue tasks stay there, marked red.

Tasks due further out stay in the priority list and show their date there.
They move up to the top section by themselves once the deadline is close
enough.

**For one task:** in its dialog, next to the deadline, set **on top
N days before**, e.g. 1 for taking out the trash. 0 means on the day itself.

**For all tasks:** open **☰ → Settings…** (or press ⌘,) and set **Deadlines
move to the top: N days ahead**. Click and type a number, or drag sideways.
This is the default for every task you haven't given its own value, and it's
stored per computer.

Tasks due today and overdue tasks are shown with a **bold** title and a bold
*today* or red *overdue* label. Overdue ones stay on top until you tick them
off.

**Dragging a task that's due soon down into the priority list removes its
deadline.** Otherwise it would jump straight back up. Tasks with a later
deadline keep their date when you drag them around. ⌘Z brings a removed
deadline back.

---

## Recurring tasks

For things that come back: taking out the trash every Friday, or checking the
tire pressure two months after you last did it.

In the task dialog, set **Repeat**:

- **On a schedule:** *Every [1] [week]* on the weekdays you click (Mo … Su),
  or every N days, months or years. Monthly and yearly repeats keep the day
  of the deadline; the 31st becomes the last day in shorter months.
- **After it's done:** *[2] [months] after it's done*. The next one is due
  that long after the day you tick it off.

The summary next to it shows what you set, e.g. *Every Friday* or *2 months
after done*. A repeating task always has a deadline: its next due date. Use
**on top N days before** (next to the deadline) so it only shows up on top when it's
relevant.

**Ticking it off** moves it to *DONE* as usual, and at the same moment the
next one appears in the same place with the new due date. Notes, group and
pinning carry over; subtasks start unticked. ⌘Z undoes both.

**Missed it?** The task stays on top, overdue and bold, until you tick it off.
The next one is then due on the next date that's still ahead, so missed
weeks don't pile up.

**Skip this time** (right-click) moves it on to the next date without
counting it as done. To stop repeating, set **Repeat** to *Never*, or delete
the task. Unticking a done occurrence removes the next one again.

On the card, ↻ marks a recurring task; hover over it to see how often.

---

## Pinning

Pin any task to the top section, with or without a deadline. Use pinning for
things that are urgent but have no date.

- **Pin:** drag the task onto the top section, or right-click → **Pin to
  top**, or tick **Pin: Keep on top** in the task dialog. You can drop it on
  another area's top section to move and pin it in one go.
- **Unpin:** drag it back into the priority list, or right-click → **Unpin**.
  An unpinned task goes to the bottom of the priority list, or to wherever
  you dropped it.

**Order in the top section.** Tasks arrive in date order, with pinned tasks
without a deadline after them. Drag a task up or down within the section, or
use the keyboard moves, to put it where you want; the others keep their
places. Tasks that join the section later (their deadline came close, or you
pinned them) go below the ones you've arranged, in date order. Dropping a task
from the priority list into the top section puts it exactly where you let go.

---

## Editing, completing and deleting

- **Edit:** double-click a task, or select it and press **Enter**, or
  right-click → **Edit…**. You can change the title, notes, subtasks, area,
  group, deadline and pinning. The first line of the notes shows under the title.
- **Done:** tick the checkbox. The task moves to **DONE (n)** at the bottom of
  the area. Untick it there, or right-click → **Reopen**, to bring it back.
  Done tasks stay visible for 7 days and are hidden after that. Change this
  under **Done tasks stay visible for** in Settings. Hidden tasks aren't
  deleted: they stay in your data and backups, and a longer setting shows
  them again.
- **Delete:** right-click → **Delete**, ⌘⌫, or **Delete** at the top right of
  the edit dialog.
  There's no confirmation; ⌘Z undoes it.

---

## Subtasks

A task can have a checklist of subtasks, e.g. *Prepare kick-off workshop* →
*Book a room*, *Send invitations*, *Prepare slides*.

- **Add, rename, reorder or remove:** open the task's edit dialog. Under
  **Subtasks**, type into *Add a subtask…* and press **Enter** (or click
  **Add**); edit a subtask's text in place; **↑** moves it up, **×** removes
  it. Changes take effect when you click **Save**.
- **Check off:** click the counter **▶ 1/3** on the card to unfold the
  checklist (right-click → **Show subtasks** works too), then tick items off
  right there. Click the counter again to fold it away.
- Ticking off subtasks doesn't complete the task; tick the task itself when
  you're done with it.

Subtasks are stored as part of their task. If you change the checklist of the
same task on both computers before they sync, the newer version of the whole
task wins.

---

## Groups

Groups are colour-coded sub-groups within an area, e.g. *Clients* and
*Admin* under *Work*. Each task shows its group's colour on its left edge and
the group's name above its title.

The bottom row of every area is about groups: **Groups**, a button showing
which group is visible (**All ▾**, or e.g. **● Clients ▾**), and **+ New**.

- **New group:** click **+ New**, enter a name and pick a colour.
- **Assign:** choose the group in the task dialog.
- **Show only one group:** click **All ▾** and pick the group. Pick **All** to
  see everything again.
- **Edit, move or delete:** click **All ▾** → **Edit group** → the group. In
  its dialog you can rename it, change its colour, pick another **Area** (the
  group moves there with all its tasks: open ones join the bottom of that
  area's priority list in their order, those due soon or pinned go to its top
  section, done ones move along), or **Delete** it (top right; its tasks stay,
  without a group). ⌘Z undoes any of these.
---

## Areas

- **Add:** **+ Area** after the last area.
- **New task in an area:** right-click an empty spot in it, or right-click
  its title → **New task…**.
- **Reorder:** drag an area by its title. A blue vertical line shows where
  it will go. Or right-click the title → **Move left** / **Move right**.
- **Rename, active hours:** double-click the title, or right-click →
  **Edit area…**.
- **Delete:** right-click the title → **Delete area…**. The dialog says how
  many tasks and groups go with it; those are deleted too. ⌘Z brings
  everything back.

### Folding an area by hand

Click **‹** in an area's title bar (or right-click the title → **Fold**) to
fold it into a narrow strip, whether or not it's active. It stays folded,
even after a restart, until you click the strip (or right-click it →
**Open**). Which areas you've folded is remembered per computer.

### Active hours: folding areas away

An area can be limited to the times it's relevant, e.g. *Work* on weekdays
from 8 to 5, and *Family* the other way round. Outside its active hours the
area is folded to a narrow strip showing its name sideways, the number of
open tasks and, in red, how many are overdue, so nothing urgent can hide.

- **Set it up:** **Edit area…** → tick **Only active at certain times**, click
  the weekdays, set **from** and **to** (click a time and type, e.g. `08:30`,
  or drag sideways), and choose **Active during these times** or **Active
  outside them**. For *Family*, use the same days and times as *Work* with
  *outside*: then it's open in the evenings and at weekends.
- **Automatic:** areas fold and unfold by themselves when the app starts and
  whenever their active time begins or ends.
- **Peek in:** click a folded strip to open the area. It folds again as soon
  as the Prioritizer window loses focus (you switch to another app or the
  screen locks), unless its active time has begun by then.
- **Drop a task on a strip** to move it into that area; it goes to the bottom
  of the area's priority list. Right-click a strip for the area's menu.

Active hours sync to your other computer, like the area's name.

---

## Undo and redo

**⌘Z** undoes and **⇧⌘Z** redoes, or use **↶ / ↷** next to the title. This
covers every change: tasks, groups, areas and restoring backups. The last
100 steps are kept.

The undo history is cleared when changes arrive from your other computer.
Undoing past those changes would otherwise also revert what you did over
there.

---

## Backups

Prioritizer backs everything up once a day: when it starts, and again after
midnight if it's still running. Old backups are thinned out automatically:

| Age | Kept |
| --- | --- |
| up to 7 days | every backup |
| up to 2 months | one per week |
| up to 2 years | one per month |
| older | one per year |

Backups stay on each computer and are not synced.

**Restoring.** Open **☰ → Backups…**, or **Manage backups…** in Settings.

- Each backup is listed with its date, time and number of open tasks. Click
  **Restore…** next to the one you want and confirm.
- **Delete…** next to it removes a backup after you confirm. That can't be
  undone.
- **Restore from file…** opens any backup, or a `device-….json` sync file,
  from anywhere.
- **Back up now** makes an extra backup immediately.
- **Show folder** opens the backup folder.

Before restoring, Prioritizer saves the current state as an extra backup.
⌘Z undoes a restore. The restored state also reaches your other computer at
the next sync, like any other change.

---

## Using two computers

Prioritizer can keep a Mac and a Linux laptop in sync without any online
service.

- Each computer writes its own file, `device-<name>.json`, into the **sync
  folder** and reads the other computer's file from there.
- If you edit on both, nothing is lost. For each task, area and group, the
  newest change wins, and deletions carry over.
- While the app runs, it checks for changes every 2 seconds.

You only have to keep the sync folder the same on both computers:

- **Syncthing** over your home network, which is automatic. The README has
  step-by-step setup instructions.
- **A USB stick:** copy the `device-*.json` files across in both directions.
  Each computer only writes its own file, so copying can't overwrite anything
  the other one needs.

The sync folder is:

- macOS: `~/Library/Application Support/Prioritizer/sync`
- Linux: `~/.local/share/prioritizer/sync`

---

## Settings and files

**☰ → Settings…** (⌘,) shows:

- **Deadlines move to the top:** the deadline window, 10 days by default.
- **Done tasks stay visible for:** 7 days by default.
- **Area width:** 280 pixels by default, anywhere from 200 to 600.
  **Default** resets it.
- **Backups:** how many there are, and **Manage backups…**.
- **This computer:** its device name and sync folder. Click the folder to
  open it.

All settings are stored in `config.json`:

- macOS: `~/Library/Application Support/Prioritizer/config.json`
- Linux: `~/.config/prioritizer/config.json`

| Setting | Meaning |
| --- | --- |
| `device` | this computer's name, used for its sync file |
| `sync_dir` | the sync folder |
| `backup_dir` | the backup folder |
| `deadline_horizon_days` | the deadline window (also in Settings) |
| `done_days` | how long done tasks stay visible (also in Settings) |
| `column_width` | width of each area in pixels (also in Settings) |

Quit Prioritizer before editing the file by hand, and start it again
afterwards. Give each computer its own `device` name.

---

## All keyboard shortcuts

| Keys | Action |
| --- | --- |
| ⌘N | new task |
| ⌘, | settings |
| ⌥↑ / ⌥↓ | move selected task up / down |
| ⌥⌘↑ / ⌥⌘↓ | move selected task to top / bottom |
| Enter | edit selected task |
| ⌘⌫ | delete selected task |
| Esc | deselect, close a dialog, or cancel a drag |
| ⌘Z / ⇧⌘Z | undo / redo |

---

## Questions

**A task disappeared.** Check these in order:

1. Is a group filter on? The area's **Groups** button shows a group instead
   of **All**.
2. Is the task done? Look under **DONE (n)**. Tasks done more than 7 days
   ago are hidden; raise *Done tasks stay visible for* in Settings to see them.
3. Did its deadline come close? Then it moved up to the top section.
4. Otherwise press ⌘Z, or restore yesterday's backup.

**I deleted an area and it came back.** Your other computer added a task to
it before the two had synced. The area returns so that task isn't lost.
Delete the area again once you've dealt with the task.

**A red ⚠ appears in the toolbar.** Hover over it to see the message. A sync
file couldn't be read, for example because the copy was incomplete. Your own
data is unaffected. Copy the file again, or check Syncthing.

**The two computers show different things.** Make sure the sync folder is
really shared, and that each computer's `config.json` has a different
`device` name. Also check that both computers' clocks are roughly right; the
newest change wins, so the clocks matter.
