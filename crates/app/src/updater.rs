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
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub version: String,
    pub tag_name: String,
    pub name: String,
    pub html_url: String,
    pub prerelease: bool,
}

#[derive(Debug)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(AvailableUpdate),
    Error(String),
}

pub struct UpdateChecker {
    current_version: String,
    status: UpdateStatus,
    receiver: Option<mpsc::Receiver<Result<Option<AvailableUpdate>, String>>>,
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
            let send_result = sender.send(result);
            if send_result.is_err() {
                // The settings window may close before a background check finishes.
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
            Ok(Some(update)) => UpdateStatus::Available(update),
            Ok(None) => UpdateStatus::UpToDate,
            Err(e) => UpdateStatus::Error(e),
        };
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    prerelease: bool,
    draft: bool,
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
        })
}

fn strip_tag_prefix(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
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
