//! Keyboard shortcut recording logic for the settings panel.

use crate::keybindings::{KeyBinding, KeybindingManager, Modifiers};
use gpui::Keystroke;

/// Convert a GPUI keystroke event into a displayable binding string.
///
/// Returns `None` for bare keys (no primary modifier) because those cannot
/// be safely bound as app shortcuts without conflicting with terminal text input.
/// Shift-only is also rejected.
///
/// On macOS: `platform` (Cmd) → "Cmd", `control` → "Ctrl".
/// On Windows/Linux: `platform` (Win/Super) or `control` (Ctrl) both → "Ctrl".
pub(super) fn format_keystroke_as_binding(keystroke: &Keystroke) -> Option<String> {
    let mods = &keystroke.modifiers;

    // Require at least one non-Shift primary modifier.
    let has_primary = mods.platform || mods.control || mods.alt;
    if !has_primary {
        return None;
    }

    let modifiers = build_modifiers(mods);
    let key = normalise_key_name(&keystroke.key);
    let binding = KeyBinding::new(key, modifiers);
    Some(KeybindingManager::format_binding(&binding))
}

#[cfg(target_os = "macos")]
fn build_modifiers(mods: &gpui::Modifiers) -> Modifiers {
    Modifiers {
        cmd: mods.platform,
        ctrl: mods.control,
        alt: mods.alt,
        shift: mods.shift,
    }
}

#[cfg(not(target_os = "macos"))]
fn build_modifiers(mods: &gpui::Modifiers) -> Modifiers {
    // On Windows/Linux, both the Super/Win key (platform) and Ctrl key (control)
    // are treated as the platform modifier and displayed as "Ctrl".
    Modifiers {
        cmd: mods.platform || mods.control,
        ctrl: false, // folded into cmd above
        alt: mods.alt,
        shift: mods.shift,
    }
}

/// Normalise GPUI key names to Title case for round-tripping through parse_binding.
pub(super) fn normalise_key_name(key: &str) -> String {
    match key {
        "backspace" => "Backspace".to_string(),
        "enter" | "return" => "Enter".to_string(),
        "tab" => "Tab".to_string(),
        "escape" => "Escape".to_string(),
        "space" => "Space".to_string(),
        "up" => "Up".to_string(),
        "down" => "Down".to_string(),
        "left" => "Left".to_string(),
        "right" => "Right".to_string(),
        "delete" => "Delete".to_string(),
        "pageup" => "PageUp".to_string(),
        "pagedown" => "PageDown".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "insert" => "Insert".to_string(),
        "f1" => "F1".to_string(),
        "f2" => "F2".to_string(),
        "f3" => "F3".to_string(),
        "f4" => "F4".to_string(),
        "f5" => "F5".to_string(),
        "f6" => "F6".to_string(),
        "f7" => "F7".to_string(),
        "f8" => "F8".to_string(),
        "f9" => "F9".to_string(),
        "f10" => "F10".to_string(),
        "f11" => "F11".to_string(),
        "f12" => "F12".to_string(),
        _ => {
            let mut chars = key.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Keystroke, Modifiers};

    fn make_keystroke(
        key: &str,
        control: bool,
        alt: bool,
        shift: bool,
        platform: bool,
    ) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                control,
                alt,
                shift,
                platform,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        }
    }

    #[test]
    fn test_format_keystroke_ctrl_key() {
        // control=true: on macOS maps to Ctrl modifier; on Windows/Linux folds
        // into the cmd/platform slot which still displays as "Ctrl".
        // Both platforms produce "Ctrl+N" for this keystroke.
        let ks = make_keystroke("n", true, false, false, false);
        let result = format_keystroke_as_binding(&ks);
        assert_eq!(result, Some("Ctrl+N".to_string()));
    }

    #[test]
    fn test_format_keystroke_platform_n() {
        let ks = make_keystroke("n", false, false, false, true);
        let result = format_keystroke_as_binding(&ks);
        #[cfg(target_os = "macos")]
        assert_eq!(result, Some("Cmd+N".to_string()));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(result, Some("Ctrl+N".to_string())); // platform maps to Ctrl display on Windows
    }

    #[test]
    fn test_format_keystroke_bare_key_returns_none() {
        let ks = make_keystroke("a", false, false, false, false);
        let result = format_keystroke_as_binding(&ks);
        assert_eq!(result, None);
    }

    #[test]
    fn test_format_keystroke_shift_only_returns_none() {
        let ks = make_keystroke("a", false, false, true, false);
        let result = format_keystroke_as_binding(&ks);
        assert_eq!(result, None);
    }

    #[test]
    fn test_format_keystroke_ctrl_shift_n() {
        let ks = make_keystroke("n", true, false, true, false);
        let result = format_keystroke_as_binding(&ks);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(result, Some("Ctrl+Shift+N".to_string()));
        #[cfg(target_os = "macos")]
        assert_eq!(result, Some("Ctrl+Shift+N".to_string()));
    }

    #[test]
    fn test_normalise_key_name_special_keys() {
        assert_eq!(normalise_key_name("backspace"), "Backspace");
        assert_eq!(normalise_key_name("enter"), "Enter");
        assert_eq!(normalise_key_name("tab"), "Tab");
        assert_eq!(normalise_key_name("escape"), "Escape");
    }

    #[test]
    fn test_normalise_key_name_plain_letter() {
        assert_eq!(normalise_key_name("n"), "N");
        assert_eq!(normalise_key_name("a"), "A");
    }

    #[test]
    fn test_normalise_key_name_extended_keys() {
        assert_eq!(normalise_key_name("pageup"), "PageUp");
        assert_eq!(normalise_key_name("pagedown"), "PageDown");
        assert_eq!(normalise_key_name("home"), "Home");
        assert_eq!(normalise_key_name("end"), "End");
        assert_eq!(normalise_key_name("insert"), "Insert");
        assert_eq!(normalise_key_name("f1"), "F1");
        assert_eq!(normalise_key_name("f12"), "F12");
    }
}
