//! Idle-window deploys: the busy check and the binary swap.

mod support;

use std::os::unix::fs::PermissionsExt;
use support::fixture;
use tempfile::TempDir;
use threadbase_server::config::Config;
use threadbase_server::deploy;
use threadbase_shared::{now_ms, ThreadStatus};

/// Only work that a restart would damage counts as busy.
#[tokio::test]
async fn only_running_work_blocks_a_deploy() {
    let (fixture, _app) = fixture(Config::default()).await;
    let pool = &fixture.pool;
    let thread = fixture.thread_id.as_str();
    assert!(!deploy::busy(pool).await.unwrap(), "idle workspace");

    sqlx::query("UPDATE threads SET status = 'running' WHERE id = ?")
        .bind(thread)
        .execute(pool)
        .await
        .unwrap();
    assert!(deploy::busy(pool).await.unwrap(), "running turn");
    sqlx::query("UPDATE threads SET status = 'idle' WHERE id = ?")
        .bind(thread)
        .execute(pool)
        .await
        .unwrap();

    // A pending approval is waiting on a person; a restart would cancel it.
    let now = now_ms();
    sqlx::query(
        "INSERT INTO approvals(id, thread_id, agent, title, kind, paths_json, options_json,
            status, created_at)
         VALUES('a1', ?, 'codex', 'Run tests', 'command', '[]', '[]', 'pending', ?)",
    )
    .bind(thread)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    assert!(deploy::busy(pool).await.unwrap(), "pending approval");
    sqlx::query("UPDATE approvals SET status = 'approved' WHERE id = 'a1'")
        .execute(pool)
        .await
        .unwrap();
    assert!(!deploy::busy(pool).await.unwrap());

    sqlx::query(
        "INSERT INTO queued_inputs(id, thread_id, author_id, text, status, position,
            created_at, updated_at)
         VALUES('q1', ?, ?, 'waiting', 'queued', 0, ?, ?)",
    )
    .bind(thread)
    .bind(fixture.current.user.id.as_str())
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    assert!(deploy::busy(pool).await.unwrap(), "queued input");
    sqlx::query("DELETE FROM queued_inputs WHERE id = 'q1'")
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO subagents(id, thread_id, title, status, can_cancel, last_seq,
            created_at, updated_at)
         VALUES('s1', ?, 'reviewer', 'running', 1, 0, ?, ?)",
    )
    .bind(thread)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    assert!(deploy::busy(pool).await.unwrap(), "running subagent");
    sqlx::query("DELETE FROM subagents WHERE id = 's1'")
        .execute(pool)
        .await
        .unwrap();

    // A thread that is only archived or idle is not work.
    sqlx::query("UPDATE threads SET status = ? WHERE id = ?")
        .bind(ThreadStatus::Archived.as_str())
        .bind(thread)
        .execute(pool)
        .await
        .unwrap();
    assert!(!deploy::busy(pool).await.unwrap(), "archived thread");
}

#[tokio::test]
async fn installing_keeps_the_previous_binary_and_consumes_the_staged_one() {
    let dir = TempDir::new().unwrap();
    let staged = dir.path().join("incoming");
    std::fs::create_dir_all(&staged).unwrap();
    let incoming = staged.join("threadbase");
    std::fs::write(&incoming, b"new binary").unwrap();
    std::fs::set_permissions(&incoming, std::fs::Permissions::from_mode(0o755)).unwrap();
    let target = dir.path().join("threadbase");
    std::fs::write(&target, b"old binary").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    let backups = dir.path().join("backups");

    let backup = deploy::install(&incoming, &target, &backups).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"new binary");
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o755,
        "the installed binary stays executable"
    );
    assert_eq!(std::fs::read(&backup).unwrap(), b"old binary");
    assert!(
        !staged.join("threadbase").exists(),
        "the staged file is consumed so it cannot install twice"
    );
    assert!(staged.join("threadbase.installed").exists());
}

#[tokio::test]
async fn a_non_executable_or_missing_file_is_not_staged() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("threadbase");
    assert!(!deploy::is_staged(&path).await, "missing file");
    std::fs::write(&path, b"not runnable").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!deploy::is_staged(&path).await, "not executable");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(deploy::is_staged(&path).await);
}

/// The deploy loop must not restart the service itself: a restart stops this
/// unit and waits for this process to exit, so it cannot return while it is
/// still awaited. It installs, exits, and the service manager starts the new
/// binary — the path that used to deadlock until systemd killed the stop and
/// dropped the queued start.
#[test]
fn a_staged_binary_exits_for_the_service_manager_to_restart() {
    let data = TempDir::new().unwrap();
    let incoming = data.path().join("incoming");
    std::fs::create_dir_all(&incoming).unwrap();
    let staged = incoming.join("threadbase");
    // The real staged file is a build of this server, and `install` renames it
    // over the running executable, so it has to be this very binary.
    std::fs::copy(env!("CARGO_BIN_EXE_threadbase"), &staged).unwrap();
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).unwrap();

    // A stub service manager, so a regression fails here instead of bouncing
    // whatever unit happens to be installed on this machine.
    let bin = data.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let calls = data.path().join("service-manager-calls");
    for tool in ["systemctl", "launchctl"] {
        let stub = bin.join(tool);
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\necho {tool} \"$@\" >> {}\nexit 0\n",
                calls.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_threadbase"))
        .arg("serve")
        .env("THREADBASE_DATA_DIR", data.path())
        .env("THREADBASE_BIND", "127.0.0.1:0")
        .env("THREADBASE_ENV_FILE", data.path().join("no-env-file"))
        .env("THREADBASE_AUTO_DEPLOY", "1")
        .env("THREADBASE_AUTO_DEPLOY_PATH", &staged)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("RUST_LOG", "threadbase=warn")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn threadbase serve");

    // One poll interval plus startup.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the deploy loop did not exit; it must not restart itself");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };

    assert_eq!(
        status.code(),
        Some(threadbase_server::deploy::EXIT_RESTART),
        "a deploy exits with the restart code so the unit comes back"
    );
    assert!(
        !calls.exists(),
        "the deploy must not ask the service manager to restart it"
    );
}
