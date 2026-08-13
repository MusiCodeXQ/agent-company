use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::json;

use crate::{
    a2a, adk_runtime,
    app_state::AppState,
    db::{self, AgentJob},
    events,
    llm::{self, ChatMessage},
    model_profiles, offboarding, openclaw, organization, spatial, spatial_commands,
};

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        if let Err(error) = worker_loop(state).await {
            tracing::error!(%error, "agent worker stopped");
        }
    });
}

async fn worker_loop(state: AppState) -> Result<()> {
    tracing::info!("agent worker started");
    loop {
        match db::claim_next_job(&state.pool).await {
            Ok(Some(job)) => {
                if let Err(error) = process_job(&state, job.clone()).await {
                    tracing::error!(job_id = %job.id, %error, "agent job failed unexpectedly");
                    let error_text = format!("{error:#}");
                    let _ = db::finish_job(&state.pool, &job.id, "FAILED", None, Some(&error_text))
                        .await;
                    let _ = events::transition(
                        &state,
                        &job.company_id,
                        &job.employee_id,
                        "ERROR",
                        job.task_id.as_deref(),
                        Some(&job.id),
                    )
                    .await;
                    let _ = emit_job_state(&state, &job, "FAILED").await;
                }
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, "agent worker database poll failed; retrying");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        }
        tokio::select! {
            _ = state.worker_notify.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(300)) => {}
        }
    }
}

async fn process_job(state: &AppState, job: AgentJob) -> Result<()> {
    emit_job_state(state, &job, "RUNNING").await?;
    match job.job_type.as_str() {
        "MESSAGE_RESPONSE" => process_message_job(state, &job).await,
        "A2A_RESPONSE" => a2a::process_job(state, &job).await,
        other => {
            db::finish_job(
                &state.pool,
                &job.id,
                "FAILED",
                None,
                Some(&format!("unsupported job type: {other}")),
            )
            .await?;
            emit_job_state(state, &job, "FAILED").await?;
            Ok(())
        }
    }
}

