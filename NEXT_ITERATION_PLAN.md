# Next Iteration Plan

## Current Status
**Completed:** 10 of 16 features (63%)
**Phase 1:** ✅ 100% Complete (6/6)
**Phase 2:** ⚠️ 67% Complete (4/6)
**Phase 3:** ❌ 0% Complete (0/4)

## This Iteration's Achievements
1. ✅ **C2**: Task board actions → TaskManager backend (COMPLETE)
   - Added TaskManager integration with Arc<Mutex<>> pattern
   - Wired all task lifecycle events to backend methods
   - Implemented FileStorageService for task persistence
   - Fallback storage handling for edge cases
   - Build successful, no errors

## Remaining Work

### Immediate Priority: Complete Phase 2 (2 tasks)

#### 1. C4: Session Context Menu (~2-3 hours) - NEXT
**Complexity:** MEDIUM-HIGH
**Blocker:** None
**Priority:** HIGH (Completes Phase 2)

**Challenge:**
GPUI doesn't provide built-in context menu or right-click event handling. Need custom implementation.

**Recommended Approach:**
Instead of complex right-click handling, add a "..." menu button to each session item in the sidebar.

**Implementation Plan:**

```rust
// In render_sidebar(), add menu button to session item

.child(
    div()
        .flex_1()
        .overflow_hidden()
        .text_ellipsis()
        .child(session.name.clone()),
)
.child(
    // Menu button
    div()
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|style| style.bg(hover_bg.opacity(0.15)))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.open_session_menu(session_id, cx);
            cx.notify();
        }))
        .child("⋮"), // Vertical ellipsis
)
```

**State Management:**
```rust
// Add to WorkspaceView
pub(super) session_menu_open: Option<SessionId>,  // Which session's menu is open
```

**Menu Rendering:**
```rust
fn render_session_menu(&self, session_id: SessionId, cx: ...) -> Option<impl IntoElement> {
    if self.session_menu_open != Some(session_id) {
        return None;
    }

    // Render modal overlay with menu options
    Some(
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(Hsla::black().opacity(0.3))
            .on_click(/* close menu */)
            .child(
                div()
                    .bg(theme.panel_background)
                    .border_1()
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .child(menu_item("Rename Session", ...))
                    .child(menu_item("Assign to Group", ...))
                    .child(menu_item("Remove from Group", ...))
                    .child(menu_item("Close Session", ...))
            )
    )
}
```

**Menu Actions:**
- **Rename**: Open text input modal
- **Assign to Group**: Show group picker (with existing groups + "New Group")
- **Remove from Group**: Call `session_manager.set_session_group(id, None, None)`
- **Close Session**: Call `session_manager.close_session(id)`

**Backend Wiring:**
```rust
// Session manager already has these methods:
session_manager.rename_session(session_id, new_name)?;
session_manager.set_session_group(session_id, Some(group_name), Some(color))?;
```

**Files to Modify:**
- `workspace/gpui.rs`: Add `session_menu_open` field, `open_session_menu()` method
- `workspace/render.rs`: Add menu button, `render_session_menu()`, menu item helpers

**Estimated Time:** 2-3 hours

---

#### 2. C3: File Tree Drag-to-Terminal (~30 min)
**Complexity:** LOW
**Blocker:** B2 (File tree integration)
**Priority:** MEDIUM

**Why Blocked:**
FileTreePanel needs to be instantiated in WorkspaceView before drag events can be handled.

**Quick Implementation (after B2 complete):**
```rust
// In workspace/gpui.rs, add event handler

FileTreeEvent::PathDraggedToTerminal { path, session_id } => {
    let path_str = path.to_string_lossy();
    let input = format!("{} ", path_str);
    self.session_manager.lock().unwrap()
        .send_input(session_id, input.as_bytes())
        .ok();
}
```

**Estimated Time:** 30 minutes (after B2)

---

### Phase 3: Major Features (4 tasks, ~13 hours)

#### 3. B2: File Tree Integration (~3 hours) - HIGH PRIORITY
**Complexity:** MEDIUM
**Blocker:** None
**Priority:** HIGH (Unblocks C3)

