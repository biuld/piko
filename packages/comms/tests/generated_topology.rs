#[test]
fn checked_in_topology_matches_catalog() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("comms crate must live under workspace/packages");
    let markdown_path = workspace.join("docs/generated/communication-topology.md");
    let json_path = workspace.join("docs/generated/communication-topology.json");

    assert_eq!(
        std::fs::read_to_string(markdown_path).expect("generated Mermaid must be checked in"),
        piko_comms::render_mermaid(piko_comms::ALL_SPECS),
        "generated Mermaid topology is stale"
    );
    assert_eq!(
        std::fs::read_to_string(json_path).expect("generated JSON must be checked in"),
        piko_comms::render_json(piko_comms::ALL_SPECS).expect("catalog must serialize"),
        "generated JSON topology is stale"
    );
}
