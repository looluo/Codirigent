//! Shell detection and resolution for cross-platform PTY sessions.
//!
//! Provides platform-specific shell detection:
//! - Unix: reads `$SHELL`, parses `/etc/shells`, `which` fallback
//! - Windows: probes for PowerShell 7, Windows PowerShell, cmd.exe
//!
//! Shell integration (OSC 7/133) setup for bash and zsh is also handled here.

#[cfg(unix)]
use anyhow::{Context, Result};
#[cfg(unix)]
use tracing::warn;

/// Windows process creation flag: suppress console window for spawned processes.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(windows)]
fn system32_executable(name: &str) -> std::path::PathBuf {
    std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join(name)
}

#[cfg(windows)]
fn command_exists(program: &std::path::Path, args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let mut cmd = Command::new(program);
    cmd.args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.status().map(|status| status.success()).unwrap_or(false)
}

#[cfg(windows)]
fn resolve_on_path(executable: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let where_exe = system32_executable("where.exe");
    let output = Command::new(where_exe)
        .arg(executable)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

#[cfg(windows)]
fn resolve_pwsh_path() -> Option<String> {
    let candidates = [
        std::env::var_os("ProgramFiles").map(std::path::PathBuf::from),
        std::env::var_os("ProgramW6432").map(std::path::PathBuf::from),
    ];

    for base in candidates.into_iter().flatten() {
        let path = base.join("PowerShell").join("7").join("pwsh.exe");
        if command_exists(&path, &["--version"]) {
            return Some(path.to_string_lossy().to_string());
        }
    }

    resolve_on_path("pwsh.exe")
        .filter(|path| command_exists(std::path::Path::new(path), &["--version"]))
}

#[cfg(windows)]
fn resolve_windows_powershell_path() -> Option<String> {
    let path = system32_executable(r"WindowsPowerShell\v1.0\powershell.exe");
    if command_exists(&path, &["-Command", "exit"]) {
        Some(path.to_string_lossy().to_string())
    } else {
        None
    }
}

#[cfg(windows)]
fn resolve_cmd_path() -> String {
    let path = system32_executable("cmd.exe");
    if path.is_file() {
        path.to_string_lossy().to_string()
    } else {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }
}

/// Shell command and arguments selected for a PTY session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCommand {
    /// The shell executable path or name.
    pub program: String,
    /// Arguments to pass to the shell.
    pub args: Vec<String>,
}

const TEST_LINE_FORWARDER_SENTINEL: &str = "__codirigent_test_line_forwarder__";

/// PowerShell initialization command for UTF-8 encoding and shell integration.
///
/// Sets up UTF-8 encoding and implements OSC 133 shell integration markers:
/// - D: Marks the finish of the previous command
/// - A: Marks the start of the prompt
/// - B: Marks the start of command input
///
/// Also implements OSC 7 for current working directory tracking.
#[cfg(windows)]
const POWERSHELL_INIT_COMMAND: &str = concat!(
    "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; ",
    "$OutputEncoding=[System.Text.Encoding]::UTF8; ",
    "function prompt { ",
    "$gle = $global:LASTEXITCODE; ",
    "if ($null -eq $gle) { $gle = 0 }; ",
    "$p = $executionContext.SessionState.Path.CurrentLocation.ProviderPath; ",
    "$h = [System.Net.Dns]::GetHostName(); ",
    "$u = $p.Replace('\\','/'); ",
    "\"$([char]27)]133;D;$gle$([char]7)\" + ",
    "\"$([char]27)]133;A$([char]7)\" + ",
    "\"$([char]27)]7;file://$h/$u$([char]27)\\\" + ",
    "\"PS $($executionContext.SessionState.Path.CurrentLocation)> \" + ",
    "\"$([char]27)]133;B$([char]7)\" ",
    "}",
);

/// Create a PowerShell `ShellCommand` with UTF-8 and shell integration init.
#[cfg(windows)]
pub fn setup_powershell_command(shell: &str) -> ShellCommand {
    ShellCommand {
        program: shell.to_string(),
        args: vec![
            "-NoLogo".to_string(),
            "-NoProfile".to_string(),
            "-NoExit".to_string(),
            "-Command".to_string(),
            POWERSHELL_INIT_COMMAND.to_string(),
        ],
    }
}

