use super::{
    models::{AddParticipantInput, CreateRequestInput, Message, Room, RoomRequest},
    operations,
    store::RoomStore,
};

const MAX_REQUESTS: usize = 200;
const UNRESOLVED: &[&str] = &["pending", "creating", "changes_requested", "verified"];

pub fn create_request(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
    input: CreateRequestInput,
) -> Result<Room, String> {
    let room = store.get(room_id)?;
    active_writer(&room, peer_id)?;
    let request = normalized_request(peer_id, input)?;

    store.update(room_id, |room| {
        active_writer(room, peer_id)?;
        if let Some(existing) = room.requests.iter().find(|candidate| {
            candidate.requester_id == peer_id
                && candidate.kind == request.kind
                && candidate.title == request.title
        }) {
            if same_request_content(existing, &request) {
                return Ok(());
            }
            return Err("a request with this title already exists with different content".into());
        }
        validate_request_kind(room, peer_id, &request)?;
        if room.requests.len() >= MAX_REQUESTS {
            return Err("room request limit reached".into());
        }
        if request.kind == "completion" {
            validate_completion_gate(room, None, false)?;
        }
        let mut request = request.clone();
        request.id = uuid::Uuid::new_v4().to_string();
        request.created_at = crate::models::now_iso();
        request.updated_at = request.created_at.clone();
        if request.kind == "completion" {
            request.work_signature = Some(work_signature(room)?);
        }
        let requester_name = room
            .participants
            .iter()
            .find(|peer| peer.id == peer_id)
            .map(|peer| peer.name.clone())
            .ok_or("participant not found")?;
        let target = request.reviewer_id.clone();
        let body = format_request_created(&request);
        room.requests.push(request.clone());
        push_message(
            room,
            &requester_name,
            body,
            Some(peer_id),
            target.as_deref(),
        );
        Ok(())
    })
}

pub fn respond_request(
    store: &RoomStore,
    room_id: &str,
    peer_id: &str,
    request_id: &str,
    approve: bool,
    response: &str,
) -> Result<Room, String> {
    let response = bounded(response, 8000, "response")?;
    let initial = store.get(room_id)?;
    active_writer(&initial, peer_id)?;
    let peer = operations::active_peer(&initial, peer_id)?;
    store.update(room_id, |room| {
        active_writer(room, peer_id)?;
        let index = room
            .requests
            .iter()
            .position(|request| request.id == request_id)
            .ok_or("request not found")?;
        let request = room.requests[index].clone();
        if !matches!(request.kind.as_str(), "review" | "completion") {
            return Err("only review and completion requests accept peer responses".into());
        }
        if request.reviewer_id.as_deref() != Some(peer_id) {
            return Err("only the appointed reviewer may respond to this request".into());
        }
        let target_status = match (request.kind.as_str(), approve) {
            ("review", true) => "approved",
            ("completion", true) => "verified",
            (_, false) => "changes_requested",
            _ => unreachable!(),
        };
        if request.status == target_status
            && request.response.as_deref() == Some(response.as_str())
            && request.resolved_by.as_deref() == Some(peer_id)
        {
            return Ok(());
        }
        if request.status != "pending" {
            return Err("request is no longer awaiting a reviewer response".into());
        }
        if request.kind == "completion" && approve {
            let captured = request
                .work_signature
                .as_deref()
                .ok_or("completion request has no work snapshot; create a new request")?;
            if work_signature(room)? != captured {
                return Err("board or claim state changed; create a new completion request".into());
            }
            validate_completion_gate(room, Some(request_id), false)?;
        }
        {
            let request = &mut room.requests[index];
            request.status = target_status.into();
            request.response = Some(response.clone());
            request.resolved_by = Some(peer_id.into());
            request.review_response = Some(response.clone());
            request.reviewed_by = Some(peer_id.into());
            request.reviewed_at = Some(crate::models::now_iso());
            request.updated_at = crate::models::now_iso();
        }
        let body = format!(
            "{} request {} {} by {}. Evidence: {}",
            request.kind,
            request.title,
            if approve { "approved" } else { "needs changes" },
            peer.name,
            response
        );
        push_message(
            room,
            &peer.name,
            body,
            Some(peer_id),
            Some(&request.requester_id),
        );
        Ok(())
    })
}

