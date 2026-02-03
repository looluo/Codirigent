# Comprehensive Fix Guide - All 7 UI Issues

Based on Zed's implementation patterns, here's how to fix all issues in Codirigent.

## Issue #1 & #2: Window Controls (Minimize/Maximize/Close) + Draggable Titlebar

### Problem
- Titlebar not draggable - can't move window
- Minimize button doesn't work correctly
- Missing `window_control_area` markers

### Solution (workspace/render.rs, lines 873-1076)

```rust
// Add to render_title_bar method

// 1. DRAG REGION (middle section)
let mut drag_region = div()
    .flex_1()
    .h_full()
    .flex()
    .items_center()
    .window_control_area(WindowControlArea::Drag)  // ✅ Mark as draggable
    .on_mouse_down(MouseButton::Left, cx.listener(|this, _event, window, cx| {
        // Windows: restore before dragging if maximized
        #[cfg(target_os = "windows")]
        if window.is_maximized() {
            window.zoom_window();
            this.title_bar.set_maximized(false);
            cx.notify();
        }

        window.start_window_move();  // ✅ Start drag
    }));

// 2. WINDOW CONTROL BUTTONS

// For macOS (traffic lights on left):
#[cfg(target_os = "macos")]
{
    for btn in &hints.controls {
        let control_area = match btn.control {
            WindowControl::Close => WindowControlArea::Close,
            WindowControl::Minimize => WindowControlArea::Min,
            WindowControl::Maximize => WindowControlArea::Max,
        };

        let mut control = div()
            .w(px(14.0))
            .h(px(14.0))
            .rounded_full()
            .bg(btn.current_color().into())
            .window_control_area(control_area);  // ✅ Mark control area

        control = match btn.control {
            WindowControl::Minimize => {
                control.on_click(cx.listener(|_this, _: &ClickEvent, window, _cx| {
                    window.minimize_window();  // ✅ Native minimize
                }))
            }
            WindowControl::Maximize => {
                control.on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    window.zoom_window();  // ✅ Toggle maximize
                    this.title_bar.set_maximized(window.is_maximized());
                    cx.notify();
                }))
            }
            WindowControl::Close => {
                control.on_click(cx.listener(|_this, _: &ClickEvent, _window, cx| {
                    cx.quit();  // ✅ Close window
                }))
            }
        };
    }
}

// For Windows/Linux (buttons on right):
#[cfg(not(target_os = "macos"))]
{
    // Minimize
    div()
        .w(px(28.0))
        .h(px(20.0))
        .window_control_area(WindowControlArea::Min)  // ✅ Mark as minimize
        .on_click(cx.listener(|_this, _: &ClickEvent, window, _cx| {
            window.minimize_window();
        }))

    // Maximize
    div()
        .w(px(28.0))
        .h(px(20.0))
        .window_control_area(WindowControlArea::Max)  // ✅ Mark as maximize
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            window.zoom_window();
            this.title_bar.set_maximized(window.is_maximized());
            cx.notify();
        }))

    // Close
    div()
        .w(px(28.0))
        .h(px(20.0))
        .window_control_area(WindowControlArea::Close)  // ✅ Mark as close
        .on_click(cx.listener(|_this, _: &ClickEvent, _window, cx| {
            cx.quit();
        }))
}
```

**Files to modify:**
- `crates/codirigent-ui/src/workspace/render.rs` (lines 873-1076)

---

## Issue #3: Task Modal Text Input Visual Display

### Problem
- Text fields show vertical line cursor improperly
- No proper text editing feedback
- Missing cursor at the end of text

### Solution Pattern (From Zed)

Zed uses two approaches:
1. **Simple**: Append `|` character to display text when focused
2. **Advanced**: Use GPUI's ElementInputHandler (complex, needs editor integration)

**Use Simple Approach for Codirigent:**

