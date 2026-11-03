//! Updates from the GitHub releases: check for a newer version and, when the
//! user agrees, download it, make sure it's genuine, install it over the
//! running app and restart. macOS only (see main.rs).
//!
//! Network access goes through the system's `curl`, so the app needs no HTTP
//! library. Before anything is replaced, the downloaded app must be signed
//! with our Developer ID and notarized by Apple.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

/// Where releases are published.
pub const RELEASES_PAGE: &str = "https://github.com/zoglauer/priogator/releases";
const LATEST_API: &str = "https://api.github.com/repos/zoglauer/priogator/releases/latest";
/// Only apps signed by this Apple Developer team are installed.
const TEAM_ID: &str = "Z86A9UVWNP";
/// How often the automatic check runs while the app is open.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// E.g. "1.0.5".
    pub version: String,
    pub notes: String,
    pub dmg_url: String,
    pub page_url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(Release),
    /// Installed into this bundle; the app restarts into it.
    Installed(PathBuf),
    Failed(String),
}

/// Fetches the latest release's description (JSON) from GitHub.
pub type Fetch = Arc<dyn Fn() -> Result<String, String> + Send + Sync>;

pub struct Updater {
    state: Arc<Mutex<State>>,
    fetch: Fetch,
    last_check: Option<Instant>,
}

impl Updater {
    pub fn new() -> Self {
        Self::with_fetch(Arc::new(fetch_latest))
    }

    /// With another source for the release description (tests).
    pub fn with_fetch(fetch: Fetch) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::Idle)),
            fetch,
            last_check: None,
        }
    }

    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or(State::Idle)
    }

    fn set(state: &Mutex<State>, new: State) {
        if let Ok(mut s) = state.lock() {
            *s = new;
        }
    }

    /// Show why an update can't go ahead.
    pub fn fail(&self, why: &str) {
        Self::set(&self.state, State::Failed(why.to_owned()));
    }

    /// Back to idle, e.g. after "Later" or closing a result.
    pub fn dismiss(&self) {
        if !matches!(self.state(), State::Installing(_) | State::Installed(_)) {
            Self::set(&self.state, State::Idle);
        }
    }

    /// Check now, in the background.
    pub fn check(&mut self, ctx: &egui::Context) {
        if matches!(
            self.state(),
            State::Checking | State::Installing(_) | State::Installed(_)
        ) {
            return;
        }
        self.last_check = Some(Instant::now());
        Self::set(&self.state, State::Checking);
        let state = self.state.clone();
        let fetch = self.fetch.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = fetch().and_then(|json| parse_release(&json));
            let new = match result {
                Ok(release) if is_newer(&release.version, env!("CARGO_PKG_VERSION")) => {
                    State::Available(release)
                }
                Ok(_) => State::UpToDate,
                Err(e) => State::Failed(e),
            };
            Self::set(&state, new);
            ctx.request_repaint();
        });
    }

    /// The automatic check: at start-up, then once a day. An update found
    /// earlier stays offered.
    pub fn check_daily(&mut self, ctx: &egui::Context) {
        let due = self.last_check.is_none_or(|t| t.elapsed() >= CHECK_EVERY);
        if due && !matches!(self.state(), State::Available(_)) {
            self.check(ctx);
        }
    }

    /// Download, verify and install `release` over `bundle`, in the background.
    pub fn install(&self, ctx: &egui::Context, release: Release, bundle: PathBuf) {
        Self::set(&self.state, State::Installing(release.clone()));
        let state = self.state.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let new = match install(&release, &bundle) {
                Ok(()) => State::Installed(bundle),
                Err(e) => State::Failed(e),
            };
            Self::set(&state, new);
            ctx.request_repaint();
        });
    }
}

/// "1.0.5" or "v1.0.5" as numbers.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(c), Some(now)) => c > now,
        _ => false,
    }
}

/// The parts of GitHub's release description we need.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "GitHub sent an unexpected answer.".to_owned())?;
    let tag = value["tag_name"]
        .as_str()
        .ok_or("GitHub's answer names no version.")?;
    let version = parse_version(tag)
        .map(|(a, b, c)| format!("{a}.{b}.{c}"))
        .ok_or_else(|| format!("Unexpected version name {tag:?}."))?;
    let dmg_url = value["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["browser_download_url"].as_str())
        .find(|url| url.ends_with(".dmg"))
        .ok_or_else(|| format!("Version {version} has no download for the Mac."))?
        .to_owned();
    Ok(Release {
        version,
        // Shown as plain text: without Markdown's bold and code marks.
        notes: value["body"]
            .as_str()
            .unwrap_or_default()
            .replace("**", "")
            .replace('`', "")
            .trim()
            .to_owned(),
        dmg_url,
        page_url: value["html_url"]
            .as_str()
            .unwrap_or(RELEASES_PAGE)
            .to_owned(),
    })
}