pub fn resolve_request(
    store: &RoomStore,
    room_id: &str,
    request_id: &str,
    approve: bool,
    response: &str,
) -> Result<Room, String> {
    let response = bounded(response, 8000, "response")?;
    store.update(room_id, |room| {
        let request_index = room
            .requests
            .iter()
            .position(|request| request.id == request_id)
            .ok_or("request not found")?;
        let request = room.requests[request_index].clone();
        let target_status = match (request.kind.as_str(), approve) {
            ("agent", true) => "creating",
            ("decision", true) => "approved",
            ("completion", true) => "accepted",
            (_, true) => return Err("human approval cannot replace peer review".into()),
            (_, false) => "rejected",
        };
        if request.status == target_status
            && request.response.as_deref() == Some(response.as_str())
            && request.resolved_by.as_deref() == Some("Human")
        {
            return Ok(());
        }
        if room.archived {
            return Err("room is archived".into());
        }
        let can_resolve = match request.kind.as_str() {
            "agent" if approve => request.status == "pending",
            "agent" => {
                matches!(request.status.as_str(), "pending" | "creating")
                    && !room.participants.iter().any(|peer| peer.id == request.id)
            }
            "decision" => request.status == "pending",
            "review" => matches!(request.status.as_str(), "pending" | "changes_requested"),
            "completion" if approve => request.status == "verified",
            "completion" => matches!(
                request.status.as_str(),
                "pending" | "verified" | "changes_requested"
            ),
            _ => false,
        };
        if !can_resolve {
            return Err("request is not in a state that can be resolved".into());
        }
        if request.kind == "completion" && approve {
            if request.reviewed_by.as_deref() != request.reviewer_id.as_deref()
                || request.review_response.as_deref().is_none_or(str::is_empty)
                || request.reviewed_at.is_none()
            {
                return Err("completion has no valid peer verification".into());
            }
            if !room.requests[request_index]
                .work_signature
                .as_deref()
                .is_some_and(|signature| work_signature(room).is_ok_and(|now| now == signature))
            {
                return Err("board or claim state changed; create a new completion request".into());
            }
            validate_completion_gate(room, Some(request_id), true)?;
        }
        {
            let request = &mut room.requests[request_index];
            request.status = target_status.into();
            request.response = Some(response.clone());
            request.resolved_by = Some("Human".into());
            request.updated_at = crate::models::now_iso();
        }
        if request.kind == "completion" && approve {
            room.archived = true;
            room.paused = true;
        }
        let target = room.requests[request_index].requester_id.clone();
        let body = format!(
            "{} request {} {} by Human. Response: {}",
            request.kind,
            request.title,
            if approve { "approved" } else { "rejected" },
            response
        );
        push_message(room, "Human", body, None, Some(&target));
        Ok(())
    })
}

pub fn record_agent_approval(
    store: &RoomStore,
    room_id: &str,
    request_id: &str,
    participant_id: &str,
) -> Result<Room, String> {
    store.update(room_id, |room| {
        if room.archived {
            return Err("room is archived".into());
        }
        let index = room
            .requests
            .iter()
            .position(|request| request.id == request_id)
            .ok_or("request not found")?;
        let request = room.requests[index].clone();
        if request.kind != "agent" {
            return Err("request is not an agent proposal".into());
        }
        if request.id != participant_id {
            return Err("approved participant id must match the request id".into());
        }
        let participant = room
            .participants
            .iter()
            .find(|participant| participant.id == participant_id)
            .ok_or("approved participant has not been added to the room")?;
        if !participant.paused {
            return Err("new participant must remain paused until approval is recorded".into());
        }
        let proposal = request
            .proposal
            .as_ref()
            .ok_or("agent request has no approved participant proposal")?;
        if participant.name != proposal.name
            || participant.provider != proposal.provider
            || participant.model != proposal.model
            || participant.effort != proposal.effort
            || participant.max_turns != proposal.max_turns
            || participant.brief != request.brief
            || participant.worktree_path.is_some() != proposal.use_worktree
            || participant.branch.is_some() != proposal.use_worktree
        {
            return Err("created participant does not match the approved proposal".into());
        }
        if request.status == "approved"
            && request.approved_participant_id.as_deref() == Some(participant_id)
        {
            return Ok(());
        }
        if request.status != "creating" {
            return Err("agent proposal is not being created".into());
        }
        let participant_name = participant.name.clone();
        {
            let request = &mut room.requests[index];
            request.status = "approved".into();
            request.approved_participant_id = Some(participant_id.into());
            request.resolved_by = Some("Human".into());
            request.updated_at = crate::models::now_iso();
        }
        let message = format!(
            "Agent request {} approved; participant {} was added paused.",
            request.title, participant_name
        );
        push_message(room, "Human", message, None, Some(&request.requester_id));
        Ok(())
    })
}

