//! Tests for the session sidebar component.

use super::*;
use std::path::PathBuf;

fn create_test_session(id: u64, name: &str, status: SessionStatus) -> Session {
    Session {
        id: SessionId(id),
        name: name.to_string(),
        status,
        working_directory: PathBuf::from("/tmp"),
        current_task: None,
        context_usage: None,
        created_at: chrono::Utc::now(),
        group: None,
        color: None,
    }
}

fn create_grouped_session(
    id: u64,
    name: &str,
    status: SessionStatus,
    group: &str,
    color: &str,
) -> Session {
    Session {
        id: SessionId(id),
        name: name.to_string(),
        status,
        working_directory: PathBuf::from("/tmp"),
        current_task: None,
        context_usage: None,
        created_at: chrono::Utc::now(),
        group: Some(group.to_string()),
        color: Some(color.to_string()),
    }
}

#[test]
fn test_sidebar_new() {
    let sidebar = SessionSidebar::new();
    assert!(sidebar.sessions().is_empty());
    assert!(sidebar.focused_session().is_none());
    assert_eq!(sidebar.width(), SessionSidebar::DEFAULT_WIDTH);
}

#[test]
fn test_sidebar_default() {
    let sidebar = SessionSidebar::default();
    assert!(sidebar.sessions().is_empty());
}

#[test]
fn test_update_sessions() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![
        create_test_session(1, "Session 1", SessionStatus::Idle),
        create_test_session(2, "Session 2", SessionStatus::Working),
    ]);
    assert_eq!(sidebar.session_count(), 2);
}

#[test]
fn test_set_focused() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_test_session(1, "S", SessionStatus::Idle)]);
    sidebar.set_focused(SessionId(1));
    assert_eq!(sidebar.focused_session(), Some(SessionId(1)));
}

#[test]
fn test_click_session() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_test_session(1, "S", SessionStatus::Idle)]);
    sidebar.click_session(SessionId(1));
    assert_eq!(sidebar.focused_session(), Some(SessionId(1)));
    assert_eq!(
        sidebar.take_events(),
        vec![SidebarEvent::FocusSession(SessionId(1))]
    );
}

#[test]
fn test_renaming_flow() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_test_session(1, "Old", SessionStatus::Idle)]);
    sidebar.start_renaming(SessionId(1));
    assert_eq!(sidebar.edit_buffer(), "Old");
    sidebar.update_edit_buffer("New".to_string());
    sidebar.finish_renaming();
    assert!(sidebar.editing_session().is_none());
    let events = sidebar.take_events();
    assert!(
        matches!(&events[0], SidebarEvent::RenameSession { new_name, .. } if new_name == "New")
    );
}

#[test]
fn test_cancel_renaming() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_test_session(1, "S", SessionStatus::Idle)]);
    sidebar.start_renaming(SessionId(1));
    sidebar.cancel_renaming();
    assert!(sidebar.editing_session().is_none());
    assert!(sidebar.edit_buffer().is_empty());
}

#[test]
fn test_request_events() {
    let mut sidebar = SessionSidebar::new();
    sidebar.request_new_session();
    sidebar.request_close_session(SessionId(1));
    let events = sidebar.take_events();
    assert_eq!(events[0], SidebarEvent::NewSession);
    assert_eq!(events[1], SidebarEvent::CloseSession(SessionId(1)));
}

#[test]
fn test_grouped_sessions() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![
        create_test_session(1, "Ungrouped", SessionStatus::Idle),
        create_grouped_session(2, "B1", SessionStatus::Working, "Backend", "#FF0000"),
    ]);
    let grouped = sidebar.sessions_by_group();
    assert_eq!(grouped.get(&None).map(|v| v.len()), Some(1));
    assert_eq!(
        grouped.get(&Some("Backend".to_string())).map(|v| v.len()),
        Some(1)
    );
}

#[test]
fn test_toggle_group() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_grouped_session(
        1,
        "S",
        SessionStatus::Idle,
        "G",
        "#F00",
    )]);
    assert!(sidebar.is_group_expanded("G"));
    sidebar.toggle_group("G");
    assert!(!sidebar.is_group_expanded("G"));
}

#[test]
fn test_group_counts() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![
        create_test_session(1, "U", SessionStatus::Idle),
        create_grouped_session(2, "B1", SessionStatus::Working, "Backend", "#FF0000"),
        create_grouped_session(3, "B2", SessionStatus::Done, "Backend", "#FF0000"),
    ]);
    assert_eq!(sidebar.group_session_count("Backend"), 2);
    assert_eq!(sidebar.ungrouped_session_count(), 1);
    assert_eq!(sidebar.session_count(), 3);
}

#[test]
fn test_width_clamping() {
    let mut sidebar = SessionSidebar::new();
    sidebar.set_width(100.0);
    assert_eq!(sidebar.width(), SessionSidebar::MIN_WIDTH);
    sidebar.set_width(500.0);
    assert_eq!(sidebar.width(), SessionSidebar::MAX_WIDTH);
}

