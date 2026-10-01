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
        .find(|f| {
            f["name"]
                .as_str()
                .is_some_and(|n| norm(n) == "remediationstatus")
        })
        .or_else(|| {
            fs.iter()
                .find(|f| f["name"].as_str().is_some_and(|n| norm(n) == "status"))
        })
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
pub async fn set_task_metadata(
    project: &RoomProject,
    task_id: &str,
    agent: Option<&str>,
    priority: Option<&str>,
) -> Result<(), String> {
    let _guard = lock_project(&project.id).await;
    let fs = fields(project).await?;
    read_task(project, task_id).await?;
    if let Some(agent) = agent {
        let agent = bounded(agent, 200, "assigned agent")?;
        let aid = agent_field(&fs)?;
        github::graphql("mutation($p:ID!,$i:ID!,$f:ID!,$v:String!){updateProjectV2ItemFieldValue(input:{projectId:$p,itemId:$i,fieldId:$f,value:{text:$v}}){projectV2Item{id}}}",json!({"p":project.id,"i":task_id,"f":aid,"v":agent})).await?;
    }
    if let Some(priority) = priority {
        let field = fs
            .iter()
            .find(|f| {
                f["name"]
                    .as_str()
                    .is_some_and(|name| norm(name) == "priority")
                    && f["options"].is_array()
            })
            .ok_or("Project has no single-select Priority field")?;
        let option = field["options"]
            .as_array()
            .and_then(|options| {
                options.iter().find(|option| {
                    option["name"]
                        .as_str()
                        .is_some_and(|name| norm(name) == norm(priority))
                })
            })
            .and_then(|option| option["id"].as_str())
            .ok_or("Requested Priority option is not available on this Project")?;
        let field_id = req(field, "id")?;
        github::graphql("mutation($p:ID!,$i:ID!,$f:ID!,$o:String!){updateProjectV2ItemFieldValue(input:{projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$o}}){projectV2Item{id}}}",json!({"p":project.id,"i":task_id,"f":field_id,"o":option})).await?;
    }
    Ok(())
}
pub async fn validate_task_metadata(
    project: &RoomProject,
    agent: Option<&str>,
    priority: Option<&str>,
) -> Result<(), String> {
    if let Some(agent) = agent {
        bounded(agent, 200, "assigned agent")?;
    }
    let _guard = lock_project(&project.id).await;
    prepare_project_locked(project).await?;
    if let Some(priority) = priority {
        let fs = fields(project).await?;
        let field = fs
            .iter()
            .find(|f| {
                f["name"]
                    .as_str()
                    .is_some_and(|name| norm(name) == "priority")
                    && f["options"].is_array()
            })
            .ok_or("Project has no single-select Priority field")?;
        if !field["options"].as_array().is_some_and(|options| {
            options.iter().any(|option| {
                option["name"]
                    .as_str()
                    .is_some_and(|name| norm(name) == norm(priority))
            })
        }) {
            return Err("Requested Priority option is not available on this Project".into());
        }
    }
    Ok(())
}
pub async fn validate_creation_input(
    project: &RoomProject,
    body: &str,
    agent: Option<&str>,
    priority: Option<&str>,
) -> Result<(), String> {
    validate_task_body(body)?;
    github::require_repository_issues(&project.repository).await?;
    validate_task_metadata(project, agent, priority).await
}
pub async fn create_task(
    project: &RoomProject,
    title: &str,
    body: &str,
    allow_create: bool,
) -> Result<String, String> {
    let _guard = lock_project(&project.id).await;
    prepare_project_locked(project).await?;
    validate_task_body(body)?;
    let marker = creation_marker(&project.id, title, body);
    let issue_body = format!("{}\n\n{}", body.trim_end(), marker);
    let board = github::read_board(&project.id).await?;
    if let Some(i) = board.items.iter().find(|i| i.title == title) {
        let existing = read_task(project, &i.id).await?;
        if !existing["body"]
            .as_str()
            .is_some_and(|body| body.contains(&marker))
        {
            return Err("A task with this exact title already exists with a different body; choose a distinct title".into());
        }
        if i.status == "Backlog" {
            let fs = fields(project).await?;
            let (sid, options) = status_field(&fs)?;
            github::graphql("mutation($p:ID!,$i:ID!,$f:ID!,$o:String!){updateProjectV2ItemFieldValue(input:{projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$o}}){projectV2Item{id}}}",json!({"p":project.id,"i":i.id,"f":sid,"o":option_id(&options,"Todo")?})).await?;
        }
        return Ok(i.id.clone());
    }
    let fs = fields(project).await?;
    let (sid, opts) = status_field(&fs)?;
    agent_field(&fs)?;
    // The Issue is the durable identity. If linking it to the Project fails, retry
    // must discover this Issue before issuing another createIssue mutation.
    let issue_id = match find_room_issue(project, title, &marker).await? {
        Some(id) => id,
        None => {
            if !allow_create {
                return Err("A prior Issue creation is unconfirmed. No duplicate was created. Inspect repository Issues for this title and reconcile the existing intent before creating a replacement.".into());
            }
            let (repo_id, _) = github::repository_ids(&project.repository).await?;
            match github::graphql(
                "mutation($r:ID!,$p:ID!,$t:String!,$b:String!){createIssue(input:{repositoryId:$r,projectV2Ids:[$p],title:$t,body:$b}){issue{id}}}",
                json!({"r":repo_id,"p":project.id,"t":title,"b":issue_body}),
            ).await {
                Ok(d) => req(&d["createIssue"]["issue"], "id")?,
                Err(error) => find_room_issue(project, title, &marker).await?.ok_or_else(|| format!("Issue creation is unconfirmed: {error}. Retry the same task to reconcile; no replacement was created."))?,
            }
        }
    };
    let linked_board = github::read_board(&project.id).await?;
    let linked_item = linked_board
        .items
        .iter()
        .find(|i| i.title == title && i.body.as_deref().is_some_and(|body| body.contains(&marker)));
    let id = if let Some(item) = linked_item {
        item.id.clone()
    } else {
        let linked = github::graphql("mutation($p:ID!,$c:ID!){addProjectV2ItemById(input:{projectId:$p,contentId:$c}){item{id}}}",json!({"p":project.id,"c":issue_id})).await;
        match linked {
            Ok(d) => req(&d["addProjectV2ItemById"]["item"], "id")?,
            Err(error) => {
                let board = github::read_board(&project.id).await?;
                let found = board.items.iter().find(|i| {
                    i.title == title && i.body.as_deref().is_some_and(|body| body.contains(&marker))
                });
                match found { Some(i) => i.id.clone(), None => return Err(format!("Issue exists but Project linking is unconfirmed: {error}. Retry the same task to reconcile.")) }
            }
        }
    };
    github::graphql("mutation($p:ID!,$i:ID!,$f:ID!,$o:String!){updateProjectV2ItemFieldValue(input:{projectId:$p,itemId:$i,fieldId:$f,value:{singleSelectOptionId:$o}}){projectV2Item{id}}}",json!({"p":project.id,"i":id,"f":sid,"o":option_id(&opts,"Todo")?})).await?;
    Ok(id)
}

