Open the DMG and drag **Priogator** into **Applications**. It runs on Apple Silicon and Intel Macs (macOS 11 or later).

**First start:** the app isn't notarized by Apple, so macOS blocks it the first time with "Apple could not verify Priogator…". Click **Done**, **not "Move to Trash"** (that deletes the app). Then either

- go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway** next to "Priogator was blocked", or
- run in Terminal: `xattr -dr com.apple.quarantine /Applications/Priogator.app`

The DMG contains a "Read Me First" file with these steps.
