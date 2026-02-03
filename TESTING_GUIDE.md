# Codirigent UI - Manual Testing Guide

**Version**: 1.0
**Date**: 2026-02-02
**Status**: Ready for Testing

---

## Overview

This guide provides step-by-step instructions for manually testing all UI fixes implemented in Phases 1-4. Follow this checklist to verify that all 11 issues have been properly resolved.

---

## Prerequisites

### Build the Application
```bash
cd C:\Users\osobo\Documents\Github\Codirigent
cargo build --release
```

### Launch the Application
```bash
cargo run --release
```

---

## Test Suite 1: Window Controls (Phase 1)

### Test 1.1: Close Button
**Priority**: CRITICAL
**Platforms**: Windows, Linux, macOS

**Steps:**
1. Launch Codirigent
2. Click the close button (X)

**Expected:**
- ✅ Application closes gracefully
- ✅ No error messages
- ✅ No need to force quit

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 1.2: Minimize Button
**Priority**: CRITICAL
**Platforms**: Windows, Linux, macOS

**Steps:**
1. Launch Codirigent
2. Click the minimize button (─)
3. Click taskbar/dock icon to restore

**Expected:**
- ✅ Window minimizes to taskbar/dock
- ✅ Window restores when clicked

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

**Note**: This may not work if GPUI doesn't fully support `window.minimize_window()`. Document behavior.

---

### Test 1.3: Maximize Button
**Priority**: CRITICAL
**Platforms**: Windows, Linux, macOS

**Steps:**
1. Launch Codirigent
2. Click the maximize button (□ or traffic light)
3. Click again to restore

**Expected:**
- ✅ Window fills screen
- ✅ Window restores to previous size
- ✅ Button reflects correct state

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 1.4: Window Dragging (Normal)
**Priority**: CRITICAL
**Platforms**: Windows, Linux, macOS

**Steps:**
1. Launch Codirigent (normal window)
2. Click and drag title bar
3. Move window around screen

**Expected:**
- ✅ Window moves smoothly
- ✅ No glitches or jumps

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 1.5: Window Dragging (Maximized - Windows Only)
**Priority**: HIGH
**Platform**: Windows

**Steps:**
1. Launch Codirigent on Windows
2. Maximize window
3. Try to drag title bar

**Expected:**
- ✅ Window auto-restores before dragging
- ✅ Window moves after restore

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Test Suite 2: Task Creation (Phase 2)

### Test 2.1: Open Task Creation Modal
**Priority**: HIGH

**Steps:**
1. Launch Codirigent
2. Locate task board in sidebar
3. Click "+ Add" button

**Expected:**
- ✅ Modal appears centered
- ✅ Modal has dark overlay
- ✅ Title field is focused
- ✅ "Create New Task" header visible

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 2.2: Create Task with Title Only
**Priority**: HIGH

**Steps:**
1. Open task creation modal
2. Type "Test Task 1" in title field
3. Press Enter

**Expected:**
- ✅ Modal closes
- ✅ Task appears in Queue tab
- ✅ Task count updates (e.g., "Queue (1)")

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 2.3: Create Task with Title and Description
**Priority**: HIGH

**Steps:**
1. Open task creation modal
2. Type "Test Task 2" in title field
3. Press Tab to switch to description
4. Type "This is a test description"
5. Press Tab to return to title
6. Press Enter

**Expected:**
- ✅ Tab switches fields correctly
- ✅ Field focus indicator shows "(active)"
- ✅ Task created with both title and description
- ✅ Modal closes

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 2.4: Task Title Validation
**Priority**: MEDIUM

**Steps:**
1. Open task creation modal
2. Leave title field empty
3. Press Enter or click "Create Task"

**Expected:**
- ✅ Error message appears: "Title is required"
- ✅ Modal stays open
- ✅ No task created

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 2.5: Cancel Task Creation
**Priority**: MEDIUM

**Steps:**
1. Open task creation modal
2. Type some text in title
3. Press Escape

**Alternative:**
1. Open task creation modal
2. Click "Cancel" button