pub fn validate_task_body(body: &str) -> Result<(), String> {
    for section in [
        "Outcome",
        "Work",
        "Acceptance criteria",
        "Progress",
        "References",
    ] {
        let heading = format!("## {section}");
        let mut in_section = false;
        let mut nonempty = false;
        for line in body.lines() {
            if line.trim() == heading {
                in_section = true;
                continue;
            }
            if in_section && line.starts_with("## ") {
                break;
            }
            if in_section && !line.trim().is_empty() {
                nonempty = true;
            }
        }
        if !nonempty {
            return Err(format!("task body needs a nonempty '## {section}' section"));
        }
    }
    if body.len() < 160 {
        return Err(
            "task body needs enough detail to guide and track the work (at least 160 bytes)".into(),
        );
    }
    Ok(())
}

fn creation_marker(project_id: &str, title: &str, body: &str) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    for part in [project_id, title, body] {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    format!("<!-- opencovibe-task:v1:{:x} -->", digest.finalize())
}

async fn find_room_issue(
    project: &RoomProject,
    title: &str,
    marker: &str,
) -> Result<Option<String>, String> {
    let (owner, name) = github::repository_parts(&project.repository)?;
    let mut after: Option<String> = None;
    let mut matching_title_without_marker = false;
    for _ in 0..100 {
        let data = github::graphql("query($o:String!,$n:String!,$a:String){repository(owner:$o,name:$n){issues(first:100,after:$a,states:[OPEN,CLOSED],orderBy:{field:CREATED_AT,direction:DESC}){nodes{id title body} pageInfo{hasNextPage endCursor}}}}",json!({"o":owner,"n":name,"a":after})).await?;
        let issues = &data["repository"]["issues"];
        let nodes = issues["nodes"]
            .as_array()
            .ok_or("Repository issues are unavailable for task reconciliation")?;
        if let Some(issue) = nodes
            .iter()
            .find(|v| v["body"].as_str().is_some_and(|body| body.contains(marker)))
        {
            return Ok(Some(req(issue, "id")?));
        }
        matching_title_without_marker |= nodes.iter().any(|v| v["title"].as_str() == Some(title));
        if issues["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            return if matching_title_without_marker {
                Err("An Issue with this title exists without the room creation marker; inspect it before creating a replacement".into())
            } else {
                Ok(None)
            };
        }
        let next = req(&issues["pageInfo"], "endCursor")?;
        if after.as_deref() == Some(&next) {
            return Err("Issue pagination did not advance".into());
        }
        after = Some(next);
    }
    Err("Issue reconciliation exceeded 10000 issues; refusing duplicate creation".into())
}
pub async fn read_task(project: &RoomProject, task_id: &str) -> Result<Value, String> {
    let d=github::graphql("query($id:ID!){node(id:$id){... on ProjectV2Item{project{id} content{__typename ... on Issue{id title body url} ... on PullRequest{id title body url} ... on DraftIssue{id title body}}}}}",json!({"id":task_id})).await?;
    parse_task(&project.id, task_id, &d["node"])
}

