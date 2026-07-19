//! English chrome string catalog helpers for the Workbench.

/// Force the v1 chrome locale (`en`) for piko and gpui-component.
pub fn init() {
    rust_i18n::set_locale("en");
    gpui_component::set_locale("en");
}

#[cfg(test)]
mod tests {
    #[test]
    fn english_catalog_resolves() {
        super::init();
        assert_eq!(crate::t!("island.sessions.title"), "Sessions");
        assert_eq!(
            crate::t!("island.sessions.action.open_directory"),
            "Open Directory"
        );
        assert_eq!(crate::t!("composer.action.send"), "Send");
        assert_eq!(
            crate::t!("chrome.toggle.sessions"),
            "Toggle Sessions sidebar"
        );
        assert_eq!(
            crate::t!("chrome.toggle.right_column"),
            "Toggle Agents sidebar"
        );
        assert_eq!(
            crate::t!("activity.item.tool_running", name = "bash"),
            "Tool running: bash"
        );
    }
}
