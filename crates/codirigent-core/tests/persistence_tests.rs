//! Persistence service tests.
//!
//! Tests verify state save/load, checkpoint management, and error handling.

use codirigent_core::persistence::{PersistentSession, PersistentState};
use codirigent_core::persistence_service::{DefaultPersistenceService, PersistenceService};
use codirigent_core::types::session::generate_session_uuid;
use codirigent_core::{Session, SessionId, SessionStatus};
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test basic save and load of state.
#[test]
fn test_save_and_load_state() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    // Create a simple state with one session
    let session = Session {
        id: SessionId(1),
        session_uuid: generate_session_uuid(),
        name: "Test Session".to_string(),
        status: SessionStatus::Idle,
        working_directory: temp.path().to_path_buf(),
        shell: None,
        current_task: None,
        context_usage: None,
        created_at: chrono::Utc::now(),
        group: None,
        color: None,
        git_info: None,
        claude_session_id: None,
        codex_session_id: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_session_id: None,
    };

    let mut state = PersistentState::default();
    state
        .sessions
        .push(PersistentSession::from_session(&session));

    // Save state
    service.save_state(&state).unwrap();

    // Load state
    let loaded = service.load_state().unwrap();
    assert!(loaded.is_some());

    let loaded_state = loaded.unwrap();
    assert_eq!(loaded_state.sessions.len(), 1);
    assert_eq!(loaded_state.sessions[0].id, SessionId(1));
    assert_eq!(loaded_state.sessions[0].name, "Test Session");
}

/// Test loading when no state file exists.
#[test]
fn test_load_nonexistent_state() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    // Load when no state file exists
    let loaded = service.load_state().unwrap();
    assert!(loaded.is_none());
}

/// Test saving empty state.
#[test]
fn test_save_empty_state() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    let state = PersistentState::default();

    // Save empty state
    service.save_state(&state).unwrap();

    // Load and verify
    let loaded = service.load_state().unwrap().unwrap();
    assert_eq!(loaded.sessions.len(), 0);
}

/// Test overwriting existing state.
#[test]
fn test_overwrite_state() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    // Save initial state with 1 session
    let session1 = Session {
        id: SessionId(1),
        session_uuid: generate_session_uuid(),
        name: "Session 1".to_string(),
        status: SessionStatus::Idle,
        working_directory: temp.path().to_path_buf(),
        shell: None,
        current_task: None,
        context_usage: None,
        created_at: chrono::Utc::now(),
        group: None,
        color: None,
        git_info: None,
        claude_session_id: None,
        codex_session_id: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_session_id: None,
    };

    let mut state = PersistentState::default();
    state
        .sessions
        .push(PersistentSession::from_session(&session1));
    service.save_state(&state).unwrap();

    // Overwrite with 2 sessions
    let session2 = Session {
        id: SessionId(2),
        session_uuid: generate_session_uuid(),
        name: "Session 2".to_string(),
        status: SessionStatus::Idle,
        working_directory: temp.path().to_path_buf(),
        shell: None,
        current_task: None,
        context_usage: None,
        created_at: chrono::Utc::now(),
        group: None,
        color: None,
        git_info: None,
        claude_session_id: None,
        codex_session_id: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_session_id: None,
    };

    state
        .sessions
        .push(PersistentSession::from_session(&session2));
    service.save_state(&state).unwrap();

    // Load and verify
    let loaded = service.load_state().unwrap().unwrap();
    assert_eq!(loaded.sessions.len(), 2);
}

/// Test checkpoint creation.
#[test]
fn test_create_checkpoint() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    let state = PersistentState::default();

    // Create checkpoint
    let checkpoint = service
        .create_checkpoint("before-refactor", &state)
        .unwrap();

    assert!(!checkpoint.id.is_empty());
    assert_eq!(checkpoint.name, "before-refactor");
}