**Alternative:**
3. Click dark overlay outside modal

**Expected:**
- ✅ Modal closes
- ✅ No task created
- ✅ Input discarded

**Actual Result:**
- [ ] Pass (Escape)
- [ ] Pass (Cancel button)
- [ ] Pass (Click overlay)
- [ ] Fail (describe issue): _____________

---

### Test 2.6: Multiline Description
**Priority**: MEDIUM

**Steps:**
1. Open task creation modal
2. Type title
3. Press Tab to description field
4. Press Enter to insert newline
5. Type more text
6. Tab back to title
7. Press Enter to create

**Expected:**
- ✅ Enter inserts newline in description (not submit)
- ✅ Enter in title field submits
- ✅ Description preserves newlines

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Test Suite 3: Task Board (Phase 2)

### Test 3.1: Task Count Accuracy
**Priority**: HIGH

**Steps:**
1. Create 3 tasks using task modal
2. Observe Queue tab count

**Expected:**
- ✅ Queue tab shows "Queue (3)"
- ✅ Count updates in real-time

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 3.2: Task List Scrolling
**Priority**: HIGH

**Steps:**
1. Create 10 tasks
2. Observe task list in Queue tab

**Expected:**
- ✅ Task list scrolls vertically
- ✅ All tasks visible with scrolling
- ✅ No content clipping

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Test Suite 4: File Tree (Phase 2)

### Test 4.1: File Tree Click Action
**Priority**: HIGH

**Steps:**
1. Expand file tree in sidebar
2. Create a terminal session (Cmd/Ctrl+N)
3. Click a file in the tree
4. Observe terminal

**Expected:**
- ✅ Terminal receives `vim <filepath>`
- ✅ Vim opens the file (if vim installed)

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Test Suite 5: Layout & Sizing (Phase 3)

### Test 5.1: Initial Window Size
**Priority**: MEDIUM

**Steps:**
1. Close Codirigent if running
2. Launch fresh instance
3. Observe window size

**Expected:**
- ✅ Window opens at comfortable size
- ✅ 2x2 grid visible and readable
- ✅ No need to resize immediately

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 5.2: Grid 2x2 Readability
**Priority**: HIGH

**Steps:**
1. Launch Codirigent
2. Create 4 terminal sessions
3. Observe grid layout

**Expected:**
- ✅ Each cell at least 400x300px
- ✅ Terminal text readable
- ✅ No cramped layout

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 5.3: Grid 3x3 Readability
**Priority**: HIGH

**Steps:**
1. Change layout to 3x3 (toolbar)
2. Create 9 terminal sessions
3. Observe grid layout

**Expected:**
- ✅ Each cell at least 400x300px
- ✅ Terminal text readable
- ✅ Layout comfortable

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 5.4: Title Bar Spacing
**Priority**: MEDIUM

**Steps:**
1. Launch Codirigent
2. Observe space between title bar and sidebar

**Expected:**
- ✅ No large gap (no 28px padding)
- ✅ Sidebar fills from title bar to bottom

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Test Suite 6: Integration Tests

### Test 6.1: Complete Task Workflow
**Priority**: HIGH

**Steps:**
1. Create task "Integration Test"
2. Verify it appears in Queue
3. Click task to select
4. Use task actions (Start, Complete, etc.)
5. Verify counts update

**Expected:**
- ✅ Task moves through statuses
- ✅ Counts update in real-time
- ✅ No errors

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 6.2: Multi-Session with File Tree
**Priority**: HIGH

**Steps:**
1. Create 2 terminal sessions
2. Focus first session
3. Click file in tree
4. Observe first terminal receives vim command
5. Focus second session
6. Click different file
7. Observe second terminal receives vim command

**Expected:**
- ✅ Vim command goes to focused session
- ✅ No cross-talk between sessions

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 6.3: Window Resize Behavior
**Priority**: MEDIUM

**Steps:**
1. Launch Codirigent
2. Create 4 sessions (2x2)
3. Resize window smaller
4. Resize window larger

