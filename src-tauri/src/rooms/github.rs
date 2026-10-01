use super::models::{Board, BoardItem, RoomProject};
use crate::agent::claude_stream::{augmented_path, which_binary};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

pub fn repository_parts(repository: &str) -> Result<(&str, &str), String> {
    let (owner, name) = repository
        .trim()
        .split_once('/')
        .ok_or("repository must be OWNER/REPO")?;
    if owner.is_empty()
        || name.is_empty()
        || owner.len() > 100
        || name.len() > 100
        || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        || name == "."
        || name == ".."
    {
        return Err("repository must be a valid GitHub OWNER/REPO".into());
    }
    Ok((owner, name))
}

pub async fn graphql(query: &str, variables: Value) -> Result<Value, String> {
    let gh = which_binary("gh")
        .ok_or("GitHub CLI unavailable. Install/authenticate gh with Project access.")?;
    let mut child = tokio::process::Command::new(gh)
        .env("PATH", augmented_path())
        .args(["api", "graphql", "--input", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            format!("GitHub CLI unavailable: {e}. Install/authenticate gh with Project access.")
        })?;
    let bytes = serde_json::to_vec(&json!({"query": query, "variables": variables}))
        .map_err(|e| e.to_string())?;
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), async move {
        let mut stdin = child.stdin.take().ok_or("GitHub CLI stdin unavailable")?;
        stdin.write_all(&bytes).await.map_err(|e| e.to_string())?;
        drop(stdin);
        child.wait_with_output().await.map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "GitHub request timed out; refresh before retrying a write".to_string())??;
    let parsed = serde_json::from_slice::<Value>(&output.stdout);
    if let Ok(value) = &parsed {
        if let Some(errors) = value.get("errors").and_then(Value::as_array) {
            if !errors.is_empty() {
                let messages = errors
                    .iter()
                    .filter_map(|e| e["message"].as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(format!("GitHub rejected request: {messages}"));
            }
        }
    }
    if !output.status.success() {
        let message: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1500)
            .collect();
        return Err(format!("GitHub request failed: {}", message.trim()));
    }
    let value = parsed.map_err(|e| format!("invalid GitHub response: {e}"))?;
    value
        .get("data")
        .cloned()
        .filter(|v| !v.is_null())
        .ok_or("GitHub response contains no data".into())
}

pub async fn repository_ids(repository: &str) -> Result<(String, String), String> {
    let (owner, name) = repository_parts(repository)?;
    let data = graphql(
        "query($owner:String!,$name:String!){repository(owner:$owner,name:$name){id owner{id}}}",
        json!({"owner": owner, "name": name}),
    )
    .await?;
    Ok((
        required_string(&data["repository"], "id")?,
        required_string(&data["repository"]["owner"], "id")?,
    ))
}

pub async fn require_repository_issues(repository: &str) -> Result<(), String> {
    let (owner, name) = repository_parts(repository)?;
    let data = graphql(
        "query($owner:String!,$name:String!){repository(owner:$owner,name:$name){hasIssuesEnabled}}",
        json!({"owner":owner,"name":name}),
    )
    .await?;
    check_repository_issues(&data["repository"], repository)
}

fn check_repository_issues(value: &Value, repository: &str) -> Result<(), String> {
    match value["hasIssuesEnabled"].as_bool() {
        Some(true) => Ok(()),
        Some(false) => Err(format!("GitHub Issues are disabled for {repository}. Enable Issues in the repository settings before creating room tasks.")),
        None => Err(format!("Could not verify whether GitHub Issues are enabled for {repository}; task creation was not started.")),
    }
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("GitHub response missing {key}; check repository/Project access"))
}

