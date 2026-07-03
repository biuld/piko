use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandCatalogItem {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub action: CommandCatalogAction,
    pub slash_name: String,
    #[serde(default)]
    pub visible_in_palette: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandCatalogAction {
    Help,
    Commands,
    Sessions,
    Tree,
    Models,
    Settings,
    Status,
    NewSession,
    ForkSession,
    CloneSession,
    RenameSession,
    ImportSession,
    ExportSession,
    DeleteSession,
    Login,
    Logout,
    Compact,
    SetThinking { level: String },
    ToggleToolsExpanded,
    ClearNotifications,
    Quit,
}
