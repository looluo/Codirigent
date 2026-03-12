//! Background PTY command worker.
//!
//! This module moves PTY writes and resize operations off the caller thread.
//! Each session gets a dedicated worker thread that owns the mutable PTY
//! handle after startup. Callers enqueue commands and return immediately.

use crate::pty::PtyHandle;
use anyhow::{anyhow, Result};
use codirigent_core::SessionId;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use tracing::{debug, trace, warn};

#[derive(Debug)]
enum SessionIoCommand {
    Write(Vec<u8>),
    Resize { rows: u16, cols: u16 },
    Shutdown,
}

trait SessionIoTarget {
    fn write(&mut self, data: &[u8]) -> Result<()>;
    fn resize(&mut self, rows: u16, cols: u16) -> Result<()>;
}

impl SessionIoTarget for PtyHandle {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.send_input(data)
    }

    fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        self.resize(rows, cols)
    }
}

/// Handle for a background PTY I/O worker.
#[derive(Clone)]
pub(crate) struct SessionIoHandle {
    session_id: SessionId,
    sender: Sender<SessionIoCommand>,
}

impl SessionIoHandle {
    /// Spawn a dedicated worker thread that owns the PTY handle.
    pub(crate) fn spawn(session_id: SessionId, pty: PtyHandle) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name(format!("session-io-{}", session_id.0))
            .spawn(move || run_worker_loop(session_id, pty, receiver))
            .map_err(|e| anyhow!("Failed to spawn session I/O worker: {e}"))?;
        Ok(Self { session_id, sender })
    }

    /// Queue PTY input for background delivery.
    pub(crate) fn send_input(&self, input: &[u8]) -> Result<()> {
        self.sender
            .send(SessionIoCommand::Write(input.to_vec()))
            .map_err(|_| anyhow!("Session I/O worker unavailable: {}", self.session_id.0))
    }

    /// Queue a PTY resize for background delivery.
    pub(crate) fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.sender
            .send(SessionIoCommand::Resize { rows, cols })
            .map_err(|_| anyhow!("Session I/O worker unavailable: {}", self.session_id.0))
    }

    /// Ask the worker to stop.
    pub(crate) fn shutdown(&self) {
        let _ = self.sender.send(SessionIoCommand::Shutdown);
    }
}

fn run_worker_loop<T: SessionIoTarget>(
    session_id: SessionId,
    mut target: T,
    receiver: Receiver<SessionIoCommand>,
) {
    let mut pending = None;

    loop {
        let command = match pending.take() {
            Some(command) => command,
            None => match receiver.recv() {
                Ok(command) => command,
                Err(_) => break,
            },
        };

        match command {
            SessionIoCommand::Write(bytes) => {
                if let Err(error) = target.write(&bytes) {
                    warn!(?session_id, %error, "Failed to send PTY input");
                }
            }
            SessionIoCommand::Resize { rows, cols } => {
                let (final_rows, final_cols, next_pending, disconnected) =
                    coalesce_contiguous_resizes(rows, cols, &receiver);
                pending = next_pending;

                trace!(
                    ?session_id,
                    rows = final_rows,
                    cols = final_cols,
                    "Applying PTY resize"
                );

                if let Err(error) = target.resize(final_rows, final_cols) {
                    warn!(
                        ?session_id,
                        rows = final_rows,
                        cols = final_cols,
                        %error,
                        "Failed to resize PTY"
                    );
                }

                if disconnected {
                    break;
                }
            }
            SessionIoCommand::Shutdown => {
                debug!(?session_id, "Stopping session I/O worker");
                break;
            }
        }
    }
}

