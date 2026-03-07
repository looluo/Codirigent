//! CLI detection and formatting helpers.
//!
//! This module provides utilities for:
//! - Detecting CLI type from terminal output
//! - Formatting task prompts for different CLI types
//! - Getting CLI-specific clear/reset commands

use codirigent_filetree::TerminalPathStyle;
use std::path::Path;
use tracing::warn;

/// Format a task prompt for sending to a session's PTY.
///
/// Collapses multi-line prompts into a single line so newlines aren't
/// interpreted as individual Enter presses by the CLI. The caller is
/// responsible for scheduling a deferred Enter keypress.
///
/// # Arguments
/// * `prompt` - The multi-line task prompt
/// * `cli_type` - The CLI type running in the session
///
/// # Returns
/// Flattened single-line prompt (no trailing newline)
pub(super) fn format_task_input(prompt: &str, cli_type: codirigent_core::CliType) -> String {
    // Collapse the multi-line prompt into a single line so newlines
    // aren't interpreted as individual Enter presses by the CLI.
    let flat: String = prompt
        .lines()
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    match cli_type {
        codirigent_core::CliType::ClaudeCode
        | codirigent_core::CliType::GeminiCli
        | codirigent_core::CliType::CodexCli => {
            // No trailing newline — caller schedules a deferred Enter
            flat
        }
        codirigent_core::CliType::GenericShell => {
            warn!("format_task_input called with GenericShell — this should not happen");
            flat
        }
    }
}

/// Return the CLI-specific command to clear/reset context between tasks.
///
/// Different CLI tools use different commands to start fresh:
/// - Claude Code: `/clear`
/// - Codex CLI: `/new`
/// - Gemini CLI: `/clear`
/// - GenericShell: empty string (no clear command)
pub(super) fn clear_command(cli_type: codirigent_core::CliType) -> String {
    match cli_type {
        codirigent_core::CliType::ClaudeCode => "/clear".to_string(),
        codirigent_core::CliType::CodexCli => "/new".to_string(),
        codirigent_core::CliType::GeminiCli => "/clear".to_string(),
        codirigent_core::CliType::GenericShell => String::new(),
    }
}

/// Returns true when a persisted CLI session ID is syntactically safe to store
/// and replay in a resume command.
pub(super) fn is_safe_cli_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Resolve the shell style used for terminal path quoting.
pub(super) fn terminal_path_style(shell_name: Option<&str>) -> TerminalPathStyle {
    let resolved = codirigent_session::resolve_shell(shell_name.unwrap_or(""));
    let program_name = Path::new(&resolved.program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&resolved.program)
        .to_ascii_lowercase();

    if program_name.contains("pwsh") || program_name.contains("powershell") {
        TerminalPathStyle::PowerShell
    } else if program_name == "cmd" || program_name == "cmd.exe" {
        TerminalPathStyle::Cmd
    } else {
        TerminalPathStyle::Posix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_task_input_multiline() {
        let prompt = "Line 1\nLine 2\n\nLine 3";
        let result = format_task_input(prompt, codirigent_core::CliType::ClaudeCode);
        assert_eq!(result, "Line 1 Line 2 Line 3");
    }

    #[test]
    fn test_clear_command() {
        assert_eq!(
            clear_command(codirigent_core::CliType::ClaudeCode),
            "/clear"
        );
        assert_eq!(clear_command(codirigent_core::CliType::CodexCli), "/new");
        assert_eq!(clear_command(codirigent_core::CliType::GeminiCli), "/clear");
        assert_eq!(clear_command(codirigent_core::CliType::GenericShell), "");
    }

    #[test]
    fn safe_cli_session_id_rejects_shell_metacharacters() {
        assert!(is_safe_cli_session_id(
            "019cc946-93b2-7172-98ee-43de7665d6ff"
        ));
        assert!(is_safe_cli_session_id(
            "7e28141b-d05e-48f5-a4dd-ad4995afd002"
        ));
        assert!(!is_safe_cli_session_id("bad;id"));
        assert!(!is_safe_cli_session_id("bad id"));
        assert!(!is_safe_cli_session_id("bad\nid"));
    }
}
