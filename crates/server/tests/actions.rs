//! Approval, elicitation and inbox integration tests.

use sqlx::SqlitePool;
use threadbase_server::auth::CurrentUser;
use threadbase_server::events::{EventMeta, EventStore};
use threadbase_server::{approvals, auth, db, elicitations, inbox};
use threadbase_shared::{
    now_ms, Actor, ApprovalDecision, ApprovalOption, ApprovalScope, ApprovalStatus,
    ElicitationAction, ElicitationDto, ElicitationMode, ElicitationStatus, ErrorCode, ProjectId,
    Role, ThreadId, UserDto, UserId,
};
use ulid::Ulid;

fn test_projection(pool: &SqlitePool) -> threadbase_server::projection::Projection {
    let (state_tx, _) = tokio::sync::broadcast::channel(16);
    threadbase_server::projection::Projection::new(pool.clone(), state_tx)
}

struct Fixture {
    pool: SqlitePool,
    events: EventStore,
    owner: CurrentUser,
    viewer: CurrentUser,
    project: ProjectId,
    thread: ThreadId,
}

async fn fixture() -> Fixture {
    let pool = db::connect_memory().await.unwrap();
    let owner_user = auth::ensure_identity_user(
        &pool,
        &auth::Identity::github(1, "ada", Some("Ada"), None),
        &[],
    )
    .await
    .unwrap();
    let viewer_user = make_user(&pool, "bob", false).await;
    let project = ProjectId::new();
    let thread = ThreadId::new();
    let now = now_ms();
    sqlx::query(
        "INSERT INTO projects(id, name, path, default_branch, created_by, created_at, updated_at)
         VALUES(?, 'repo', '/tmp/repo', 'main', ?, ?, ?)",
    )
    .bind(project.as_str())
    .bind(owner_user.id.as_str())
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO threads(id, project_id, title, status, agent, branch, base_branch, created_by, created_at, updated_at)
         VALUES(?, ?, 'Test', 'idle', 'codex', 'thread/test', 'main', ?, ?, ?)",
    )
    .bind(thread.as_str())
    .bind(project.as_str())
    .bind(owner_user.id.as_str())
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    for (user, role) in [(&owner_user, "owner"), (&viewer_user, "viewer")] {
        sqlx::query(
            "INSERT INTO thread_members(thread_id, user_id, role, joined_at) VALUES(?, ?, ?, ?)",
        )
        .bind(thread.as_str())
        .bind(user.id.as_str())
        .bind(role)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }
    Fixture {
        events: EventStore::new(pool.clone()),
        pool,
        owner: CurrentUser {
            user: owner_user,
            method: "github",
        },
        viewer: CurrentUser {
            user: viewer_user,
            method: "github",
        },
        project,
        thread,
    }
}

async fn make_user(pool: &SqlitePool, name: &str, admin: bool) -> UserDto {
    let id = Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO users(id, username, display_name, is_admin, created_at) VALUES(?,?,?,?,?)",
    )
    .bind(&id)
    .bind(name)
    .bind(name)
    .bind(i64::from(admin))
    .bind(now_ms())
    .execute(pool)
    .await
    .unwrap();
    UserDto {
        id: UserId::from(id),
        username: name.into(),
        display_name: name.into(),
        avatar_url: None,
        is_admin: admin,
        access: threadbase_shared::UserAccess::Member,
        hidden_presence: false,
        created_at: now_ms(),
    }
}