fn fetch_latest() -> Result<String, String> {
    let output = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--max-time", "20"])
        .args(["--header", "Accept: application/vnd.github+json"])
        .arg(LATEST_API)
        .output()
        .map_err(|e| format!("Couldn't run curl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Couldn't reach GitHub ({}).",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The `.app` bundle the running program belongs to, e.g.
/// `/Applications/Priogator.app` for `…/Priogator.app/Contents/MacOS/Priogator`.
/// `None` when not running from a bundle (e.g. `cargo run`).
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let bundle = exe.parent()?.parent()?.parent()?;
    let is_bundle =
        exe.parent()?.ends_with("Contents/MacOS") && bundle.extension().is_some_and(|e| e == "app");
    is_bundle.then(|| bundle.to_path_buf())
}

/// Run a command; its output, or why it failed.
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("Couldn't run {program}: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if output.status.success() {
        Ok(text)
    } else {
        Err(format!("{program} failed: {}", text.trim()))
    }
}

/// Signed by us, intact, and notarized by Apple.
fn verify(app: &Path) -> Result<(), String> {
    let app = app.as_os_str();
    run(
        "codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            app,
        ],
    )
    .map_err(|_| "The download isn't intact; nothing was changed.".to_owned())?;
    let details = run("codesign", &["-dv".as_ref(), app])?;
    if !details.contains(&format!("TeamIdentifier={TEAM_ID}")) {
        return Err(
            "The download isn't signed by Priogator's developer; nothing was changed.".into(),
        );
    }
    run(
        "spctl",
        &["-a".as_ref(), "-t".as_ref(), "exec".as_ref(), app],
    )
    .map_err(|_| "The download isn't notarized by Apple; nothing was changed.".to_owned())?;
    Ok(())
}