fn normalized_request(
    requester_id: &str,
    input: CreateRequestInput,
) -> Result<RoomRequest, String> {
    let kind = input.kind.trim().to_owned();
    let title = bounded(&input.title, 200, "request title")?;
    let body = bounded(&input.body, 8000, "request body")?;
    let evidence = input
        .evidence
        .as_deref()
        .map(|value| bounded(value, 8000, "request evidence"))
        .transpose()?;
    let brief = input
        .brief
        .as_deref()
        .map(|value| bounded(value, 4000, "agent brief"))
        .transpose()?;
    let task_id = input
        .task_id
        .as_deref()
        .map(|value| bounded(value, 200, "task id"))
        .transpose()?;
    let reviewer_id = input
        .reviewer_id
        .as_deref()
        .map(|value| bounded(value, 200, "reviewer id"))
        .transpose()?;
    let proposal = input.proposal.map(normalize_proposal);
    let options = input
        .options
        .into_iter()
        .map(|option| bounded(&option, 200, "decision option"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RoomRequest {
        id: String::new(),
        kind,
        requester_id: requester_id.to_owned(),
        title,
        body,
        evidence,
        task_id,
        reviewer_id,
        proposal,
        brief,
        options,
        status: "pending".into(),
        response: None,
        resolved_by: None,
        approved_participant_id: None,
        work_signature: None,
        review_response: None,
        reviewed_by: None,
        reviewed_at: None,
        created_at: String::new(),
        updated_at: String::new(),
    })
}

fn normalize_proposal(mut proposal: AddParticipantInput) -> AddParticipantInput {
    proposal.name = proposal.name.trim().to_owned();
    proposal.provider = proposal.provider.trim().to_owned();
    proposal.model = proposal.model.map(|value| value.trim().to_owned());
    proposal.effort = proposal.effort.map(|value| value.trim().to_owned());
    proposal
}

fn validate_request_kind(
    room: &Room,
    requester_id: &str,
    request: &RoomRequest,
) -> Result<(), String> {
    if !matches!(
        request.kind.as_str(),
        "agent" | "decision" | "review" | "completion"
    ) {
        return Err("request kind must be agent, decision, review, or completion".into());
    }
    if !room.participants.iter().any(|peer| peer.id == requester_id) {
        return Err("participant not found".into());
    }
    match request.kind.as_str() {
        "agent" => {
            reject_present(&[
                (request.evidence.is_some(), "evidence"),
                (request.task_id.is_some(), "task_id"),
                (request.reviewer_id.is_some(), "reviewer_id"),
                (!request.options.is_empty(), "options"),
            ])?;
            let proposal = request
                .proposal
                .as_ref()
                .ok_or("agent request needs a proposal")?;
            validate_proposal(proposal)?;
            if room
                .participants
                .iter()
                .any(|peer| peer.name.eq_ignore_ascii_case(proposal.name.trim()))
            {
                return Err("participant name is already used in this room".into());
            }
            request
                .brief
                .as_ref()
                .ok_or("agent request needs a brief")?;
        }
        "decision" => {
            reject_present(&[
                (request.task_id.is_some(), "task_id"),
                (request.reviewer_id.is_some(), "reviewer_id"),
                (request.proposal.is_some(), "proposal"),
                (request.brief.is_some(), "brief"),
            ])?;
            if request.options.len() > 5 {
                return Err("decision requests allow at most five options".into());
            }
        }
        "review" | "completion" => {
            reject_present(&[
                (request.proposal.is_some(), "proposal"),
                (request.brief.is_some(), "brief"),
                (!request.options.is_empty(), "options"),
            ])?;
            request
                .evidence
                .as_ref()
                .ok_or("review and completion requests need evidence")?;
            let reviewer_id = request
                .reviewer_id
                .as_deref()
                .ok_or("reviewer_id is required")?;
            if reviewer_id == requester_id
                || !room.participants.iter().any(|peer| peer.id == reviewer_id)
            {
                return Err("reviewer must be another participant in this room".into());
            }
        }
        _ => unreachable!(),
    }
    if let Some(task_id) = request.task_id.as_deref() {
        if !room.board.items.iter().any(|item| item.id == task_id) {
            return Err("task is not present on this room's board".into());
        }
        if !room
            .claims
            .iter()
            .any(|claim| claim.task_id == task_id && claim.participant_id == requester_id)
        {
            return Err("requester must own the task claim".into());
        }
    }
    Ok(())
}

fn validate_proposal(input: &AddParticipantInput) -> Result<(), String> {
    if input.name.trim().is_empty() || input.name.trim().len() > 80 {
        return Err("participant name must contain 1–80 bytes".into());
    }
    if !matches!(input.provider.as_str(), "claude" | "codex") {
        return Err("provider must be claude or codex".into());
    }
    if input.max_turns > 200 {
        return Err("turn limit must be between 1 and 200, or disabled".into());
    }
    if let Some(model) = input.model.as_deref() {
        let model = bounded(model, 200, "model")?;
        if model.chars().any(char::is_control) {
            return Err("model must be a valid nonempty name of at most 200 bytes".into());
        }
    }
    if let Some(effort) = input.effort.as_deref() {
        let allowed: &[&str] = if input.provider == "claude" {
            &["low", "medium", "high", "xhigh", "max"]
        } else {
            &["none", "minimal", "low", "medium", "high", "xhigh"]
        };
        if !allowed.contains(&effort) {
            return Err(format!("unsupported {} effort: {effort}", input.provider));
        }
    }
    Ok(())
}

fn reject_present(fields: &[(bool, &str)]) -> Result<(), String> {
    if let Some((_, field)) = fields.iter().find(|(present, _)| *present) {
        Err(format!("{field} is not valid for this request kind"))
    } else {
        Ok(())
    }
}

fn active_writer(room: &Room, peer_id: &str) -> Result<(), String> {
    let peer = operations::active_peer(room, peer_id)?;
    if peer.turn_limit_reached()
        && !peer
            .pending_delivery
            .as_ref()
            .is_some_and(|delivery| matches!(delivery.state.as_str(), "prepared" | "sent"))
    {
        return Err("participant turn budget is exhausted".into());
    }
    Ok(())
}

fn validate_completion_gate(
    room: &Room,
    current_id: Option<&str>,
    require_pause: bool,
) -> Result<(), String> {
    if room.project.is_some() && !board_is_fresh(room) {
        return Err(
            "refresh the GitHub Project board before requesting or accepting completion".into(),
        );
    }
    if room.board.items.iter().any(|item| !is_done(&item.status)) {
        return Err("all board tasks must be Done before completion".into());
    }
    if room
        .claims
        .iter()
        .any(|claim| !matches!(claim.state.as_str(), "done" | "released"))
    {
        return Err("all task claims must be done or released before completion".into());
    }
    if room.requests.iter().any(|request| {
        Some(request.id.as_str()) != current_id && UNRESOLVED.contains(&request.status.as_str())
    }) {
        return Err("resolve other pending room requests before completion".into());
    }
    if require_pause
        && (!room.paused
            || room.participants.iter().any(|peer| {
                peer.pending_delivery.is_some()
                    || matches!(peer.state.as_str(), "busy" | "waiting" | "running")
            }))
    {
        return Err(
            "pause the room and stop all busy participants and pending deliveries before acceptance"
                .into(),
        );
    }
    Ok(())
}

fn board_is_fresh(room: &Room) -> bool {
    if room.board.error.is_some() {
        return false;
    }
    room.board
        .synced_at
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|synced| {
            let age = chrono::Utc::now().timestamp_millis() - synced.timestamp_millis();
            (0..=120_000).contains(&age)
        })
}