**Why Important:**
- Unblocks C3
- Major feature completion
- File navigation is core functionality

**Implementation Plan:**

1. **Add FileTreePanel field** (~15 min)
```rust
// In workspace/gpui.rs
pub struct WorkspaceView {
    // ... existing fields
    file_tree: FileTreePanel,
}

// In new()
let file_tree = if let Ok(cwd) = std::env::current_dir() {
    FileTreePanel::new(cwd)
} else {
    FileTreePanel::new(PathBuf::from("."))
};
```

2. **Split Sidebar Rendering** (~45 min)
```rust
fn render_sidebar(&mut self, cx: ...) -> impl IntoElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .child(
            // Sessions section (existing)
            div()
                .flex_1()
                .overflow_hidden()
                .child(self.render_session_list(cx))
        )
        .child(
            // Separator
            div()
                .h(px(1.0))
                .bg(theme.border)
        )
        .child(
            // Files section (new)
            div()
                .flex_1()
                .overflow_hidden()
                .child(self.render_file_tree(cx))
        )
}
```

3. **Render File Tree** (~1 hour)
```rust
fn render_file_tree(&self, cx: ...) -> impl IntoElement {
    let items = self.file_tree.visible_items();

    let mut list = div()
        .flex()
        .flex_col()
        .overflow_y_scroll();

    // Header
    list = list.child(
        div()
            .h(px(32.0))
            .px_3()
            .flex()
            .items_center()
            .child("Files")
    );

    // File items
    for item in items {
        list = list.child(render_file_item(item, cx));
    }

    list
}
```

4. **Wire Events** (~1 hour)
```rust
// Process file tree events
FileTreeEvent::DirectoryToggled(path) => {
    self.file_tree.toggle_directory(&path);
}
FileTreeEvent::FileSelected(path) => {
    // Could open file in editor (future feature)
}
FileTreeEvent::PathDraggedToTerminal { path, session_id } => {
    // Handle in C3
}
```

**Files:**
- `workspace/gpui.rs`: Add field, init, event processing
- `workspace/render.rs`: Split sidebar, add file tree rendering

**Reference:**
- `crates/codirigent-ui/src/sidebar/file_tree.rs` (existing component)

**Estimated Time:** 3 hours

---

#### 4. B3: Task Board Expansion (~4 hours)
**Complexity:** MEDIUM-HIGH
**Blocker:** None
**Priority:** MEDIUM

**Implementation Plan:**

1. **Connect to TaskManager** (~30 min)
```rust
// In render_task_board(), fetch real tasks
let tasks = if let Ok(manager) = self.task_manager.lock() {
    manager.list_tasks()
        .into_iter()
        .filter(|t| matches_tab(t.status, selected_tab))
        .collect()
} else {
    Vec::new()
};
```

2. **Render Task Cards** (~2 hours)
```rust
fn render_task_card(&self, task: &Task, theme: &CodirigentTheme) -> impl IntoElement {
    let priority_color = match task.priority {
        TaskPriority::Critical => Hsla::red(),
        TaskPriority::High => Hsla { h: 0.03, s: 0.80, l: 0.62, a: 1.0 }, // Coral
        TaskPriority::Medium => Hsla { h: 0.15, s: 0.80, l: 0.65, a: 1.0 }, // Yellow
        TaskPriority::Low => Hsla { h: 0.60, s: 0.70, l: 0.60, a: 1.0 }, // Blue
    };

    div()
        .w_full()
        .p_3()
        .bg(theme.panel_background)
        .border_1()
        .border_color(theme.border)
        .rounded_md()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            // Header: Priority dot + Title
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded_full()
                        .bg(priority_color)
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(&task.title)
                )
        )
        .child(
            // Tags
            div()
                .flex()
                .gap_1()
                .children(task.tags.iter().map(|tag| render_tag(tag)))
        )
        .child(
            // Metadata
            div()
                .flex()
                .gap_3()
                .text_xs()
                .text_color(theme.muted)
                .child(format!("Est: {}min", task.estimated_minutes.unwrap_or(0)))
                .child(format!("Created: {}", format_time_ago(task.created_at)))
        )
        .child(
            // Actions
            div()
                .flex()
                .gap_2()
                .child(button("Assign", TaskAction::Assign))
                .child(button("Start", TaskAction::Start))
                .child(button("Delete", TaskAction::Delete))
        )
}
```

