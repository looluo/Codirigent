# Codirigent Architecture Overview

## Purpose

Codirigent is an AI Coding Agent Orchestration IDE that manages multiple AI coding sessions simultaneously, assigns tasks intelligently, and monitors progress.

## High-Level Components

```
┌─────────────────────────────────────────────────────────┐
│                     Codirigent UI                       │
│  (GPUI-based workspace, session grid, task board)      │
└────────────────────┬────────────────────────────────────┘
                     │
         ┌───────────┴───────────┐
         ▼                       ▼
┌──────────────────┐    ┌──────────────────┐
│  Core Services   │    │  Session Manager │
│  - Event Bus     │◄───┤  - PTY Handling  │
│  - Storage       │    │  - CLI Detection │
│  - Task Manager  │    │  - Git Status    │
└────────┬─────────┘    └──────────────────┘
         │
         ▼
┌──────────────────┐
│   Detectors      │
│ - Input Detector │
│ - Process Mon    │
└──────────────────┘
```

## Crate Structure

### codirigent-core
**Purpose:** Core domain logic, types, and services

**Key Types:**
- `SessionId`, `Session` - Session identification and state
- `Task`, `TaskId` - Task definitions
- `CodirigentEvent` - Event types for communication

**Key Services:**
- `EventBus` - Publish/subscribe event system
- `StorageService` - State persistence to `.codirigent/` directory
- `TaskManager` - Task queue and assignment logic

### codirigent-session
**Purpose:** Session lifecycle and terminal management

**Key Components:**
- `SessionManager` - Create, close, manage sessions
- `PtyHandle` - PTY (pseudo-terminal) wrapper
- `ClipboardService` - Cross-session clipboard
- `WorktreeManager` - Git worktree integration

### codirigent-detector
**Purpose:** Process and input detection

**Key Components:**
- `InputDetector` - Monitors sessions for status changes
- Platform-specific process monitoring (Linux, macOS, Windows)

### codirigent-ui
**Purpose:** GPUI-based user interface

**Key Components:**
- `WorkspaceView` - Main application window
- `TerminalView` - Terminal rendering with ANSI support
- `TaskBoardPanel` - Task visualization
- `SettingsPage` - Configuration UI

### codirigent-filetree
**Purpose:** File tree data structure for navigation

### codirigent-plugin
**Purpose:** Plugin system (future extensibility)

### codirigent-verification
**Purpose:** Test verification and result parsing

## Communication Patterns

### Event Bus
All components communicate via `EventBus`:

```rust
// Publisher
event_bus.publish(CodirigentEvent::SessionCreated { id });

// Subscriber
let mut rx = event_bus.subscribe();
while let Ok(event) = rx.recv().await {
    match event {
        CodirigentEvent::SessionCreated { id } => { /* handle */ }
        _ => {}
    }
}
```

### Service Traits
Core services implement traits for testability:

```rust
pub trait SessionManager {
    fn create_session(&mut self, id: SessionId, name: String) -> Result<()>;
    fn close_session(&mut self, id: SessionId) -> Result<()>;
    fn get_session(&self, id: SessionId) -> Option<&Session>;
}
```

## Data Flow

### Session Creation Flow
1. UI: User clicks "New Session"
2. UI calls `SessionManager::create_session()`
3. SessionManager creates PTY process
4. SessionManager publishes `SessionCreated` event
5. UI subscribes to event, updates display
6. Storage service subscribes, persists state

### Task Assignment Flow
1. UI: User creates task
2. TaskManager enqueues task
3. TaskManager monitors session status via `InputDetector`
4. When session becomes idle, TaskManager auto-assigns task
5. TaskManager publishes `TaskAssigned` event
6. UI updates to show assignment

## File Storage Structure

All data stored in `.codirigent/` directory:

```
.codirigent/
├── config.json         # Project configuration
├── state.json          # Runtime state (sessions, layout)
├── queue.json          # Task queue order
├── tasks/              # Individual task files
│   ├── task-001.json
│   └── task-002.json
└── context/            # Per-session context (written by CLI hooks)
    ├── session-1.json
    └── session-2.json
```

## Key Design Principles

1. **Event-Driven Architecture**: Components communicate via events, not direct calls
2. **Crate Boundaries**: Clean separation between UI, core logic, and platform services
3. **Trait Abstractions**: Services use traits for testability
4. **Atomic Persistence**: All writes are atomic (write to temp, then rename)
5. **No Circular Dependencies**: Dependency graph is acyclic

## Next Steps

- Read [Data Flow](data-flow.md) for detailed flow diagrams
- Read [Crate Dependencies](crate-dependencies.md) for dependency graph
- Read [Event Bus](event-bus.md) for event system details
