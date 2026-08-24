//! Native TUI client entry point.

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use dsh_pager::{
    drain_notifications, fork_session, list_sessions, load_session, rename_session, respond,
    submit_prompt, update_queue, DashboardModel, DashboardStatus, InteractionKind, RpcTransport,
    SessionChoice,
};
use dsh_pager_grok_ui::run_interactive;
use dsh_pager_protocol::{
    PromptMode, QueueAction, ResumeClass, TuiInteractionResponse, TUI_PROTOCOL_VERSION,
};

struct Args {
    hello_only: bool,
    load_only: bool,
    smoke_interactions: bool,
    smoke_queue: bool,
    smoke_lifecycle: bool,
    list_sessions: bool,
    dashboard: bool,
    choice: SessionChoice,
    program: String,
    program_args: Vec<String>,
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("dsh-pager: {error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32, Box<dyn Error>> {
    let args = parse_args()?;
    if smoke_requested(&args)
        && !is_mock_backend(&args)
        && env::var_os("DSH_ALLOW_REAL_SMOKE").is_none()
    {
        return Err("non-interactive smoke flags require the checked-in mock backend; set DSH_ALLOW_REAL_SMOKE=1 only for an intentional isolated real-backend run".into());
    }
    let mut transport = RpcTransport::spawn(&args.program, &args.program_args)?;
    let cwd = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .display()
        .to_string();
    let hello = transport.hello(cwd.clone())?;
    eprintln!(
        "dsh-pager: tui.hello ok clientId={} generation={} resumeClass={} server={}/{}",
        hello.client_id,
        hello.generation,
        resume_class_label(hello.resume_class),
        hello.server_info.name,
        hello.server_info.version,
    );
    if args.hello_only {
        return Ok(0);
    }
    if args.list_sessions {
        let list = list_sessions(&mut transport)?;
        for item in list.items {
            eprintln!(
                "dsh-pager: Session id={} updatedAt={} running={} blank={} cwd={} origin={} preset={}{}",
                item.session_id,
                item.updated_at,
                item.running,
                item.blank,
                item.cwd.as_deref().unwrap_or("-"),
                item.origin.as_deref().unwrap_or("-"),
                item.agent_preset.as_deref().unwrap_or("-"),
                item.parent_session_id
                    .as_deref()
                    .map_or_else(String::new, |parent| format!(" parent={parent}")),
            );
        }
        return Ok(0);
    }
    if args.dashboard {
        let list = list_sessions(&mut transport)?;
        let mut dashboard = DashboardModel::default();
        // The session list is the synchronous roster baseline; prefer the
        // control-plane mirror once workspace/archive metadata has also been
        // seeded so standalone Dashboard output matches the live overlay.
        if transport.control_plane().snapshots().next().is_some() {
            dashboard.replace_control_plane_with_workspaces(
                transport.control_plane().snapshots().cloned().collect(),
                transport.control_plane().workspaces().cloned().collect(),
                transport.control_plane().workspace_order().to_vec(),
            );
        } else {
            dashboard.replace(list.items);
        }
        for row in dashboard.rows() {
            eprintln!(
                "dsh-pager: Dashboard depth={} id={} title={:?} status={} cwd={}",
                row.depth,
                row.session_id,
                row.title,
                dashboard_status_label(row.status),
                row.cwd.as_deref().unwrap_or("-"),
            );
        }
        return Ok(0);
    }

    let session = load_session(&mut transport, &hello, args.choice, &cwd)?;
    eprintln!(
        "dsh-pager: SessionLoaded sessionId={} events={} seq={}..{}",
        session.session_id(),
        session.history().len(),
        session
            .base_seq()
            .map_or_else(|| "-".into(), |seq| seq.to_string()),
        session
            .tail_seq()
            .map_or_else(|| "-".into(), |seq| seq.to_string()),
    );
    if args.load_only {
        return Ok(0);
    }
    if args.smoke_interactions {
        run_interaction_smoke(&mut transport, session)?;
        return Ok(0);
    }
    if args.smoke_queue {
        run_queue_smoke(&mut transport, session)?;
        return Ok(0);
    }
    if args.smoke_lifecycle {
        run_lifecycle_smoke(&mut transport, session)?;
        return Ok(0);
    }
    run_interactive(transport, session)?;
    Ok(0)
}

fn smoke_requested(args: &Args) -> bool {
    args.smoke_interactions || args.smoke_queue || args.smoke_lifecycle
}

fn is_mock_backend(args: &Args) -> bool {
    args.program == "node"
        && args
            .program_args
            .iter()
            .any(|argument| argument.ends_with("mock-server.mjs"))
}

fn resume_class_label(class: ResumeClass) -> &'static str {
    match class {
        ResumeClass::ResumeAccepted => "resume-accepted",
        ResumeClass::BaselineRequired => "baseline-required",
    }
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut hello_only = false;
    let mut load_only = false;
    let mut smoke_interactions = false;
    let mut smoke_queue = false;
    let mut smoke_lifecycle = false;
    let mut list_sessions = false;
    let mut dashboard = false;
    let mut choice = SessionChoice::RecentOrCreate;
    let mut program: Option<String> = None;
    let mut program_args: Vec<String> = Vec::new();
    let mut argv = env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                eprint_help();
                std::process::exit(0);
            }
            "--hello" => hello_only = true,
            "--load-only" => load_only = true,
            "--smoke-interactions" => smoke_interactions = true,
            "--smoke-queue" => smoke_queue = true,
            "--smoke-lifecycle" => smoke_lifecycle = true,
            "--list-sessions" => list_sessions = true,
            "--dashboard" => dashboard = true,
            "--new" => choice = SessionChoice::New,
            "--session" => choice = SessionChoice::Id(required_value("--session", argv.next())?),
            "--session-search" => {
                choice = SessionChoice::Search(required_value("--session-search", argv.next())?);
            }
            "--backend" => {
                program = Some(required_value("--backend", argv.next())?);
            }
            "--backend-arg" => {
                program_args.push(required_value("--backend-arg", argv.next())?);
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    if program.is_none() {
        if let Ok(from_env) = env::var("DSH_TUI_SERVER") {
            if !from_env.is_empty() {
                let mut parts = from_env.split_whitespace();
                if let Some(first) = parts.next() {
                    program = Some(first.to_string());
                    if program_args.is_empty() {
                        program_args = parts.map(str::to_string).collect();
                    }
                }
            }
        }
    }

    let program = program.unwrap_or_else(|| "dsh".into());
    let program_args = if program_args.is_empty() && program == "dsh" {
        vec!["--profile".into(), "tui-embedded".into()]
    } else {
        program_args
    };
    Ok(Args {
        hello_only,
        load_only,
        smoke_interactions,
        smoke_queue,
        smoke_lifecycle,
        list_sessions,
        dashboard,
        choice,
        program,
        program_args,
    })
}

