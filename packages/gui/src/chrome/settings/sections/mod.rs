//! Settings section panels (content only; chrome header lives in body).

mod account;
mod agent_tools;
mod appearance;
mod context_reliability;
mod general;
mod placeholder;

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::chrome::primary_surface::SettingsSection;

pub fn render_section_panel(
    section: SettingsSection,
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> AnyElement {
    match section {
        SettingsSection::General => general::render_general(app, entity).into_any_element(),
        SettingsSection::Account => account::render_account(app, entity).into_any_element(),
        SettingsSection::AgentTools => {
            agent_tools::render_agent_tools(app, entity).into_any_element()
        }
        SettingsSection::ContextReliability => {
            context_reliability::render_context_reliability(app, entity).into_any_element()
        }
        SettingsSection::Appearance => {
            appearance::render_appearance(app, entity).into_any_element()
        }
        SettingsSection::Keyboard => placeholder::render_placeholder(section).into_any_element(),
        SettingsSection::Advanced => placeholder::render_advanced(entity).into_any_element(),
    }
}
