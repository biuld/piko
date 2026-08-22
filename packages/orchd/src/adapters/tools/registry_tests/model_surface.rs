use super::*;

fn descriptor(name: &str) -> piko_llmd::gateway::ModelDescriptor {
    piko_llmd::gateway::ModelDescriptor {
        model: piko_llmd::gateway::ModelRef::new("provider", "model"),
        display_name: "Model".into(),
        capabilities: Default::default(),
        limits: Default::default(),
        upstream_tools: vec![piko_llmd::gateway::UpstreamToolDescriptor {
            name: name.into(),
            kind: piko_llmd::capabilities::UpstreamToolKind::new("future_media").unwrap(),
            approval: piko_llmd::tools::UpstreamApprovalPolicy::OnRequest,
            wire_definition_digest: "sha256:definition-a".into(),
        }],
    }
}

#[tokio::test]
async fn wire_definition_fingerprint_participates_in_surface_digest() {
    let registry = ToolRegistryImpl::new();
    let mut first = descriptor("image_generation");
    registry.register_upstream_catalog(&first).await;
    let first_digest = registry
        .resolve_model_surface("provider", "model", Vec::new(), true)
        .await
        .unwrap()
        .digest;

    first.upstream_tools[0].wire_definition_digest = "sha256:definition-b".into();
    registry.register_upstream_catalog(&first).await;
    let second_digest = registry
        .resolve_model_surface("provider", "model", Vec::new(), true)
        .await
        .unwrap()
        .digest;

    assert_ne!(first_digest, second_digest);
}

#[tokio::test]
async fn registry_resolves_one_sorted_caller_and_upstream_surface() {
    let registry = ToolRegistryImpl::new();
    registry
        .register_upstream_catalog(&descriptor("image_generation"))
        .await;

    let surface = registry
        .resolve_model_surface(
            "provider",
            "model",
            vec![catalog_tool("workspace_read", "read")],
            true,
        )
        .await
        .unwrap();

    assert_eq!(
        surface
            .tools
            .iter()
            .map(|tool| tool.name())
            .collect::<Vec<_>>(),
        vec!["image_generation", "workspace_read"]
    );
    assert!(surface.digest.starts_with("model-tool-surface_"));
    assert!(matches!(
        surface.tools[0],
        piko_llmd::tools::InferenceTool::Upstream(_)
    ));
}

#[tokio::test]
async fn registry_excludes_upstream_when_step_disallows_tools() {
    let registry = ToolRegistryImpl::new();
    registry
        .register_upstream_catalog(&descriptor("image_generation"))
        .await;
    let surface = registry
        .resolve_model_surface("provider", "model", Vec::new(), false)
        .await
        .unwrap();
    assert!(surface.tools.is_empty());
}

#[tokio::test]
async fn registry_rejects_caller_upstream_name_collisions() {
    let registry = ToolRegistryImpl::new();
    registry
        .register_upstream_catalog(&descriptor("duplicate"))
        .await;
    let error = registry
        .resolve_model_surface(
            "provider",
            "model",
            vec![catalog_tool("duplicate", "read")],
            true,
        )
        .await
        .unwrap_err();
    assert!(error.contains("duplicate model-visible tool name"));
}
