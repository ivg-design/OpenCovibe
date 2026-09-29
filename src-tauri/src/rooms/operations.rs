use super::{github_tasks, models::Room, store::RoomStore};

pub fn active_peer(
    room: &Room,
    participant_id: &str,
) -> Result<super::models::Participant, String> {
    if room.paused || room.archived {
        return Err("room is paused or archived".into());
    }
    let peer = room
        .participants
        .iter()
        .find(|p| p.id == participant_id)
        .ok_or("participant not found")?;
    if peer.paused {
        return Err("participant is paused".into());
    }
    Ok(peer.clone())
}

pub async fn claim_task(
    store: &RoomStore,
    room_id: &str,
    participant_id: &str,
    task_id: &str,
) -> Result<Room, String> {
    let room = store.reserve_claim(room_id, participant_id, task_id)?;
    let peer = active_peer(&room, participant_id)?;
    let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
    if let Err(error) =
        github_tasks::set_task_state(project, task_id, Some(&peer.name), "active").await
    {
        mark_uncertain(store, room_id, participant_id, task_id, &error)?;
        return Err(error);
    }
    store.update(room_id, |room| {
        let claim = room
            .claims
            .iter_mut()
            .find(|c| c.task_id == task_id && c.participant_id == participant_id)
            .ok_or("claim disappeared")?;
        claim.state = "active".into();
        claim.updated_at = crate::models::now_iso();
        if let Some(item) = room.board.items.iter_mut().find(|i| i.id == task_id) {
            item.status = "In Progress".into();
            item.agent = Some(peer.name);
        }
        Ok(())
    })
}

fn mark_uncertain(
    store: &RoomStore,
    room_id: &str,
    participant_id: &str,
    task_id: &str,
    error: &str,
) -> Result<Room, String> {
    store.update(room_id, |room| {
        if let Some(claim) = room.claims.iter_mut().find(|c| c.task_id == task_id) { claim.state = "uncertain".into(); }
        if let Some(peer) = room.participants.iter_mut().find(|p| p.id == participant_id) {
            peer.paused = true; peer.state = "waiting".into(); peer.last_error = Some(format!("Task update unconfirmed: {error}. Refresh and reconcile or release the claim before continuing."));
        }
        room.board.error = Some(error.into());
        Ok(())
    })
}

pub async fn finish_task(
    store: &RoomStore,
    room_id: &str,
    participant_id: &str,
    task_id: &str,
    summary: &str,
    evidence: &str,
) -> Result<Room, String> {
    let summary = bounded_text(summary, 4000, "completion summary")?;
    let evidence = bounded_text(evidence, 8000, "completion evidence")?;
    let room = store.get(room_id)?;
    let peer = active_peer(&room, participant_id)?;
    let claim = room
        .claims
        .iter()
        .find(|c| c.task_id == task_id && c.participant_id == participant_id)
        .ok_or("this participant does not own the task")?;
    if claim.state == "done" {
        return Ok(room);
    }
    if !matches!(
        claim.state.as_str(),
        "active" | "reserved" | "completing" | "uncertain"
    ) {
        return Err("claim is blocked or released".into());
    }
    let project = room.project.as_ref().ok_or("room has no GitHub Project")?;
    store.update(room_id, |room| {
        let claim = room
            .claims
            .iter_mut()
            .find(|c| c.task_id == task_id && c.participant_id == participant_id)
            .ok_or("claim disappeared")?;
        claim.state = "completing".into();
        claim.summary = Some(summary.clone());
        claim.evidence = Some(evidence.clone());
        claim.updated_at = crate::models::now_iso();
        Ok(())
    })?;
    if let Err(error) = github_tasks::record_completion(project, task_id, &summary, &evidence).await
    {
        mark_uncertain(store, room_id, participant_id, task_id, &error)?;
        return Err(error);
    }
    if let Err(error) =
        github_tasks::set_task_state(project, task_id, Some(&peer.name), "done").await
    {
        mark_uncertain(store, room_id, participant_id, task_id, &error)?;
        return Err(error);
    }
    store.update(room_id, |room| {
        let claim = room
            .claims
            .iter_mut()
            .find(|c| c.task_id == task_id && c.participant_id == participant_id)
            .ok_or("claim disappeared")?;
        claim.state = "done".into();
        claim.updated_at = crate::models::now_iso();
        if let Some(item) = room.board.items.iter_mut().find(|i| i.id == task_id) {
            item.status = "Done".into();
            item.agent = Some(peer.name.clone());
        }
        Ok(())
    })?;
    store.append_message(
        room_id,
        &peer.name,
        format!("Completed task {task_id}: {summary}\nEvidence: {evidence}"),
        Some(participant_id.into()),
        None,
        Some(format!("task:{task_id}:{}", claim.updated_at)),
    )
}