fn is_done(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "done" | "complete" | "completed"
    )
}

fn work_signature(room: &Room) -> Result<String, String> {
    use sha2::Digest;
    let mut items = room.board.items.clone();
    items.sort_by(|left, right| left.id.cmp(&right.id));
    let mut claims = room.claims.iter().collect::<Vec<_>>();
    claims.sort_by(|left, right| {
        (&left.task_id, &left.participant_id, &left.state).cmp(&(
            &right.task_id,
            &right.participant_id,
            &right.state,
        ))
    });
    let mut participants = room.participants.iter().collect::<Vec<_>>();
    participants.sort_by(|left, right| left.id.cmp(&right.id));
    let value = serde_json::json!({
        "items": items.iter().map(|item| serde_json::json!({
            "id": item.id, "title": item.title, "url": item.url,
            "status": item.status, "priority": item.priority,
            "agent": item.agent, "kind": item.kind,
        })).collect::<Vec<_>>(),
        "claims": claims.iter().map(|claim| serde_json::json!({
            "task_id": claim.task_id, "participant_id": claim.participant_id,
            "state": claim.state, "summary": claim.summary, "evidence": claim.evidence,
        })).collect::<Vec<_>>(),
        "participants": participants.iter().map(|peer| serde_json::json!({
            "id": peer.id, "name": peer.name, "provider": peer.provider,
            "model": peer.model, "effort": peer.effort, "branch": peer.branch,
            "brief": peer.brief,
        })).collect::<Vec<_>>(),
    });
    let serialized = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", sha2::Sha256::digest(serialized)))
}

fn same_request_content(left: &RoomRequest, right: &RoomRequest) -> bool {
    left.evidence == right.evidence
        && left.task_id == right.task_id
        && left.reviewer_id == right.reviewer_id
        && left.proposal == right.proposal
        && left.brief == right.brief
        && left.options == right.options
        && left.body == right.body
}

