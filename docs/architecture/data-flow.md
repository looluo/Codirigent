# Codirigent Data Flow

Detailed sequence diagrams for key operations.

## Session Creation Flow

```
User          WorkspaceView      SessionManager    EventBus        Storage
  │                 │                   │              │             │
  │─New Session────>│                   │              │             │
  │                 │─create_session()─>│              │             │
  │                 │                   │─spawn_pty()  │             │
  │                 │                   │<─(PTY created)             │
  │                 │                   │              │             │
  │                 │                   │─publish()───>│             │
  │                 │                   │  SessionCreated            │
  │                 │<─session_id───────│              │             │
  │                 │                   │              │─subscribe──>│
  │                 │                   │              │  (receives) │
  │                 │                   │              │─save()─────>│
  │<─UI updates─────│                   │              │             │
```

## Task Assignment Flow

```
TaskManager    InputDetector   SessionManager   EventBus      UI
     │               │                │             │          │
     │─poll()───────>│                │             │          │
     │<─status───────│                │             │          │
     │  (Idle)       │                │             │          │
     │               │                │             │          │
     │─next_task()   │                │             │          │
     │  (task-001)   │                │             │          │
     │               │                │             │          │
     │─assign()──────┼───────────────>│             │          │
     │               │                │─send_input()│          │
     │               │                │  (task desc)│          │
     │               │                │             │          │
     │─publish()────────────────────────────────────>│          │
     │  TaskAssigned                                 │          │
     │               │                │             │─update──>│
```

## State Persistence Flow

```
WorkspaceView    EventBus    Storage       Filesystem
      │             │           │               │
      │─modify()    │           │               │
      │  (state)    │           │               │
      │             │           │               │
      │─publish()──>│           │               │
      │  StateChanged          │               │
      │             │─recv()───>│               │
      │             │           │               │
      │             │           │─save_state()  │
      │             │           │─write_temp()─>│
      │             │           │               │─temp.json
      │             │           │<─success──────│
      │             │           │               │
      │             │           │─rename()─────>│
      │             │           │               │─state.json
      │             │           │<─success──────│ (atomic!)
```

## Input Detection Flow

```
InputDetector   ProcessMonitor   SessionManager   EventBus
      │               │                 │             │
      │─poll()───────>│                 │             │
      │<─cpu_usage────│ (12%)           │             │
      │  (Working)    │                 │             │
      │               │                 │             │
      │─check_osc()──────────────────────>│             │
      │<─shell_state──────────────────────│             │
      │  (CommandExecuted)                │             │
      │               │                 │             │
      │─publish()────────────────────────────────────>│
      │  StatusChanged(Working)                       │
      │               │                 │             │
      │  (5 seconds later)              │             │
      │─poll()───────>│                 │             │
      │<─cpu_usage────│ (0%)            │             │
      │  (Idle?)      │                 │             │
      │               │                 │             │
      │─check_osc()──────────────────────>│             │
      │<─shell_state──────────────────────│             │
      │  (PromptStart)                    │             │
      │               │                 │             │
      │─publish()────────────────────────────────────>│
      │  StatusChanged(Idle)                          │
```

## Clipboard Sync Flow

```
Session-1    ClipboardService    EventBus    Session-2
    │               │                │           │
    │─copy()───────>│                │           │
    │  "code"       │                │           │
    │               │─save()         │           │
    │               │  (to storage)  │           │
    │               │                │           │
    │               │─publish()─────>│           │
    │               │  ClipboardUpdate          │
    │               │                │─notify───>│
    │               │                │           │
    │               │<─get_latest()──────────────│
    │               │──"code"────────────────────>│
```

## Key Observations

1. **Event Bus is Central**: Almost all flows involve event publishing
2. **Async by Nature**: Most operations are async (tokio-based)
3. **Atomic Operations**: Storage uses temp files + rename for safety
4. **Polling for Detection**: Input detection polls every 500ms
5. **No Direct Calls Between Layers**: UI → Core → Session (one direction)
