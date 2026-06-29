//! GitHub release update checking.
//!
//! The checker keeps network I/O outside the egui frame by moving fetch work onto a
//! short-lived thread and polling a channel from the settings window.  The UI only
//! sees `UpdateStatus`, while this module owns source discovery, release parsing,
//! semantic-version comparison, and prerelease filtering.
//!
//! Source resolution is intentionally layered: explicit runtime/configured API URL
//! wins, repository metadata is converted to the GitHub releases endpoint, and debug
//! builds may fall back to the local `origin` remote.  Release builds avoid shelling
//! out so packaged apps are not coupled to a git checkout.
//!
//! Older worker threads may finish after a newer check starts; the active receiver
//! is the authority, so stale results are allowed to disappear with their channel.

use rust_i18n::t;
use semver::Version;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub version: String,
    pub tag_name: String,
    pub name: String,
    pub html_url: String,
    pub prerelease: bool,
    pub installer: Option<UpdateAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedInstaller {
    pub update: AvailableUpdate,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(AvailableUpdate),
    DownloadingInstaller(AvailableUpdate),
    InstallerOpened(OpenedInstaller),
    InstallError {
        update: AvailableUpdate,
        message: String,
    },
    Error(String),
}

pub struct UpdateChecker {
    current_version: String,
    status: UpdateStatus,
    receiver: Option<mpsc::Receiver<UpdateJobResult>>,
}

impl UpdateChecker {
    pub fn new(current_version: impl Into<String>) -> Self {
        Self {
            current_version: current_version.into(),
            status: UpdateStatus::Idle,
            receiver: None,
        }
    }

    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    pub fn is_checking(&self) -> bool {
        matches!(self.status, UpdateStatus::Checking)
    }

    pub fn is_busy(&self) -> bool {
        matches!(
            self.status,
            UpdateStatus::Checking | UpdateStatus::DownloadingInstaller(_)
        )
    }

    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    pub fn begin(&mut self, include_prereleases: bool) {
        self.status = UpdateStatus::Checking;
        let current_version = self.current_version.clone();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        // A new check replaces the receiver immediately.  If an older worker
        // finishes later, its send will fail harmlessly because nothing is polling
        // that channel anymore.
        std::thread::spawn(move || {
            let result = check_for_update(&current_version, include_prereleases);
            let send_result = sender.send(UpdateJobResult::Check(result));
            if send_result.is_err() {
                // The settings window may close before a background check finishes.
            }
        });
    }

    pub fn begin_install(&mut self, update: AvailableUpdate) {
        if self.is_busy() {
            return;
        }

        self.status = UpdateStatus::DownloadingInstaller(update.clone());
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);

        std::thread::spawn(move || {
            let result = download_and_open_installer(update);
            let send_result = sender.send(UpdateJobResult::Install(result));
            if send_result.is_err() {
                // The settings window may close before a background install task finishes.
            }
        });
    }

    pub fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
        // Polling is non-blocking because settings can call this from the egui
        // frame loop without stalling redraw or input.
        let Ok(result) = receiver.try_recv() else {
            return;
        };
        self.receiver = None;
        self.status = match result {
            UpdateJobResult::Check(Ok(Some(update))) => UpdateStatus::Available(update),
            UpdateJobResult::Check(Ok(None)) => UpdateStatus::UpToDate,
            UpdateJobResult::Check(Err(e)) => UpdateStatus::Error(e),
            UpdateJobResult::Install(Ok(opened)) => UpdateStatus::InstallerOpened(opened),
            UpdateJobResult::Install(Err(err)) => UpdateStatus::InstallError {
                update: *err.update,
                message: err.message,
            },
        };
    }
}

enum UpdateJobResult {
    Check(Result<Option<AvailableUpdate>, String>),
    Install(Result<OpenedInstaller, UpdateInstallError>),
}

struct UpdateInstallError {
    update: Box<AvailableUpdate>,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    prerelease: bool,
    draft: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Clone, Copy)]
struct ReleaseSourceConfig<'a> {
    runtime_api: Option<&'a str>,
    compile_api: Option<&'a str>,
    runtime_repo: Option<&'a str>,
    compile_repo: Option<&'a str>,
    package_repo: Option<&'a str>,
}

fn check_for_update(
    current_version: &str,
    include_prereleases: bool,
) -> Result<Option<AvailableUpdate>, String> {
    let source = release_source_url().ok_or_else(|| t!("update_source_missing").to_string())?;
    let releases = fetch_releases(&source)?;
    Ok(select_latest_update(
        current_version,
        include_prereleases,
        &releases,
    ))
}

