//! PTY (pseudo-terminal) abstraction layer.
//!
//! Provides cross-platform terminal creation and I/O using portable-pty.
//! This module handles spawning shells, managing terminal dimensions,
//! and async output reading.

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize as PortablePtySize};
use std::io::{Read, Write};
use std::path::Path;
use std::thread;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// PTY dimensions.
///
/// Represents the terminal size in rows and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Terminal height in rows.
    pub rows: u16,
    /// Terminal width in columns.
    pub cols: u16,
}

impl PtySize {
    /// Create a new PtySize with the given dimensions.
    pub fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }
}

impl Default for PtySize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// Handle to a PTY (pseudo-terminal).
///
/// Wraps the platform-specific PTY implementation and provides
/// a unified interface for terminal I/O. The handle manages
/// the master side of the PTY pair.
pub struct PtyHandle {
    /// The master PTY for resize operations.
    master: Box<dyn MasterPty + Send>,
    /// Writer for sending input to the PTY.
    writer: Box<dyn Write + Send>,
    /// Reader for receiving output (taken when async reader spawned).
    reader: Option<Box<dyn Read + Send>>,
    /// Child process ID.
    child_pid: u32,
    /// Current terminal size.
    size: PtySize,
}

impl PtyHandle {
    /// Spawn a new PTY with the default shell.
    ///
    /// Detects the default shell for the current platform ($SHELL on Unix,
    /// %COMSPEC% on Windows) and spawns it in a new PTY.
    ///
    /// # Arguments
    /// * `working_dir` - Initial working directory for the shell
    /// * `rows` - Initial terminal height
    /// * `cols` - Initial terminal width
    ///
    /// # Errors
    /// Returns an error if the PTY cannot be created or the shell cannot be spawned.
    ///
    /// # Example
    /// ```no_run
    /// use dirigent_session::PtyHandle;
    /// use std::path::Path;
    ///
    /// let pty = PtyHandle::spawn(Path::new("/tmp"), 24, 80).unwrap();
    /// assert!(pty.child_pid() > 0);
    /// ```
    pub fn spawn(working_dir: &Path, rows: u16, cols: u16) -> Result<Self> {
        let shell = detect_shell();
        Self::spawn_command(working_dir, &shell, &[], rows, cols)
    }

    /// Spawn a PTY with a specific command.
    ///
    /// Allows running a specific command instead of the default shell.
    ///
    /// # Arguments
    /// * `working_dir` - Initial working directory
    /// * `command` - The command to execute
    /// * `args` - Arguments to pass to the command
    /// * `rows` - Initial terminal height
    /// * `cols` - Initial terminal width
    ///
    /// # Errors
    /// Returns an error if the PTY cannot be created or the command cannot be spawned.
    ///
    /// # Example
    /// ```no_run
    /// use dirigent_session::PtyHandle;
    /// use std::path::Path;
    ///
    /// let pty = PtyHandle::spawn_command(
    ///     Path::new("/tmp"),
    ///     "echo",
    ///     &["hello"],
    ///     24,
    ///     80
    /// ).unwrap();
    /// ```
    pub fn spawn_command(
        working_dir: &Path,
        command: &str,
        args: &[&str],
        rows: u16,
        cols: u16,
    ) -> Result<Self> {
        info!(command, ?working_dir, rows, cols, "Spawning PTY");

        let pty_system = native_pty_system();
        let size = PortablePtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system.openpty(size).context("Failed to open PTY")?;

        let mut cmd = CommandBuilder::new(command);
        for arg in args {
            cmd.arg(*arg);
        }
        cmd.cwd(working_dir);

        // Set common environment variables for proper terminal behavior
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn command")?;

        let child_pid = child.process_id().unwrap_or(0);
        debug!(child_pid, "PTY child spawned");

        let reader = pair
            .master
            .try_clone_reader()
            .context("Failed to clone PTY reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("Failed to take PTY writer")?;

        Ok(Self {
            master: pair.master,
            writer,
            reader: Some(reader),
            child_pid,
            size: PtySize { rows, cols },
        })
    }

    /// Send input to the PTY.
    ///
    /// Writes the given bytes to the PTY's input stream and flushes.
    ///
    /// # Arguments
    /// * `data` - The bytes to send
    ///
    /// # Errors
    /// Returns an error if the write or flush fails.
    ///
    /// # Example
    /// ```no_run
    /// use dirigent_session::PtyHandle;
    /// use std::path::Path;
    ///
    /// let mut pty = PtyHandle::spawn(Path::new("/tmp"), 24, 80).unwrap();
    /// pty.send_input(b"echo hello\n").unwrap();
    /// ```
    pub fn send_input(&mut self, data: &[u8]) -> Result<()> {
        self.writer
            .write_all(data)
            .context("Failed to write to PTY")?;
        self.writer.flush().context("Failed to flush PTY")?;
        Ok(())
    }

    /// Resize the PTY.
    ///
    /// Updates the terminal dimensions. This sends a SIGWINCH signal
    /// to the child process on Unix systems.
    ///
    /// # Arguments
    /// * `rows` - New terminal height
    /// * `cols` - New terminal width
    ///
    /// # Errors
    /// Returns an error if the resize operation fails.
    ///
    /// # Example
    /// ```no_run
    /// use dirigent_session::PtyHandle;
    /// use std::path::Path;
    ///
    /// let mut pty = PtyHandle::spawn(Path::new("/tmp"), 24, 80).unwrap();
    /// pty.resize(48, 120).unwrap();
    /// assert_eq!(pty.size().rows, 48);
    /// ```
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        debug!(rows, cols, "Resizing PTY");

        let size = PortablePtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        self.master.resize(size).context("Failed to resize PTY")?;

        self.size = PtySize { rows, cols };
        Ok(())
    }