async fn process_message_job(state: &AppState, job: &AgentJob) -> Result<()> {
    let employee = db::employees(&state.pool, &job.company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == job.employee_id)
        .ok_or_else(|| anyhow!("employee {} was not found", job.employee_id))?;
    if employee.employment_status != "ACTIVE" {
        db::finish_job(
            &state.pool,
            &job.id,
            "CANCELLED",
            None,
            Some("employee is not active"),
        )
        .await?;
        emit_job_state(state, job, "CANCELLED").await?;
        return Ok(());
    }
    let meeting_job = job.meeting_id.is_some();
    if !meeting_job {
        events::transition(
            state,
            &job.company_id,
            &job.employee_id,
            "CHECK_PHONE",
            job.task_id.as_deref(),
            Some(&job.id),
        )
        .await
        .context("transition employee to CHECK_PHONE")?;
        tokio::time::sleep(Duration::from_millis(450)).await;
    }
    events::transition(
        state,
        &job.company_id,
        &job.employee_id,
        "THINKING",
        job.task_id.as_deref(),
        Some(&job.id),
    )
    .await
    .context("transition employee to THINKING")?;
    emit_job_state(state, job, "RUNNING").await?;

    let user_content = job_message_content(job);
    let spatial_command = spatial_commands::parse(&user_content);
    let command_mode = spatial_command.is_some();
    let existing_message = db::agent_reply_for_job(&state.pool, &job.id)
        .await
        .context("check for existing agent reply")?;
    let termination_reply =
        if existing_message.is_none() && is_administrator_job(state, job).await? {
            offboarding::handle_administrator_message(
                state,
                &job.company_id,
                &job.employee_id,
                &user_content,
            )
            .await?
        } else {
            None
        };
    let patrol_result = if existing_message.is_none()
        && termination_reply.is_none()
        && employee.is_floor_administrator
        && organization::administrator_patrol_intent(&user_content)
    {
        if let Some(floor_id) = employee.floor_id.as_deref() {
            Some(
                organization::administrator_patrol(
                    &state.pool,
                    &job.company_id,
                    floor_id,
                    &job.id,
                    organization::administrator_intervention_intent(&user_content),
                )
                .await?,
            )
        } else {
            None
        }
    } else {
        None
    };
    if let Some(result) = patrol_result.as_ref() {
        events::emit(
            state,
            &job.company_id,
            "administrator.patrol.completed",
            Some(&job.employee_id),
            Some(&job.id),
            json!({
                "riskyJobIds": result.risky_job_ids,
                "cancelledJobIds": result.cancelled_job_ids,
            }),
        )
        .await?;
    }
    let mut hiring_was_refreshed = false;
    let mut hiring_request = if existing_message.is_none()
        && termination_reply.is_none()
        && patrol_result.is_none()
        && is_administrator_job(state, job).await?
    {
        if organization::candidate_refresh_intent(&user_content) {
            match organization::refresh_latest_candidates(
                &state.pool,
                &job.company_id,
                &job.employee_id,
            )
            .await
            {
                Ok(created) => {
                    hiring_was_refreshed = true;
                    Some(created)
                }
                Err(error) => {
                    tracing::warn!(%error, job_id = %job.id, "administrator could not refresh candidates");
                    None
                }
            }
        } else if organization::hiring_intent(&user_content) {
            match organization::create_hiring_from_message(
                &state.pool,
                &job.company_id,
                &job.employee_id,
                job_message_id(job).as_deref(),
                &user_content,
            )
            .await
            {
                Ok(created) => Some(created),
                Err(error) => {
                    tracing::warn!(%error, job_id = %job.id, "administrator message did not produce a structured hiring request");
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };
    let mut hiring_generation: Option<organization::HiringPersonalizationResult> = None;
    if let Some(created) = hiring_request.as_mut() {
        match organization::personalize_hiring_candidates(
            &state.pool,
            &job.company_id,
            &created.request.id,
            Some(&employee.id),
            Some(&job.id),
        )
        .await
        {
            Ok(generation) => {
                created.request = generation.request.clone();
                hiring_generation = Some(generation);
            }
            Err(_error) => {
                tracing::warn!(
                    job_id = %job.id,
                    fallback_reason = organization::FALLBACK_GENERATION_SERVICE_UNAVAILABLE,
                    "administrator model candidate generation failed; keeping local fallback candidates"
                );
                hiring_generation = Some(organization::HiringPersonalizationResult::fallback(
                    created.request.clone(),
                    organization::FALLBACK_GENERATION_SERVICE_UNAVAILABLE,
                ));
            }
        }
    }
    let requires_workstation = !command_mode
        && !meeting_job
        && existing_message.is_none()
        && termination_reply.is_none()
        && patrol_result.is_none()
        && hiring_request.is_none();
    let message = if let Some(message) = existing_message {
        message
    } else {
        let reply = if let Some(reply) = termination_reply {
            reply
        } else if let Some(result) = patrol_result {
            result.reply
        } else if let Some(created) = hiring_request.as_ref() {
            let candidate_count = created
                .request
                .candidates
                .iter()
                .filter(|candidate| candidate.status == "AVAILABLE")
                .count();
            if hiring_was_refreshed {
                format!(
                    "我已经为“{}”换了一批候选人，共 {} 位。请在招聘面板查看姓名、年龄、职位、外观、性格与技能；只有你选中并确认录用后，候选人才会成为正式员工。",
                    created.request.summary, candidate_count
                )
            } else {
                format!(
                    "我已经根据“{}”整理出 {} 位候选人。请在招聘面板查看并选择；在你确认以前，我不会自动录用任何人。",
                    created.request.summary, candidate_count
                )
            }
        } else if let Some(command) = spatial_command {
            match spatial_commands::execute(state, &job.company_id, &job.employee_id, command).await
            {
                Ok(execution) => execution.reply,
                Err(error) => {
                    let _ = events::emit(
                        state,
                        &job.company_id,
                        "AGENT_SPATIAL_COMMAND_FAILED",
                        Some(&job.employee_id),
                        Some(&job.id),
                        json!({ "content": user_content, "error": error.to_string() }),
                    )
                    .await;
                    format!("我暂时无法执行这个移动指令：{}", error)
                }
            }
        } else {
            let result = generate_reply(state, job).await;
            match result {
                Ok(reply) if reply.starts_with("__AI_COMPANY_WAITING_APPROVAL__:") => {
                    db::finish_job(
                        &state.pool,
                        &job.id,
                        "WAITING",
                        None,
                        Some("等待用户审批高风险工具调用"),
                    )
                    .await?;
                    events::emit(
                        state,
                        &job.company_id,
                        "runtime.approval.required",
                        Some(&job.employee_id),
                        Some(&job.id),
                        json!({
                            "jobId": job.id,
                            "approvalId": reply.trim_start_matches("__AI_COMPANY_WAITING_APPROVAL__:")
                        }),
                    )
                    .await?;
                    emit_job_state(state, job, "WAITING").await?;
                    return Ok(());
                }
                Ok(reply) => reply,
                Err(error) => {
                    let _ = events::transition(
                        state,
                        &job.company_id,
                        &job.employee_id,
                        "ERROR",
                        job.task_id.as_deref(),
                        Some(&job.id),
                    )
                    .await;
                    db::finish_job(
                        &state.pool,
                        &job.id,
                        "FAILED",
                        None,
                        Some(&error.to_string()),
                    )
                    .await?;
                    emit_job_state(state, job, "FAILED").await?;
                    return Ok(());
                }
            }
        };
        if db::job_is_cancelled(&state.pool, &job.id).await? {
            emit_job_state(state, job, "CANCELLED").await?;
            return Ok(());
        }
        let broadcast_id = meeting_broadcast_id(job);
        db::insert_agent_reply(
            &state.pool,
            &job.company_id,
            &job.employee_id,
            &reply,
            &job.id,
            job.meeting_id.as_deref(),
            broadcast_id.as_deref(),
        )
        .await
        .context("persist agent reply")?
    };
    let reply = message.content.clone();
    if let Some(created) = hiring_request {
        let generation = hiring_generation.unwrap_or_else(|| {
            organization::HiringPersonalizationResult::mock(created.request.clone())
        });
        let payload = json!({
            "request": generation.request,
            "floorId": created.floor_id,
            "generationMode": generation.generation_mode,
            "fallbackReason": generation.fallback_reason,
        });
        if !hiring_was_refreshed {
            events::emit(
                state,
                &job.company_id,
                "hiring.request.created",
                Some(&job.employee_id),
                Some(&job.id),
                payload.clone(),
            )
            .await?;
        }
        events::emit(
            state,
            &job.company_id,
            if hiring_was_refreshed {
                "hiring.candidates.refreshed"
            } else {
                "hiring.candidates.ready"
            },
            Some(&job.employee_id),
            Some(&job.id),
            payload,
        )
        .await?;
    }
    if meeting_job && reply.trim().eq_ignore_ascii_case("NO_REPLY") {
        events::transition(
            state,
            &job.company_id,
            &job.employee_id,
            "MEETING",
            job.task_id.as_deref(),
            Some(&job.id),
        )
        .await
        .context("restore meeting state after no-reply decision")?;
        db::finish_job(&state.pool, &job.id, "COMPLETED", Some(&reply), None).await?;
        emit_job_state(state, job, "COMPLETED").await?;
        return Ok(());
    }
    let memory_text = format!("用户消息：{}\nAgent 回复：{}", user_content, reply);
    if let Err(error) =
        db::save_memory(&state.pool, &job.company_id, &job.employee_id, &memory_text).await
    {
        tracing::warn!(%error, employee_id = %job.employee_id, "unable to persist agent memory");
    }
    if requires_workstation {
        let employee = db::employees(&state.pool, &job.company_id)
            .await?
            .into_iter()
            .find(|employee| employee.id == job.employee_id)
            .ok_or_else(|| anyhow!("employee {} was not found", job.employee_id))?;
        let workstation =
            spatial::employee_workstation_location(state, &job.company_id, &employee).await?;
        if employee.location_id.as_deref() != Some(workstation.as_str()) {
            spatial::request_move(
                state,
                &job.company_id,
                &job.employee_id,
                &workstation,
                "CODING_LOCATION_REQUIREMENT",
            )
            .await
            .context("move employee to workstation")?;
            wait_until_arrived(state, &job.company_id, &job.employee_id, &workstation).await?;
        }
        events::transition(
            state,
            &job.company_id,
            &job.employee_id,
            "WORKING",
            job.task_id.as_deref(),
            Some(&job.id),
        )
        .await
        .context("transition employee to WORKING")?;
        tokio::time::sleep(Duration::from_millis(350)).await;
    }
    events::transition(
        state,
        &job.company_id,
        &job.employee_id,
        if meeting_job { "MEETING" } else { "IDLE" },
        job.task_id.as_deref(),
        Some(&job.id),
    )
    .await
    .context("restore employee activity after message")?;
    db::finish_job(&state.pool, &job.id, "COMPLETED", Some(&reply), None).await?;
    events::emit(
        state,
        &job.company_id,
        "message.received",
        Some(&job.employee_id),
        Some(&job.id),
        json!({
            "messageId": message.id,
            "fromType": "employee",
            "fromId": job.employee_id,
            "toType": "user",
            "toId": "user",
            "content": message.content,
            "meetingId": message.meeting_id,
            "broadcastId": message.broadcast_id,
            "unread": true
        }),
    )
    .await?;
    emit_job_state(state, job, "COMPLETED").await?;
    Ok(())
}

async fn wait_until_arrived(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
    location_id: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let employee = db::employees(&state.pool, company_id)
            .await?
            .into_iter()
            .find(|employee| employee.id == employee_id)
            .ok_or_else(|| anyhow!("employee {employee_id} was not found"))?;
        if employee.location_id.as_deref() == Some(location_id)
            && employee.active_movement_id.is_none()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out waiting for agent {employee_id} to arrive"
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub(crate) async fn generate_reply(state: &AppState, job: &AgentJob) -> Result<String> {
    let employee = db::employees(&state.pool, &job.company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == job.employee_id)
        .ok_or_else(|| anyhow!("employee {} was not found", job.employee_id))?;
    let profile_id = employee
        .model_profile_id
        .as_deref()
        .ok_or_else(|| anyhow!("employee {} has no model profile", employee.id))?;
    let profile = model_profiles::get_credentials(&state.pool, profile_id).await?;
    let a2a_payload = serde_json::from_str::<serde_json::Value>(&job.payload).ok();
    let a2a_sender = a2a_payload
        .as_ref()
        .and_then(|value| value.get("senderId"))
        .and_then(serde_json::Value::as_str);
    let history = if a2a_sender.is_some() {
        Vec::new()
    } else {
        db::messages_for_employee(&state.pool, &job.company_id, &job.employee_id).await?
    };
    let mut messages = history
        .into_iter()
        .map(|message| ChatMessage {
            role: if message.from_type == "user" {
                "user".to_string()
            } else {
                "assistant".to_string()
            },
            content: message.content,
        })
        .collect::<Vec<_>>();
    if a2a_sender.is_some() {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: job_message_content(job),
        });
    }
    let voice_profile = format!(
        "性格：{}。\n沟通风格：{}。\n决策风格：{}。\n说话习惯：{}。\n行为习惯：{}。\n工作方式：{}。\n这些特征只影响措辞、关注点和主动程度，不能降低事实准确性或偏离岗位职责，也不能替代系统的移动与环境规则。",
        employee.personality.traits.join("、"),
        employee.personality.communication_style,
        employee.personality.decision_style,
        employee.personality.speech_habits.join("；"),
        employee.personality.behavior_habits.join("；"),
        employee.work_style
    );
    let system_prompt = if let Some(sender_id) = a2a_sender {
        format!(
            "你是星河软件公司的 Agent。姓名：{}；职位：{}；部门：{}。\n{}\n你正与员工 {} 在会议室面对面进行受治理的 A2A 对话。请保持上述个人说话风格，直接回答对方，只输出可发送的自然语言内容，不暴露隐藏思考过程。",
            employee.name, employee.title, employee.department, voice_profile, sender_id
        )
    } else if job.meeting_id.is_some() {
        format!(
            "你是星河软件公司的 Agent。姓名：{}；职位：{}；部门：{}。\n{}\n技能：{}。\n你正在会议桌旁参加一场跨职能会议，CEO 刚刚向所有当前到席 Agent 广播了一条消息。请只从自己的职能判断：如果目标、范围、约束、优先级或验收标准中有会阻碍你执行的关键不确定性，就用上述个人说话风格提出 1 到 2 个具体澄清问题；如果信息已经足够，或你没有需要追问的内容，必须严格只输出 NO_REPLY。不要泛泛确认，不要替 CEO 做隐藏思考，也不要把问题局限于软件开发。",
            employee.name,
            employee.title,
            employee.department,
            voice_profile,
            employee
                .skills
                .iter()
                .map(|skill| format!("{}({})", skill.name, skill.level))
                .collect::<Vec<_>>()
                .join("、")
        )
    } else {
        format!(
            "你是星河软件公司的 Agent。姓名：{}；职位：{}；部门：{}。\n{}\n技能：{}。\n请保持上述个人说话风格，只输出可直接发给 CEO 的简洁工作回复，不要暴露隐藏思考过程。",
        employee.name,
        employee.title,
        employee.department,
        voice_profile,
        employee
            .skills
            .iter()
            .map(|skill| format!("{}({})", skill.name, skill.level))
            .collect::<Vec<_>>()
            .join("、")
        )
    };
    if employee.runtime_binding.as_ref().is_some_and(|binding| {
        binding.runtime_type == "ADK_RUST"
            && binding.provision_status == "READY"
            && binding.runtime_status != "ARCHIVED"
            && binding.runtime_status != "DISABLED"
    }) {
        return adk_runtime::run_turn(state, job, &employee, profile_id, &profile, &system_prompt)
            .await;
    }
    if openclaw::config().enabled
        && employee.runtime_binding.as_ref().is_some_and(|binding| {
            binding.runtime_type == "OPENCLAW"
                && binding.provision_status == "READY"
                && binding.runtime_status != "ARCHIVED"
                && binding.runtime_status != "DISABLED"
        })
    {
        let binding = employee
            .runtime_binding
            .as_ref()
            .expect("binding was checked");
        let session_key = format!("agent:{}:main", binding.runtime_agent_id);
        let runtime_message = format!(
            "{}\n\n公司消息：{}",
            system_prompt,
            job_message_content(job)
        );
        let _ = db::upsert_runtime_session_binding(
            &state.pool,
            &job.company_id,
            &job.employee_id,
            "USER_CHAT",
            "user",
            &session_key,
        )
        .await;
        let started = Instant::now();
        let started_payload = json!({ "jobId": job.id, "sessionKey": session_key }).to_string();
        let _ = db::record_runtime_event(
            &state.pool,
            &job.company_id,
            Some(&job.employee_id),
            Some(&binding.runtime_agent_id),
            None,
            "run.started",
            &started_payload,
        )
        .await;
        match openclaw::run_turn(&binding.runtime_agent_id, &session_key, &runtime_message).await {
            Ok(result) => {
                let latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
                let usage = crate::models::TokenUsageSummary::default();
                let _ = db::record_llm_call(
                    &state.pool,
                    db::LlmCallInput {
                        company_id: &job.company_id,
                        employee_id: &job.employee_id,
                        job_id: Some(&job.id),
                        model_profile_id: profile_id,
                        model: &profile.model,
                        status: "COMPLETED",
                        usage: &usage,
                        latency_ms,
                        error: None,
                    },
                )
                .await;
                let _ =
                    db::touch_runtime_health(&state.pool, &job.company_id, &job.employee_id, None)
                        .await;
                let _ = db::record_runtime_event(
                    &state.pool,
                    &job.company_id,
                    Some(&job.employee_id),
                    Some(&binding.runtime_agent_id),
                    result.run_id.as_deref(),
                    "run.completed",
                    &json!({ "jobId": job.id }).to_string(),
                )
                .await;
                return Ok(result.content);
            }
            Err(error) => {
                let error_text = error.to_string();
                let _ = db::update_runtime_binding_status(
                    &state.pool,
                    &job.company_id,
                    &job.employee_id,
                    None,
                    Some("ACTIVE"),
                    Some(&error_text),
                )
                .await;
                let _ = db::record_runtime_event(
                    &state.pool,
                    &job.company_id,
                    Some(&job.employee_id),
                    Some(&binding.runtime_agent_id),
                    None,
                    "run.failed",
                    &json!({ "jobId": job.id, "error": error_text }).to_string(),
                )
                .await;
                return Err(error);
            }
        }
    }
    let started = Instant::now();
    match llm::chat(&profile, &system_prompt, &messages).await {
        Ok(response) => {
            let latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
            match db::record_llm_call(
                &state.pool,
                db::LlmCallInput {
                    company_id: &job.company_id,
                    employee_id: &job.employee_id,
                    job_id: Some(&job.id),
                    model_profile_id: profile_id,
                    model: &profile.model,
                    status: "COMPLETED",
                    usage: &response.usage,
                    latency_ms,
                    error: None,
                },
            )
            .await
            {
                Ok(call_id) => {
                    let company_usage = db::company_token_usage(&state.pool, &job.company_id).await;
                    let employee_usage =
                        db::employee_token_usage(&state.pool, &job.company_id, &job.employee_id)
                            .await;
                    if let (Ok(company_usage), Ok(employee_usage)) = (company_usage, employee_usage)
                    {
                        if let Err(error) = events::emit(
                            state,
                            &job.company_id,
                            "llm.usage.recorded",
                            Some(&job.employee_id),
                            Some(&job.id),
                            json!({
                                "llmCallId": call_id,
                                "usage": response.usage,
                                "employeeUsage": employee_usage,
                                "companyUsage": company_usage,
                            }),
                        )
                        .await
                        {
                            tracing::warn!(%error, job_id = %job.id, "unable to emit LLM usage event");
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, job_id = %job.id, "unable to persist LLM usage");
                }
            }
            Ok(response.content)
        }
        Err(error) => {
            let latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
            let error_text = error.to_string();
            let empty_usage = crate::models::TokenUsageSummary::default();
            if let Err(record_error) = db::record_llm_call(
                &state.pool,
                db::LlmCallInput {
                    company_id: &job.company_id,
                    employee_id: &job.employee_id,
                    job_id: Some(&job.id),
                    model_profile_id: profile_id,
                    model: &profile.model,
                    status: "FAILED",
                    usage: &empty_usage,
                    latency_ms,
                    error: Some(&error_text),
                },
            )
            .await
            {
                tracing::warn!(%record_error, job_id = %job.id, "unable to persist failed LLM call");
            }
            Err(error)
        }
    }
}

pub(crate) fn job_message_content(job: &AgentJob) -> String {
    serde_json::from_str::<serde_json::Value>(&job.payload)
        .ok()
        .and_then(|value| {
            value
                .get("content")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "（消息内容不可用）".to_string())
}

fn job_message_id(job: &AgentJob) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&job.payload)
        .ok()
        .and_then(|value| {
            value
                .get("messageId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
}

async fn is_administrator_job(state: &AppState, job: &AgentJob) -> Result<bool> {
    let employee = db::employees(&state.pool, &job.company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == job.employee_id)
        .ok_or_else(|| anyhow!("employee {} was not found", job.employee_id))?;
    Ok(employee.is_floor_administrator)
}

fn meeting_broadcast_id(job: &AgentJob) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(&job.payload)
        .ok()
        .and_then(|value| {
            value
                .get("broadcastId")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
}

async fn emit_job_state(state: &AppState, job: &AgentJob, status: &str) -> Result<()> {
    events::emit(
        state,
        &job.company_id,
        "agent.job.state.changed",
        Some(&job.employee_id),
        Some(&job.id),
        json!({ "jobId": job.id, "status": status, "attempts": job.attempts }),
    )
    .await?;
    Ok(())
}