/// List every open ProjectV2 explicitly linked to the repository. Returns candidates in
/// GitHub's order; callers must not auto-select if more than one result is present.
pub async fn list_repository_projects(repository: &str) -> Result<Vec<RoomProject>, String> {
    let (owner, name) = repository_parts(repository)?;
    let mut after: Option<String> = None;
    let mut projects = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..100 {
        let data = graphql(
            "query($owner:String!,$name:String!,$after:String){repository(owner:$owner,name:$name){projectsV2(first:100,after:$after){nodes{id number title url closed owner{... on User{login} ... on Organization{login}}} pageInfo{hasNextPage endCursor}}}}",
            json!({"owner": owner, "name": name, "after": after}),
        )
        .await?;
        let connection = &data["repository"]["projectsV2"];
        if connection.is_null() {
            return Err("GitHub repository not found or Projects access is unavailable".into());
        }
        for project in parse_repository_project_page(connection, repository)? {
            if seen.insert(project.id.clone()) {
                projects.push(project);
            }
        }
        if !next_cursor(&connection["pageInfo"], &mut after)? {
            return Ok(projects);
        }
    }
    Err("Repository Project discovery exceeded its pagination limit".into())
}

fn parse_repository_project_page(
    connection: &Value,
    repository: &str,
) -> Result<Vec<RoomProject>, String> {
    let nodes = connection["nodes"]
        .as_array()
        .ok_or("GitHub repository Project list unavailable; check Project access")?;
    let mut projects = Vec::new();
    for node in nodes {
        // The repository.projectsV2 connection contains only Projects linked to this repo.
        // Require an explicit open state so incomplete/private response shapes aren't guessed.
        if node["closed"].as_bool() != Some(false) {
            continue;
        }
        projects.push(parse_project(node, repository)?);
    }
    Ok(projects)
}

fn parse_project(value: &Value, repository: &str) -> Result<RoomProject, String> {
    repository_parts(repository)?;
    Ok(RoomProject {
        id: required_string(value, "id")?,
        number: value["number"]
            .as_u64()
            .ok_or("GitHub project number missing")?,
        title: required_string(value, "title")?,
        url: required_string(value, "url")?,
        owner: required_string(&value["owner"], "login")?,
        repository: repository.into(),
    })
}

pub async fn find_project(
    owner_id: &str,
    repository: &str,
    title: &str,
) -> Result<Option<RoomProject>, String> {
    let mut after: Option<String> = None;
    for _ in 0..100 {
        let data = graphql(
            "query($id:ID!,$after:String){node(id:$id){... on User{projectsV2(first:100,after:$after){nodes{id number title url owner{... on User{login} ... on Organization{login}}} pageInfo{hasNextPage endCursor}}} ... on Organization{projectsV2(first:100,after:$after){nodes{id number title url owner{... on User{login} ... on Organization{login}}} pageInfo{hasNextPage endCursor}}}}}",
            json!({"id": owner_id, "after": after}),
        ).await?;
        let projects = &data["node"]["projectsV2"];
        let nodes = projects["nodes"]
            .as_array()
            .ok_or("GitHub Projects unavailable; check Project permissions")?;
        for node in nodes {
            if node["title"].as_str() == Some(title) {
                return parse_project(node, repository).map(Some);
            }
        }
        if !next_cursor(&projects["pageInfo"], &mut after)? {
            return Ok(None);
        }
    }
    Err("Project lookup exceeded its pagination limit; no new Project was created".into())
}

pub async fn create_project(
    repository: &str,
    repo_id: &str,
    owner_id: &str,
    title: &str,
) -> Result<RoomProject, String> {
    let data = graphql(
        "mutation($owner:ID!,$repo:ID!,$title:String!){createProjectV2(input:{ownerId:$owner,repositoryId:$repo,title:$title}){projectV2{id number title url owner{... on User{login} ... on Organization{login}}}}}",
        json!({"owner": owner_id, "repo": repo_id, "title": title}),
    ).await?;
    parse_project(&data["createProjectV2"]["projectV2"], repository)
}

fn next_cursor(page: &Value, cursor: &mut Option<String>) -> Result<bool, String> {
    if page["hasNextPage"].as_bool() != Some(true) {
        return Ok(false);
    }
    let next = required_string(page, "endCursor")?;
    if cursor.as_deref() == Some(&next) {
        return Err("GitHub pagination did not advance".into());
    }
    *cursor = Some(next);
    Ok(true)
}