/// Detect the default shell for the current platform.
///
/// On Unix, returns the value of `$SHELL` or `/bin/bash` as fallback.
/// On Windows, prioritizes PowerShell 7 (pwsh), then Windows PowerShell,
/// then falls back to `%COMSPEC%` or `cmd.exe`.
///
/// The `CODIRIGENT_SHELL` env var always takes priority.
pub(crate) fn detect_shell_command() -> ShellCommand {
    if let Ok(shell) = std::env::var("CODIRIGENT_SHELL") {
        let args = std::env::var("CODIRIGENT_SHELL_ARGS")
            .ok()
            .map(|value| split_shell_args(&value))
            .unwrap_or_default();
        return ShellCommand {
            program: shell,
            args,
        };
    }

    #[cfg(unix)]
    {
        let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        ShellCommand {
            program,
            args: vec!["-l".to_string()],
        }
    }

    #[cfg(windows)]
    {
        detect_windows_shell()
    }

    #[cfg(not(any(unix, windows)))]
    {
        ShellCommand {
            program: "/bin/sh".to_string(),
            args: Vec::new(),
        }
    }
}

// --- Windows shell detection ---

/// Probe for available Windows shells in preference order.
#[cfg(windows)]
fn detect_windows_shell() -> ShellCommand {
    // Try PowerShell 7 first
    if let Some(pwsh_path) = resolve_pwsh_path() {
        return setup_powershell_command(&pwsh_path);
    }
    // Try Windows PowerShell
    if let Some(powershell_path) = resolve_windows_powershell_path() {
        return setup_powershell_command(&powershell_path);
    }
    // Fall back to cmd.exe
    let program = resolve_cmd_path();
    ShellCommand {
        program,
        args: vec!["/K".to_string(), "chcp".to_string(), "65001".to_string()],
    }
}

/// Detect shells available on the system.
///
/// On Unix, parses `/etc/shells` (skipping comments), extracts basenames, and deduplicates.
/// On Windows, probes for `pwsh.exe`, `powershell.exe`, and `cmd.exe`.
pub fn detect_available_shells() -> Vec<String> {
    let mut shells = Vec::new();

    #[cfg(unix)]
    {
        if let Ok(contents) = std::fs::read_to_string("/etc/shells") {
            let mut seen = std::collections::HashSet::new();
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some(basename) = std::path::Path::new(line)
                    .file_name()
                    .and_then(|n| n.to_str())
                {
                    if seen.insert(basename.to_string()) {
                        shells.push(basename.to_string());
                    }
                }
            }
        }
        if shells.is_empty() {
            shells = vec!["bash".to_string(), "sh".to_string()];
        }
    }

    #[cfg(windows)]
    {
        if resolve_pwsh_path().is_some() {
            shells.push("pwsh".to_string());
        }
        if resolve_windows_powershell_path().is_some() {
            shells.push("powershell".to_string());
        }
        shells.push("cmd".to_string());
    }

    #[cfg(not(any(unix, windows)))]
    {
        shells.push("sh".to_string());
    }

    shells
}

/// Resolve a shell name to a full `ShellCommand`.
///
/// If `shell_name` is empty, falls through to `detect_shell_command()`.
/// The `CODIRIGENT_SHELL` env var always takes priority.
pub fn resolve_shell(shell_name: &str) -> ShellCommand {
    if std::env::var("CODIRIGENT_SHELL").is_ok() {
        return detect_shell_command();
    }

    if cfg!(test) && shell_name == TEST_LINE_FORWARDER_SENTINEL {
        return resolve_test_line_forwarder();
    }

    if shell_name.is_empty() {
        return detect_shell_command();
    }

    #[cfg(unix)]
    {
        resolve_unix_shell(shell_name)
    }

    #[cfg(windows)]
    {
        resolve_windows_shell(shell_name)
    }

    #[cfg(not(any(unix, windows)))]
    {
        detect_shell_command()
    }
}

fn resolve_test_line_forwarder() -> ShellCommand {
    #[cfg(unix)]
    {
        ShellCommand {
            program: "/bin/cat".to_string(),
            args: Vec::new(),
        }
    }

    #[cfg(windows)]
    {
        ShellCommand {
            // `more.com` forwards stdin to stdout without the interactive shell
            // startup sequences that make PTY tests flaky on Windows.
            program: system32_executable("more.com")
                .to_string_lossy()
                .to_string(),
            args: Vec::new(),
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        detect_shell_command()
    }
}

// --- Unix shell resolution ---

#[cfg(unix)]
fn resolve_unix_shell(shell_name: &str) -> ShellCommand {
    // Try to find in /etc/shells
    if let Ok(contents) = std::fs::read_to_string("/etc/shells") {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(basename) = std::path::Path::new(line)
                .file_name()
                .and_then(|n| n.to_str())
            {
                if basename == shell_name {
                    return ShellCommand {
                        program: line.to_string(),
                        args: vec!["-l".to_string()],
                    };
                }
            }
        }
    }

    // Fallback to `which`
    if let Ok(output) = std::process::Command::new("which").arg(shell_name).output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return ShellCommand {
                    program: path,
                    args: vec!["-l".to_string()],
                };
            }
        }
    }

    detect_shell_command()
}