**Expected:**
- ✅ Cells maintain minimum 400x300
- ✅ Layout adjusts proportionally
- ✅ No visual glitches

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Test Suite 7: Edge Cases

### Test 7.1: Rapid Task Creation
**Priority**: LOW

**Steps:**
1. Open task modal
2. Create task
3. Immediately open modal again
4. Create another task
5. Repeat 5 times quickly

**Expected:**
- ✅ No crashes
- ✅ All tasks created
- ✅ Counts accurate

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 7.2: Long Task Titles
**Priority**: LOW

**Steps:**
1. Open task modal
2. Type 200+ character title
3. Create task

**Expected:**
- ✅ Task created successfully
- ✅ Title truncated in UI if needed
- ✅ No overflow

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Test 7.3: Special Characters in Task
**Priority**: LOW

**Steps:**
1. Open task modal
2. Use title: "Test @#$% & <>&"
3. Create task

**Expected:**
- ✅ Task created
- ✅ Special chars preserved
- ✅ No rendering issues

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Performance Tests

### Perf 1: Task List with 100 Tasks
**Priority**: MEDIUM

**Steps:**
1. Create 100 tasks (can use script)
2. Scroll through task list
3. Observe performance

**Expected:**
- ✅ Smooth scrolling
- ✅ No lag or stutter
- ✅ UI remains responsive

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

### Perf 2: Window Operations
**Priority**: MEDIUM

**Steps:**
1. Rapidly minimize/restore window 10 times
2. Rapidly maximize/restore window 10 times

**Expected:**
- ✅ No crashes
- ✅ Smooth animations
- ✅ State tracked correctly

**Actual Result:**
- [ ] Pass
- [ ] Fail (describe issue): _____________

---

## Platform-Specific Tests

### Windows-Specific Tests
- [ ] Test 1.1: Close button
- [ ] Test 1.2: Minimize button
- [ ] Test 1.3: Maximize button
- [ ] Test 1.5: Drag maximized window

### Linux-Specific Tests
- [ ] Test 1.1: Close button
- [ ] Test 1.2: Minimize button
- [ ] Test 1.3: Maximize button

### macOS-Specific Tests
- [ ] Test 1.1: Close button (red traffic light)
- [ ] Test 1.2: Minimize button (yellow traffic light)
- [ ] Test 1.3: Maximize button (green traffic light)

---

## Regression Tests

### Verify Previous Fixes Still Work
- [ ] Logo size is appropriate (not too large)
- [ ] Text inputs have proper element IDs
- [ ] No compilation warnings

---

## Bug Report Template

If you find a bug, please report using this format:

```markdown
**Test**: [Test ID and name]
**Platform**: [Windows/Linux/macOS]
**Priority**: [Critical/High/Medium/Low]

**Steps to Reproduce**:
1. [Step 1]
2. [Step 2]
3. [Step 3]

**Expected Behavior**:
[What should happen]

**Actual Behavior**:
[What actually happened]

**Screenshots/Logs**:
[Attach if available]

**Workaround**:
[If any exists]
```

---

## Test Summary Template

After completing all tests, fill out this summary:

```markdown
**Date**: [Date]
**Tester**: [Name]
**Platform**: [Windows/Linux/macOS]
**Build**: [Commit hash or version]

**Tests Passed**: __ / __
**Tests Failed**: __
**Tests Skipped**: __

**Critical Issues**: [List any]
**High Issues**: [List any]
**Medium Issues**: [List any]
**Low Issues**: [List any]

**Overall Assessment**:
- [ ] Ready for release
- [ ] Needs minor fixes
- [ ] Needs major fixes

**Notes**:
[Any additional observations]
```

---

## Automated Testing (Future)

For future iterations, consider:
1. Unit tests for modal state management
2. Integration tests for task creation flow
3. UI tests with GPUI test harness
4. Performance benchmarks

---

## Contact

For questions or to report issues:
- GitHub Issues: [Repository URL]
- Documentation: `IMPLEMENTATION_COMPLETE.md`