pub async fn read_board(project_id: &str) -> Result<Board, String> {
    let mut after: Option<String> = None;
    let mut items = vec![];
    let mut total_count = 0;
    for _ in 0..100 {
        let data = graphql(
            "query($id:ID!,$after:String){node(id:$id){... on ProjectV2{items(first:100,after:$after){totalCount pageInfo{hasNextPage endCursor} nodes{id content{__typename ... on Issue{title url body number updatedAt assignees(first:20){nodes{login}} labels(first:20){nodes{name}} closedByPullRequestsReferences(first:20){nodes{url}}} ... on PullRequest{title url body number updatedAt assignees(first:20){nodes{login}} labels(first:20){nodes{name}}} ... on DraftIssue{title body}} fieldValues(first:100){nodes{... on ProjectV2ItemFieldSingleSelectValue{name field{... on ProjectV2FieldCommon{name}}} ... on ProjectV2ItemFieldTextValue{text field{... on ProjectV2FieldCommon{name}}}}}}}}}}",
            json!({"id": project_id, "after": after}),
        ).await?;
        let connection = &data["node"]["items"];
        let nodes = connection["nodes"]
            .as_array()
            .ok_or("GitHub Project inaccessible; previous board retained")?;
        total_count = connection["totalCount"].as_u64().unwrap_or(total_count);
        for node in nodes {
            items.push(parse_board_item(node)?);
        }
        if !next_cursor(&connection["pageInfo"], &mut after)? {
            return Ok(Board {
                items,
                synced_at: Some(crate::models::now_iso()),
                error: None,
                total_count,
            });
        }
    }
    Err("GitHub board pagination limit reached; previous complete board retained".into())
}

// Project item connections can lag a successful add mutation. Do not report the
// new task as usable until it is visible in the same snapshot used for claiming.
pub async fn read_board_containing(project_id: &str, task_id: &str) -> Result<Board, String> {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let board = read_board(project_id).await?;
            if board.items.iter().any(|item| item.id == task_id) {
                return Ok(board);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }).await.map_err(|_| "GitHub's Project item list has not caught up with task creation. Retry create_task with the identical title/body to reconcile; do not create a replacement task.".to_string())?
}

