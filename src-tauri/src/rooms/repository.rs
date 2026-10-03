use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct GitHubRepository {
    pub remote: String,
    pub repository: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct RepositoryInspection {
    pub repo_path: String,
    pub repository: String,
    pub repositories: Vec<GitHubRepository>,
}

pub fn github_repository(remote_url: &str) -> Option<String> {
    let remote = remote_url.trim();
    let (host, path) = if let Some(rest) = remote.strip_prefix("https://") {
        let (host, path) = rest.split_once('/')?;
        (host.rsplit('@').next()?, path)
    } else if let Some(rest) = remote.strip_prefix("http://") {
        let (host, path) = rest.split_once('/')?;
        (host.rsplit('@').next()?, path)
    } else if let Some(rest) = remote.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/')?;
        (authority.rsplit('@').next()?, path)
    } else if let Some(rest) = remote.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        (host, path)
    } else {
        return None;
    };
    let host = host.split(':').next().unwrap_or(host);
    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }
    let path = path.split(['?', '#']).next()?.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if owner.is_empty()
        || name.is_empty()
        || parts.next().is_some()
        || owner.contains(['@', ':'])
        || name.contains(['@', ':'])
    {
        return None;
    }
    let repository = format!("{owner}/{name}");
    super::github::repository_parts(&repository).ok()?;
    Some(repository)
}

async fn git(path: &Path, args: &[&str]) -> Result<String, String> {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new("git")
            .env("PATH", crate::agent::claude_stream::augmented_path())
            .args(["-C", &path.to_string_lossy()])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "Repository lookup timed out".to_string())?
    .map_err(|_| {
        "Could not inspect this folder. Check that Git is installed and try again.".to_string()
    })?;
    if !output.status.success() {
        return Err("Choose an existing Git repository folder.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub async fn inspect(path: &str) -> Result<RepositoryInspection, String> {
    let path = PathBuf::from(path.trim());
    if !path.is_dir() {
        return Err("Choose an existing folder.".into());
    }
    let root = git(&path, &["rev-parse", "--show-toplevel"]).await?;
    let remotes = git(&path, &["remote"]).await.unwrap_or_default();
    let mut repositories = Vec::new();
    for name in remotes
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let Ok(url) = git(&path, &["remote", "get-url", name]).await else {
            continue;
        };
        if let Some(repository) = github_repository(&url) {
            repositories.push(GitHubRepository {
                remote: name.to_owned(),
                repository,
            });
        }
    }
    repositories.sort_by_key(|r| match r.remote.as_str() {
        "origin" => 0,
        "upstream" => 1,
        _ => 2,
    });
    let repository = repositories
        .first()
        .map(|r| r.repository.clone())
        .unwrap_or_default();
    Ok(RepositoryInspection {
        repo_path: root,
        repository,
        repositories,
    })
}

/// Linked worktrees share this path even when their checkout roots differ.
pub async fn common_directory(path: &str) -> Result<String, String> {
    let common = git(
        Path::new(path),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await?;
    std::fs::canonicalize(common)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn git_at(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn parses_github_https_and_ssh_forms_and_rejects_other_hosts() {
        assert_eq!(
            github_repository("https://github.com/acme/widget.git"),
            Some("acme/widget".into())
        );
        assert_eq!(
            github_repository("git@github.com:acme/widget.git"),
            Some("acme/widget".into())
        );
        assert_eq!(
            github_repository("ssh://git@github.com/acme/widget"),
            Some("acme/widget".into())
        );
        assert_eq!(
            github_repository("https://github.example.com/acme/widget"),
            None
        );
        assert_eq!(github_repository("https://github.com/acme"), None);
    }

    #[tokio::test]
    async fn repository_identity_matches_linked_worktrees_but_not_another_checkout() {
        let temp = TempDir::new().unwrap();
        let main = temp.path().join("main");
        let other = temp.path().join("other");
        let linked = temp.path().join("linked");
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        git_at(&main, &["init"]);
        git_at(&other, &["init"]);
        git_at(
            &main,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--allow-empty",
                "-m",
                "Seed",
            ],
        );
        git_at(
            &main,
            &["worktree", "add", "-b", "peer", linked.to_str().unwrap()],
        );
        let root = common_directory(main.to_str().unwrap()).await.unwrap();
        assert_eq!(
            root,
            common_directory(linked.to_str().unwrap()).await.unwrap()
        );
        assert_ne!(
            root,
            common_directory(other.to_str().unwrap()).await.unwrap()
        );
    }

    #[tokio::test]
    async fn resolves_nested_folder_to_repository_root_and_prefers_upstream_fallback() {
        let dir = TempDir::new().unwrap();
        git_at(dir.path(), &["init", "-q"]);
        git_at(
            dir.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://gitlab.com/acme/widget.git",
            ],
        );
        git_at(
            dir.path(),
            &[
                "remote",
                "add",
                "upstream",
                "git@github.com:acme/widget.git",
            ],
        );
        let nested = dir.path().join("src").join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let result = inspect(nested.to_str().unwrap()).await.unwrap();
        assert_eq!(
            Path::new(&result.repo_path),
            dir.path().canonicalize().unwrap()
        );
        assert_eq!(result.repository, "acme/widget");
        assert_eq!(result.repositories[0].remote, "upstream");
    }

    #[tokio::test]
    async fn no_github_remote_is_valid_and_non_git_folder_is_rejected() {
        let dir = TempDir::new().unwrap();
        git_at(dir.path(), &["init", "-q"]);
        git_at(
            dir.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://gitlab.com/acme/widget.git",
            ],
        );
        let result = inspect(dir.path().to_str().unwrap()).await.unwrap();
        assert!(result.repository.is_empty());
        assert!(result.repositories.is_empty());
        let plain = TempDir::new().unwrap();
        assert!(inspect(plain.path().to_str().unwrap()).await.is_err());
    }

    #[tokio::test]
    async fn linked_worktree_resolves_to_its_own_root() {
        let dir = TempDir::new().unwrap();
        git_at(dir.path(), &["init", "-q"]);
        git_at(
            dir.path(),
            &["config", "user.email", "room-test@example.invalid"],
        );
        git_at(dir.path(), &["config", "user.name", "Room Test"]);
        std::fs::write(dir.path().join("README"), "test").unwrap();
        git_at(dir.path(), &["add", "README"]);
        git_at(dir.path(), &["commit", "-qm", "initial"]);
        let worktree = dir.path().join("linked-worktree");
        git_at(
            dir.path(),
            &[
                "worktree",
                "add",
                "-qb",
                "room-worktree",
                worktree.to_str().unwrap(),
            ],
        );
        let nested = worktree.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        let result = inspect(nested.to_str().unwrap()).await.unwrap();
        assert_eq!(
            Path::new(&result.repo_path),
            worktree.canonicalize().unwrap()
        );
    }
}
