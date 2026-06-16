use adapter_core::AdapterAuthoringGuide;

// Return agent authoring guide for the CSV adapter.
pub fn authoring_guide() -> AdapterAuthoringGuide {
    AdapterAuthoringGuide {
        adapter: "csv".into(),
        binding_file_suffix: "csv".into(),
        entity_from_meaning: "CSV filename (not full path — path is in profile file_path)".into(),
        introspect_schema_name: None,
        forbidden_binding_keys: vec![
            "schema_name".into(),
            "adapter".into(),
            "playbook_id".into(),
            "file_path".into(),
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
            "propose_binding → test_binding(execute=true) → save_binding(adapter_suffix=csv)".into(),
        ],
    }
}
