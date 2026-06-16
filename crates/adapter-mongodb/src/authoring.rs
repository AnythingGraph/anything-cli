use adapter_core::AdapterAuthoringGuide;

// Return agent authoring guide for the MongoDB adapter.
pub fn authoring_guide() -> AdapterAuthoringGuide {
    AdapterAuthoringGuide {
        adapter: "mongodb".into(),
        binding_file_suffix: "mongodb".into(),
        entity_from_meaning: "MongoDB collection name".into(),
        introspect_schema_name: Some(
            "MongoDB database name for introspect_source (or set profile database)".into(),
        ),
        forbidden_binding_keys: vec![
            "schema_name".into(),
            "adapter".into(),
            "dsn".into(),
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
            "introspect_source(source_id, schema_name=database)".into(),
            "propose_binding → test_binding(execute=true) → save_binding(adapter_suffix=mongodb)".into(),
        ],
    }
}
