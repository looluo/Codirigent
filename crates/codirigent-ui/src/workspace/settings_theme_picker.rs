use crate::theme_manager::ThemeManager;

const DARK_THEME_SECTION_TITLE: &str = "Dark";
const LIGHT_THEME_SECTION_TITLE: &str = "Light";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ThemePickerOption {
    pub(super) id: String,
    pub(super) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ThemePickerSection {
    pub(super) title: &'static str,
    pub(super) options: Vec<ThemePickerOption>,
}

pub(super) fn build_theme_picker_sections(theme_manager: &ThemeManager) -> Vec<ThemePickerSection> {
    let mut dark_options = Vec::new();
    let mut light_options = Vec::new();

    for theme in theme_manager.list() {
        let option = ThemePickerOption {
            id: theme.id.clone(),
            label: theme.name.clone(),
        };

        if theme.is_dark {
            dark_options.push(option);
        } else {
            light_options.push(option);
        }
    }

    sort_theme_picker_options(&mut dark_options);
    sort_theme_picker_options(&mut light_options);

    let mut sections = Vec::new();
    if !dark_options.is_empty() {
        sections.push(ThemePickerSection {
            title: DARK_THEME_SECTION_TITLE,
            options: dark_options,
        });
    }
    if !light_options.is_empty() {
        sections.push(ThemePickerSection {
            title: LIGHT_THEME_SECTION_TITLE,
            options: light_options,
        });
    }

    sections
}

pub(super) fn theme_picker_display_label(
    theme_manager: &ThemeManager,
    selected_id: &str,
) -> String {
    theme_manager
        .get(selected_id)
        .map(|theme| theme.name.clone())
        .unwrap_or_else(|| selected_id.to_string())
}

fn sort_theme_picker_options(options: &mut [ThemePickerOption]) {
    options.sort_by_key(theme_picker_sort_key);
}

fn theme_picker_sort_key(option: &ThemePickerOption) -> (String, String) {
    (
        option.label.to_ascii_lowercase(),
        option.id.to_ascii_lowercase(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::CodirigentTheme;
    use crate::theme_config::Theme;

    #[test]
    fn build_theme_picker_sections_groups_dark_before_light_and_sorts_by_name() {
        let mut manager = ThemeManager::with_defaults();
        manager.add_theme(Theme::from_runtime(
            "night-owl",
            "Night Owl",
            true,
            &CodirigentTheme::dark(),
        ));
        manager.add_theme(Theme::from_runtime(
            "zenburn",
            "Zenburn",
            true,
            &CodirigentTheme::dark(),
        ));
        manager.add_theme(Theme::from_runtime(
            "aurora",
            "Aurora",
            false,
            &CodirigentTheme::light(),
        ));

        let sections = build_theme_picker_sections(&manager);

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, DARK_THEME_SECTION_TITLE);
        assert_eq!(
            sections[0]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Catppuccin Mocha",
                "Dark",
                "Gruvbox Dark",
                "Night Owl",
                "One Dark",
                "Solarized Dark",
                "Tokyo Night",
                "Zenburn",
            ]
        );
        assert_eq!(sections[1].title, LIGHT_THEME_SECTION_TITLE);
        assert_eq!(
            sections[1]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Aurora", "Light"]
        );
    }

    #[test]
    fn theme_picker_display_label_uses_theme_name_when_available() {
        let manager = ThemeManager::with_defaults();

        assert_eq!(theme_picker_display_label(&manager, "dark"), "Dark");
        assert_eq!(
            theme_picker_display_label(&manager, "missing-theme"),
            "missing-theme"
        );
    }
}
