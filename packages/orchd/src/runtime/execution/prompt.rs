pub(super) fn fallback_run_prompt(
    request: &piko_protocol::PromptAssemblyRequest,
) -> piko_protocol::SemanticRunPrompt {
    let content = request.agent_spec.base_instructions.trim().to_string();
    let assembly_version = piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION.to_string();
    let content_digest =
        piko_orchd_api::stable_internal_id("prompt-block", &["agent.instructions", &content]);
    let blocks = if content.is_empty() {
        Vec::new()
    } else {
        vec![piko_protocol::PromptBlock {
            id: "agent.instructions".into(),
            kind: piko_protocol::PromptBlockKind::Instruction,
            authority: piko_protocol::InstructionAuthority::Agent,
            trust: if matches!(
                request.agent_spec.provenance.kind.as_str(),
                "built-in-agent" | "global-agent"
            ) {
                piko_protocol::ContentTrust::Trusted
            } else {
                piko_protocol::ContentTrust::WorkspaceControlled
            },
            source: request.agent_spec.provenance.clone(),
            content,
            content_digest: content_digest.clone(),
            cache_scope: piko_protocol::CacheScope::AgentStable,
        }]
    };
    let source_digest = piko_orchd_api::stable_internal_id(
        "prompt",
        &[
            &assembly_version,
            &content_digest,
            &request.tool_catalog.digest,
        ],
    );
    piko_protocol::SemanticRunPrompt {
        blocks,
        assembly_version: piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
        source_digest: source_digest.clone(),
        cache_plan: piko_protocol::PromptCachePlan {
            policy: piko_protocol::PromptCachePolicy::ProviderDefault,
            prefix_segments: Vec::new(),
            semantic_prefix_digest: source_digest,
        },
    }
}

pub(super) fn resolved_tool_catalog(
    mut tools: Vec<piko_protocol::ToolDef>,
) -> piko_protocol::ResolvedToolCatalog {
    tools.sort_by(|left, right| left.name.cmp(&right.name));
    let serialized = serde_json::to_string(&tools)
        .expect("resolved tool catalog must serialize for prompt diagnostics");
    let digest = piko_orchd_api::stable_internal_id("tool-catalog", &[&serialized]);
    piko_protocol::ResolvedToolCatalog::new(tools, digest)
}