fn dashboard_status_label(status: DashboardStatus) -> &'static str {
    match status {
        DashboardStatus::NeedsInput => "needs-input",
        DashboardStatus::Failed => "failed",
        DashboardStatus::Running => "running",
        DashboardStatus::Idle => "idle",
        DashboardStatus::Blank => "blank",
    }
}

fn required_value(flag: &str, value: Option<String>) -> Result<String, Box<dyn Error>> {
    value.ok_or_else(|| format!("{flag} needs a value").into())
}

fn eprint_help() {
    eprintln!(
        "dsh-pager {} — protocol version {}\n\
         Usage: dsh-pager [--hello|--list-sessions|--dashboard|--load-only|--smoke-interactions|--smoke-queue|--smoke-lifecycle] [--new|--session <id>|--session-search <query>]\n\
                          [--backend <program>] [--backend-arg <arg>]...\n\
         Default backend: dsh --profile tui-embedded\n\
         Without --hello, loads the most recent non-subagent session (or creates one)\n\
         and enters the pager. DSH_TUI_SERVER overrides the default program.",
        env!("CARGO_PKG_VERSION"),
        TUI_PROTOCOL_VERSION,
    );
}

const INTERACTION_SMOKE_TIMEOUT: Duration = Duration::from_secs(60);

