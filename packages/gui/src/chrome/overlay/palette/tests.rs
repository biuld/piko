use piko_protocol::{CommandCatalogAction, CommandCatalogItem};

use super::{CommandPalette, PaletteRowAction};

fn catalog_item(action: CommandCatalogAction, title: &str, slash: &str) -> CommandCatalogItem {
    CommandCatalogItem {
        id: title.to_lowercase(),
        title: title.into(),
        detail: "detail".into(),
        action,
        slash_name: slash.into(),
        visible_in_palette: true,
    }
}

#[test]
fn root_marks_models_as_submenu() {
    let catalog = vec![
        catalog_item(CommandCatalogAction::Models, "Models", "/models"),
        catalog_item(CommandCatalogAction::Quit, "Quit", "/quit"),
    ];
    let frame = CommandPalette::root_frame(&catalog);
    assert_eq!(frame.rows.len(), 2);
    assert!(matches!(
        frame.rows[0].action,
        PaletteRowAction::EnterModels
    ));
    assert!(frame.rows[0].enabled);
    assert!(matches!(
        frame.rows[1].action,
        PaletteRowAction::Catalog(CommandCatalogAction::Quit)
    ));
}