async fn new_approval(fixture: &Fixture) -> threadbase_shared::ApprovalDto {
    approvals::create(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        approvals::NewApproval {
            id: Ulid::new().to_string(),
            thread_id: fixture.thread.0.clone(),
            subagent_id: None,
            turn_id: None,
            tool_call_id: None,
            agent: "codex".into(),
            title: "Run tests".into(),
            kind: "command".into(),
            command: Some("cargo test".into()),
            cwd: None,
            paths: vec![],
            options: vec![
                ApprovalOption {
                    id: "once".into(),
                    kind: ApprovalDecision::AllowOnce,
                    label: "Allow once".into(),
                },
                ApprovalOption {
                    id: "thread".into(),
                    kind: ApprovalDecision::AllowThread,
                    label: "Allow thread".into(),
                },
                ApprovalOption {
                    id: "deny".into(),
                    kind: ApprovalDecision::Deny,
                    label: "Deny".into(),
                },
            ],
            input: Some(r#"{"command":"cargo test"}"#.into()),
        },
        Actor::agent("codex"),
        EventMeta::default(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn approvals_are_visible_and_viewers_cannot_decide() {
    let fixture = fixture().await;
    let approval = new_approval(&fixture).await;
    assert_eq!(approval.status, ApprovalStatus::Pending);
    let pending = approvals::list_pending_for_thread(&fixture.pool, fixture.thread.as_str())
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);

    let err = approvals::decide(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        approval.id.as_str(),
        &fixture.viewer,
        ApprovalDecision::AllowOnce,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Forbidden);

    let decided = approvals::decide(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        approval.id.as_str(),
        &fixture.owner,
        ApprovalDecision::AllowThread,
        Some("trusted command"),
    )
    .await
    .unwrap();
    assert_eq!(decided.status, ApprovalStatus::Allowed);
    assert_eq!(decided.decision, Some(ApprovalDecision::AllowThread));
    assert_eq!(decided.decided_by.as_ref().unwrap().username, "ada");

    let audit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE action = 'approval.decided' AND target_id = ?",
    )
    .bind(approval.id.as_str())
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(audit, 1);
    let rules = approvals::list_rules(&fixture.pool).await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].scope, ApprovalScope::Thread);
    assert_eq!(
        rules[0].thread_id.as_ref().unwrap().as_str(),
        fixture.thread.as_str()
    );
    approvals::delete_rule(&fixture.pool, &rules[0].id)
        .await
        .unwrap();
    assert!(approvals::list_rules(&fixture.pool)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn inbox_tracks_pending_items_and_read_state() {
    let fixture = fixture().await;
    let _ = new_approval(&fixture).await;
    let inbox = inbox::list(&fixture.pool, &fixture.owner, false)
        .await
        .unwrap();
    assert_eq!(inbox.items.len(), 1);
    assert_eq!(inbox.unread, 1);
    let read = inbox::mark_read(
        &fixture.pool,
        &fixture.owner,
        Some(fixture.thread.as_str()),
        false,
    )
    .await
    .unwrap();
    assert_eq!(read, 1);
    let inbox = inbox::list(&fixture.pool, &fixture.owner, false)
        .await
        .unwrap();
    assert_eq!(inbox.unread, 0);
    assert_eq!(inbox.items[0].project_id, fixture.project);
}

fn elicitation(fixture: &Fixture, schema: serde_json::Value) -> ElicitationDto {
    ElicitationDto {
        id: Ulid::new().to_string().into(),
        thread_id: fixture.thread.clone(),
        subagent_id: None,
        tool_call_id: None,
        agent: "codex".into(),
        message: "Which environment?".into(),
        mode: ElicitationMode::Form,
        requested_schema: schema,
        blocking: true,
        status: ElicitationStatus::Pending,
        response: None,
        responded_by: None,
        responded_at: None,
        created_at: now_ms(),
    }
}

#[tokio::test]
async fn first_elicitation_response_wins_and_secrets_are_rejected() {
    let fixture = fixture().await;
    let secret = elicitation(
        &fixture,
        serde_json::json!({"type":"object","properties":{"password":{"type":"string"}}}),
    );
    elicitations::create(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        secret.clone(),
        Actor::agent("codex"),
        EventMeta::default(),
    )
    .await
    .unwrap();
    let err = elicitations::respond_for_user(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        secret.id.as_str(),
        &fixture.owner,
        ElicitationAction::Accept,
        Some(serde_json::json!({"password":"hunter2"})),
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Validation);

    let form = elicitation(
        &fixture,
        serde_json::json!({"type":"object","properties":{"env":{"type":"string"}}}),
    );
    elicitations::create(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        form.clone(),
        Actor::agent("codex"),
        EventMeta::default(),
    )
    .await
    .unwrap();
    let answered = elicitations::respond_for_user(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        form.id.as_str(),
        &fixture.owner,
        ElicitationAction::Accept,
        Some(serde_json::json!({"env":"staging"})),
    )
    .await
    .unwrap();
    assert_eq!(answered.status, ElicitationStatus::Accepted);
    assert_eq!(answered.responded_by.as_ref().unwrap().username, "ada");
    let err = elicitations::respond_for_user(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        form.id.as_str(),
        &fixture.viewer,
        ElicitationAction::Decline,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.0.code, ErrorCode::Conflict);
}

#[tokio::test]
async fn subagent_outside_worktree_does_not_get_a_reusable_rule() {
    let fixture = fixture().await;
    sqlx::query("UPDATE threads SET worktree_path = '/tmp/thread-worktree' WHERE id = ?")
        .bind(fixture.thread.as_str())
        .execute(&fixture.pool)
        .await
        .unwrap();
    let approval = approvals::create(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        approvals::NewApproval {
            id: Ulid::new().to_string(),
            thread_id: fixture.thread.0.clone(),
            subagent_id: Some("sub-1".into()),
            turn_id: None,
            tool_call_id: None,
            agent: "codex".into(),
            title: "Run command".into(),
            kind: "command".into(),
            command: Some("make".into()),
            cwd: Some("/tmp/outside".into()),
            paths: vec![],
            options: vec![],
            input: None,
        },
        Actor::agent("codex"),
        EventMeta::default(),
    )
    .await
    .unwrap();
    let decided = approvals::decide(
        &fixture.pool,
        &test_projection(&fixture.pool),
        &fixture.events,
        approval.id.as_str(),
        &fixture.owner,
        ApprovalDecision::AllowThread,
        None,
    )
    .await
    .unwrap();
    assert_eq!(decided.status, ApprovalStatus::Allowed);
    assert_eq!(decided.scope, Some(ApprovalScope::Once));
    assert!(approvals::list_rules(&fixture.pool)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn decline_and_cancel_have_distinct_states() {
    let fixture = fixture().await;
    for (action, expected) in [
        (ElicitationAction::Decline, ElicitationStatus::Declined),
        (ElicitationAction::Cancel, ElicitationStatus::Cancelled),
    ] {
        let question = elicitation(&fixture, serde_json::json!({"type":"object"}));
        elicitations::create(
            &fixture.pool,
            &test_projection(&fixture.pool),
            &fixture.events,
            question.clone(),
            Actor::agent("codex"),
            EventMeta::default(),
        )
        .await
        .unwrap();
        let answered = elicitations::respond_for_user(
            &fixture.pool,
            &test_projection(&fixture.pool),
            &fixture.events,
            question.id.as_str(),
            &fixture.owner,
            action,
            None,
        )
        .await
        .unwrap();
        assert_eq!(answered.status, expected);
    }
    let _ = Role::Owner;
}
