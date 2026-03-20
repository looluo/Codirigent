//! Keyboard navigation for the Keyboard Shortcuts settings panel.

use crate::settings::SettingsPage;

/// Compute the next focused row after a navigation key press.
///
/// `sorted_keys` must match the order used to render the table (alphabetical).
/// `down` = true for ArrowDown / Tab; false for ArrowUp / Shift+Tab.
pub(super) fn navigate_shortcuts_focus(
    page: &SettingsPage,
    sorted_keys: &[String],
    down: bool,
) -> Option<String> {
    if sorted_keys.is_empty() {
        return None;
    }
    let current = page.focused_shortcut_row.as_deref();
    let pos = current.and_then(|c| sorted_keys.iter().position(|k| k == c));
    let next = match pos {
        None => 0,
        Some(i) if down => (i + 1).min(sorted_keys.len() - 1),
        Some(0) => 0,
        Some(i) => i - 1,
    };
    Some(sorted_keys[next].clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsPage;
    use codirigent_core::config::ProjectConfig;
    use codirigent_core::config::UserSettings;

    fn make_page() -> SettingsPage {
        SettingsPage::new(
            UserSettings::default(),
            ProjectConfig::default(),
            vec![],
            vec![],
            vec![],
            codirigent_core::config::TerminalThemeOverrides::default(),
        )
    }

    fn sorted_keys() -> Vec<String> {
        let mut v: Vec<String> = UserSettings::default_keybindings()
            .keys()
            .cloned()
            .collect();
        v.sort();
        v
    }

    #[test]
    fn test_navigate_down_from_none_selects_first() {
        let page = make_page();
        let keys = sorted_keys();
        let result = navigate_shortcuts_focus(&page, &keys, true);
        assert_eq!(result, Some(keys[0].clone()));
    }

    #[test]
    fn test_navigate_down_from_first_selects_second() {
        let mut page = make_page();
        let keys = sorted_keys();
        page.focused_shortcut_row = Some(keys[0].clone());
        let result = navigate_shortcuts_focus(&page, &keys, true);
        assert_eq!(result, Some(keys[1].clone()));
    }

    #[test]
    fn test_navigate_up_from_second_selects_first() {
        let mut page = make_page();
        let keys = sorted_keys();
        page.focused_shortcut_row = Some(keys[1].clone());
        let result = navigate_shortcuts_focus(&page, &keys, false);
        assert_eq!(result, Some(keys[0].clone()));
    }

    #[test]
    fn test_navigate_down_at_end_stays_at_last() {
        let mut page = make_page();
        let keys = sorted_keys();
        let last = keys.last().unwrap().clone();
        page.focused_shortcut_row = Some(last.clone());
        let result = navigate_shortcuts_focus(&page, &keys, true);
        assert_eq!(result, Some(last));
    }

    #[test]
    fn test_navigate_up_at_start_stays_at_first() {
        let mut page = make_page();
        let keys = sorted_keys();
        page.focused_shortcut_row = Some(keys[0].clone());
        let result = navigate_shortcuts_focus(&page, &keys, false);
        assert_eq!(result, Some(keys[0].clone()));
    }

    #[test]
    fn test_navigate_empty_list_returns_none() {
        let page = make_page();
        let result = navigate_shortcuts_focus(&page, &[], true);
        assert_eq!(result, None);
    }
}