```rust
// workspace/render.rs - render_task_creation_modal

let title_focused = modal.focused_field == 0;
let desc_focused = modal.focused_field == 1;

// Add cursor character to focused field
let title_display = if title_focused {
    format!("{}|", modal.title)  // ✅ Cursor at end
} else {
    modal.title.clone()
};

let desc_display = if desc_focused {
    format!("{}|", modal.description)  // ✅ Cursor at end
} else {
    modal.description.clone()
};

// Render inputs with cursor
text_input("title", title_display, title_focused, false, &style)
text_input("desc", desc_display, desc_focused, false, &style)
```

**Also update keyboard handling (workspace/gpui.rs):**

```rust
// Handle Tab to switch between fields
"tab" => {
    if let Some(modal) = &mut self.task_creation_modal {
        modal.focused_field = (modal.focused_field + 1) % 2;  // Toggle 0<->1
        cx.notify();
    }
}

// Handle character input
_ if key.key.len() == 1 => {
    if let Some(modal) = &mut self.task_creation_modal {
        if let Some(ch) = key.key.chars().next() {
            if modal.focused_field == 0 {
                modal.title.push(ch);
            } else {
                modal.description.push(ch);
            }
            cx.notify();
        }
    }
}

// Handle backspace
"backspace" => {
    if let Some(modal) = &mut self.task_creation_modal {
        if modal.focused_field == 0 {
            modal.title.pop();
        } else {
            modal.description.pop();
        }
        cx.notify();
    }
}
```

**Files to modify:**
- `crates/codirigent-ui/src/workspace/render.rs` (render_task_creation_modal)
- `crates/codirigent-ui/src/workspace/gpui.rs` (keyboard input handling)

---

## Issue #4: Remove Unnecessary Task Status Collapsing

### Problem
Tasks are collapsed under status headers when tasks are only content.

### Solution
Simplify task board UI - show tasks directly without collapsible sections.

```rust
// task_board/panel.rs - simplify structure

// Current (complex):
// [Status Header ▼]
//   [Task 1]
//   [Task 2]

// New (simple):
// [Task 1]
// [Task 2]

// In render method, remove status grouping:
fn render_task_list(&self) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .children(
            tasks.iter().map(|task| self.render_task_item(task))
        )
}
```

**Files to modify:**
- `crates/codirigent-ui/src/workspace/render.rs` (task rendering logic)
- `crates/codirigent-ui/src/task_board/panel.rs` (remove collapsible state)

---

## Issue #5: Fix Terminal Overflow When Resizing Window

### Problem
Terminal gets hidden behind overflow instead of adjusting to window size.

### Solution
Fix layout calculations to respect container bounds.

```rust
// workspace/render.rs - main render method

// Calculate available space correctly
let window_height = /* get from window */;
let title_bar_height = 32.0;
let status_bar_height = 24.0;
let task_board_height = if task_board_expanded { 200.0 } else { 44.0 };

let available_for_content = window_height
    - title_bar_height
    - status_bar_height
    - task_board_height;

// Apply to content area
div()
    .h(px(available_for_content))  // ✅ Constrain height
    .overflow_hidden()              // ✅ Clip overflow
    .child(terminal_grid)
```

**Key GPUI methods:**
- `.overflow_hidden()` - Clip content that exceeds bounds
- `.h(px(exact_height))` - Set exact height
- `.flex_1()` - Take remaining space (use carefully)

**Files to modify:**
- `crates/codirigent-ui/src/workspace/render.rs` (layout calculations)

---

## Issue #6: Add Group Dropdown for Task Assignment

### Problem
"Assign to group" requires manual text input every time.

### Solution
Add dropdown with existing groups + "New group..." option.