// --- Windows shell resolution ---

#[cfg(windows)]
fn resolve_windows_shell(shell_name: &str) -> ShellCommand {
    match shell_name {
        "pwsh" => resolve_pwsh_path()
            .map(|path| setup_powershell_command(&path))
            .unwrap_or_else(detect_windows_shell),
        "powershell" => resolve_windows_powershell_path()
            .map(|path| setup_powershell_command(&path))
            .unwrap_or_else(detect_windows_shell),
        "cmd" => {
            let program = resolve_cmd_path();
            ShellCommand {
                program,
                args: vec!["/K".to_string(), "chcp".to_string(), "65001".to_string()],
            }
        }
        _ => detect_shell_command(),
    }
}

/// Split a space-delimited shell args string.
fn split_shell_args(value: &str) -> Vec<String> {
    value.split_whitespace().map(str::to_string).collect()
}

// --- Unix shell integration ---

/// Check whether the given command is a zsh shell.
#[cfg(unix)]
pub(crate) fn is_zsh_shell(command: &str) -> bool {
    std::path::Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        == Some("zsh")
}

/// Set up zsh shell integration via a ZDOTDIR redirect.
///
/// Creates a temporary directory containing `.zshenv`, `.zprofile`, and `.zshrc`
/// that forward to the user's original startup files and append an OSC 7 / OSC 133
/// `precmd` hook at the end of `.zshrc`.
#[cfg(unix)]
pub(crate) fn setup_zsh_integration() -> Result<std::path::PathBuf> {
    let zdotdir = std::env::temp_dir().join("codirigent-zsh-integration");
    std::fs::create_dir_all(&zdotdir).context("Failed to create zsh integration directory")?;

    std::fs::write(
        zdotdir.join(".zshenv"),
        r#"# Codirigent shell integration — forward to user's .zshenv
if [[ -f "${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}/.zshenv" ]]; then
  ZDOTDIR="${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}" source "${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}/.zshenv"
fi
"#,
    )
    .context("Failed to write .zshenv")?;

    std::fs::write(
        zdotdir.join(".zprofile"),
        r#"# Codirigent shell integration — forward to user's .zprofile
if [[ -f "${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}/.zprofile" ]]; then
  source "${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}/.zprofile"
fi
"#,
    )
    .context("Failed to write .zprofile")?;

    std::fs::write(
        zdotdir.join(".zshrc"),
        r#"# Codirigent shell integration — forward to user's .zshrc + add hooks
if [[ -f "${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}/.zshrc" ]]; then
  ZDOTDIR="${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}" source "${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}/.zshrc"
fi

# Restore ZDOTDIR so subshells and .zlogin use the user's config
ZDOTDIR="${CODIRIGENT_ORIG_ZDOTDIR:-$HOME}"

# OSC 133 (shell state) + OSC 7 (CWD tracking)
__codirigent_precmd() {
  local ec=$?
  printf '\e]133;D;%s\a\e]133;A\a' "$ec"
  printf '\e]7;file://%s%s\e\\' "$(hostname)" "$PWD"
}
precmd_functions+=(__codirigent_precmd)

# OSC 133;C — marks the moment a command begins executing
__codirigent_preexec() {
  printf '\e]133;C\a'
}
preexec_functions+=(__codirigent_preexec)
"#,
    )
    .context("Failed to write .zshrc")?;

    Ok(zdotdir)
}