fn release_source_url() -> Option<String> {
    // Runtime values allow packaged builds and CI to override metadata without
    // recompiling.  Compile-time env and Cargo repository fields are fallbacks.
    let runtime_api = std::env::var("MORN_RELEASES_API_URL").ok();
    let runtime_repo = std::env::var("GITHUB_REPOSITORY").ok();
    configured_release_source_url(ReleaseSourceConfig {
        runtime_api: runtime_api.as_deref(),
        compile_api: option_env!("MORN_RELEASES_API_URL"),
        runtime_repo: runtime_repo.as_deref(),
        compile_repo: option_env!("GITHUB_REPOSITORY"),
        package_repo: option_env!("CARGO_PKG_REPOSITORY"),
    })
    .or_else(git_remote_release_source_url)
}

fn git_remote_release_source_url() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        read_git_remote_origin_url().and_then(|repo| github_releases_url_from_repository(&repo))
    }

    #[cfg(not(debug_assertions))]
    {
        None
    }
}

fn configured_release_source_url(config: ReleaseSourceConfig<'_>) -> Option<String> {
    // Direct API URLs win over repository names because they may point at mirrors,
    // enterprise GitHub, or test fixtures.
    [config.runtime_api, config.compile_api]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|source| !source.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            [
                config.runtime_repo,
                config.compile_repo,
                config.package_repo,
            ]
            .into_iter()
            .flatten()
            .find_map(github_releases_url_from_repository)
        })
}

fn github_releases_url_from_repository(repository: &str) -> Option<String> {
    let repository = repository.trim();
    if repository.is_empty() {
        return None;
    }

    // Accept the common GitHub URL spellings plus owner/repo.  Anything with extra
    // path segments is rejected so the generated endpoint is predictable.
    let path = repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("http://github.com/"))
        .or_else(|| repository.strip_prefix("ssh://git@github.com/"))
        .or_else(|| repository.strip_prefix("git@github.com:"))
        .unwrap_or(repository);
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    if parts.next().is_some() || owner.contains(':') || repo.contains(':') {
        return None;
    }

    Some(format!(
        "https://api.github.com/repos/{owner}/{repo}/releases"
    ))
}

#[cfg(debug_assertions)]
fn read_git_remote_origin_url() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|remote| !remote.is_empty())
}

fn fetch_releases(source: &str) -> Result<Vec<GithubRelease>, String> {
    // Keep the timeout short: update checks are advisory and should not leave a
    // background worker hanging for the lifetime of the app.
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .build();
    let request = agent.get(source);
    request
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "Morn update checker")
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())
}

fn select_latest_update(
    current_version: &str,
    include_prereleases: bool,
    releases: &[GithubRelease],
) -> Option<AvailableUpdate> {
    // Tags are compared as semantic versions after dropping a leading `v`, which
    // lets GitHub release names stay conventional without leaking into UI labels.
    let current = Version::parse(strip_tag_prefix(current_version)).ok()?;
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter(|release| include_prereleases || !release.prerelease)
        .filter_map(|release| {
            let version = Version::parse(strip_tag_prefix(&release.tag_name)).ok()?;
            (version > current).then_some((version, release))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(version, release)| AvailableUpdate {
            version: version.to_string(),
            tag_name: release.tag_name.clone(),
            name: release
                .name
                .clone()
                .unwrap_or_else(|| release.tag_name.clone()),
            html_url: release.html_url.clone(),
            prerelease: release.prerelease,
            installer: release_asset_for_current_platform(&release.assets),
        })
}

fn strip_tag_prefix(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

fn release_asset_for_current_platform(assets: &[GithubAsset]) -> Option<UpdateAsset> {
    release_asset_for_target(
        assets,
        std::env::consts::OS,
        std::env::consts::ARCH,
        linux_package_preference(),
    )
}

fn release_asset_for_target(
    assets: &[GithubAsset],
    os: &str,
    arch: &str,
    linux_preference: LinuxPackagePreference,
) -> Option<UpdateAsset> {
    let suffix = target_asset_suffix(os, arch)?;
    let extensions = preferred_asset_extensions(os, linux_preference);
    extensions
        .into_iter()
        .find_map(|extension| release_asset_with_extension(assets, suffix, extension))
}

fn release_asset_with_extension(
    assets: &[GithubAsset],
    suffix: &str,
    extension: &str,
) -> Option<UpdateAsset> {
    let extension = extension.to_ascii_lowercase();
    assets.iter().find_map(|asset| {
        let name = asset.name.to_ascii_lowercase();
        (name.contains(suffix)
            && name.ends_with(&extension)
            && !asset.browser_download_url.trim().is_empty())
        .then(|| UpdateAsset {
            name: asset.name.clone(),
            download_url: asset.browser_download_url.clone(),
        })
    })
}

fn target_asset_suffix(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "x86_64") => Some("macos-x86_64"),
        ("macos", "aarch64") => Some("macos-arm64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("windows", "aarch64") => Some("windows-arm64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxPackagePreference {
    Deb,
    Rpm,
    AppImage,
}

fn preferred_asset_extensions(
    os: &str,
    linux_preference: LinuxPackagePreference,
) -> Vec<&'static str> {
    match os {
        "macos" => vec![".dmg"],
        // The NSIS installer is easier to launch from the app. MSI remains a
        // fallback for releases that omit the executable installer.
        "windows" => vec![".exe", ".msi"],
        "linux" => match linux_preference {
            LinuxPackagePreference::Deb => vec![".deb", ".appimage", ".rpm"],
            LinuxPackagePreference::Rpm => vec![".rpm", ".appimage", ".deb"],
            LinuxPackagePreference::AppImage => vec![".appimage", ".deb", ".rpm"],
        },
        _ => Vec::new(),
    }
}

fn linux_package_preference() -> LinuxPackagePreference {
    if std::env::consts::OS != "linux" {
        return LinuxPackagePreference::AppImage;
    }
    if command_exists("dpkg") {
        LinuxPackagePreference::Deb
    } else if command_exists("rpm") {
        LinuxPackagePreference::Rpm
    } else {
        LinuxPackagePreference::AppImage
    }
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(command).is_file()))
        .unwrap_or(false)
}