```rust
// New component: GroupSelector

pub struct GroupSelector {
    available_groups: Vec<String>,  // Existing groups
    selected: Option<String>,
    dropdown_open: bool,
    new_group_input: String,
    creating_new: bool,
}

impl GroupSelector {
    pub fn render(&self) -> impl IntoElement {
        div()
            .child(
                // Selected value display
                div()
                    .on_click(|_| self.dropdown_open = !self.dropdown_open)
                    .child(self.selected.unwrap_or("Select group..."))
            )
            .when(self.dropdown_open, |div| {
                div.child(
                    // Dropdown menu
                    v_flex()
                        .children(
                            self.available_groups.iter().map(|group| {
                                div()
                                    .on_click(|_| self.select(group))
                                    .child(group)
                            })
                        )
                        .child(
                            // "New group..." option
                            div()
                                .on_click(|_| self.creating_new = true)
                                .child("+ New group...")
                        )
                )
            })
            .when(self.creating_new, |div| {
                div.child(
                    // Input for new group name
                    text_input("new-group", &self.new_group_input, true, false, &style)
                )
            })
    }
}
```

**Files to modify:**
- Create `crates/codirigent-ui/src/components/group_selector.rs`
- Update `crates/codirigent-ui/src/workspace/render.rs` (use GroupSelector)

---

## Issue #7: Fix Git Branch Display Accuracy

### Problem
Git branch not showing accurate branches.

### Solution
Use libgit2 (via git2 crate) to detect current branch.

```rust
// Add to Cargo.toml
// git2 = "0.18"

// In workspace or status bar:
use git2::Repository;

pub fn get_current_branch(repo_path: &Path) -> Option<String> {
    let repo = Repository::open(repo_path).ok()?;
    let head = repo.head().ok()?;

    if head.is_branch() {
        head.shorthand().map(String::from)
    } else {
        // Detached HEAD - show commit hash
        let commit = head.peel_to_commit().ok()?;
        Some(format!("{:.7}", commit.id()))
    }
}

// Display in title bar or status bar:
pub fn render_branch_indicator(&self) -> impl IntoElement {
    if let Some(branch) = self.current_branch.as_ref() {
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child("") // Git branch icon
            )
            .child(
                div()
                    .text_xs()
                    .text_color(fg)
                    .child(branch)
            )
    } else {
        div() // No git repo
    }
}
```

**Files to modify:**
- `crates/codirigent-ui/Cargo.toml` (add git2 dependency)
- `crates/codirigent-ui/src/workspace/gpui.rs` (add branch tracking)
- `crates/codirigent-ui/src/status_bar.rs` (display branch)

---

## Implementation Order (By Priority)

1. **#2 - Draggable titlebar** (30 min) - CRITICAL, blocks basic usage
2. **#1 - Window controls** (20 min) - CRITICAL, part of same fix
3. **#3 - Text input** (45 min) - HIGH, modal is broken
4. **#5 - Terminal overflow** (30 min) - MEDIUM, affects daily use
5. **#4 - Task collapsing** (20 min) - LOW, UI polish
6. **#6 - Group dropdown** (1 hour) - LOW, convenience
7. **#7 - Git branch** (30 min) - LOW, informational

**Total estimated time: 3-4 hours**

---

## Testing Checklist

After each fix:
- [ ] Window can be dragged by titlebar
- [ ] Minimize button minimizes to taskbar
- [ ] Maximize button toggles fullscreen
- [ ] Close button closes app
- [ ] Text input shows cursor at end
- [ ] Tab switches between input fields
- [ ] Backspace deletes characters
- [ ] Terminal resizes with window
- [ ] Tasks show without unnecessary nesting
- [ ] Group dropdown shows existing groups
- [ ] Git branch shows current branch name

---

## Key GPUI Patterns Learned from Zed

1. **Window drag**: `window_control_area(WindowControlArea::Drag)` + `window.start_window_move()`
2. **Control buttons**: `window_control_area(Min/Max/Close)` + native window methods
3. **Text cursor**: Append `|` character to display text when focused
4. **Focus management**: Track focused field index, update on click/tab
5. **Layout constraints**: Use exact heights, not flexible, when overflow is an issue
6. **Dropdown menus**: Toggle visibility, absolute position over content
7. **Git integration**: Use git2 crate for reliable branch detection

---

## Next Steps

Ready to implement? I can:
- A) Implement all fixes in order (#2 → #1 → #3 → #5 → #4 → #6 → #7)
- B) Implement just the critical ones first (#1, #2, #3)
- C) You review this document and tell me which to prioritize

Your choice?