3. **Expand/Collapse** (~1 hour)
```rust
// Add expanded state per tab
pub(super) task_board_expanded: HashMap<TaskBoardTab, bool>,

// Toggle in render
.on_click(cx.listener(move |this, _, _, cx| {
    let current = this.task_board_expanded.get(&tab).copied().unwrap_or(false);
    this.task_board_expanded.insert(tab, !current);
    cx.notify();
}))
```

4. **Empty States** (~30 min)
```rust
if tasks.is_empty() {
    return div()
        .p_4()
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme.muted)
        .child("No tasks in this state");
}
```

**Estimated Time:** 4 hours

---

#### 5. B5: Git Worktree Full UI (~6 hours)
**Complexity:** HIGH
**Blocker:** None
**Priority:** LOW (Can defer)

**Scope:**
- New `worktree_panel.rs` component (~400 lines)
- Worktree list view with branch names
- Session binding indicators
- "Create Worktree" modal with branch selection
- Remove worktree buttons
- "Cleanup Merged" action
- Full WorktreeManager backend integration

**Backend Already Complete:**
- `codirigent-session/src/worktree.rs` (8 methods)
- Just needs UI layer

**Estimated Time:** 6 hours

---

## Recommended Execution Order

1. **C4: Session Context Menu** (2-3 hours) ← START HERE
   - Completes Phase 2
   - No blockers
   - User-facing feature

2. **B2: File Tree Integration** (3 hours)
   - Unblocks C3
   - Major feature

3. **C3: File Tree Drag-to-Terminal** (30 min)
   - Quick win after B2

4. **B3: Task Board Expansion** (4 hours)
   - Enhances task management

5. **B5: Git Worktree UI** (6 hours)
   - Can defer if time-constrained
   - Complex but isolated

**Total Remaining:** ~15.5 hours
**Critical Path:** C4 → B2 → C3 → B3 → B5

---

## Risk Mitigation

### C4 Risks:
- **Risk:** Modal state management complexity
- **Mitigation:** Reuse custom layout picker pattern
- **Risk:** Group picker UI complexity
- **Mitigation:** Start with simple list, iterate

### B2 Risks:
- **Risk:** FileTreePanel API changes needed
- **Mitigation:** Study existing component first
- **Risk:** Scroll performance with large directories
- **Mitigation:** Use existing FileTreePanel optimization

### B3 Risks:
- **Risk:** Task card rendering performance
- **Mitigation:** Limit visible tasks per tab (e.g., 20)
- **Risk:** Time formatting dependencies
- **Mitigation:** Use simple relative time ("2h ago")

---

## Success Criteria

### Phase 2 Complete:
- ✅ Session menu accessible (C4)
- ✅ Can rename sessions
- ✅ Can assign sessions to groups
- ✅ File tree visible in sidebar (B2 + C3)
- ✅ Can drag files to terminal

### Phase 3 Complete:
- ✅ Task cards show in task board
- ✅ Can expand/collapse task tabs
- ✅ Task actions work from cards
- ✅ Worktree UI fully functional

---

## Development Tips

1. **Test Incrementally**
   - Build after each major change
   - Test each feature in isolation

2. **Reuse Patterns**
   - Custom layout picker modal → Session menu modal
   - Task board tabs → Worktree panel

3. **Mock Data**
   - Create comprehensive test data upfront
   - Helps visualize edge cases

4. **Error Handling**
   - Always handle lock() failures
   - Provide user feedback for errors
   - Log extensively for debugging

5. **Performance**
   - Profile with 9 sessions + 20 tasks
   - Optimize if frame rate < 60fps
   - Consider virtualization for long lists

---

## Notes for Next Session

- C4 is the critical path to Phase 2 completion
- B2 is high priority (unblocks C3)
- B5 can be deferred if needed
- Build remains stable, no regressions
- All patterns established, just need execution time