pub async fn read_task_with_progress(
    project: &RoomProject,
    task_id: &str,
) -> Result<Value, String> {
    let mut task = read_task(project, task_id).await?;
    if task["kind"] == "Issue" {
        let subject_id = req(&task, "content_id")?;
        let data = github::graphql("query($id:ID!){node(id:$id){... on Issue{comments(last:50){nodes{body url createdAt author{login}}}}}}",json!({"id":subject_id})).await?;
        task["progress_updates"] =
            json!(parse_progress_updates(&data["node"]["comments"]["nodes"]));
    }
    Ok(task)
}

fn parse_progress_updates(nodes: &Value) -> Vec<Value> {
    nodes.as_array().map(|nodes| nodes.iter().filter(|node| node["body"].as_str().is_some_and(|body| body.contains("<!-- opencovibe-progress:v1:"))).map(|node| json!({"body":node["body"],"url":node["url"],"created_at":node["createdAt"],"author":node["author"]["login"]})).collect()).unwrap_or_default()
}

pub async fn record_progress(
    project: &RoomProject,
    task_id: &str,
    progress: &str,
    evidence: &str,
    references: &[String],
) -> Result<(), String> {
    let _guard = lock_project(&project.id).await;
    let progress = bounded(progress, 4000, "progress")?;
    let evidence = bounded(evidence, 8000, "evidence")?;
    if references.len() > 30
        || references
            .iter()
            .any(|r| r.trim().is_empty() || r.len() > 500)
    {
        return Err("provide at most 30 nonempty references, each at most 500 bytes".into());
    }
    let task = read_task(project, task_id).await?;
    if task["kind"] != "Issue" {
        return Err("progress updates require a repository Issue; convert a draft first".into());
    }
    let subject = req(&task, "content_id")?;
    let refs = if references.is_empty() {
        "None yet".into()
    } else {
        references
            .iter()
            .map(|r| format!("- {}", r.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let marker = progress_marker(task_id, &progress, &evidence, references);
    let note = format!("## Progress update\n\n**Work completed / current state**\n{progress}\n\n**Evidence**\n{evidence}\n\n**Commits, PRs, assignments, or other references**\n{refs}\n\n{marker}");
    if find_comment_marker(&subject, &marker).await? {
        return Ok(());
    }
    match github::graphql("mutation($id:ID!,$body:String!){addComment(input:{subjectId:$id,body:$body}){commentEdge{node{id}}}}",json!({"id":subject,"body":note})).await {
        Ok(_) => Ok(()),
        Err(error) => if find_comment_marker(&subject, &marker).await? { Ok(()) } else { Err(error) },
    }
}

fn progress_marker(task_id: &str, progress: &str, evidence: &str, references: &[String]) -> String {
    use sha2::Digest;
    let mut digest = sha2::Sha256::new();
    for part in [task_id, progress, evidence] {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    for reference in references {
        digest.update(reference.as_bytes());
        digest.update([0]);
    }
    format!("<!-- opencovibe-progress:v1:{:x} -->", digest.finalize())
}

async fn find_comment_marker(subject_id: &str, marker: &str) -> Result<bool, String> {
    let mut after: Option<String> = None;
    for _ in 0..100 {
        let data=github::graphql("query($id:ID!,$after:String){node(id:$id){... on Issue{comments(first:100,after:$after){nodes{body}pageInfo{hasNextPage endCursor}}}}}",json!({"id":subject_id,"after":after})).await?;
        let comments = &data["node"]["comments"];
        let nodes = comments["nodes"]
            .as_array()
            .ok_or("Issue comments unavailable")?;
        if nodes.iter().any(|comment| {
            comment["body"]
                .as_str()
                .is_some_and(|body| body.contains(marker))
        }) {
            return Ok(true);
        }
        if comments["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            return Ok(false);
        }
        let next = req(&comments["pageInfo"], "endCursor")?;
        if after.as_deref() == Some(&next) {
            return Err("Issue comment pagination did not advance".into());
        }
        after = Some(next);
    }
    Err("Issue comment lookup exceeded pagination limit".into())
}

pub async fn convert_draft_task(
    project: &RoomProject,
    task_id: &str,
    body: &str,
) -> Result<(), String> {
    let _guard = lock_project(&project.id).await;
    validate_task_body(body)?;
    let task = read_task(project, task_id).await?;
    if task["kind"] == "Issue" {
        return if task["body"].as_str() == Some(body) {
            Ok(())
        } else {
            Err(
                "task is already an Issue with different content; inspect before changing it"
                    .into(),
            )
        };
    }
    if task["kind"] != "DraftIssue" {
        return Err("only Project draft issues can be converted".into());
    }
    if task["body"].as_str() != Some(body) {
        let content_id = req(&task, "content_id")?;
        let updated = github::graphql("mutation($id:ID!,$body:String!){updateProjectV2DraftIssue(input:{draftIssueId:$id,body:$body}){draftIssue{id}}}",json!({"id":content_id,"body":body})).await;
        if let Err(error) = updated {
            let latest = read_task(project, task_id).await?;
            if latest["body"].as_str() != Some(body) {
                return Err(error);
            }
        }
    }
    let (repository_id, _) = github::repository_ids(&project.repository).await?;
    match github::graphql("mutation($i:ID!,$r:ID!){convertProjectV2DraftIssueItemToIssue(input:{itemId:$i,repositoryId:$r}){item{id}}}",json!({"i":task_id,"r":repository_id})).await {
        Ok(_) => Ok(()),
        Err(error) => if read_task(project, task_id).await?["kind"] == "Issue" { Ok(()) } else { Err(error) },
    }
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
    fn remediation_workflow_updates_the_same_field_as_the_board_reader() {
        let fields = vec![
            json!({"id":"generic","name":"Status","options":[{"id":"todo","name":"To do"},{"id":"active","name":"In progress"},{"id":"shipped","name":"Already there"}]}),
            json!({"id":"remediation","name":"Remediation status","options":[{"id":"ready","name":"Ready"},{"id":"working","name":"In progress"},{"id":"review","name":"Review"},{"id":"done","name":"Done"}]}),
        ];
        let (field, options) = status_field(&fields).unwrap();
        assert_eq!(field, "remediation");
        assert_eq!(
            option_id(&options, status_target("ready").unwrap()).unwrap(),
            "ready"
        );
        assert_eq!(
            option_id(&options, status_target("active").unwrap()).unwrap(),
            "working"
        );
        assert_eq!(
            option_id(&options, status_target("done").unwrap()).unwrap(),
            "done"
        );

        // A malformed canonical workflow must not silently update the generic field.
        let fields = vec![
            json!({"id":"generic","name":"Status","options":[{"id":"1","name":"Todo"},{"id":"2","name":"Doing"},{"id":"3","name":"Done"}]}),
            json!({"id":"remediation","name":"Remediation status","options":[]}),
        ];
        assert!(status_field(&fields).is_err());
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
    #[test]
    fn new_tasks_require_useful_sections_and_have_stable_recovery_markers() {
        let body = "## Outcome\nA visible room Project Issue that explains the expected result.\n\n## Work\nImplement and review the changed behavior.\n\n## Acceptance criteria\nA focused check passes and the reviewer can inspect the evidence.\n\n## Progress\nReady to claim.\n\n## References\nNone yet.";
        assert!(validate_task_body(body).is_ok());
        assert!(validate_task_body("Task details").is_err());
        assert!(validate_task_body(&body.replace("Ready to claim.", "")).is_err());
        assert_eq!(
            creation_marker("P1", "Task", body),
            creation_marker("P1", "Task", body)
        );
        assert_ne!(
            creation_marker("P1", "Task", body),
            creation_marker("P2", "Task", body)
        );
        assert_ne!(
            progress_marker("I1", "work", "proof", &["PR #1".into()]),
            progress_marker("I1", "work", "proof", &["PR #2".into()])
        );
    }
    #[test]
    fn progress_payload_only_exposes_room_tracking_comments() {
        let comments = json!([
            {"body":"Unrelated discussion", "url":"https://example.test/1"},
            {"body":"## Progress update\nImplemented parser\n<!-- opencovibe-progress:v1:abc -->", "url":"https://example.test/2", "createdAt":"2026-10-01T00:00:00Z", "author":{"login":"coder"}}
        ]);
        let history = parse_progress_updates(&comments);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0]["author"], "coder");
        assert_eq!(history[0]["url"], "https://example.test/2");
        assert!(parse_progress_updates(&Value::Null).is_empty());
    }
}
