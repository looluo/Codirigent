//! Platform-specific window drag helpers.
//!
//! On Windows, GPUI 0.2.x has a timing issue where `WindowControlArea::Drag`
//! doesn't reliably initiate window moves (stale `mouse_hit_test` in
//! WM_NCHITTEST). Worse, when WM_NCHITTEST *does* return HTCAPTION, Windows
//! enters a modal drag loop inside `DefWindowProc` that re-enters the message
//! pump while GPUI still holds `RefCell` borrows — causing a panic / freeze.
//!
//! This module provides a direct Win32 workaround: on mouse-down we post
//! `WM_NCLBUTTONDOWN(HTCAPTION)` **asynchronously** via `PostMessageW`, so the
//! modal drag loop begins on the *next* message-pump iteration, after GPUI's
//! borrows are released.
//!
//! Remove this module after upgrading GPUI to a version that fixes the issue.

/// Begin a native title-bar drag on Windows.
///
/// Posts `ReleaseCapture` + `WM_NCLBUTTONDOWN(HTCAPTION)` so the OS
/// takes over the drag loop. Uses `PostMessageW` (async) instead of
/// `SendMessageW` (sync) to avoid reentrancy — `SendMessageW` starts a
/// modal drag loop that pumps messages while GPUI's `RefCell` is still
/// borrowed by the event callback, causing a panic.
///
/// The `lparam` carries the current cursor screen coordinates packed as
/// `MAKELPARAM(x, y)`. Passing 0 caused some Windows builds to silently
/// ignore the message when the (0,0) position fell outside the window.
#[cfg(target_os = "windows")]
pub fn begin_title_bar_drag(hwnd: isize) {
    use std::ffi::c_int;

    #[allow(clippy::upper_case_acronyms)]
    type HWND = isize;
    #[allow(clippy::upper_case_acronyms)]
    type WPARAM = usize;
    #[allow(clippy::upper_case_acronyms)]
    type LPARAM = isize;
    #[allow(clippy::upper_case_acronyms)]
    type BOOL = c_int;

    #[repr(C)]
    #[allow(clippy::upper_case_acronyms)]
    struct POINT {
        x: i32,
        y: i32,
    }

    const WM_NCLBUTTONDOWN: u32 = 0x00A1;
    const HTCAPTION: WPARAM = 2;

    extern "system" {
        fn ReleaseCapture() -> BOOL;
        fn PostMessageW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> BOOL;
        fn GetCursorPos(point: *mut POINT) -> BOOL;
    }

    // Safety: hwnd is obtained from raw_window_handle and the window is alive
    // during the mouse-down handler that calls this function.
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        // Pack screen coordinates as MAKELPARAM(x, y) = (y << 16) | (x & 0xFFFF)
        let lparam = (((pt.y & 0xFFFF) as LPARAM) << 16) | ((pt.x & 0xFFFF) as LPARAM);
        ReleaseCapture();
        PostMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION, lparam);
    }
}

/// No-op on non-Windows platforms (drag is handled by GPUI natively).
#[cfg(not(target_os = "windows"))]
pub fn begin_title_bar_drag(_hwnd: isize) {}
