# Codirigent Developer Onboarding

Welcome! This guide will get you from zero to productive in 1-2 weeks.

## Prerequisites

- Rust 1.75+ installed
- Git installed
- Basic understanding of async Rust (tokio)
- Familiarity with terminal applications

## Day 1: Setup & Build

### 1. Clone and Build

```bash
git clone https://github.com/user/codirigent.git
cd codirigent
cargo build --release
```

Expected: Clean build in 3-5 minutes (first time)

### 2. Run Tests

```bash
cargo test --workspace
```

Expected: 1,765+ tests pass

### 3. Run Application

```bash
cargo run --release
```

Expected: Codirigent UI launches

### 4. Read Architecture

Read these docs in order:
1. [Architecture Overview](../architecture/overview.md) - 20 min
2. [Data Flow](../architecture/data-flow.md) - 15 min

## Day 2-3: Code Exploration

### Explore codirigent-core

**Goal:** Understand the core types and services

**Files to read:**
1. `crates/codirigent-core/src/types.rs` - Core types
2. `crates/codirigent-core/src/events.rs` - Event definitions
3. `crates/codirigent-core/src/event_bus.rs` - Event bus implementation

**Exercise:** Add a new event type
1. Add variant to `CodirigentEvent` enum
2. Publish from one component
3. Subscribe in another
4. Verify event received in tests

### Explore codirigent-session

**Goal:** Understand session lifecycle

**Files to read:**
1. `crates/codirigent-session/src/manager.rs` - Session management
2. `crates/codirigent-session/src/pty.rs` - PTY handling
3. `crates/codirigent-session/src/session.rs` - Session struct

**Exercise:** Create a test session
```rust
#[test]
fn test_create_session() {
    let temp = TempDir::new().unwrap();
    let event_bus = Arc::new(DefaultEventBus::new(16));
    let manager = DefaultSessionManager::new(event_bus);
    let id = manager.create_session("test".to_string(), temp.path().to_path_buf(), None).unwrap();
    assert!(manager.get_session(id).is_some());
}
```

### Explore codirigent-ui

**Goal:** Understand GPUI rendering

**Files to read:**
1. `crates/codirigent-ui/src/workspace/gpui.rs` - Main workspace view
2. `crates/codirigent-ui/src/terminal_view.rs` - Terminal rendering
3. `crates/codirigent-ui/src/theme.rs` - Theming system

**Exercise:** Modify theme color
1. Change a color in `CodirigentTheme`
2. Rebuild and run
3. Verify color change visible

## Day 4-5: First Contribution

### Find a Starter Issue

Look for issues labeled `good-first-issue` on GitHub.

Recommended starter tasks:
- Add a new keyboard shortcut
- Add a theme color option
- Improve error message text
- Add unit test for uncovered function

### Development Workflow

1. **Create branch**
   ```bash
   git checkout -b feature/your-feature
   ```

2. **Make changes**
   - Write test first (TDD)
   - Implement minimal code
   - Run tests: `cargo test`

3. **Verify build**
   ```bash
   cargo build --workspace
   cargo clippy --workspace
   ```

4. **Commit**
   ```bash
   git add .
   git commit -m "feat: add your feature"
   ```

5. **Push and create PR**
   ```bash
   git push origin feature/your-feature
   ```

## Week 2: Dive Deeper

### Advanced Topics

Choose areas based on interest:

**Option A: Core Logic**
- Study task assignment algorithm (`scheduler.rs`)
- Understand priority-based task ordering
- Learn dependency tracking

**Option B: Session Management**
- Study PTY management and process detection
- Learn OSC 133 shell integration
- Understand clipboard synchronization

**Option C: UI Development**
- Study GPUI rendering pipeline
- Learn terminal ANSI parsing
- Understand workspace layout system

### Pair with Team Member

Schedule a 1-hour pairing session to:
- Review your understanding
- Ask questions
- Get architectural guidance
- Learn team conventions

## Common Gotchas

### 1. GPUI Context Lifetime

```rust
// ❌ WRONG - storing context
struct MyView {
    cx: &ViewContext<Self>, // Can't store context!
}

// ✅ CORRECT - context passed as parameter
impl MyView {
    fn render(&mut self, cx: &mut ViewContext<Self>) {
        // Use cx here
    }
}
```

### 2. Event Bus Subscription

```rust
// ❌ WRONG - subscribing repeatedly
loop {
    let rx = event_bus.subscribe(); // Creates new subscription each time!
    let event = rx.recv().await;
}

// ✅ CORRECT - subscribe once
let mut rx = event_bus.subscribe();
loop {
    let event = rx.recv().await;
}
```

### 3. Storage Path Construction

```rust
// ❌ WRONG - manual path construction
let path = format!("{}/.codirigent/tasks/{}.json", project, task_id);

// ✅ CORRECT - use Path methods
let path = project.join(".codirigent").join("tasks")
    .join(format!("{}.json", task_id));
```

## Resources

- [Rust Book](https://doc.rust-lang.org/book/) - Rust fundamentals
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) - Async Rust
- [GPUI Examples](https://github.com/zed-industries/zed) - GPUI usage patterns
- [Team Slack/Discord] - Ask questions anytime!

## Getting Help

- **Code questions**: Ask in #development channel
- **Architecture questions**: Schedule office hours with tech lead
- **Stuck on issue**: Create draft PR and ask for guidance

Welcome to the team! 🚀
