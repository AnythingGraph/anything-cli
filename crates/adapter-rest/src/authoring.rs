use adapter_core::AdapterAuthoringGuide;

// Return agent authoring guide for the REST adapter.
pub fn authoring_guide() -> AdapterAuthoringGuide {
    AdapterAuthoringGuide {
        adapter: "rest".into(),
        binding_file_suffix: "rest".into(),
        entity_from_meaning: "REST resource path starting with / (relative to profile base_url)".into(),
        introspect_schema_name: Some(
            "Optional resource path prefix for introspect_source".into(),
        ),
        forbidden_binding_keys: vec![
            "schema_name".into(),
            "adapter".into(),
            "base_url".into(),
            "lookup".into(),
            "operations".into(),
        ],
        allowed_top_level_keys: vec![
            "source_id".into(),
            "entities".into(),
            "relationships".into(),
        ],
        instructions_markdown: include_str!("../AGENTS.md").into(),
        example_binding_yaml: Some(include_str!("../examples/binding.yaml").into()),
        workflow_steps: vec![
            "get_adapter_guide(source_id)".into(),
            "introspect_source(source_id)".into(),
            "propose_binding → test_binding(execute=true) → save_binding(adapter_suffix=rest)".into(),
        ],
    }
}