#[test]
fn test_color_from_hex() {
    let c = Color::from_hex("#FF5733");
    assert!((c.r - 1.0).abs() < 0.01);
    let inv = Color::from_hex("invalid");
    assert_eq!(inv.r, 0.5);
}

#[test]
fn test_status_colors() {
    let colors = StatusColors::default();
    assert_ne!(colors.idle.r, colors.working.r);
    let _ = colors.color_for(SessionStatus::WaitingForInput);
}

#[test]
fn test_session_group() {
    let mut g = SessionGroup::new("G".to_string(), "#F00".to_string());
    assert!(g.expanded);
    g.toggle();
    assert!(!g.expanded);
}

#[test]
fn test_render_hints_empty() {
    let hints = SessionSidebar::new().render_hints();
    assert!(hints.items.is_empty());
}

#[test]
fn test_render_hints_with_sessions() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_test_session(1, "S", SessionStatus::Idle)]);
    let hints = sidebar.render_hints();
    assert_eq!(hints.items.len(), 1);
    assert!(matches!(&hints.items[0], SidebarItem::Session { id, .. } if *id == SessionId(1)));
}

#[test]
fn test_render_hints_with_groups() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![
        create_test_session(1, "U", SessionStatus::Idle),
        create_grouped_session(2, "B", SessionStatus::Working, "Backend", "#F00"),
    ]);
    let hints = sidebar.render_hints();
    // 1 ungrouped + 1 header + 1 grouped = 3 items
    assert_eq!(hints.items.len(), 3);
}

#[test]
fn test_render_hints_collapsed_group() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_grouped_session(
        1,
        "B",
        SessionStatus::Idle,
        "G",
        "#F00",
    )]);
    sidebar.toggle_group("G");
    let hints = sidebar.render_hints();
    // Only header, no sessions
    assert_eq!(hints.items.len(), 1);
}

#[test]
fn test_render_hints_height() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![
        create_test_session(1, "S1", SessionStatus::Idle),
        create_test_session(2, "S2", SessionStatus::Working),
    ]);
    let hints = sidebar.render_hints();
    // header (40) + 2 items (32 each) + new session button (44)
    assert_eq!(hints.total_height, 40.0 + 32.0 + 32.0 + 44.0);
}

#[test]
fn test_event_equality() {
    assert_eq!(SidebarEvent::NewSession, SidebarEvent::NewSession);
    assert_ne!(
        SidebarEvent::FocusSession(SessionId(1)),
        SidebarEvent::FocusSession(SessionId(2))
    );
}

#[test]
fn test_with_status_colors() {
    let mut colors = StatusColors::default();
    colors.idle = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let sidebar = SessionSidebar::with_status_colors(colors);
    let c = sidebar.status_color(SessionStatus::Idle);
    assert_eq!(c.r, 1.0);
}

#[test]
fn test_color_rgba() {
    let c = Color::rgba(0.5, 0.6, 0.7, 0.8);
    assert_eq!(c.r, 0.5);
    assert_eq!(c.g, 0.6);
    assert_eq!(c.b, 0.7);
    assert_eq!(c.a, 0.8);
}

#[test]
fn test_session_group_default() {
    let g = SessionGroup::default();
    assert_eq!(g.name, "Default");
    assert_eq!(g.color, "#6c7086");
    assert!(g.expanded);
}

#[test]
fn test_group_names_sorted() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![
        create_grouped_session(1, "Z", SessionStatus::Idle, "Zebra", "#000"),
        create_grouped_session(2, "A", SessionStatus::Idle, "Alpha", "#000"),
    ]);
    let names = sidebar.group_names();
    assert_eq!(names[0], "Alpha");
    assert_eq!(names[1], "Zebra");
}

#[test]
fn test_get_group() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_grouped_session(
        1,
        "S",
        SessionStatus::Idle,
        "G",
        "#F00",
    )]);
    assert!(sidebar.get_group("G").is_some());
    assert!(sidebar.get_group("X").is_none());
}

#[test]
fn test_render_hints_focused() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![
        create_test_session(1, "S1", SessionStatus::Idle),
        create_test_session(2, "S2", SessionStatus::Working),
    ]);
    sidebar.set_focused(SessionId(2));
    let hints = sidebar.render_hints();
    if let SidebarItem::Session { is_focused, .. } = &hints.items[1] {
        assert!(is_focused);
    }
}

#[test]
fn test_render_hints_editing() {
    let mut sidebar = SessionSidebar::new();
    sidebar.update_sessions(vec![create_test_session(1, "S", SessionStatus::Idle)]);
    sidebar.start_renaming(SessionId(1));
    let hints = sidebar.render_hints();
    if let SidebarItem::Session { is_editing, .. } = &hints.items[0] {
        assert!(is_editing);
    }
}

#[test]
fn test_take_events_clears() {
    let mut sidebar = SessionSidebar::new();
    sidebar.request_new_session();
    let _ = sidebar.take_events();
    assert!(sidebar.take_events().is_empty());
}