fn download_and_open_installer(
    update: AvailableUpdate,
) -> Result<OpenedInstaller, UpdateInstallError> {
    let result = download_and_open_installer_inner(&update);
    result.map_err(|message| UpdateInstallError {
        update: Box::new(update),
        message,
    })
}

fn download_and_open_installer_inner(update: &AvailableUpdate) -> Result<OpenedInstaller, String> {
    let installer = update
        .installer
        .as_ref()
        .ok_or_else(|| t!("update_installer_missing").to_string())?;
    let path = download_update_asset(update, installer)?;
    open_installer_file(&path)?;
    Ok(OpenedInstaller {
        update: update.clone(),
        path,
    })
}

fn download_update_asset(update: &AvailableUpdate, asset: &UpdateAsset) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir()
        .join("morn-updates")
        .join(sanitize_file_name(&update.tag_name));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = available_download_path(&dir, &sanitize_file_name(&asset.name));
    let partial = path.with_file_name(format!(
        "{}.part",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("morn-update")
    ));

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(120))
        .build();
    let mut reader = agent
        .get(&asset.download_url)
        .set("User-Agent", "Morn update downloader")
        .call()
        .map_err(|e| e.to_string())?
        .into_reader();
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let sanitized = sanitized.trim_matches(['.', ' ']);
    if sanitized.is_empty() {
        "morn-update".to_string()
    } else {
        sanitized.to_string()
    }
}

fn available_download_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("morn-update");
    let extension = path.extension().and_then(|extension| extension.to_str());
    for index in 1..1000 {
        let name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-latest"))
}

