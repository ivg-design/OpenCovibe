use crate::agent::claude_stream::augmented_path;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use uuid::Uuid;

async fn git(args: &[&str]) -> Result<String, String> {
    let mut c = tokio::process::Command::new("git");
    c.env("PATH", augmented_path())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(30), c.output())
        .await
        .map_err(|_| "Git command timed out".to_string())?
        .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        let e = String::from_utf8_lossy(&out.stderr)
            .chars()
            .take(1500)
            .collect::<String>();
        return Err(format!("git failed: {}", e.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
fn validate_id(s: &str, label: &str) -> Result<(), String> {
    Uuid::parse_str(s)
        .map(|_| ())
        .map_err(|_| format!("{label} must be a UUID"))
}
async fn root(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("repository path must be absolute".into());
    }
    let canonical = p
        .canonicalize()
        .map_err(|e| format!("repository path unavailable: {e}"))?;
    let s = canonical.to_string_lossy().into_owned();
    let top = git(&["-C", &s, "rev-parse", "--show-toplevel"]).await?;
    if Path::new(&top).canonicalize().ok().as_deref() != Some(canonical.as_path()) {
        return Err("repository path must be the Git working tree root".into());
    }
    Ok(canonical)
}
async fn status(path: &str) -> Result<(), String> {
    let s = git(&[
        "-C",
        path,
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
    ])
    .await?;
    if !s.is_empty() {
        return Err(format!(
            "working tree is dirty: {}",
            s.chars().take(300).collect::<String>()
        ));
    }
    Ok(())
}
fn managed_path_at(data_dir: &Path, room: &str, peer: &str) -> PathBuf {
    data_dir.join("worktrees").join(room).join(peer)
}
fn short(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase()
}
pub async fn create(
    repo_path: &str,
    room_id: &str,
    participant_id: &str,
) -> Result<(String, String), String> {
    create_at(
        repo_path,
        room_id,
        participant_id,
        &crate::storage::data_dir(),
    )
    .await
}
async fn create_at(
    repo_path: &str,
    room_id: &str,
    participant_id: &str,
    data_dir: &Path,
) -> Result<(String, String), String> {
    validate_id(room_id, "room id")?;
    validate_id(participant_id, "participant id")?;
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("could not create worktree storage directory: {e}"))?;
    let data_dir = data_dir
        .canonicalize()
        .map_err(|e| format!("worktree storage directory unavailable: {e}"))?;
    let repo = root(repo_path).await?;
    let rp = repo.to_string_lossy().into_owned();
    let branch = format!("room/{}/{}", short(room_id), short(participant_id));
    let path = managed_path_at(&data_dir, room_id, participant_id);
    let target = path.to_string_lossy().into_owned();
    let head = git(&["-C", &rp, "rev-parse", "--verify", "HEAD"]).await?;
    let output = git(&["-C", &rp, "worktree", "list", "--porcelain"]).await?;
    for block in output.split("\n\n") {
        let p = block.lines().find_map(|l| l.strip_prefix("worktree "));
        if p == Some(target.as_str()) {
            let b = block
                .lines()
                .find_map(|l| l.strip_prefix("branch refs/heads/"));
            if b == Some(branch.as_str()) {
                return Ok((target, branch));
            }
            return Err(
                "managed worktree path is already registered with a different branch".into(),
            );
        }
    }
    if path.exists() {
        return Err(
            "managed worktree path exists but is not registered; refusing to overwrite".into(),
        );
    }
    std::fs::create_dir_all(path.parent().ok_or("invalid worktree path")?)
        .map_err(|e| format!("could not create worktree parent: {e}"))?;
    // Use the observed committed HEAD explicitly; dirty root changes are never copied.
    git(&["-C", &rp, "worktree", "add", "-b", &branch, &target, &head]).await?;
    Ok((target, branch))
}
pub async fn merge(repo_path: &str, worktree_path: &str, branch: &str) -> Result<String, String> {
    merge_at(
        repo_path,
        worktree_path,
        branch,
        &crate::storage::data_dir(),
    )
    .await
}
async fn merge_at(
    repo_path: &str,
    worktree_path: &str,
    branch: &str,
    data_dir: &Path,
) -> Result<String, String> {
    let repo = root(repo_path).await?;
    let rp = repo.to_string_lossy().into_owned();
    let source = Path::new(worktree_path)
        .canonicalize()
        .map_err(|e| format!("worktree path unavailable: {e}"))?;
    let src = source.to_string_lossy().into_owned();
    if !branch.starts_with("room/")
        || branch.len() > 40
        || !branch[5..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '/')
        || branch.matches('/').count() != 2
    {
        return Err("branch is not a managed room branch".into());
    }
    let expected = data_dir.join("worktrees");
    let expected = expected.canonicalize().unwrap_or(expected);
    if !source.starts_with(&expected) || source == repo {
        return Err("worktree is outside the managed worktree directory".into());
    }
    let lists = git(&["-C", &rp, "worktree", "list", "--porcelain"]).await?;
    let registered = lists.split("\n\n").any(|b| {
        b.lines()
            .any(|l| l.strip_prefix("worktree ") == Some(src.as_str()))
            && b.lines()
                .any(|l| l.strip_prefix("branch refs/heads/") == Some(branch))
    });
    if !registered {
        return Err("worktree path and branch are not an exact registered pair".into());
    }
    let merge_head = git(&["-C", &rp, "rev-parse", "--git-path", "MERGE_HEAD"]).await?;
    if Path::new(&rp).join(merge_head).exists() {
        return Err("repository already has a merge in progress".into());
    }
    status(&rp).await?;
    status(&src).await?;
    let result = git(&["-C", &rp, "merge", "--no-edit", branch]).await;
    match result {
        Ok(_) => git(&["-C", &rp, "rev-parse", "HEAD"]).await,
        Err(e) => {
            let merge_head = git(&["-C", &rp, "rev-parse", "--git-path", "MERGE_HEAD"])
                .await
                .unwrap_or_default();
            if Path::new(&rp).join(merge_head).exists() {
                let _ = git(&["-C", &rp, "merge", "--abort"]).await;
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    fn fixture() -> (tempfile::TempDir, String) {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("repo");
        std::fs::create_dir(&p).unwrap();
        for a in [
            &["init", "-q"][..],
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "Test"],
        ] {
            assert!(Command::new("git")
                .current_dir(&p)
                .args(a)
                .status()
                .unwrap()
                .success())
        }
        std::fs::write(p.join("base"), "base\n").unwrap();
        assert!(Command::new("git")
            .current_dir(&p)
            .args(["add", "base"])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .current_dir(&p)
            .args(["commit", "-qm", "base"])
            .status()
            .unwrap()
            .success());
        (t, p.to_string_lossy().into_owned())
    }
    #[tokio::test]
    async fn create_reuses_exact_worktree_and_starts_at_committed_head() {
        let (t, repo) = fixture();
        let data = t.path().join("data");
        let room = Uuid::new_v4().to_string();
        let peer = Uuid::new_v4().to_string();
        std::fs::write(Path::new(&repo).join("uncommitted"), "root change\n").unwrap();
        let (root, branch) = create_at(&repo, &room, &peer, &data).await.unwrap();
        assert!(branch.starts_with("room/"));
        let listed = git(&["-C", &repo, "worktree", "list", "--porcelain"])
            .await
            .unwrap();
        assert_eq!(
            create_at(&repo, &room, &peer, &data)
                .await
                .unwrap_or_else(|e| panic!("{e}; target={root}; branch={branch}; list={listed}")),
            (root.clone(), branch)
        );
        assert!(!Path::new(&root).join("uncommitted").exists());
        std::fs::remove_file(Path::new(&repo).join("uncommitted")).unwrap();
        let _ = git(&["-C", &repo, "worktree", "remove", &root]).await;
        let _ = std::fs::remove_dir_all(data.join("worktrees"));
        drop(t);
    }
    #[tokio::test]
    async fn merge_rejects_dirty_target() {
        let (t, repo) = fixture();
        let data = t.path().join("data");
        let room = Uuid::new_v4().to_string();
        let peer = Uuid::new_v4().to_string();
        let (p, b) = create_at(&repo, &room, &peer, &data).await.unwrap();
        std::fs::write(Path::new(&repo).join("dirty"), "x").unwrap();
        assert!(merge_at(&repo, &p, &b, &data)
            .await
            .unwrap_err()
            .contains("dirty"));
        std::fs::remove_file(Path::new(&repo).join("dirty")).unwrap();
        std::fs::write(Path::new(&p).join("source-dirty"), "x").unwrap();
        assert!(merge_at(&repo, &p, &b, &data)
            .await
            .unwrap_err()
            .contains("dirty"));
        let _ = git(&["-C", &repo, "worktree", "remove", "--force", &p]).await;
        let _ = std::fs::remove_dir_all(data.join("worktrees"));
    }
    #[tokio::test]
    async fn merge_success_returns_commit_and_conflict_aborts_own_merge() {
        let (t, repo) = fixture();
        let data = t.path().join("data");
        let room = Uuid::new_v4().to_string();
        let peer = Uuid::new_v4().to_string();
        let (p, b) = create_at(&repo, &room, &peer, &data).await.unwrap();
        std::fs::write(Path::new(&p).join("feature"), "feature\n").unwrap();
        assert!(Command::new("git")
            .args(["-C", &p, "add", "feature"])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["-C", &p, "commit", "-qm", "feature"])
            .status()
            .unwrap()
            .success());
        let sha = merge_at(&repo, &p, &b, &data).await.unwrap();
        assert_eq!(git(&["-C", &repo, "rev-parse", "HEAD"]).await.unwrap(), sha);
        assert_eq!(
            std::fs::read_to_string(Path::new(&repo).join("feature")).unwrap(),
            "feature\n"
        );
        let _ = git(&["-C", &repo, "worktree", "remove", &p]).await;
        let _ = std::fs::remove_dir_all(data.join("worktrees"));

        let (t, repo) = fixture();
        let data = t.path().join("data");
        let room = Uuid::new_v4().to_string();
        let peer = Uuid::new_v4().to_string();
        let (p, b) = create_at(&repo, &room, &peer, &data).await.unwrap();
        std::fs::write(Path::new(&p).join("base"), "branch side\n").unwrap();
        assert!(Command::new("git")
            .args(["-C", &p, "add", "base"])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["-C", &p, "commit", "-qm", "branch edit"])
            .status()
            .unwrap()
            .success());
        std::fs::write(Path::new(&repo).join("base"), "main side\n").unwrap();
        assert!(Command::new("git")
            .args(["-C", &repo, "add", "base"])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["-C", &repo, "commit", "-qm", "main edit"])
            .status()
            .unwrap()
            .success());
        assert!(merge_at(&repo, &p, &b, &data)
            .await
            .unwrap_err()
            .contains("git failed"));
        let merge_head = git(&["-C", &repo, "rev-parse", "--git-path", "MERGE_HEAD"])
            .await
            .unwrap();
        assert!(!Path::new(&repo).join(merge_head).exists());
        assert!(git(&["-C", &repo, "status", "--porcelain"])
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            std::fs::read_to_string(Path::new(&repo).join("base")).unwrap(),
            "main side\n"
        );
        let _ = git(&["-C", &repo, "worktree", "remove", "--force", &p]).await;
        let _ = std::fs::remove_dir_all(data.join("worktrees"));
    }
}