/// Wait for a server-owned interaction after an admitted prompt.
///
/// `session.prompt` is only an admission receipt.  A real Harness may need
/// several model/tool turns before it emits an approval or question, while
/// the checked-in mock emits it in the same read cycle.  Polling the existing
/// notification queue keeps both paths deterministic without changing the
/// wire contract.
fn wait_for_interaction(
    transport: &mut RpcTransport,
    state: &mut dsh_pager::SessionState,
    label: &str,
    expected_kind: InteractionKind,
    history_before: usize,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + INTERACTION_SMOKE_TIMEOUT;
    loop {
        drain_notifications(transport, state)?;
        if let Some(interaction) = state.pending_interaction() {
            if interaction.kind == expected_kind {
                return Ok(());
            }
            return Err(format!(
                "expected {label} interaction, received {:?}",
                interaction.kind
            )
            .into());
        }
        if state.history().len() > history_before && !state.running() {
            return Err(format!(
                "backend completed without {label} interaction (history entries={})",
                state.history().len()
            )
            .into());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for {label} interaction after {}s",
                INTERACTION_SMOKE_TIMEOUT.as_secs()
            )
            .into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until the server acknowledges the final interaction response.
fn wait_for_interaction_clear(
    transport: &mut RpcTransport,
    state: &mut dsh_pager::SessionState,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + INTERACTION_SMOKE_TIMEOUT;
    loop {
        drain_notifications(transport, state)?;
        if state.pending_interaction().is_none() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for interaction response to clear".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn smoke_question_answers(questions: &[serde_json::Value]) -> serde_json::Value {
    let answers = questions
        .iter()
        .filter_map(|question| {
            let id = question.get("id")?.as_str()?;
            let selected = question
                .get("options")
                .and_then(serde_json::Value::as_array)
                .and_then(|options| options.first())
                .and_then(|option| option.get("label"))
                .and_then(serde_json::Value::as_str)
                .map(|label| vec![serde_json::Value::String(label.to_string())])
                .unwrap_or_default();
            Some(serde_json::json!({ "id": id, "selected": selected }))
        })
        .collect::<Vec<_>>();
    serde_json::json!({ "answers": answers })
}

fn run_interaction_smoke(
    transport: &mut RpcTransport,
    mut session: dsh_pager::SessionState,
) -> Result<(), Box<dyn Error>> {
    let history_before_prompt = session.history().len();
    let prompt = submit_prompt(
        transport,
        &session,
        "trigger interaction smoke".into(),
        PromptMode::Steer,
    )?;
    if !prompt.accepted {
        return Err("mock rejected smoke prompt".into());
    }
    wait_for_interaction(
        transport,
        &mut session,
        "approval",
        InteractionKind::Approval,
        history_before_prompt,
    )?;
    let approval = session
        .pending_interaction()
        .cloned()
        .ok_or("approval notification was not delivered")?;
    let approval_id = approval.approval_id.clone().ok_or("approval id missing")?;
    respond(
        transport,
        &session,
        approval.request_id.clone(),
        TuiInteractionResponse::Approval {
            approval_id,
            outcome: "allowed-once".into(),
        },
    )?;
    let history_before_question = session.history().len();
    wait_for_interaction(
        transport,
        &mut session,
        "question",
        InteractionKind::Question,
        history_before_question,
    )?;
    let question = session
        .pending_interaction()
        .cloned()
        .ok_or("question notification was not delivered")?;
    respond(
        transport,
        &session,
        question.request_id,
        TuiInteractionResponse::Question {
            answers: smoke_question_answers(&question.questions),
        },
    )?;
    wait_for_interaction_clear(transport, &mut session)?;
    eprintln!("dsh-pager: interaction smoke ok");
    Ok(())
}

fn run_queue_smoke(
    transport: &mut RpcTransport,
    mut session: dsh_pager::SessionState,
) -> Result<(), Box<dyn Error>> {
    let item_id = session
        .queue()
        .first()
        .map(|item| item.id.clone())
        .ok_or("queue snapshot was not delivered")?;
    update_queue(
        transport,
        &session,
        item_id.clone(),
        QueueAction::Edit {
            content: vec![serde_json::json!({ "type": "text", "text": "edited" })],
        },
    )?;
    drain_notifications(transport, &mut session)?;
    update_queue(transport, &session, item_id.clone(), QueueAction::Remove)?;
    drain_notifications(transport, &mut session)?;
    if session.queue_item(&item_id).is_some() {
        return Err("queue remove did not converge to the host snapshot".into());
    }
    eprintln!("dsh-pager: queue smoke ok");
    Ok(())
}

fn run_lifecycle_smoke(
    transport: &mut RpcTransport,
    mut session: dsh_pager::SessionState,
) -> Result<(), Box<dyn Error>> {
    let renamed = rename_session(transport, &mut session, "native lifecycle smoke".into())?;
    if renamed.title != "native lifecycle smoke" {
        return Err("rename receipt did not preserve the accepted title".into());
    }
    drain_notifications(transport, &mut session)?;
    let forked = fork_session(transport, &session, None)?;
    if forked.session_id.is_empty() || forked.session_id == session.session_id() {
        return Err("fork receipt did not return a distinct child session".into());
    }
    eprintln!(
        "dsh-pager: lifecycle smoke ok title={} child={}",
        renamed.title, forked.session_id
    );
    Ok(())
}
