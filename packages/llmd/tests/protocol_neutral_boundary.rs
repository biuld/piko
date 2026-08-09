use std::path::{Path, PathBuf};

fn read_tree(path: &Path, output: &mut String) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            read_tree(&path, output);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            output.push_str(&std::fs::read_to_string(path).unwrap());
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

#[test]
fn model_call_consumers_have_no_protocol_or_checkpoint_payload_branch() {
    let root = workspace_root();
    let mut consumers = String::new();
    for relative in [
        "packages/orchd/src",
        "packages/hostd/src/application/compaction",
        "packages/hostd/src/application/guardian.rs",
        "packages/hostd/src/domain/compaction",
        "packages/hostd/src/domain/guardian",
    ] {
        let path = root.join(relative);
        if path.is_dir() {
            read_tree(&path, &mut consumers);
        } else {
            consumers.push_str(&std::fs::read_to_string(path).unwrap());
        }
    }
    for forbidden in [
        "ProtocolProfile",
        "ResponsesContinuationPolicy",
        "previous_response_id",
        "encrypted_content",
        "AdapterItemIdentity",
        "ModelContinuation",
        "serde_json::to_value(checkpoint",
        "serde_json::from_value(checkpoint",
    ] {
        assert!(
            !consumers.contains(forbidden),
            "model-call consumer leaked protocol detail: {forbidden}"
        );
    }
}

#[test]
fn public_gateway_and_protocol_dtos_expose_no_adapter_state_shape() {
    let root = workspace_root();
    let public_gateway =
        std::fs::read_to_string(root.join("packages/llmd/src/gateway.rs")).unwrap();
    let protocol_messages =
        std::fs::read_to_string(root.join("packages/protocol/src/messages.rs")).unwrap();
    for forbidden in [
        "ProtocolProfile",
        "ResponsesContinuationPolicy",
        "previous_response_id",
        "output_index",
        "content_index",
        "AdapterItemIdentity",
    ] {
        assert!(
            !public_gateway.contains(forbidden),
            "public gateway leaked {forbidden}"
        );
    }
    for forbidden in [
        "ModelContinuation",
        "base_url",
        "adapter: String",
        "state: serde_json::Value",
    ] {
        assert!(
            !protocol_messages.contains(forbidden),
            "protocol DTO leaked {forbidden}"
        );
    }
}
