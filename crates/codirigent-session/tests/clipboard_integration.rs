//! Integration tests for clipboard functionality.

use codirigent_core::{CliType, ClipboardContent, ImageData, ImageFormat, SessionId};
use codirigent_session::{ClipboardService, DefaultClipboardService};
use tempfile::tempdir;

#[test]
fn test_clipboard_workflow_text() {
    let temp = tempdir().unwrap();
    let service = DefaultClipboardService::new(temp.path());

    let content = ClipboardContent::Text("Hello, world!".to_string());
    let formatted = service.format_for_cli(&content, CliType::ClaudeCode).unwrap();

    assert_eq!(formatted, "Hello, world!");
}

#[test]
fn test_clipboard_workflow_image_path_formatting() {
    let temp = tempdir().unwrap();
    let service = DefaultClipboardService::new(temp.path());

    // Create and save an image
    let image = ImageData {
        bytes: vec![0x89, 0x50, 0x4E, 0x47], // PNG header
        width: 100,
        height: 100,
        format: ImageFormat::Png,
    };

    let saved_path = service.save_image(&image).unwrap();
    assert!(saved_path.exists());

    // Test formatting for different CLI types
    let content = ClipboardContent::Image(image);

    let claude_fmt = service.format_for_cli(&content, CliType::ClaudeCode).unwrap();
    assert!(!claude_fmt.starts_with('@'));

    let gemini_fmt = service.format_for_cli(&content, CliType::GeminiCli).unwrap();
    assert!(gemini_fmt.starts_with('@'));
}

#[test]
fn test_session_cli_type_tracking() {
    let temp = tempdir().unwrap();
    let mut service = DefaultClipboardService::new(temp.path());

    let session1 = SessionId(1);
    let session2 = SessionId(2);

    // Default should be ClaudeCode
    assert_eq!(service.get_session_cli_type(session1), CliType::ClaudeCode);

    // Set session 1 to Gemini
    service.set_session_cli_type(session1, CliType::GeminiCli);
    assert_eq!(service.get_session_cli_type(session1), CliType::GeminiCli);

    // Session 2 should still be default
    assert_eq!(service.get_session_cli_type(session2), CliType::ClaudeCode);
}

#[test]
fn test_temp_file_lifecycle() {
    let temp = tempdir().unwrap();
    let service = DefaultClipboardService::new(temp.path());

    // Verify temp directory
    let temp_dir = service.temp_dir();
    assert!(temp_dir.ends_with("temp"));

    // Save an image
    let image = ImageData {
        bytes: vec![0xFF, 0xD8, 0xFF], // JPEG header
        width: 50,
        height: 50,
        format: ImageFormat::Jpeg,
    };

    let saved_path = service.save_image(&image).unwrap();
    assert!(saved_path.exists());
    assert!(saved_path.extension().unwrap() == "jpg");

    // Cleanup should not remove fresh files
    let cleaned = service.cleanup_temp_files(std::time::Duration::from_secs(3600)).unwrap();
    assert_eq!(cleaned, 0);
    assert!(saved_path.exists());
}