    /// Get the child process ID.
    ///
    /// Returns the PID of the spawned child process.
    /// Returns 0 if the PID could not be determined.
    pub fn child_pid(&self) -> u32 {
        self.child_pid
    }

    /// Get current terminal size.
    pub fn size(&self) -> PtySize {
        self.size
    }

    /// Take the reader for async processing.
    ///
    /// This consumes the reader, returning it for use with
    /// [`spawn_output_reader`]. Once taken, the reader cannot
    /// be retrieved again.
    ///
    /// # Returns
    /// `Some(reader)` if the reader hasn't been taken yet, `None` otherwise.
    pub fn take_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        self.reader.take()
    }

    /// Check if the reader is still available.
    pub fn has_reader(&self) -> bool {
        self.reader.is_some()
    }
}

/// Detect the default shell for the current platform.
///
/// On Unix systems, returns the value of `$SHELL` or `/bin/bash` as fallback.
/// On Windows, returns the value of `%COMSPEC%` or `cmd.exe` as fallback.
fn detect_shell() -> String {
    #[cfg(unix)]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    }

    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
    }

    #[cfg(not(any(unix, windows)))]
    {
        "/bin/sh".to_string()
    }
}

/// Buffer size for reading PTY output.
const OUTPUT_BUFFER_SIZE: usize = 4096;

/// Channel capacity for output messages.
const OUTPUT_CHANNEL_CAPACITY: usize = 256;