/// Test listing checkpoints.
#[test]
fn test_list_checkpoints() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    let state = PersistentState::default();

    // Initially no checkpoints
    let checkpoints = service.list_checkpoints().unwrap();
    assert_eq!(checkpoints.len(), 0);

    // Create some checkpoints
    let cp1 = service.create_checkpoint("checkpoint-1", &state).unwrap();

    // Small delay to ensure unique timestamps
    std::thread::sleep(std::time::Duration::from_millis(10));

    let cp2 = service.create_checkpoint("checkpoint-2", &state).unwrap();

    // Ensure they have different IDs
    assert_ne!(cp1.id, cp2.id);

    // List should have at least 2
    let checkpoints = service.list_checkpoints().unwrap();
    assert!(
        checkpoints.len() >= 2,
        "Expected at least 2 checkpoints, got {}",
        checkpoints.len()
    );
}

/// Test loading a specific checkpoint.
#[test]
fn test_load_checkpoint() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    let state = PersistentState::default();

    // Create checkpoint
    let created = service
        .create_checkpoint("test-checkpoint", &state)
        .unwrap();

    // Load it back
    let loaded = service.load_checkpoint(&created.id).unwrap();
    assert!(loaded.is_some());

    let checkpoint = loaded.unwrap();
    assert_eq!(checkpoint.id, created.id);
    assert_eq!(checkpoint.name, "test-checkpoint");
}

/// Test loading nonexistent checkpoint.
#[test]
fn test_load_nonexistent_checkpoint() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    // Try to load checkpoint that doesn't exist
    let loaded = service.load_checkpoint("nonexistent-id").unwrap();
    assert!(loaded.is_none());
}

/// Test deleting a checkpoint.
#[test]
fn test_delete_checkpoint() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    let state = PersistentState::default();

    // Create checkpoint
    let checkpoint = service.create_checkpoint("to-delete", &state).unwrap();

    // Verify it exists
    let loaded = service.load_checkpoint(&checkpoint.id).unwrap();
    assert!(loaded.is_some());

    // Delete it
    service.delete_checkpoint(&checkpoint.id).unwrap();

    // Verify it's gone
    let loaded = service.load_checkpoint(&checkpoint.id).unwrap();
    assert!(loaded.is_none());
}

/// Test multiple checkpoints are independent.
#[test]
fn test_multiple_checkpoints_independent() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    // Create different states
    let mut state1 = PersistentState::default();
    let session1 = Session {
        id: SessionId(1),
        session_uuid: generate_session_uuid(),
        name: "State 1".to_string(),
        status: SessionStatus::Idle,
        working_directory: temp.path().to_path_buf(),
        shell: None,
        current_task: None,
        context_usage: None,
        created_at: chrono::Utc::now(),
        group: None,
        color: None,
        git_info: None,
        claude_session_id: None,
        codex_session_id: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_session_id: None,
    };
    state1
        .sessions
        .push(PersistentSession::from_session(&session1));

    let mut state2 = PersistentState::default();
    let session2 = Session {
        id: SessionId(2),
        session_uuid: generate_session_uuid(),
        name: "State 2".to_string(),
        status: SessionStatus::Idle,
        working_directory: temp.path().to_path_buf(),
        shell: None,
        current_task: None,
        context_usage: None,
        created_at: chrono::Utc::now(),
        group: None,
        color: None,
        git_info: None,
        claude_session_id: None,
        codex_session_id: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_session_id: None,
    };
    state2
        .sessions
        .push(PersistentSession::from_session(&session2));

    // Create checkpoints
    let cp1 = service.create_checkpoint("checkpoint-1", &state1).unwrap();

    // Small delay to ensure unique timestamps
    std::thread::sleep(std::time::Duration::from_millis(10));

    let cp2 = service.create_checkpoint("checkpoint-2", &state2).unwrap();

    // Ensure they have different IDs
    assert_ne!(cp1.id, cp2.id, "Checkpoints should have unique IDs");

    // Load and verify they're different
    let loaded1 = service.load_checkpoint(&cp1.id).unwrap().unwrap();
    let loaded2 = service.load_checkpoint(&cp2.id).unwrap().unwrap();

    // Verify checkpoint names match
    assert_eq!(loaded1.name, "checkpoint-1");
    assert_eq!(loaded2.name, "checkpoint-2");

    // Verify states are correct
    assert!(
        !loaded1.state.sessions.is_empty(),
        "Checkpoint 1 should have sessions"
    );
    assert!(
        !loaded2.state.sessions.is_empty(),
        "Checkpoint 2 should have sessions"
    );

    // Verify the session names match what we saved
    assert_eq!(loaded1.state.sessions[0].name, "State 1");
    assert_eq!(loaded2.state.sessions[0].name, "State 2");
}