pub fn block_task(
    store: &RoomStore,
    room_id: &str,
    participant_id: &str,
    task_id: &str,
    reason: &str,
) -> Result<Room, String> {
    let reason = bounded_text(reason, 4000, "blocking reason")?;
    let room = store.get(room_id)?;
    let peer = active_peer(&room, participant_id)?;
    store.update(room_id, |room| {
        let claim = room
            .claims
            .iter_mut()
            .find(|c| c.task_id == task_id && c.participant_id == participant_id)
            .ok_or("this participant does not own the task")?;
        if claim.state == "done" || claim.state == "released" {
            return Err("task is already complete or released".into());
        }
        claim.state = "blocked".into();
        claim.summary = Some(reason.clone());
        claim.updated_at = crate::models::now_iso();
        let peer = room
            .participants
            .iter_mut()
            .find(|p| p.id == participant_id)
            .ok_or("participant not found")?;
        peer.paused = true;
        peer.state = "blocked".into();
        peer.last_error = Some(reason.clone());
        Ok(())
    })?;
    store.append_message(
        room_id,
        &peer.name,
        format!("Blocked task {task_id}: {reason}"),
        Some(participant_id.into()),
        None,
        None,
    )
}

pub async fn release_claim(
    store: &RoomStore,
    room_id: &str,
    task_id: &str,
) -> Result<Room, String> {
    let room = store.get(room_id)?;
    let claim = room
        .claims
        .iter()
        .find(|c| c.task_id == task_id)
        .ok_or("claim not found")?;
    if matches!(claim.state.as_str(), "done" | "released") {
        return Err("Task is already complete or released; reopen it explicitly on GitHub to start new work.".into());
    }
    let peer = room
        .participants
        .iter()
        .find(|p| p.id == claim.participant_id)
        .ok_or("claim owner not found")?;
    if !room.paused && (!peer.paused || peer.pending_delivery.is_some()) {
        return Err("pause the owner and stop its turn before releasing the claim".into());
    }
    let project = room.project.as_ref().ok_or("room has no Project")?;
    store.update(room_id, |r| {
        r.claims
            .iter_mut()
            .find(|c| c.task_id == task_id)
            .ok_or("claim not found")?
            .state = "releasing".into();
        Ok(())
    })?;
    if let Err(error) = github_tasks::set_task_state(project, task_id, None, "ready").await {
        mark_uncertain(store, room_id, &peer.id, task_id, &error)?;
        return Err(error);
    }
    store.update(room_id, |room| {
        let claim = room
            .claims
            .iter_mut()
            .find(|c| c.task_id == task_id)
            .ok_or("claim not found")?;
        claim.state = "released".into();
        claim.updated_at = crate::models::now_iso();
        if let Some(item) = room.board.items.iter_mut().find(|i| i.id == task_id) {
            item.status = "Todo".into();
            item.agent = None;
        }
        Ok(())
    })
}

pub fn bounded_text(value: &str, limit: usize, label: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > limit {
        return Err(format!("{label} must contain 1–{limit} bytes"));
    }
    Ok(value.into())
}