fn parse_board_item(node: &Value) -> Result<BoardItem, String> {
    let content = &node["content"];
    let kind = match content["__typename"].as_str() {
        Some("Issue") => "issue",
        Some("PullRequest") => "pull_request",
        Some("DraftIssue") => "draft",
        _ => "redacted",
    };
    let mut status = None;
    let mut generic_status = None;
    let mut priority = None;
    let mut agent = None;
    if let Some(fields) = node["fieldValues"]["nodes"].as_array() {
        for field in fields {
            let value = field["name"]
                .as_str()
                .or_else(|| field["text"].as_str())
                .map(str::to_owned);
            match field["field"]["name"]
                .as_str()
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("remediation status") => {
                    if status.is_none() {
                        status = value;
                    }
                }
                Some("status") => {
                    if generic_status.is_none() {
                        generic_status = value;
                    }
                }
                Some("priority") => priority = value,
                Some("agent") | Some("agent owner") => agent = value,
                _ => {}
            }
        }
    }
    let status = status
        .or(generic_status)
        .unwrap_or_else(|| "Backlog".into());
    Ok(BoardItem {
        id: required_string(node, "id")?,
        title: if kind == "redacted" {
            "Restricted item".into()
        } else {
            required_string(content, "title")?
        },
        url: if kind == "redacted" {
            None
        } else {
            content["url"].as_str().map(str::to_owned)
        },
        status,
        priority,
        agent,
        kind: kind.into(),
        body: content["body"].as_str().map(str::to_owned),
        number: content["number"].as_u64(),
        assignees: content["assignees"]["nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|v| v["login"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        labels: content["labels"]["nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|v| v["name"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        linked_prs: content["closedByPullRequestsReferences"]["nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|v| v["url"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        updated_at: content["updatedAt"].as_str().map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_creation_preflight_reports_disabled_or_unverifiable_repositories() {
        assert!(check_repository_issues(&json!({"hasIssuesEnabled":true}), "owner/repo").is_ok());
        let disabled =
            check_repository_issues(&json!({"hasIssuesEnabled":false}), "owner/repo").unwrap_err();
        assert!(disabled.contains("Enable Issues in the repository settings"));
        assert!(disabled.contains("owner/repo"));
        assert!(check_repository_issues(&Value::Null, "owner/repo")
            .unwrap_err()
            .contains("task creation was not started"));
    }

    #[test]
    fn rejects_urls_and_extra_path_segments_without_shell_interpolation() {
        assert!(repository_parts("owner/repo").is_ok());
        for bad in [
            "https://github.com/owner/repo",
            "owner/repo/issues",
            "owner/..",
            "a;bad/repo",
            "owner/",
        ] {
            assert!(repository_parts(bad).is_err());
        }
    }

    #[test]
    fn repository_project_candidates_keep_actual_owner_and_only_open_linked_projects() {
        let connection = json!({
            "nodes": [
                {
                    "id": "PVT_open_1",
                    "number": 2,
                    "title": "Nemo Feature Roadmap",
                    "url": "https://github.com/users/board-owner/projects/2",
                    "closed": false,
                    "owner": {"login": "board-owner"}
                },
                {
                    "id": "PVT_open_2",
                    "number": 7,
                    "title": "Shared planning",
                    "url": "https://github.com/orgs/team/projects/7",
                    "closed": false,
                    "owner": {"login": "team"}
                },
                {
                    "id": "PVT_closed",
                    "number": 8,
                    "title": "Archived board",
                    "url": "https://github.com/users/board-owner/projects/8",
                    "closed": true,
                    "owner": {"login": "board-owner"}
                }
            ],
            "pageInfo": {"hasNextPage": false, "endCursor": "cursor-1"}
        });

        let candidates = parse_repository_project_page(&connection, "repo-owner/nemo").unwrap();
        assert_eq!(candidates.len(), 2, "return all linked open choices");
        assert_eq!(candidates[0].owner, "board-owner");
        assert_eq!(candidates[0].repository, "repo-owner/nemo");
        assert_eq!(candidates[0].number, 2);
        assert_eq!(candidates[1].owner, "team");
    }

    #[test]
    fn reads_custom_status_priority_and_agent_fields() {
        let item = parse_board_item(&json!({"id":"item", "content":{"__typename":"Issue","title":"Do work","url":"https://github.com/o/r/issues/1","body":"## Outcome\nDone","number":1,"updatedAt":"2026-10-01T00:00:00Z","assignees":{"nodes":[{"login":"coder"}]},"labels":{"nodes":[{"name":"urgent"}]},"closedByPullRequestsReferences":{"nodes":[{"url":"https://github.com/o/r/pull/4"}]}},"fieldValues":{"nodes":[{"name":"Ready","field":{"name":"Status"}},{"name":"P1","field":{"name":"Priority"}},{"text":"Claude review","field":{"name":"Agent"}}]}})).unwrap();
        assert_eq!(item.status, "Ready");
        assert_eq!(item.priority.as_deref(), Some("P1"));
        assert_eq!(item.agent.as_deref(), Some("Claude review"));
        assert_eq!(item.body.as_deref(), Some("## Outcome\nDone"));
        assert_eq!(item.number, Some(1));
        assert_eq!(item.assignees, ["coder"]);
        assert_eq!(item.labels, ["urgent"]);
        assert_eq!(item.linked_prs, ["https://github.com/o/r/pull/4"]);
    }

    #[test]
    fn remediation_status_takes_precedence_over_generic_status() {
        let item = parse_board_item(&json!({
            "id": "item",
            "content": {"__typename": "Issue", "title": "Pivot remediation"},
            "fieldValues": {"nodes": [
                {"name": "Backlog", "field": {"name": "Status"}},
                {"name": "In progress", "field": {"name": "Remediation status"}},
                {"name": "Review", "field": {"name": "Remediation status"}}
            ]}
        }))
        .unwrap();
        assert_eq!(item.status, "In progress");
    }

    #[test]
    fn inaccessible_items_are_never_presented_as_actionable_content() {
        let item = parse_board_item(&json!({"id":"hidden", "content":null})).unwrap();
        assert_eq!(item.kind, "redacted");
        assert_eq!(item.title, "Restricted item");
        assert!(item.url.is_none());
    }

    #[test]
    fn pagination_rejects_nonadvancing_or_missing_cursors() {
        let mut cursor = Some("same".into());
        assert!(next_cursor(&json!({"hasNextPage":true,"endCursor":"same"}), &mut cursor).is_err());
        assert!(next_cursor(&json!({"hasNextPage":true}), &mut cursor).is_err());
    }
}
