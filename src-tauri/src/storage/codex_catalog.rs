//! Read-only desktop naming/source metadata. Transcripts remain authoritative
//! for import; missing/older desktop databases fall back to the naming index.
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Clone, Default)]
pub struct ThreadInfo {
    pub name: Option<String>,
    pub prompt: String,
    pub cwd: String,
    pub source: Value,
    pub model: Option<String>,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

pub fn home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| super::dirs_next().map(|p| p.join(".codex")))
}

pub fn parent_session(source: &Value) -> Option<String> {
    source
        .pointer("/subagent/thread_spawn/parent_thread_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn is_subagent(source: &Value) -> bool {
    source.get("subagent").is_some() || source.as_str().is_some_and(|s| s == "subagent")
}

fn timestamp(seconds: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
        .map(|v| v.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

pub fn read(home: &Path) -> HashMap<String, ThreadInfo> {
    let mut result = HashMap::new();
    // Prefer the active desktop database, with legacy CLI database as fallback.
    for path in [
        home.join("state_5.sqlite"),
        home.join("sqlite/state_5.sqlite"),
    ] {
        if !path.is_file() {
            continue;
        }
        if let Ok(entries) = read_database(&path) {
            for (id, info) in entries {
                result.entry(id).or_insert(info);
            }
        }
    }
    if let Ok(file) = File::open(home.join("session_index.jsonl")) {
        // Append-only index: last name wins unless the active DB has a name.
        let mut names = HashMap::new();
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if let (Some(id), Some(name)) = (v["id"].as_str(), v["thread_name"].as_str()) {
                    if !name.trim().is_empty() {
                        names.insert(id.to_owned(), name.to_owned());
                    }
                }
            }
        }
        for (id, name) in names {
            result.entry(id).or_default().name.get_or_insert(name);
        }
    }
    result
}

fn read_database(path: &Path) -> rusqlite::Result<HashMap<String, ThreadInfo>> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.busy_timeout(std::time::Duration::from_millis(500))?;
    let columns: Vec<String> = db
        .prepare("PRAGMA table_info(threads)")?
        .query_map([], |r| r.get(1))?
        .collect::<rusqlite::Result<_>>()?;
    // Optional columns differ across desktop/CLI versions.
    let name = if columns.iter().any(|c| c == "name") {
        "name"
    } else {
        "NULL"
    };
    let model = if columns.iter().any(|c| c == "model") {
        "model"
    } else {
        "NULL"
    };
    let prompt = if columns.iter().any(|c| c == "first_user_message") {
        "substr(first_user_message,1,200)"
    } else {
        "substr(title,1,200)"
    };
    let query = format!(
        "SELECT id,{name},{prompt},cwd,source,{model},archived,created_at,updated_at FROM threads"
    );
    let mut statement = db.prepare(&query)?;
    let entries = statement
        .query_map([], |r| {
            let source: String = r.get(4)?;
            let name: Option<String> = r.get(1)?;
            Ok((
                r.get(0)?,
                ThreadInfo {
                    name: name.filter(|s| !s.trim().is_empty()),
                    prompt: r.get(2)?,
                    cwd: r.get(3)?,
                    source: serde_json::from_str(&source).unwrap_or(Value::String(source)),
                    model: r.get(5)?,
                    archived: r.get::<_, i64>(6)? != 0,
                    created_at: timestamp(r.get(7)?),
                    updated_at: timestamp(r.get(8)?),
                },
            ))
        })?
        .collect();
    entries
}

/// Associate real Git worktrees and nested directories with their primary repo.
/// A worktree is not evidence that a session is a subagent.
pub fn project_path(cwd: &str) -> String {
    let path = Path::new(cwd);
    for ancestor in path.ancestors() {
        let git = ancestor.join(".git");
        if git.is_dir() {
            return ancestor.to_string_lossy().into_owned();
        }
        if git.is_file() {
            if let Ok(content) = std::fs::read_to_string(&git) {
                if let Some(dir) = content.trim().strip_prefix("gitdir: ") {
                    let gitdir = ancestor.join(dir);
                    if let Ok(common) = std::fs::read_to_string(gitdir.join("commondir")) {
                        if let Ok(common) = gitdir.join(common.trim()).canonicalize() {
                            if common.file_name().is_some_and(|n| n == ".git") {
                                if let Some(root) = common.parent() {
                                    return root.to_string_lossy().into_owned();
                                }
                            }
                        }
                    }
                }
            }
            return ancestor.to_string_lossy().into_owned();
        }
    }
    cwd.trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_only_names_and_sources_survive_index_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state_5.sqlite");
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE threads(id TEXT,name TEXT,title TEXT,cwd TEXT,source TEXT,archived INTEGER,created_at INTEGER,updated_at INTEGER); INSERT INTO threads VALUES('main','Named RAV chat','Opening prompt','/repo','vscode',0,1,2); INSERT INTO threads VALUES('child',NULL,'Task prompt','/worktree','{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"main\"}}}',0,1,2);").unwrap();
        drop(db);
        std::fs::write(tmp.path().join("session_index.jsonl"), "{\"id\":\"main\",\"thread_name\":\"Old name\"}\n{\"id\":\"legacy\",\"thread_name\":\"Old\"}\n{\"id\":\"legacy\",\"thread_name\":\"New name\"}\n").unwrap();
        let before = std::fs::read(&path).unwrap();
        let catalog = read(tmp.path());
        assert_eq!(catalog["main"].name.as_deref(), Some("Named RAV chat"));
        assert_eq!(catalog["legacy"].name.as_deref(), Some("New name"));
        assert_eq!(
            parent_session(&catalog["child"].source).as_deref(),
            Some("main")
        );
        assert!(!is_subagent(&catalog["main"].source));
        assert_eq!(before, std::fs::read(path).unwrap());
    }
    #[test]
    fn nested_and_worktree_project_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let wt = tmp.path().join("worktree");
        std::fs::create_dir_all(repo.join(".git/worktrees/peer")).unwrap();
        std::fs::create_dir_all(repo.join("src/nested")).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}", repo.join(".git/worktrees/peer").display()),
        )
        .unwrap();
        std::fs::write(repo.join(".git/worktrees/peer/commondir"), "../..").unwrap();
        assert_eq!(
            project_path(repo.join("src/nested").to_str().unwrap()),
            repo.to_str().unwrap()
        );
        assert_eq!(
            project_path(wt.to_str().unwrap()),
            repo.canonicalize().unwrap().to_str().unwrap()
        );
    }
}