/// Test checkpoint sorting (newest first).
#[test]
fn test_checkpoint_sorting() {
    let temp = TempDir::new().unwrap();
    let service = DefaultPersistenceService::new(temp.path());

    let state = PersistentState::default();

    // Create checkpoints in order
    let cp1 = service.create_checkpoint("first", &state).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let cp2 = service.create_checkpoint("second", &state).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let cp3 = service.create_checkpoint("third", &state).unwrap();

    // List should be sorted newest first
    let checkpoints = service.list_checkpoints().unwrap();
    assert_eq!(checkpoints.len(), 3);

    // Newest (third) should be first
    assert_eq!(checkpoints[0].id, cp3.id);
    assert_eq!(checkpoints[1].id, cp2.id);
    assert_eq!(checkpoints[2].id, cp1.id);
}

/// Test persistent state default values.
#[test]
fn test_persistent_state_defaults() {
    let state = PersistentState::default();

    assert_eq!(state.sessions.len(), 0);
    assert!(state.active_session.is_none());
}

/// Test session to persistent session conversion.
#[test]
fn test_session_to_persistent_conversion() {
    let session = Session {
        id: SessionId(42),
        session_uuid: generate_session_uuid(),
        name: "Test".to_string(),
        status: SessionStatus::Working,
        working_directory: PathBuf::from("/tmp"),
        shell: None,
        current_task: None,
        context_usage: Some(0.5),
        created_at: chrono::Utc::now(),
        group: Some("backend".to_string()),
        color: Some("#ff0000".to_string()),
        git_info: None,
        claude_session_id: None,
        codex_session_id: None,
        codex_execution_mode: None,
        codex_started_at: None,
        gemini_session_id: None,
    };

    let persistent = PersistentSession::from_session(&session);

    assert_eq!(persistent.id, SessionId(42));
    assert_eq!(persistent.session_uuid, session.session_uuid);
    assert_eq!(persistent.name, "Test");
    assert_eq!(persistent.group, Some("backend".to_string()));
    assert_eq!(persistent.color, Some("#ff0000".to_string()));
}

/// Test legacy persisted sessions get a UUID assigned and keep it after save-back.
#[test]
fn test_legacy_persistent_session_missing_uuid_gets_generated_and_persists() {
    let legacy_json = json!({
        "id": 7,
        "name": "Legacy Session",
        "status": "Idle",
        "working_directory": "/tmp",
        "shell": null,
        "current_task": null,
        "worktree_path": null,
        "context_usage": null,
        "started_at": "2026-03-15T00:00:00Z",
        "last_checkpoint": "2026-03-15T00:00:00Z",
        "scrollback_hash": null,
        "group": null,
        "color": null,
        "claude_session_id": null,
        "codex_session_id": null,
        "codex_execution_mode": null,
        "codex_started_at": null,
        "gemini_session_id": null,
        "claude_permission_mode": null
    });

    let upgraded: PersistentSession = serde_json::from_value(legacy_json).unwrap();
    assert!(!upgraded.session_uuid.is_empty());

    let round_tripped: PersistentSession =
        serde_json::from_str(&serde_json::to_string(&upgraded).unwrap()).unwrap();
    assert_eq!(round_tripped.session_uuid, upgraded.session_uuid);
}
