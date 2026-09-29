use super::{github, models::RoomProject};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::OwnedMutexGuard;

static PROJECT_LOCKS: Lazy<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
async fn lock_project(id: &str) -> OwnedMutexGuard<()> {
    let lock = {
        let mut locks = PROJECT_LOCKS.lock().expect("project locks poisoned");
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        locks
            .entry(id.to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    lock.lock_owned().await
}

fn req(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("GitHub Project response missing {key}"))
}
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
fn status_target(s: &str) -> Result<&'static str, String> {
    match norm(s).as_str() {
        "active" | "inprogress" => Ok("In Progress"),
        "done" | "complete" | "completed" => Ok("Done"),
        "ready" | "todo" | "tostart" => Ok("Todo"),
        "blocked" => Ok("In Progress"),
        _ => Err(format!("unsupported task state: {s}")),
    }
}
async fn fields(project: &RoomProject) -> Result<Vec<Value>, String> {
    let d = github::graphql("query($id:ID!){node(id:$id){... on ProjectV2{fields(first:100){nodes{__typename ... on ProjectV2SingleSelectField{id name options{id name}} ... on ProjectV2Field{id name}}}}}}", json!({"id":project.id})).await?;
    d["node"]["fields"]["nodes"]
        .as_array()
        .cloned()
        .ok_or("GitHub Project fields unavailable".into())
}
fn status_field(fs: &[Value]) -> Result<(String, Vec<Value>), String> {
    let f = fs
        .iter()
        .find(|f| f["name"].as_str().is_some_and(|n| norm(n) == "status"))
        .ok_or("Required GitHub Project field 'Status' is missing")?;
    let opts = f["options"]
        .as_array()
        .cloned()
        .ok_or("Required GitHub Project field 'Status' must be a single-select field")?;
    for wanted in ["Todo", "In Progress", "Done"] {
        option_id(&opts, wanted)?;
    }
    Ok((
        f["id"].as_str().ok_or("Status field id missing")?.into(),
        opts,
    ))
}
fn option_id(opts: &[Value], target: &str) -> Result<String, String> {
    let wanted = norm(target);
    let aliases: &[&str] = match wanted.as_str() {
        "todo" => &["todo", "tostart", "ready"],
        "inprogress" => &["inprogress", "doing", "active"],
        _ => &["done", "complete", "completed"],
    };
    opts.iter()
        .find(|o| {
            o["name"]
                .as_str()
                .is_some_and(|s| aliases.contains(&norm(s).as_str()))
        })
        .and_then(|o| o["id"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("Required Status option '{target}' is missing"))
}
fn agent_field(fs: &[Value]) -> Result<String, String> {
    fs.iter()
        .find(|f| {
            f["__typename"].as_str() == Some("ProjectV2Field")
                && f["name"].as_str().is_some_and(|n| norm(n) == "agent")
        })
        .and_then(|f| f["id"].as_str())
        .map(str::to_owned)
        .ok_or("Required GitHub Project text field 'Agent' is missing".into())
}
pub async fn prepare_project(project: &RoomProject) -> Result<(), String> {
    let _guard = lock_project(&project.id).await;
    prepare_project_locked(project).await
}
async fn prepare_project_locked(project: &RoomProject) -> Result<(), String> {
    let fs = fields(project).await?;
    status_field(&fs)?;
    if agent_field(&fs).is_err() {
        let create = github::graphql(
            "mutation($p:ID!){createProjectV2Field(input:{projectId:$p,name:\"Agent\",dataType:TEXT}){projectV2Field{... on ProjectV2Field{id name}}}}",
            json!({"p":project.id}),
        ).await;
        if let Err(error) = create {
            // The write may have reached GitHub even if its response was lost. Re-read before
            // returning the error so a retry never creates a duplicate text field.
            let refreshed = fields(project).await?;
            if agent_field(&refreshed).is_err() {
                return Err(error);
            }
        }
        let refreshed = fields(project).await?;
        agent_field(&refreshed)?;
    }
    Ok(())
}
pub async fn set_task_state(
    project: &RoomProject,
    task_id: &str,
    agent_name: Option<&str>,
    state: &str,
) -> Result<(), String> {
    let _guard = lock_project(&project.id).await;
    prepare_project_locked(project).await?;
    read_task(project, task_id).await?;
    let target = status_target(state)?;
    let fs = fields(project).await?;
    let (sid, opts) = status_field(&fs)?;
    let aid = agent_field(&fs)?;
    github::graphql("mutation($p:ID!,$i:ID!,$f:ID!,$o:String!){updateProjectV2ItemFieldValue(input:{projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$o}}){projectV2Item{id}}}",json!({"p":project.id,"i":task_id,"f":sid,"o":option_id(&opts,target)?})).await?;
    github::graphql("mutation($p:ID!,$i:ID!,$f:ID!,$v:String!){updateProjectV2ItemFieldValue(input:{projectId:$p,itemId:$i,fieldId:$f,value:{text:$v}}){projectV2Item{id}}}",json!({"p":project.id,"i":task_id,"f":aid,"v":agent_name.unwrap_or("")})).await?;
    Ok(())
}
pub async fn create_task(project: &RoomProject, title: &str, body: &str) -> Result<String, String> {
    let _guard = lock_project(&project.id).await;
    prepare_project_locked(project).await?;
    let board = github::read_board(&project.id).await?;
    if let Some(i) = board.items.iter().find(|i| i.title == title) {
        let existing = read_task(project, &i.id).await?;
        if existing["body"].as_str() != Some(body) {
            return Err("A task with this exact title already exists with a different body; choose a distinct title".into());
        }
        return Ok(i.id.clone());
    }
    let fs = fields(project).await?;
    let (sid, opts) = status_field(&fs)?;
    agent_field(&fs)?;
    let created = github::graphql(
        "mutation($p:ID!,$t:String!,$b:String!){addProjectV2DraftIssue(input:{projectId:$p,title:$t,body:$b}){projectItem{id}}}",
        json!({"p":project.id,"t":title,"b":body}),
    ).await;
    let d = match created {
        Ok(d) => d,
        Err(error) => {
            // Resolve a lost response before retrying creation, since duplicate titles are not safe.
            let refreshed = github::read_board(&project.id).await?;
            if let Some(existing) = refreshed.items.iter().find(|item| item.title == title) {
                let task = read_task(project, &existing.id).await?;
                if task["body"].as_str() == Some(body) {
                    return Ok(existing.id.clone());
                }
                return Err("A task with this exact title exists with a different body; choose a distinct title".into());
            }
            return Err(error);
        }
    };
    let id = req(&d["addProjectV2DraftIssue"]["projectItem"], "id")?;
    github::graphql("mutation($p:ID!,$i:ID!,$f:ID!,$o:String!){updateProjectV2ItemFieldValue(input:{projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$o}}){projectV2Item{id}}}",json!({"p":project.id,"i":id,"f":sid,"o":option_id(&opts,"Todo")?})).await?;
    Ok(id)
}
pub async fn read_task(project: &RoomProject, task_id: &str) -> Result<Value, String> {
    let d=github::graphql("query($id:ID!){node(id:$id){... on ProjectV2Item{project{id} content{__typename ... on Issue{id title body url} ... on PullRequest{id title body url} ... on DraftIssue{id title body}}}}}",json!({"id":task_id})).await?;
    parse_task(&project.id, task_id, &d["node"])
}
fn parse_task(project_id: &str, task_id: &str, item: &Value) -> Result<Value, String> {
    if req(&item["project"], "id")? != project_id {
        return Err("Task does not belong to the selected GitHub Project".into());
    }
    let content = &item["content"];
    if content.is_null() {
        return Err("Task content is inaccessible or has been deleted".into());
    }
    let kind = content["__typename"]
        .as_str()
        .ok_or("Task content type missing")?;
    if !matches!(kind, "Issue" | "PullRequest" | "DraftIssue") {
        return Err("Unsupported task content type".into());
    }
    Ok(
        json!({"id":task_id,"content_id":req(content,"id")?,"title":req(content,"title")?,"body":content["body"].as_str().unwrap_or(""),"url":content["url"].as_str(),"kind":kind}),
    )
}
pub async fn record_completion(
    project: &RoomProject,
    task_id: &str,
    summary: &str,
    evidence: &str,
) -> Result<(), String> {
    let _guard = lock_project(&project.id).await;
    let summary = bounded(summary, 4000, "completion summary")?;
    let evidence = bounded(evidence, 8000, "completion evidence")?;
    let marker = completion_marker(&summary, &evidence);
    let note = format!(
        "## Completion evidence\n\n**Summary**\n{}\n\n**Evidence**\n{}\n\n{}",
        summary, evidence, marker
    );
    let task = read_task(project, task_id).await?;
    match task["kind"].as_str().unwrap_or("") {
        "DraftIssue" => {
            let original = task["body"].as_str().unwrap_or("").to_owned();
            if has_completion_marker(&original, &marker)? {
                return Ok(());
            }
            if has_any_completion_marker(&original) {
                return Err(
                    "task already contains different completion evidence; refusing to overwrite it"
                        .into(),
                );
            }
            let body = append_note(&original, &note);
            let current = read_task(project, task_id).await?;
            if current["body"].as_str().unwrap_or("") != original {
                return Err("task body changed while preparing completion evidence; retry after reviewing it".into());
            }
            match github::graphql("mutation($id:ID!,$body:String!){updateProjectV2DraftIssue(input:{draftIssueId:$id,body:$body}){draftIssue{id}}}",json!({"id":task["content_id"],"body":body})).await{
                Ok(_)=>Ok(()),
                Err(error)=>{let latest=read_task(project,task_id).await?;if has_completion_marker(latest["body"].as_str().unwrap_or(""),&marker)?{Ok(())}else{Err(error)}}
            }
        }
        "Issue" | "PullRequest" => {
            let subject_id = task["content_id"]
                .as_str()
                .ok_or("task content id missing")?;
            if find_completion_comment(subject_id, &marker).await? {
                return Ok(());
            }
            match github::graphql("mutation($id:ID!,$body:String!){addComment(input:{subjectId:$id,body:$body}){commentEdge{node{id}}}}",json!({"id":subject_id,"body":note})).await{
                Ok(_)=>Ok(()),
                Err(error)=>if find_completion_comment(subject_id,&marker).await?{Ok(())}else{Err(error)}
            }
        }
        _ => Err("unsupported task content type for completion evidence".into()),
    }
}
fn bounded(value: &str, limit: usize, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > limit {
        return Err(format!("{label} must contain 1–{limit} bytes"));
    }
    Ok(value.to_owned())
}
fn completion_marker(summary: &str, evidence: &str) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    digest.update(summary.as_bytes());
    digest.update([0]);
    digest.update(evidence.as_bytes());
    format!("<!-- opencovibe-completion:v1:{:x} -->", digest.finalize())
}
fn append_note(body: &str, note: &str) -> String {
    if body.trim().is_empty() {
        note.into()
    } else {
        format!("{}\n\n---\n\n{}", body.trim_end(), note)
    }
}
fn has_any_completion_marker(text: &str) -> bool {
    text.contains("<!-- opencovibe-completion:v1:")
}
fn has_completion_marker(text: &str, marker: &str) -> Result<bool, String> {
    if has_any_completion_marker(text) && !text.contains(marker) {
        return Err(
            "task already contains different completion evidence; refusing to overwrite it".into(),
        );
    }
    Ok(text.contains(marker))
}
async fn find_completion_comment(subject_id: &str, marker: &str) -> Result<bool, String> {
    let mut after: Option<String> = None;
    let mut found_other = false;
    for _ in 0..100 {
        let data=github::graphql("query($id:ID!,$after:String){node(id:$id){... on Issue{comments(first:100,after:$after){nodes{body}pageInfo{hasNextPage endCursor}}} ... on PullRequest{comments(first:100,after:$after){nodes{body}pageInfo{hasNextPage endCursor}}}}}",json!({"id":subject_id,"after":after})).await?;
        let comments = &data["node"]["comments"];
        let nodes = comments["nodes"]
            .as_array()
            .ok_or("GitHub comment list unavailable")?;
        if nodes.iter().any(|comment| {
            comment["body"]
                .as_str()
                .is_some_and(|body| body.contains(marker))
        }) {
            return Ok(true);
        }
        found_other |= nodes.iter().any(|comment| {
            comment["body"]
                .as_str()
                .is_some_and(has_any_completion_marker)
        });
        if comments["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            return if found_other {
                Err("task already has different completion evidence; refusing to add another record".into())
            } else {
                Ok(false)
            };
        }
        let cursor = comments["pageInfo"]["endCursor"]
            .as_str()
            .ok_or("GitHub comment pagination cursor missing")?
            .to_owned();
        if after.as_deref() == Some(&cursor) {
            return Err("GitHub comment pagination did not advance".into());
        }
        after = Some(cursor);
    }
    Err("GitHub completion comment lookup exceeded its pagination limit".into())
}
pub async fn lookup_project(repository: &str, number: u64) -> Result<RoomProject, String> {
    let (owner, _) = github::repository_parts(repository)?;
    let r = github::graphql(
        "query($o:String!,$n:String!){repository(owner:$o,name:$n){owner{__typename id login}}}",
        json!({"o":owner,"n":repository.split_once('/').unwrap().1}),
    )
    .await?;
    let own = &r["repository"]["owner"];
    let id = req(own, "id")?;
    let login = req(own, "login")?;
    let d=github::graphql("query($id:ID!,$n:Int!){node(id:$id){... on User{projectV2(number:$n){id number title url}} ... on Organization{projectV2(number:$n){id number title url}}}}",json!({"id":id,"n":number})).await?;
    let p = &d["node"]["projectV2"];
    let mut project = RoomProject {
        id: req(p, "id")?,
        number: p["number"].as_u64().ok_or("Project number missing")?,
        title: req(p, "title")?,
        url: req(p, "url")?,
        owner: login,
        repository: repository.into(),
    };
    if project.number != number {
        return Err("GitHub returned a different Project number".into());
    }
    project.repository = repository.into();
    Ok(project)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_aliases_and_fields_are_bounded() {
        assert_eq!(status_target("blocked").unwrap(), "In Progress");
        assert_eq!(status_target("complete").unwrap(), "Done");
        assert_eq!(status_target("ready").unwrap(), "Todo");
        assert!(status_target("unknown").is_err());
        let f = vec![
            json!({"id":"s","name":"Status","options":[{"id":"1","name":"Ready"},{"id":"2","name":"Doing"},{"id":"3","name":"Complete"}]}),
            json!({"id":"a","name":"Agent","__typename":"ProjectV2Field"}),
        ];
        assert_eq!(status_field(&f).unwrap().0, "s");
        let bad = vec![
            json!({"id":"s","name":"Status","options":[{"id":"1","name":"Todo"},{"id":"2","name":"In Progress"}]}),
            json!({"id":"a","name":"Agent","__typename":"ProjectV2Field"}),
        ];
        assert!(status_field(&bad).is_err());
        let f = vec![
            json!({"id":"s","name":"Status","options":[{"id":"1","name":"Todo"},{"id":"2","name":"In Progress"},{"id":"3","name":"Done"}]}),
            json!({"id":"a","name":"Agent","__typename":"ProjectV2Field"}),
        ];
        assert_eq!(agent_field(&f).unwrap(), "a");
        assert_eq!(
            option_id(&f[0]["options"].as_array().unwrap(), "Todo").unwrap(),
            "1"
        );
    }
    #[test]
    fn task_parser_checks_project_and_returns_body_for_each_supported_kind() {
        let item = json!({"project":{"id":"P1"},"content":{"id":"C1","__typename":"DraftIssue","title":"Task","body":"instructions"}});
        let parsed = parse_task("P1", "I1", &item).unwrap();
        assert_eq!(parsed["title"], "Task");
        assert_eq!(parsed["body"], "instructions");
        assert_eq!(parsed["kind"], "DraftIssue");
        assert!(parse_task("P2", "I1", &item)
            .unwrap_err()
            .contains("does not belong"));
        assert!(parse_task("P1", "I1", &json!({"project":{"id":"P1"},"content":null})).is_err());
    }
    #[test]
    fn completion_marker_is_stable_and_existing_body_is_preserved() {
        let a = completion_marker("summary", "evidence");
        assert_eq!(a, completion_marker("summary", "evidence"));
        assert_ne!(a, completion_marker("summary", "other"));
        let body = append_note("existing user content", &a);
        assert!(body.starts_with("existing user content\n\n---"));
        assert!(has_completion_marker(&body, &a).unwrap());
        assert!(has_completion_marker(&body, &completion_marker("different", "proof")).is_err());
    }
}
