use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let check = matches!(args.next().as_deref(), Some("--check"));
    let first_path = if check {
        args.next()
    } else {
        std::env::args().nth(1)
    };
    let markdown_path = PathBuf::from(
        first_path.ok_or("usage: piko-comms-topology [--check] <markdown-path> <json-path>")?,
    );
    let json_path = PathBuf::from(
        args.next()
            .ok_or("usage: piko-comms-topology [--check] <markdown-path> <json-path>")?,
    );
    piko_comms::validate_catalog(piko_comms::ALL_SPECS)
        .map_err(|errors| format!("invalid communication catalog: {errors:#?}"))?;
    let markdown = piko_comms::render_mermaid(piko_comms::ALL_SPECS);
    let json = piko_comms::render_json(piko_comms::ALL_SPECS)?;
    if check {
        check_file(&markdown_path, &markdown)?;
        check_file(&json_path, &json)?;
    } else {
        if let Some(parent) = markdown_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(parent) = json_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(markdown_path, markdown)?;
        std::fs::write(json_path, json)?;
    }
    Ok(())
}

fn check_file(path: &std::path::Path, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
    let actual = std::fs::read_to_string(path)?;
    if actual != expected {
        return Err(format!(
            "generated communication topology is stale: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}
