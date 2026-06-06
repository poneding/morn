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

        std::thread::spawn(move || {
            let result = check_for_update(&current_version, include_prereleases);
            let _ = sender.send(result);
        });
    }

    pub fn poll(&mut self) {
        let Some(receiver) = &self.receiver else {
            return;
        };
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
    let runtime_api = std::env::var("MORN_RELEASES_API_URL").ok();
    let runtime_repo = std::env::var("GITHUB_REPOSITORY").ok();
    configured_release_source_url(
        runtime_api.as_deref(),
        option_env!("MORN_RELEASES_API_URL"),
        runtime_repo.as_deref(),
        option_env!("GITHUB_REPOSITORY"),
        option_env!("CARGO_PKG_REPOSITORY"),
    )
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

fn configured_release_source_url(
    runtime_api: Option<&str>,
    compile_api: Option<&str>,
    runtime_repo: Option<&str>,
    compile_repo: Option<&str>,
    package_repo: Option<&str>,
) -> Option<String> {
    [runtime_api, compile_api]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|source| !source.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            [runtime_repo, compile_repo, package_repo]
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
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .build();
    agent
        .get(source)
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

        let update = select_latest_update("0.1.0", false, &releases).unwrap();

        assert_eq!(update.version, "0.1.1");
        assert!(!update.prerelease);
    }

    #[test]
    fn beta_channel_allows_prereleases() {
        let releases = vec![release("v0.2.0-beta.1", true), release("v0.1.1", false)];

        let update = select_latest_update("0.1.0", true, &releases).unwrap();

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
            configured_release_source_url(
                None,
                None,
                None,
                None,
                Some("https://github.com/owner/repo")
            )
            .as_deref(),
            Some("https://api.github.com/repos/owner/repo/releases")
        );
    }

    #[test]
    fn release_source_only_uses_git_remote_in_debug_builds() {
        let source = include_str!("updater.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("#[cfg(debug_assertions)]"));
        assert!(source.contains("read_git_remote_origin_url()"));
        assert!(source.contains("#[cfg(not(debug_assertions))]"));
    }
}
