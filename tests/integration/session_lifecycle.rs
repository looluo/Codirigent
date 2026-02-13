//! Integration tests for session lifecycle.

use crate::integration::helpers::TestFixture;
use codirigent_core::{CodirigentEvent, EventBus, SessionManager, SessionStatus};

#[tokio::test]
async fn test_session_complete_lifecycle() {
    // Setup
    let fixture = TestFixture::new().expect("Failed to create test fixture");

    // Subscribe to events before creating session
    let mut rx = fixture.event_bus.subscribe();

    // Test: Create session
    let session_id = fixture
        .create_session("test-session")
        .expect("Failed to create session");

    // Verify: SessionCreated event published
    let event = tokio::time::timeout(std::time::Duration::from_millis(1000), async {
        while let Ok(event) = rx.recv().await {
            if matches!(event, CodirigentEvent::SessionCreated { id } if id == session_id) {
                return Some(event);
            }
        }
        None
    })
    .await;

    assert!(
        event.is_ok() && event.unwrap().is_some(),
        "SessionCreated event not received"
    );

    // Test: Session starts in Idle status
    {
        let manager = fixture.session_manager.lock().unwrap();
        let session = manager.get_session(session_id).expect("Session not found");
        assert_eq!(session.status, SessionStatus::Idle);
    }

    // Test: Close session
    fixture
        .session_manager
        .lock()
        .unwrap()
        .close_session(session_id)
        .expect("Failed to close session");

    // Verify: SessionClosed event published
    let event = tokio::time::timeout(std::time::Duration::from_millis(1000), async {
        while let Ok(event) = rx.recv().await {
            if matches!(event, CodirigentEvent::SessionClosed { id } if id == session_id) {
                return Some(event);
            }
        }
        None
    })
    .await;

    assert!(
        event.is_ok() && event.unwrap().is_some(),
        "SessionClosed event not received"
    );
}

#[tokio::test]
async fn test_multiple_sessions_isolated() {
    // Setup
    let fixture = TestFixture::new().expect("Failed to create test fixture");

    // Test: Create 3 sessions
    let session1 = fixture
        .create_session("session-1")
        .expect("Failed to create session 1");
    let session2 = fixture
        .create_session("session-2")
        .expect("Failed to create session 2");
    let session3 = fixture
        .create_session("session-3")
        .expect("Failed to create session 3");

    // Verify: All sessions exist and are isolated
    let manager = fixture.session_manager.lock().unwrap();
    assert!(manager.get_session(session1).is_some());
    assert!(manager.get_session(session2).is_some());
    assert!(manager.get_session(session3).is_some());
    assert_eq!(manager.session_count(), 3);

    // Verify: Sessions have unique IDs
    assert_ne!(session1, session2);
    assert_ne!(session2, session3);
    assert_ne!(session1, session3);
}
