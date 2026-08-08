use super::*;

#[test]
fn agent_run_prompt_describes_only_the_resolved_tool_catalog() {
    let prompt =
        assemble_agent_run_prompt(&prompt_request(vec![prompt_tool("bash", "Run commands")]));

    assert!(
        !prompt
            .blocks
            .iter()
            .any(|block| block.id == "catalog.skills")
    );
}

#[test]
fn agent_run_prompt_is_deterministic_for_equivalent_tool_catalogs() {
    let first = assemble_agent_run_prompt(&prompt_request(vec![
        prompt_tool("read", "Read files"),
        prompt_tool("bash", "Run commands"),
    ]));
    let second = assemble_agent_run_prompt(&prompt_request(vec![
        prompt_tool("bash", "Run commands"),
        prompt_tool("read", "Read files"),
    ]));

    assert_eq!(first, second);
    assert!(
        first
            .blocks
            .iter()
            .any(|block| block.id == "catalog.skills")
    );
}

#[test]
fn run_dynamic_environment_does_not_invalidate_stable_cache_prefix() {
    let mut first_request = prompt_request(vec![prompt_tool("read", "Read files")]);
    let mut second_request = first_request.clone();
    let environment = second_request
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id == "environment.run")
        .unwrap();
    environment.content = "Current date: 2099-01-01\nCurrent working directory: /other".into();
    environment.content_digest = "changed-environment".into();

    let first = assemble_agent_run_prompt(&first_request);
    let second = assemble_agent_run_prompt(&second_request);
    assert_ne!(first.source_digest, second.source_digest);
    assert_eq!(
        first.cache_plan.semantic_prefix_digest,
        second.cache_plan.semantic_prefix_digest
    );

    let project = first_request
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id.starts_with("project.context"))
        .unwrap();
    project.content = "changed project policy".into();
    project.content_digest = "changed-project".into();
    let changed_project = assemble_agent_run_prompt(&first_request);
    assert_ne!(
        first.cache_plan.semantic_prefix_digest,
        changed_project.cache_plan.semantic_prefix_digest
    );
}

#[test]
fn catalog_stable_scopes_are_labeled_independently_of_project_context() {
    let request = prompt_request(vec![prompt_tool("read", "Read files")]);
    let skills = request
        .resources
        .blocks
        .iter()
        .find(|block| block.id == "catalog.skills")
        .expect("skills catalog");
    let project = request
        .resources
        .blocks
        .iter()
        .find(|block| block.id.starts_with("project.context"))
        .expect("project context");
    assert_eq!(skills.cache_scope, piko_protocol::CacheScope::CatalogStable);
    assert_eq!(
        project.cache_scope,
        piko_protocol::CacheScope::ResourceSnapshot
    );
    assert_eq!(
        piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
        5,
        "D-28 assembly version"
    );
}

#[test]
fn project_and_catalog_invalidations_are_independent() {
    let baseline =
        assemble_agent_run_prompt(&prompt_request(vec![prompt_tool("read", "Read files")]));
    let resource_digest = |plan: &piko_protocol::PromptCachePlan| {
        plan.prefix_segments
            .iter()
            .find(|segment| segment.scope == piko_protocol::CacheScope::ResourceSnapshot)
            .map(|segment| segment.segment_digest.clone())
    };
    let catalog_digest = |plan: &piko_protocol::PromptCachePlan| {
        plan.prefix_segments
            .iter()
            .find(|segment| segment.scope == piko_protocol::CacheScope::CatalogStable)
            .map(|segment| segment.segment_digest.clone())
    };
    let base_resource = resource_digest(&baseline.cache_plan).expect("resource segment");
    let base_catalog = catalog_digest(&baseline.cache_plan).expect("catalog segment");

    let mut project_changed = prompt_request(vec![prompt_tool("read", "Read files")]);
    let project = project_changed
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id.starts_with("project.context"))
        .unwrap();
    project.content = "rewritten project".into();
    // Re-digest matches assemble's content_digest convention only for segment
    // comparison of stable digests field — assembly recomputes from content.
    project.content_digest = "manual".into();
    let after_project = assemble_agent_run_prompt(&project_changed);
    assert_ne!(
        resource_digest(&after_project.cache_plan).as_ref(),
        Some(&base_resource)
    );
    assert_eq!(
        catalog_digest(&after_project.cache_plan).as_ref(),
        Some(&base_catalog),
        "project edits must not invalidate catalog segment"
    );

    let mut skill_changed = prompt_request(vec![prompt_tool("read", "Read files")]);
    let skills = skill_changed
        .resources
        .blocks
        .iter_mut()
        .find(|block| block.id == "catalog.skills")
        .unwrap();
    skills.content = "Available skill metadata:\n- name: other".into();
    skills.content_digest = "skill-changed".into();
    let after_skills = assemble_agent_run_prompt(&skill_changed);
    assert_ne!(
        catalog_digest(&after_skills.cache_plan).as_ref(),
        Some(&base_catalog)
    );
    assert_eq!(
        resource_digest(&after_skills.cache_plan).as_ref(),
        Some(&base_resource),
        "skill catalog edits must not invalidate resource segment"
    );

    let tools_changed = assemble_agent_run_prompt(&prompt_request(vec![
        prompt_tool("read", "Read files"),
        prompt_tool("bash", "Run commands"),
    ]));
    assert_ne!(
        catalog_digest(&tools_changed.cache_plan).as_ref(),
        Some(&base_catalog)
    );
    assert_eq!(
        resource_digest(&tools_changed.cache_plan).as_ref(),
        Some(&base_resource)
    );
}
