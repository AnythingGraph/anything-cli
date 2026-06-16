use adapter_core::AdapterAuthoringGuide;

// Return agent authoring guide for the SOQL / Salesforce adapter.
pub fn authoring_guide() -> AdapterAuthoringGuide {
    AdapterAuthoringGuide {
        adapter: "soql".into(),
        binding_file_suffix: "salesforce".into(),
        entity_from_meaning: "Salesforce object API name (e.g. Account, Contact)".into(),
        introspect_schema_name: Some(
            "Optional on introspect_source: Salesforce object API name filter".into(),
        ),
        forbidden_binding_keys: vec![
            "schema_name".into(),
            "adapter".into(),
            "instance_url".into(),
            "auth".into(),
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
            "introspect_source(source_id, schema_name optional)".into(),
            "propose_binding → test_binding(execute=true) → save_binding(adapter_suffix=salesforce)".into(),
        ],
    }
}