fn bounded(value: &str, limit: usize, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > limit {
        return Err(format!("{label} must contain 1–{limit} bytes"));
    }
    Ok(value.to_owned())
}

fn push_message(
    room: &mut Room,
    sender: &str,
    body: String,
    participant_id: Option<&str>,
    target: Option<&str>,
) {
    room.messages.push(Message {
        id: uuid::Uuid::new_v4().to_string(),
        sender: sender.into(),
        body,
        created_at: crate::models::now_iso(),
        participant_id: participant_id.map(str::to_owned),
        target_participant_id: target.map(str::to_owned),
        target_participant_ids: vec![],
        source_event_id: None,
        sidechat_id: None,
        attachments: vec![],
    });
    super::runtime::wake_dormant_for_unread_messages(room);
}

fn format_request_created(request: &RoomRequest) -> String {
    let mut body = format!(
        "{} request: {}\n{}",
        request.kind, request.title, request.body
    );
    if let Some(evidence) = &request.evidence {
        body.push_str("\nEvidence: ");
        body.push_str(evidence);
    }
    if let Some(brief) = &request.brief {
        body.push_str("\nBrief: ");
        body.push_str(brief);
    }
    if !request.options.is_empty() {
        body.push_str("\nOptions: ");
        body.push_str(&request.options.join(" | "));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rooms::models::{
        Board, BoardItem, Claim, CreateRoomInput, Participant, RoomProject,
    };
    use std::sync::Barrier;

    struct Fixture {
        temp: tempfile::TempDir,
        store: RoomStore,
        room_id: String,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let store = RoomStore::open(&temp.path().join("rooms.sqlite3")).unwrap();
            let room = store
                .create(CreateRoomInput {
                    title: "Governance test".into(),
                    objective: "Review shared room decisions".into(),
                    repo_path: temp.path().to_string_lossy().into_owned(),
                    repository: "owner/repository".into(),
                    create_project: false,
                })
                .unwrap();
            store
                .update(&room.id, |room| {
                    room.paused = false;
                    room.participants =
                        vec![peer("requester", "Requester"), peer("reviewer", "Reviewer")];
                    Ok(())
                })
                .unwrap();
            Self {
                temp,
                store,
                room_id: room.id,
            }
        }

        fn add_project_board(&self, done: bool) {
            self.store
                .update(&self.room_id, |room| {
                    room.project = Some(RoomProject {
                        id: "project-id".into(),
                        number: 1,
                        title: "Project".into(),
                        url: "https://example.test/project".into(),
                        owner: "owner".into(),
                        repository: "owner/repository".into(),
                    });
                    room.board = Board {
                        items: vec![BoardItem {
                            id: "task-1".into(),
                            title: "Task one".into(),
                            url: None,
                            status: if done { "Done" } else { "In Progress" }.into(),
                            priority: None,
                            agent: None,
                            kind: "issue".into(),
                            body: None,
                            number: None,
                            assignees: vec![],
                            labels: vec![],
                            linked_prs: vec![],
                            updated_at: None,
                        }],
                        synced_at: Some(crate::models::now_iso()),
                        error: None,
                        total_count: 1,
                    };
                    Ok(())
                })
                .unwrap();
        }
    }

    fn peer(id: &str, name: &str) -> Participant {
        Participant {
            id: id.into(),
            name: name.into(),
            provider: "codex".into(),
            run_id: format!("run-{id}"),
            paused: false,
            max_turns: 10,
            state: "idle".into(),
            ..Participant::default()
        }
    }

    fn request(kind: &str, title: &str) -> CreateRequestInput {
        CreateRequestInput {
            kind: kind.into(),
            title: title.into(),
            body: "Please review this bounded item".into(),
            evidence: None,
            task_id: None,
            reviewer_id: None,
            proposal: None,
            brief: None,
            options: vec![],
        }
    }

    fn review_request() -> CreateRequestInput {
        CreateRequestInput {
            evidence: Some("Checked the changed files and acceptance notes".into()),
            reviewer_id: Some("reviewer".into()),
            ..request("review", "Review implementation")
        }
    }

    fn completion_request() -> CreateRequestInput {
        CreateRequestInput {
            kind: "completion".into(),
            title: "Room complete".into(),
            body: "All planned work is finished".into(),
            evidence: Some("Every project item is Done and evidence is recorded".into()),
            task_id: None,
            reviewer_id: Some("reviewer".into()),
            proposal: None,
            brief: None,
            options: vec![],
        }
    }

    #[test]
    fn review_request_wakes_automatic_dormancy_but_retains_manual_pause() {
        for state in ["blocked", "no_progress", "completed", "paused", "waiting"] {
            let fixture = Fixture::new();
            fixture
                .store
                .update(&fixture.room_id, |room| {
                    let reviewer = &mut room.participants[1];
                    reviewer.paused = true;
                    reviewer.state = state.into();
                    Ok(())
                })
                .unwrap();
            let room = create_request(
                &fixture.store,
                &fixture.room_id,
                "requester",
                review_request(),
            )
            .unwrap();
            let should_wake = matches!(state, "blocked" | "no_progress" | "completed");
            assert_eq!(room.participants[1].paused, !should_wake, "{state}");
            assert_eq!(room.requests[0].status, "pending");
            if should_wake {
                assert_eq!(
                    super::super::runtime::plan(
                        &room,
                        &room.participants[1],
                        chrono::Utc::now().timestamp_millis()
                    )
                    .unwrap()
                    .reason,
                    "message"
                );
            }
        }
    }

    #[test]
    fn permissions_self_review_and_identical_retry_are_enforced() {
        let fixture = Fixture::new();
        let mut self_review = review_request();
        self_review.reviewer_id = Some("requester".into());
        assert!(
            create_request(&fixture.store, &fixture.room_id, "requester", self_review).is_err()
        );

        let room = create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            review_request(),
        )
        .unwrap();
        let id = room.requests[0].id.clone();
        assert_eq!(room.messages.len(), 1);
        assert_eq!(
            room.messages[0].target_participant_id.as_deref(),
            Some("reviewer")
        );
        assert_eq!(
            create_request(
                &fixture.store,
                &fixture.room_id,
                "requester",
                review_request()
            )
            .unwrap()
            .requests
            .len(),
            1
        );

        let mut changed = review_request();
        changed.body.push_str(" with a different scope");
        assert!(create_request(&fixture.store, &fixture.room_id, "requester", changed).is_err());
        assert!(respond_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            &id,
            true,
            "Looks good after checking the artifact"
        )
        .is_err());
        assert!(create_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            review_request()
        )
        .is_err());
        let mut question = request("decision", "Question without preset choices");
        question.evidence = Some("Context for the open question".into());
        let question_room =
            create_request(&fixture.store, &fixture.room_id, "requester", question).unwrap();
        assert!(question_room
            .requests
            .iter()
            .any(|item| item.title == "Question without preset choices"));
    }

    #[test]
    fn reviewer_resolution_is_atomic_logged_and_repeatable() {
        let fixture = Fixture::new();
        let room = create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            review_request(),
        )
        .unwrap();
        let id = room.requests[0].id.clone();
        let response = "Compared the output with the stated requirements";
        let resolved = respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &id,
            true,
            response,
        )
        .unwrap();
        assert_eq!(resolved.requests[0].status, "approved");
        assert_eq!(resolved.messages.len(), 2);
        assert_eq!(
            resolved.messages[1].target_participant_id.as_deref(),
            Some("requester")
        );
        let repeated = respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &id,
            true,
            response,
        )
        .unwrap();
        assert_eq!(repeated.messages.len(), 2);
    }

    #[test]
    fn exhausted_peer_cannot_create_or_respond_without_pending_delivery() {
        let fixture = Fixture::new();
        let room = create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            review_request(),
        )
        .unwrap();
        let id = room.requests[0].id.clone();
        fixture
            .store
            .update(&fixture.room_id, |room| {
                let reviewer = room
                    .participants
                    .iter_mut()
                    .find(|peer| peer.id == "reviewer")
                    .unwrap();
                reviewer.wake_count = reviewer.max_turns;
                Ok(())
            })
            .unwrap();
        assert!(respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &id,
            true,
            "Inspected the evidence"
        )
        .is_err());
        fixture
            .store
            .update(&fixture.room_id, |room| {
                let reviewer = room
                    .participants
                    .iter_mut()
                    .find(|peer| peer.id == "reviewer")
                    .unwrap();
                reviewer.pending_delivery = Some(crate::rooms::models::Delivery {
                    id: "delivery".into(),
                    reason: "followup".into(),
                    text: "Continue".into(),
                    created_at: 1,
                    state: "queued".into(),
                    turn_started: false,
                    task_id: None,
                    timer_id: None,
                    sidechat_id: None,
                    message_id: None,
                    attachment_ids: vec![],
                });
                Ok(())
            })
            .unwrap();
        assert!(respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &id,
            true,
            "Inspected the evidence"
        )
        .is_err());
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.participants
                    .iter_mut()
                    .find(|peer| peer.id == "reviewer")
                    .unwrap()
                    .pending_delivery
                    .as_mut()
                    .unwrap()
                    .state = "prepared".into();
                Ok(())
            })
            .unwrap();
        assert!(respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &id,
            true,
            "Inspected the evidence"
        )
        .is_ok());

        let sent = Fixture::new();
        let request =
            create_request(&sent.store, &sent.room_id, "requester", review_request()).unwrap();
        sent.store
            .update(&sent.room_id, |room| {
                let reviewer = room
                    .participants
                    .iter_mut()
                    .find(|peer| peer.id == "reviewer")
                    .unwrap();
                reviewer.wake_count = reviewer.max_turns;
                reviewer.pending_delivery = Some(crate::rooms::models::Delivery {
                    id: "sent-delivery".into(),
                    reason: "followup".into(),
                    text: "Continue".into(),
                    created_at: 1,
                    state: "sent".into(),
                    turn_started: true,
                    task_id: None,
                    timer_id: None,
                    sidechat_id: None,
                    message_id: None,
                    attachment_ids: vec![],
                });
                Ok(())
            })
            .unwrap();
        assert!(respond_request(
            &sent.store,
            &sent.room_id,
            "reviewer",
            &request.requests[0].id,
            true,
            "Reviewed after the sent followup"
        )
        .is_ok());
    }

    #[test]
    fn concurrent_identical_creates_converge_to_one_request_and_message() {
        let fixture = Fixture::new();
        let path = fixture.temp.path().join("rooms.sqlite3");
        let other = RoomStore::open(&path).unwrap();
        let barrier = Barrier::new(2);
        let (one, two) = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                create_request(
                    &fixture.store,
                    &fixture.room_id,
                    "requester",
                    review_request(),
                )
                .unwrap()
            });
            let b = scope.spawn(|| {
                barrier.wait();
                create_request(&other, &fixture.room_id, "requester", review_request()).unwrap()
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_eq!(one.requests.len(), 1);
        assert_eq!(two.requests.len(), 1);
        assert_eq!(fixture.store.get(&one.id).unwrap().messages.len(), 1);
    }

    #[test]
    fn requests_and_legacy_defaults_survive_reopen() {
        let fixture = Fixture::new();
        create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            review_request(),
        )
        .unwrap();
        let room = fixture.store.get(&fixture.room_id).unwrap();
        let mut value = serde_json::to_value(room).unwrap();
        value.as_object_mut().unwrap().remove("requests");
        let old: Room = serde_json::from_value(value).unwrap();
        assert!(old.requests.is_empty());

        let db = fixture.temp.path().join("rooms.sqlite3");
        drop(fixture.store);
        let reopened = RoomStore::open(&db).unwrap();
        let recovered = reopened.get(&fixture.room_id).unwrap();
        assert_eq!(recovered.requests.len(), 1);
        assert_eq!(
            recovered.requests[0].evidence.as_deref(),
            Some("Checked the changed files and acceptance notes")
        );
    }

    #[test]
    fn completion_requires_fresh_complete_board_and_no_active_claims() {
        let fixture = Fixture::new();
        fixture.add_project_board(false);
        assert!(create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            completion_request()
        )
        .is_err());
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.board.items[0].status = "Done".into();
                room.claims.push(Claim {
                    task_id: "task-1".into(),
                    participant_id: "requester".into(),
                    state: "active".into(),
                    updated_at: crate::models::now_iso(),
                    summary: None,
                    evidence: None,
                });
                Ok(())
            })
            .unwrap();
        assert!(create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            completion_request()
        )
        .is_err());
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.claims[0].state = "done".into();
                room.board.synced_at = Some("2000-01-01T00:00:00Z".into());
                Ok(())
            })
            .unwrap();
        assert!(create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            completion_request()
        )
        .is_err());
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.board.synced_at = Some(crate::models::now_iso());
                Ok(())
            })
            .unwrap();
        assert!(create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            completion_request()
        )
        .is_ok());
    }

    #[test]
    fn completion_verification_and_acceptance_reject_changed_work() {
        let fixture = Fixture::new();
        fixture.add_project_board(true);
        let room = create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            completion_request(),
        )
        .unwrap();
        let id = room.requests[0].id.clone();
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.board.items[0].title = "Changed while review was pending".into();
                Ok(())
            })
            .unwrap();
        assert!(respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &id,
            true,
            "Verified all project work"
        )
        .is_err());

        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.board.items[0].title = "Task one".into();
                Ok(())
            })
            .unwrap();
        respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &id,
            true,
            "Verified all project work",
        )
        .unwrap();
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.board.items.push(BoardItem {
                    id: "new-done-task".into(),
                    title: "Newly added completed task".into(),
                    url: None,
                    status: "Done".into(),
                    priority: None,
                    agent: None,
                    kind: "issue".into(),
                    body: None,
                    number: None,
                    assignees: vec![],
                    labels: vec![],
                    linked_prs: vec![],
                    updated_at: None,
                });
                Ok(())
            })
            .unwrap();
        assert!(resolve_request(
            &fixture.store,
            &fixture.room_id,
            &id,
            true,
            "Accepting verified completion"
        )
        .is_err());
    }

    #[test]
    fn final_completion_acceptance_archives_paused_room_and_human_cannot_approve_peer_review() {
        let fixture = Fixture::new();
        fixture.add_project_board(true);
        let completion = create_request(
            &fixture.store,
            &fixture.room_id,
            "requester",
            completion_request(),
        )
        .unwrap();
        let completion_id = completion.requests[0].id.clone();
        respond_request(
            &fixture.store,
            &fixture.room_id,
            "reviewer",
            &completion_id,
            true,
            "Checked final board and evidence",
        )
        .unwrap();
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.paused = true;
                for peer in &mut room.participants {
                    peer.paused = true;
                    peer.state = "paused".into();
                }
                Ok(())
            })
            .unwrap();
        let accepted = resolve_request(
            &fixture.store,
            &fixture.room_id,
            &completion_id,
            true,
            "Accepted after final room review",
        )
        .unwrap();
        assert!(accepted.archived);
        assert!(accepted.paused);
        assert_eq!(accepted.requests[0].status, "accepted");
        let repeated = resolve_request(
            &fixture.store,
            &fixture.room_id,
            &completion_id,
            true,
            "Accepted after final room review",
        )
        .unwrap();
        assert_eq!(repeated.messages.len(), accepted.messages.len());

        let second = Fixture::new();
        let review = create_request(
            &second.store,
            &second.room_id,
            "requester",
            review_request(),
        )
        .unwrap();
        assert!(resolve_request(
            &second.store,
            &second.room_id,
            &review.requests[0].id,
            true,
            "Looks good"
        )
        .is_err());
        let rejected = resolve_request(
            &second.store,
            &second.room_id,
            &review.requests[0].id,
            false,
            "Please revise the evidence",
        )
        .unwrap();
        assert_eq!(rejected.requests[0].status, "rejected");
        assert!(rejected.messages.last().unwrap().participant_id.is_none());
    }

    #[test]
    fn approved_agent_requires_paused_stable_id_participant() {
        let fixture = Fixture::new();
        let input = CreateRequestInput {
            kind: "agent".into(),
            title: "Add helper".into(),
            body: "Need a helper for review".into(),
            evidence: None,
            task_id: None,
            reviewer_id: None,
            proposal: Some(AddParticipantInput {
                name: "Helper".into(),
                provider: "codex".into(),
                model: Some("gpt-test".into()),
                effort: Some("medium".into()),
                use_worktree: true,
                max_turns: 5,
            }),
            brief: Some("Review task artifacts and report evidence".into()),
            options: vec![],
        };
        let room =
            create_request(&fixture.store, &fixture.room_id, "requester", input.clone()).unwrap();
        let id = room.requests[0].id.clone();
        resolve_request(
            &fixture.store,
            &fixture.room_id,
            &id,
            true,
            "Approved helper creation",
        )
        .unwrap();
        let mut approved = peer(&id, "Helper");
        approved.paused = true;
        approved.model = Some("gpt-test".into());
        approved.effort = Some("medium".into());
        approved.max_turns = 5;
        approved.brief = Some("Review task artifacts and report evidence".into());
        approved.worktree_path = Some("/tmp/helper-worktree".into());
        approved.branch = Some("helper-branch".into());
        fixture
            .store
            .update(&fixture.room_id, |room| {
                room.participants.push(approved);
                Ok(())
            })
            .unwrap();
        assert!(resolve_request(
            &fixture.store,
            &fixture.room_id,
            &id,
            false,
            "Cancel helper creation"
        )
        .is_err());
        assert!(record_agent_approval(&fixture.store, &fixture.room_id, &id, "wrong-id").is_err());
        let result = record_agent_approval(&fixture.store, &fixture.room_id, &id, &id).unwrap();
        assert_eq!(result.requests[0].status, "approved");
        assert_eq!(
            result.requests[0].approved_participant_id.as_deref(),
            Some(id.as_str())
        );
        let repeated =
            create_request(&fixture.store, &fixture.room_id, "requester", input).unwrap();
        assert_eq!(repeated.requests.len(), 1);
        assert_eq!(repeated.messages.len(), result.messages.len());
    }
}
