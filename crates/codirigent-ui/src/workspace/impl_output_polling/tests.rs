use super::*;
use crate::workspace::types::{CachedCliStatus, CliStatusSource};
use codirigent_core::{DefaultEventBus, ImageData, ImageFormat, SessionStatus};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[test]
fn detector_maintenance_merge_dedupes_and_preserves_priority() {
    let merged = merge_detector_maintenance_session_ids(
        vec![SessionId(2), SessionId(1), SessionId(2)],
        vec![SessionId(3), SessionId(1), SessionId(4)],
    );

    assert_eq!(
        merged,
        vec![SessionId(2), SessionId(1), SessionId(3), SessionId(4)]
    );
}

#[test]
fn detector_maintenance_batch_includes_stale_cached_sessions() {
    let detector = Arc::new(Mutex::new(codirigent_detector::InputDetector::new(
        codirigent_detector::DetectorConfig::default(),
        Arc::new(DefaultEventBus::new(16)),
    )));
    let cli_readers = Arc::new(Mutex::new(super::super::types::CliReaders::new()));
    let stale_id = SessionId(17);

    cli_readers
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .cached_status
        .insert(
            stale_id,
            CachedCliStatus {
                status: SessionStatus::NeedsAttention,
                seen_at: Instant::now(),
                source: CliStatusSource::Hook,
                status_since: Instant::now(),
                ttl: Duration::from_secs(30),
            },
        );

    let batch = collect_detector_maintenance_batch(&detector, &cli_readers);
    assert_eq!(batch.session_ids, vec![stale_id]);
}

#[test]
fn tiny_dib_preview_is_suppressed() {
    let image = ImageData {
        bytes: vec![0; 8 * 1024],
        width: 32,
        height: 32,
        format: ImageFormat::Dib,
    };

    assert!(!should_show_clipboard_preview(&image));
}

#[test]
fn larger_dib_preview_is_allowed() {
    let image = ImageData {
        bytes: vec![0; 40 * 1024],
        width: 320,
        height: 240,
        format: ImageFormat::Dib,
    };

    assert!(should_show_clipboard_preview(&image));
}

#[test]
fn focused_schedulable_output_is_prioritized() {
    let session_ids = vec![SessionId(1), SessionId(2), SessionId(3)];
    let schedulable = HashSet::from([SessionId(2), SessionId(3)]);

    let (ready, deferred) =
        prioritize_and_partition_output_sessions(session_ids, Some(SessionId(2)), |id| {
            schedulable.contains(&id)
        });

    assert_eq!(ready, vec![SessionId(2), SessionId(3)]);
    assert_eq!(deferred, vec![SessionId(1)]);
}

#[test]
fn unschedulable_output_sessions_are_deferred_instead_of_dropped() {
    let session_ids = vec![SessionId(1), SessionId(2), SessionId(3)];
    let schedulable = HashSet::from([SessionId(3)]);

    let (ready, deferred) =
        prioritize_and_partition_output_sessions(session_ids, Some(SessionId(2)), |id| {
            schedulable.contains(&id)
        });

    assert_eq!(ready, vec![SessionId(3)]);
    assert_eq!(deferred, vec![SessionId(2), SessionId(1)]);
}