/// Spawn an async task to read PTY output.
///
/// Creates a background thread that reads from the PTY and sends
/// output through an mpsc channel. The thread terminates when
/// the PTY closes (EOF), a read error occurs, or the channel receiver
/// is dropped.
///
/// # Arguments
/// * `reader` - The PTY reader (obtained via [`PtyHandle::take_reader`])
///
/// # Returns
/// An mpsc receiver for output data chunks.
///
/// # Example
/// ```no_run
/// use dirigent_session::{PtyHandle, spawn_output_reader};
/// use std::path::Path;
///
/// # async fn example() {
/// let mut pty = PtyHandle::spawn(Path::new("/tmp"), 24, 80).unwrap();
/// let reader = pty.take_reader().unwrap();
/// let mut rx = spawn_output_reader(reader);
///
/// // Receive output asynchronously
/// while let Some(data) = rx.recv().await {
///     println!("Received {} bytes", data.len());
/// }
/// # }
/// ```
pub fn spawn_output_reader(mut reader: Box<dyn Read + Send>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel(OUTPUT_CHANNEL_CAPACITY);

    thread::spawn(move || {
        let mut buf = [0u8; OUTPUT_BUFFER_SIZE];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    debug!("PTY reader reached EOF");
                    break;
                }
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    if tx.blocking_send(data).is_err() {
                        debug!("PTY output channel closed");
                        break;
                    }
                }
                Err(e) => {
                    debug!(?e, "PTY read error");
                    break;
                }
            }
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pty_size_new() {
        let size = PtySize::new(48, 120);
        assert_eq!(size.rows, 48);
        assert_eq!(size.cols, 120);
    }

    #[test]
    fn test_pty_size_default() {
        let size = PtySize::default();
        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
    }

    #[test]
    fn test_pty_size_equality() {
        let size1 = PtySize::new(24, 80);
        let size2 = PtySize::new(24, 80);
        let size3 = PtySize::new(48, 80);
        assert_eq!(size1, size2);
        assert_ne!(size1, size3);
    }

    #[test]
    fn test_pty_size_clone() {
        let size = PtySize::new(24, 80);
        let cloned = size;
        assert_eq!(size, cloned);
    }

    #[test]
    fn test_detect_shell() {
        let shell = detect_shell();
        assert!(!shell.is_empty());
        // Shell should be a valid path or command
        #[cfg(unix)]
        assert!(shell.contains('/') || shell == "bash" || shell == "sh" || shell == "zsh");
        #[cfg(windows)]
        assert!(shell.contains("cmd") || shell.contains("powershell"));
    }

    #[test]
    fn test_spawn_pty() {
        let temp = TempDir::new().unwrap();
        let pty = PtyHandle::spawn(temp.path(), 24, 80);
        assert!(pty.is_ok(), "Failed to spawn PTY: {:?}", pty.err());

        let pty = pty.unwrap();
        assert!(pty.child_pid() > 0);
        assert_eq!(pty.size().rows, 24);
        assert_eq!(pty.size().cols, 80);
        assert!(pty.has_reader());
    }

    #[test]
    fn test_spawn_pty_with_custom_size() {
        let temp = TempDir::new().unwrap();
        let pty = PtyHandle::spawn(temp.path(), 48, 120).unwrap();

        assert_eq!(pty.size().rows, 48);
        assert_eq!(pty.size().cols, 120);
    }

    #[test]
    fn test_spawn_command() {
        let temp = TempDir::new().unwrap();

        #[cfg(unix)]
        let pty = PtyHandle::spawn_command(temp.path(), "/bin/sh", &[], 24, 80);

        #[cfg(windows)]
        let pty = PtyHandle::spawn_command(temp.path(), "cmd.exe", &[], 24, 80);

        assert!(pty.is_ok(), "Failed to spawn command: {:?}", pty.err());
        assert!(pty.unwrap().child_pid() > 0);
    }

    #[test]
    fn test_spawn_command_with_args() {
        let temp = TempDir::new().unwrap();

        #[cfg(unix)]
        let pty = PtyHandle::spawn_command(temp.path(), "echo", &["hello", "world"], 24, 80);

        #[cfg(windows)]
        let pty = PtyHandle::spawn_command(temp.path(), "cmd.exe", &["/c", "echo", "hello"], 24, 80);

        assert!(pty.is_ok());
    }

    #[test]
    fn test_send_input() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        // Send a simple command
        let result = pty.send_input(b"echo hello\n");
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_input_multiple_times() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        assert!(pty.send_input(b"echo 1\n").is_ok());
        assert!(pty.send_input(b"echo 2\n").is_ok());
        assert!(pty.send_input(b"echo 3\n").is_ok());
    }

    #[test]
    fn test_send_input_special_characters() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        // Test control characters
        assert!(pty.send_input(&[0x03]).is_ok()); // Ctrl+C
        assert!(pty.send_input(&[0x04]).is_ok()); // Ctrl+D
        assert!(pty.send_input(&[0x1b, b'[', b'A']).is_ok()); // Up arrow
    }

    #[test]
    fn test_resize() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        let result = pty.resize(48, 120);
        assert!(result.is_ok());
        assert_eq!(pty.size().rows, 48);
        assert_eq!(pty.size().cols, 120);
    }

    #[test]
    fn test_resize_multiple_times() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        pty.resize(30, 100).unwrap();
        assert_eq!(pty.size().rows, 30);

        pty.resize(50, 150).unwrap();
        assert_eq!(pty.size().cols, 150);

        pty.resize(10, 40).unwrap();
        assert_eq!(pty.size().rows, 10);
        assert_eq!(pty.size().cols, 40);
    }

    #[test]
    fn test_take_reader() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        assert!(pty.has_reader());

        let reader = pty.take_reader();
        assert!(reader.is_some());
        assert!(!pty.has_reader());

        // Second take should return None
        let reader2 = pty.take_reader();
        assert!(reader2.is_none());
    }

    #[test]
    fn test_child_pid() {
        let temp = TempDir::new().unwrap();
        let pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        let pid = pty.child_pid();
        assert!(pid > 0, "Child PID should be positive");
    }

    #[tokio::test]
    async fn test_spawn_output_reader() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        let reader = pty.take_reader().expect("Reader should exist");
        let mut rx = spawn_output_reader(reader);

        // Send a command that produces output
        #[cfg(unix)]
        pty.send_input(b"echo test_output_12345\n").unwrap();

        #[cfg(windows)]
        pty.send_input(b"echo test_output_12345\r\n").unwrap();

        // Wait for output (with timeout)
        let mut found = false;
        for _ in 0..50 {
            if let Ok(data) =
                tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await
            {
                if let Some(bytes) = data {
                    let output = String::from_utf8_lossy(&bytes);
                    if output.contains("test_output_12345") {
                        found = true;
                        break;
                    }
                }
            }
        }

        assert!(found, "Expected to find output in PTY stream");
    }

    #[tokio::test]
    async fn test_spawn_output_reader_receives_multiple_chunks() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        let reader = pty.take_reader().expect("Reader should exist");
        let mut rx = spawn_output_reader(reader);

        // Send multiple commands
        for i in 0..3 {
            #[cfg(unix)]
            pty.send_input(format!("echo chunk_{}\n", i).as_bytes())
                .unwrap();

            #[cfg(windows)]
            pty.send_input(format!("echo chunk_{}\r\n", i).as_bytes())
                .unwrap();
        }

        // Collect output
        let mut chunks_found = 0;
        let mut all_output = String::new();

        for _ in 0..100 {
            if let Ok(data) =
                tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await
            {
                if let Some(bytes) = data {
                    let output = String::from_utf8_lossy(&bytes);
                    all_output.push_str(&output);

                    for i in 0..3 {
                        if all_output.contains(&format!("chunk_{}", i)) {
                            chunks_found |= 1 << i;
                        }
                    }

                    if chunks_found == 0b111 {
                        break;
                    }
                }
            }
        }

        // At least some output should be received
        assert!(!all_output.is_empty(), "Should receive some output");
    }

    #[tokio::test]
    async fn test_output_reader_channel_closure() {
        let temp = TempDir::new().unwrap();
        let mut pty = PtyHandle::spawn(temp.path(), 24, 80).unwrap();

        let reader = pty.take_reader().expect("Reader should exist");
        let rx = spawn_output_reader(reader);

        // Drop the receiver - the thread should detect this and stop
        drop(rx);

        // Send input to trigger the thread to check the channel
        let _ = pty.send_input(b"echo test\n");

        // Give the thread time to detect closure
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Test passes if we don't hang or crash
    }

    #[test]
    fn test_spawn_invalid_command() {
        let temp = TempDir::new().unwrap();
        let result =
            PtyHandle::spawn_command(temp.path(), "/nonexistent/command/path", &[], 24, 80);

        // Should fail to spawn an invalid command
        assert!(result.is_err());
    }

    #[test]
    fn test_spawn_with_invalid_working_dir() {
        use std::path::PathBuf;
        let invalid_path = PathBuf::from("/nonexistent/path/that/does/not/exist");

        // This may or may not fail depending on the platform
        // On some systems, cwd errors are deferred
        let result = PtyHandle::spawn(&invalid_path, 24, 80);
        // We just check it doesn't panic
        let _ = result;
    }
}