#[cfg(target_os = "windows")]
fn open_installer_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn open_installer_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn open_installer_file(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("appimage"))
    {
        make_executable(path)?;
        return std::process::Command::new(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string());
    }
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn make_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(path, permissions).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn require_update(update: Option<AvailableUpdate>) -> AvailableUpdate {
        match update {
            Some(update) => update,
            None => panic!("expected an available update"),
        }
    }

    fn source_before_tests() -> &'static str {
        include_str!("updater.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
    }

    fn release(tag_name: &str, prerelease: bool) -> GithubRelease {
        GithubRelease {
            tag_name: tag_name.to_string(),
            name: Some(tag_name.to_string()),
            html_url: format!("https://example.test/releases/{tag_name}"),
            prerelease,
            draft: false,
            assets: Vec::new(),
        }
    }

    fn asset(name: &str) -> GithubAsset {
        GithubAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.test/downloads/{name}"),
        }
    }

    #[test]
    fn stable_channel_ignores_prereleases() {
        let releases = vec![release("v0.2.0-beta.1", true), release("v0.1.1", false)];

        let update = require_update(select_latest_update("0.1.0", false, &releases));

        assert_eq!(update.version, "0.1.1");
        assert!(!update.prerelease);
    }

    #[test]
    fn beta_channel_allows_prereleases() {
        let releases = vec![release("v0.2.0-beta.1", true), release("v0.1.1", false)];

        let update = require_update(select_latest_update("0.1.0", true, &releases));

        assert_eq!(update.version, "0.2.0-beta.1");
        assert!(update.prerelease);
    }

    #[test]
    fn current_or_older_releases_are_not_updates() {
        let releases = vec![release("v0.1.0", false), release("v0.0.9", false)];

        assert!(select_latest_update("0.1.0", true, &releases).is_none());
    }

    #[test]
    fn release_assets_match_current_platform_suffixes() {
        assert_eq!(target_asset_suffix("macos", "aarch64"), Some("macos-arm64"));
        assert_eq!(
            target_asset_suffix("windows", "x86_64"),
            Some("windows-x86_64")
        );
        assert_eq!(target_asset_suffix("linux", "aarch64"), Some("linux-arm64"));
        assert_eq!(target_asset_suffix("linux", "arm"), None);
    }

    #[test]
    fn windows_asset_selection_prefers_exe_installer() {
        let assets = vec![
            asset("morn-v0.2.0-windows-x86_64.msi"),
            asset("morn-v0.2.0-windows-x86_64.exe"),
        ];

        let selected = release_asset_for_target(
            &assets,
            "windows",
            "x86_64",
            LinuxPackagePreference::AppImage,
        )
        .expect("expected windows asset");

        assert_eq!(selected.name, "morn-v0.2.0-windows-x86_64.exe");
    }

    #[test]
    fn linux_asset_selection_follows_package_preference() {
        let assets = vec![
            asset("morn-v0.2.0-linux-x86_64.AppImage"),
            asset("morn-v0.2.0-linux-x86_64.deb"),
            asset("morn-v0.2.0-linux-x86_64.rpm"),
        ];

        let deb = release_asset_for_target(&assets, "linux", "x86_64", LinuxPackagePreference::Deb)
            .expect("expected deb asset");
        let rpm = release_asset_for_target(&assets, "linux", "x86_64", LinuxPackagePreference::Rpm)
            .expect("expected rpm asset");

        assert_eq!(deb.name, "morn-v0.2.0-linux-x86_64.deb");
        assert_eq!(rpm.name, "morn-v0.2.0-linux-x86_64.rpm");
    }

    #[test]
    fn selected_update_includes_matching_installer_asset() {
        let mut newer = release("v0.2.0", false);
        newer.assets = vec![
            asset("morn-v0.2.0-macos-arm64.dmg"),
            asset("morn-v0.2.0-windows-x86_64.exe"),
        ];

        let update = require_update(select_latest_update("0.1.0", false, &[newer]));

        if let Some(suffix) = target_asset_suffix(std::env::consts::OS, std::env::consts::ARCH) {
            if suffix == "macos-arm64" || suffix == "windows-x86_64" {
                assert!(update.installer.is_some());
            }
        }
    }

    #[test]
    fn downloaded_asset_names_are_sanitized() {
        assert_eq!(
            sanitize_file_name("../morn:v0.2.0?.dmg"),
            "_morn_v0.2.0_.dmg"
        );
        assert_eq!(sanitize_file_name("..."), "morn-update");
    }

    #[test]
    fn github_repository_values_become_releases_api_urls() {
        assert_eq!(
            github_releases_url_from_repository("owner/repo").as_deref(),
            Some("https://api.github.com/repos/owner/repo/releases")
        );
        assert_eq!(
            github_releases_url_from_repository("https://github.com/owner/repo.git").as_deref(),
            Some("https://api.github.com/repos/owner/repo/releases")
        );
        assert_eq!(
            github_releases_url_from_repository("git@github.com:owner/repo.git").as_deref(),
            Some("https://api.github.com/repos/owner/repo/releases")
        );
    }

    #[test]
    fn configured_release_source_uses_package_repository_as_fallback() {
        assert_eq!(
            configured_release_source_url(ReleaseSourceConfig {
                runtime_api: None,
                compile_api: None,
                runtime_repo: None,
                compile_repo: None,
                package_repo: Some("https://github.com/owner/repo"),
            })
            .as_deref(),
            Some("https://api.github.com/repos/owner/repo/releases")
        );
    }

    #[test]
    fn compiled_package_repository_is_present_for_release_fallback() {
        assert_eq!(
            option_env!("CARGO_PKG_REPOSITORY"),
            Some("https://github.com/poneding/morn")
        );
    }

    #[test]
    fn release_source_only_uses_git_remote_in_debug_builds() {
        let source = source_before_tests();

        assert!(source.contains("#[cfg(debug_assertions)]"));
        assert!(source.contains("read_git_remote_origin_url()"));
        assert!(source.contains("#[cfg(not(debug_assertions))]"));
    }
}
