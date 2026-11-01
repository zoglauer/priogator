Open the DMG and drag **Priogator** into **Applications**. It runs on Apple Silicon and Intel Macs (macOS 11 or later).

**First start:** the app isn't notarized by Apple, so macOS blocks it the first time. Either

- open it once, then go to **System Settings → Privacy & Security** and click **Open Anyway**, or
- run in Terminal: `xattr -dr com.apple.quarantine /Applications/Priogator.app`