fn install(release: &Release, bundle: &Path) -> Result<(), String> {
    let work = std::env::temp_dir().join(format!("priogator-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let dmg = work.join("update.dmg");
    let mount = work.join("mount");
    let result = (|| {
        run(
            "curl",
            &[
                "--fail".as_ref(),
                "--silent".as_ref(),
                "--show-error".as_ref(),
                "--location".as_ref(),
                "--max-time".as_ref(),
                "900".as_ref(),
                "--output".as_ref(),
                dmg.as_os_str(),
                release.dmg_url.as_ref(),
            ],
        )
        .map_err(|e| format!("The download failed. {e}"))?;
        std::fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
        run(
            "hdiutil",
            &[
                "attach".as_ref(),
                "-nobrowse".as_ref(),
                "-noautoopen".as_ref(),
                "-readonly".as_ref(),
                "-mountpoint".as_ref(),
                mount.as_os_str(),
                dmg.as_os_str(),
            ],
        )?;
        let replaced = replace(&mount.join("Priogator.app"), bundle);
        let _ = run(
            "hdiutil",
            &["detach".as_ref(), "-quiet".as_ref(), mount.as_os_str()],
        );
        replaced
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Put the verified `new` app in place of `bundle`, keeping the old one until
/// the new one is there.
fn replace(new: &Path, bundle: &Path) -> Result<(), String> {
    verify(new)?;
    let parent = bundle.parent().ok_or("The app has no folder.")?;
    let staged = parent.join(".Priogator-update.app");
    let old = parent.join(".Priogator-old.app");
    let _ = std::fs::remove_dir_all(&staged);
    let _ = std::fs::remove_dir_all(&old);
    let cant_write = |e: std::io::Error| {
        format!(
            "Couldn't write to {} ({e}). Download the update from the releases page instead.",
            parent.display()
        )
    };
    run("ditto", &[new.as_os_str(), staged.as_os_str()])
        .map_err(|_| cant_write(std::io::Error::other("copy failed")))?;
    std::fs::rename(bundle, &old).map_err(cant_write)?;
    if let Err(e) = std::fs::rename(&staged, bundle) {
        let _ = std::fs::rename(&old, bundle);
        return Err(cant_write(e));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Open `bundle` again once this process has quit.
pub fn relaunch_after_exit(bundle: &Path) {
    let script = "while kill -0 \"$1\" 2>/dev/null; do sleep 0.2; done; open \"$2\"";
    let _ = Command::new("/bin/sh")
        .args(["-c", script, "sh"])
        .arg(std::process::id().to_string())
        .arg(bundle)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATEST: &str = r#"{
        "tag_name": "v1.2.3",
        "html_url": "https://github.com/zoglauer/priogator/releases/tag/v1.2.3",
        "body": "  Faster sync.\n",
        "assets": [
            {"name": "notes.txt", "browser_download_url": "https://example.com/notes.txt"},
            {"name": "Priogator-1.2.3.dmg", "browser_download_url": "https://example.com/Priogator-1.2.3.dmg"}
        ]
    }"#;

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.10.0"), Some((1, 10, 0)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("v1.2.x"), None);
        assert!(is_newer("1.10.0", "1.9.9"), "not compared as text");
        assert!(is_newer("v2.0.0", "1.99.99"));
        assert!(!is_newer("1.0.4", "1.0.4"));
        assert!(!is_newer("1.0.3", "1.0.4"));
        assert!(!is_newer("nonsense", "1.0.4"));
    }

    #[test]
    fn the_latest_release_is_read_from_githubs_answer() {
        let release = parse_release(LATEST).unwrap();
        assert_eq!(release.version, "1.2.3");
        assert_eq!(release.dmg_url, "https://example.com/Priogator-1.2.3.dmg");
        assert_eq!(release.notes, "Faster sync.");
        let bold = LATEST.replace("Faster sync.", "**Faster** `sync`.");
        assert_eq!(parse_release(&bold).unwrap().notes, "Faster sync.");
        assert!(release.page_url.ends_with("/v1.2.3"));
    }

    #[test]
    fn unusable_answers_are_explained() {
        assert!(parse_release("<html>rate limited</html>").is_err());
        assert!(parse_release(r#"{"message": "Not Found"}"#).is_err());
        let no_dmg = r#"{"tag_name": "v1.2.3", "assets": []}"#;
        assert!(parse_release(no_dmg).unwrap_err().contains("no download"));
    }

    #[test]
    fn the_bundle_is_found_from_the_program() {
        assert_eq!(
            bundle_of(Path::new(
                "/Applications/Priogator.app/Contents/MacOS/Priogator"
            )),
            Some(PathBuf::from("/Applications/Priogator.app"))
        );
        assert_eq!(
            bundle_of(Path::new("/Users/a/target/release/priogator")),
            None
        );
    }

    fn updater_answering(json: &'static str) -> Updater {
        Updater::with_fetch(Arc::new(move || Ok(json.to_owned())))
    }

    fn wait_while_checking(updater: &Updater) -> State {
        for _ in 0..500 {
            let state = updater.state();
            if state != State::Checking {
                return state;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("still checking");
    }

    #[test]
    fn a_newer_release_is_offered_and_the_same_one_is_not() {
        let ctx = egui::Context::default();
        let mut updater = updater_answering(LATEST.leak_version("v99.0.0"));
        updater.check(&ctx);
        assert!(
            matches!(wait_while_checking(&updater), State::Available(r) if r.version == "99.0.0")
        );

        let current = format!("v{}", env!("CARGO_PKG_VERSION"));
        let mut updater = updater_answering(LATEST.leak_version(&current));
        updater.check(&ctx);
        assert_eq!(wait_while_checking(&updater), State::UpToDate);
    }

    #[test]
    fn a_failed_check_says_why_and_can_be_dismissed() {
        let ctx = egui::Context::default();
        let mut updater = Updater::with_fetch(Arc::new(|| Err("offline".into())));
        updater.check(&ctx);
        assert_eq!(
            wait_while_checking(&updater),
            State::Failed("offline".into())
        );
        updater.dismiss();
        assert_eq!(updater.state(), State::Idle);
    }

    #[test]
    fn the_daily_check_runs_once_a_day() {
        let ctx = egui::Context::default();
        let mut updater = updater_answering(LATEST.leak_version("v0.0.1"));
        updater.check_daily(&ctx);
        assert_eq!(wait_while_checking(&updater), State::UpToDate);
        updater.dismiss();
        updater.check_daily(&ctx);
        assert_eq!(updater.state(), State::Idle, "not again the same day");
        updater.last_check = Some(Instant::now() - CHECK_EVERY);
        updater.check_daily(&ctx);
        assert_eq!(wait_while_checking(&updater), State::UpToDate);
    }

    /// The whole way with the real latest release from GitHub: download,
    /// verify, and install over a stand-in app in a temporary folder.
    /// Network and Apple's servers needed, so only on request:
    /// `cargo test real_update -- --ignored`.
    #[test]
    #[ignore]
    fn real_update_installs_the_latest_release() {
        let release = parse_release(&fetch_latest().unwrap()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Priogator.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/Priogator"), "old").unwrap();
        install(&release, &bundle).unwrap();
        verify(&bundle).unwrap();
        let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
        assert!(plist.contains(&release.version));
        let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "only the new app is left");
    }

    #[test]
    fn only_our_signed_and_notarized_app_passes() {
        // A folder that isn't a signed app at all.
        let dir = tempfile::tempdir().unwrap();
        assert!(verify(dir.path()).is_err());
    }

    /// The sample answer with another version, for the duration of the tests.
    trait WithVersion {
        fn leak_version(&self, tag: &str) -> &'static str;
    }

    impl WithVersion for str {
        fn leak_version(&self, tag: &str) -> &'static str {
            self.replace("v1.2.3", tag).leak()
        }
    }
}