fn coalesce_contiguous_resizes(
    mut rows: u16,
    mut cols: u16,
    receiver: &Receiver<SessionIoCommand>,
) -> (u16, u16, Option<SessionIoCommand>, bool) {
    loop {
        match receiver.try_recv() {
            Ok(SessionIoCommand::Resize {
                rows: next_rows,
                cols: next_cols,
            }) => {
                rows = next_rows;
                cols = next_cols;
            }
            Ok(other) => return (rows, cols, Some(other), false),
            Err(TryRecvError::Empty) => return (rows, cols, None, false),
            Err(TryRecvError::Disconnected) => return (rows, cols, None, true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestOp {
        Write(Vec<u8>),
        Resize { rows: u16, cols: u16 },
    }

    #[derive(Clone)]
    struct RecordingTarget {
        ops: Arc<Mutex<Vec<TestOp>>>,
    }

    impl RecordingTarget {
        fn new() -> (Self, Arc<Mutex<Vec<TestOp>>>) {
            let ops = Arc::new(Mutex::new(Vec::new()));
            (Self { ops: ops.clone() }, ops)
        }
    }

    impl SessionIoTarget for RecordingTarget {
        fn write(&mut self, data: &[u8]) -> Result<()> {
            self.ops
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(TestOp::Write(data.to_vec()));
            Ok(())
        }

        fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
            self.ops
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(TestOp::Resize { rows, cols });
            Ok(())
        }
    }

    fn spawn_test_worker(
        target: RecordingTarget,
    ) -> (Sender<SessionIoCommand>, thread::JoinHandle<()>) {
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || run_worker_loop(SessionId(1), target, receiver));
        (sender, handle)
    }

    #[test]
    fn test_write_ordering_is_preserved() {
        let (target, ops) = RecordingTarget::new();
        let (sender, handle) = spawn_test_worker(target);

        // Queue ordering is the worker's core contract: later writes must never
        // overtake earlier ones, regardless of caller timing.
        sender
            .send(SessionIoCommand::Write(b"one".to_vec()))
            .unwrap();
        sender
            .send(SessionIoCommand::Write(b"two".to_vec()))
            .unwrap();
        sender
            .send(SessionIoCommand::Write(b"three".to_vec()))
            .unwrap();
        sender.send(SessionIoCommand::Shutdown).unwrap();
        handle.join().unwrap();

        assert_eq!(
            *ops.lock().unwrap_or_else(|p| p.into_inner()),
            vec![
                TestOp::Write(b"one".to_vec()),
                TestOp::Write(b"two".to_vec()),
                TestOp::Write(b"three".to_vec())
            ]
        );
    }

    #[test]
    fn test_resize_coalesces_contiguous_commands() {
        let (target, ops) = RecordingTarget::new();
        let (sender, handle) = spawn_test_worker(target);

        sender
            .send(SessionIoCommand::Resize { rows: 30, cols: 90 })
            .unwrap();
        sender
            .send(SessionIoCommand::Resize { rows: 31, cols: 91 })
            .unwrap();
        sender
            .send(SessionIoCommand::Write(b"after".to_vec()))
            .unwrap();
        sender
            .send(SessionIoCommand::Resize {
                rows: 40,
                cols: 120,
            })
            .unwrap();
        sender
            .send(SessionIoCommand::Resize {
                rows: 41,
                cols: 121,
            })
            .unwrap();
        sender.send(SessionIoCommand::Shutdown).unwrap();
        handle.join().unwrap();

        assert_eq!(
            *ops.lock().unwrap_or_else(|p| p.into_inner()),
            vec![
                TestOp::Resize { rows: 31, cols: 91 },
                TestOp::Write(b"after".to_vec()),
                TestOp::Resize {
                    rows: 41,
                    cols: 121
                }
            ]
        );
    }

    #[test]
    fn test_commands_after_shutdown_fail() {
        let (target, _ops) = RecordingTarget::new();
        let handle = thread::spawn(move || {
            let (sender, receiver) = mpsc::channel();
            let worker = thread::spawn(move || run_worker_loop(SessionId(2), target, receiver));
            sender.send(SessionIoCommand::Shutdown).unwrap();
            worker.join().unwrap();
            sender.send(SessionIoCommand::Write(b"late".to_vec()))
        });

        let result = handle.join().unwrap();
        assert!(result.is_err());
    }
}