/// Configure shell integration environment variables on Unix.
///
/// Sets OSC 7 and OSC 133 hooks via PROMPT_COMMAND (bash) or ZDOTDIR (zsh).
#[cfg(unix)]
pub(crate) fn configure_shell_integration(cmd: &mut portable_pty::CommandBuilder, command: &str) {
    cmd.env(
        "CODIRIGENT_OSC7",
        r#"printf '\e]7;file://%s%s\e\\' "$(hostname)" "$PWD""#,
    );

    if is_zsh_shell(command) {
        match setup_zsh_integration() {
            Ok(zdotdir) => {
                if let Ok(orig) = std::env::var("ZDOTDIR") {
                    cmd.env("CODIRIGENT_ORIG_ZDOTDIR", orig);
                }
                cmd.env("ZDOTDIR", &zdotdir);
            }
            Err(e) => {
                warn!(%e, "Zsh integration setup failed, falling back to PROMPT_COMMAND");
                cmd.env(
                    "PROMPT_COMMAND",
                    concat!(
                        r#"printf "\e]133;D;$?\a\e]133;A\a"; "#,
                        r#"printf "\e]7;file://%s%s\e\\" "$(hostname)" "$PWD""#,
                    ),
                );
            }
        }
    } else {
        cmd.env(
            "PROMPT_COMMAND",
            concat!(
                r#"printf "\e]133;D;$?\a\e]133;A\a"; "#,
                r#"printf "\e]7;file://%s%s\e\\" "$(hostname)" "$PWD""#,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::ffi::OsString;

    #[cfg(unix)]
    struct EnvVarGuard {
        key: &'static str,
        value: Option<OsString>,
    }

    #[cfg(unix)]
    impl EnvVarGuard {
        fn capture(key: &'static str) -> Self {
            Self {
                key,
                value: std::env::var_os(key),
            }
        }
    }

    #[cfg(unix)]
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.value {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn test_detect_shell() {
        let shell = detect_shell_command();
        assert!(!shell.program.is_empty());
        #[cfg(unix)]
        assert!(
            shell.program.contains('/')
                || shell.program == "bash"
                || shell.program == "sh"
                || shell.program == "zsh"
        );
        #[cfg(windows)]
        assert!(
            shell.program.contains("cmd")
                || shell.program.contains("powershell")
                || shell.program.contains("pwsh")
        );
    }

    #[test]
    fn test_split_shell_args() {
        let args = split_shell_args("-NoLogo -NoProfile -NoExit");
        assert_eq!(args, vec!["-NoLogo", "-NoProfile", "-NoExit"]);

        let args = split_shell_args("   /K   chcp   65001  ");
        assert_eq!(args, vec!["/K", "chcp", "65001"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_is_zsh_shell() {
        assert!(is_zsh_shell("/bin/zsh"));
        assert!(is_zsh_shell("/usr/bin/zsh"));
        assert!(is_zsh_shell("zsh"));
        assert!(!is_zsh_shell("/bin/bash"));
        assert!(!is_zsh_shell("bash"));
        assert!(!is_zsh_shell(""));
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn test_detect_shell_command_uses_login_shell_args() {
        let _shell_guard = EnvVarGuard::capture("SHELL");
        let _codirigent_shell_guard = EnvVarGuard::capture("CODIRIGENT_SHELL");
        let _codirigent_shell_args_guard = EnvVarGuard::capture("CODIRIGENT_SHELL_ARGS");

        std::env::set_var("SHELL", "/bin/zsh");
        std::env::remove_var("CODIRIGENT_SHELL");
        std::env::remove_var("CODIRIGENT_SHELL_ARGS");

        let shell = detect_shell_command();
        assert_eq!(shell.program, "/bin/zsh");
        assert_eq!(shell.args, vec!["-l"]);
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn test_detect_shell_command_respects_codirigent_shell_args_override() {
        let _shell_guard = EnvVarGuard::capture("SHELL");
        let _codirigent_shell_guard = EnvVarGuard::capture("CODIRIGENT_SHELL");
        let _codirigent_shell_args_guard = EnvVarGuard::capture("CODIRIGENT_SHELL_ARGS");

        std::env::set_var("SHELL", "/bin/zsh");
        std::env::set_var("CODIRIGENT_SHELL", "/opt/custom-shell");
        std::env::set_var("CODIRIGENT_SHELL_ARGS", "--norc -i");

        let shell = detect_shell_command();
        assert_eq!(shell.program, "/opt/custom-shell");
        assert_eq!(shell.args, vec!["--norc", "-i"]);
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn test_setup_zsh_integration_creates_files() {
        let zdotdir = setup_zsh_integration().expect("should succeed");
        assert!(zdotdir.join(".zshenv").exists());
        assert!(zdotdir.join(".zprofile").exists());
        assert!(zdotdir.join(".zshrc").exists());
    }

    #[cfg(unix)]
    #[test]
    #[serial_test::serial]
    fn test_setup_zsh_integration_idempotent() {
        let dir1 = setup_zsh_integration().unwrap();
        let content1 = std::fs::read_to_string(dir1.join(".zshrc")).unwrap();
        let dir2 = setup_zsh_integration().unwrap();
        let content2 = std::fs::read_to_string(dir2.join(".zshrc")).unwrap();
        assert_eq!(dir1, dir2);
        assert_eq!(content1, content2);
    }

    #[cfg(windows)]
    #[test]
    fn test_setup_powershell_command() {
        let cmd = setup_powershell_command("pwsh.exe");
        assert_eq!(cmd.program, "pwsh.exe");
        assert!(cmd.args.contains(&"-NoLogo".to_string()));
        assert!(cmd.args.contains(&"-NoExit".to_string()));
    }
}
