import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { z } from 'zod';
import {
  isToolAllowedForRole,
  type McpAuthRole,
} from './auth.js';
import {
  compilePlan,
  executePlan,
  getBinding,
  getAdapterGuide,
  getPlaybookContext,
  introspectSource,
  listBindings,
  listPlaybooks,
  listSources,
  proposeBinding,
  proposePlaybook,
  reasoningHealthCheck,
  rebacAllowedRows,
  runQuery,
  saveBinding,
  savePlaybook,
  sampleSource,
  setReasoningAuthToken,
  suggestBindings,
  testBinding,
} from './reasoningClient.js';

export interface ThinMcpServerOptions {
  role: McpAuthRole;
  authToken?: string;
}

// Build MCP instructions text for the caller role.
function buildMcpInstructions(role: McpAuthRole): string {
  const compactAuthoring = [
    'COMPACT AUTHORING (required for new playbooks/bindings):',
    '- Playbook JSON: entities use identifier + attributes; relationships/sources maps; optional access block. See ag-cli/AGENTS.md.',
    '- Binding YAML: source_id + entities (from, id, fields) + relationships (object, link_column) only.',
    '- Playbook identifier/attributes are logical vocabulary; binding id/fields map to physical storage (table/column/property).',
    '- NEVER include: lookup, operations, join, raw SQL/SOQL, adapter, version, playbook_id in bindings.',
    '- SQL/CSV templates: get_binding("crm-payroll-access.postgres") and get_binding("crm-payroll-access.csv").',
    '- Mongo/REST/SOQL: get_adapter_guide(source_id) → use example_binding_yaml + instructions_markdown; introspect_source + sample_source before propose_binding.',
    '- Exploring data in a source (no playbook): introspect_source for schema, sample_source(resource=table|collection|object) for example rows.',
    '- propose_playbook / propose_binding validate only. save_* must use YOUR SAME compact input — NOT debug_compiled_binding_yaml.',
  ];

  const shared = [
    'AnythingGraph thin reasoning layer — MCP front-end over Rust reasoning-service.',
    'Full agent guide: ag-cli/AGENTS.md',
    'USE MCP TOOLS ONLY: call registered MCP tools (health_check, introspect_source, query_graph, etc.).',
    'Do NOT use curl, shell HTTP, or Python scripts to call http://127.0.0.1:8787 unless the user explicitly requests a standalone script or MCP is unavailable.',
    'Profiles/credentials are configured manually in profiles/local.yaml (never written via MCP).',
    'Data sources are read-only at query time — MCP cannot insert/update/delete live records.',
    'Pass Authorization: Bearer <token> on MCP HTTP requests; token maps to admin or user role.',
    'After list_sources: call get_adapter_guide(source_id) before propose_binding (per-adapter rules).',
  ];

  if (role === 'admin') {
    return [
      ...shared,
      ...compactAuthoring,
      'Admin onboarding workflow:',
      '1) health_check → list_sources → get_adapter_guide(source_id) for each source you will bind',
      '2) introspect_source(source_id, schema_name when required) — sample_source(resource=...) for raw row preview',
      '3) propose_playbook → save_playbook → suggest_bindings → propose_binding → test_binding → save_binding',
      '4) Users query with query_graph(playbook_id, subject_id, ...).',
    ].join('\n');
  }

  return [
    ...shared,
    'User workflow:',
    '1) list_playbooks → get_playbook_context(playbook_id)',
    '2) list_entity / sample_entity to browse rows; query_graph for resolve + relationship counts',
    '3) list_allowed_rows(playbook_id, subject_id) under enforced ReBAC',
  ].join('\n');
}

// Build reasoning-service query JSON from MCP tool arguments.
function buildQueryRequestFromToolArgs(
  args: Record<string, unknown>,
): Record<string, unknown> {
  const baseRequest: Record<string, unknown> = {
    playbook_id: args.playbook_id,
    subject_id: args.subject_id,
    binding_name: args.binding_name,
  };

  if (args.sample_entity === true) {
    return {
      ...baseRequest,
      sample_entity: {
        entity: args.entity,
        limit: args.limit,
      },
    };
  }

  if (args.list_entity === true) {
    return {
      ...baseRequest,
      list_entity: {
        entity: args.entity,
        limit: args.limit,
      },
    };
  }

  return {
    ...baseRequest,
    resolve: {
      entity: args.entity,
      by_name: args.by_name,
      by_identifier: args.by_identifier,
    },
    count: args.count_relationship
      ? {
          relationship: args.count_relationship,
          object_entity: args.count_object_entity,
        }
      : undefined,
    list: args.list_relationship
      ? {
          relationship: args.list_relationship,
          object_entity: args.count_object_entity,
          limit: args.list_limit,
        }
      : undefined,
  };
}

// Create MCP server wired to Rust reasoning-service HTTP API.
export function createThinMcpServer(options: ThinMcpServerOptions): McpServer {
  const role = options.role;
  setReasoningAuthToken(options.authToken);

  const server = new McpServer(
    {
      name: 'anythinggraph-cli',
      version: '0.3.1',
    },
    {
      instructions: buildMcpInstructions(role),
    },
  );

  // Register a tool only when the caller role is allowed to use it.
  function registerRoleTool(
    toolName: string,
    title: string,
    schema: Record<string, z.ZodTypeAny>,
    handler: (args: Record<string, unknown>) => Promise<{ content: Array<{ type: 'text'; text: string }> }>,
  ): void {
    if (!isToolAllowedForRole(toolName, role)) {
      return;
    }

    server.tool(toolName, title, schema, handler as never);
  }

  registerRoleTool('health_check', 'Ping Rust reasoning-service', {}, async () => {
    const result = await reasoningHealthCheck();
    return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
  });

  registerRoleTool('list_playbooks', 'List playbook ids loaded from playbooks/', {}, async () => {
    const result = await listPlaybooks();
    return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
  });

  registerRoleTool(
    'get_playbook_context',
    'Load playbook schema summary (entities and relationships) from Rust core',
    { playbook_id: z.string().describe('Playbook id, e.g. simple-crm-access') },
    async ({ playbook_id: playbookId }) => {
      const result = await getPlaybookContext(String(playbookId));
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'list_sources',
    'List configured data sources from profiles/local.yaml (no secrets). Each entry includes authoring_next_step — call get_adapter_guide next.',
    {},
    async () => {
      const result = await listSources();
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'get_adapter_guide',
    'Per-adapter binding authoring guide for a profile source_id. Call after list_sources, before propose_binding. Returns instructions_markdown, example_binding_yaml, forbidden keys.',
    {
      source_id: z.string().describe('Profile source id from list_sources, e.g. warehouse_pg or payroll_csv'),
    },
    async ({ source_id: sourceId }) => {
      const result = await getAdapterGuide(String(sourceId));
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'list_bindings',
    'List loaded binding file stems in bindings/',
    {},
    async () => {
      const result = await listBindings();
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'get_binding',
    'Load one saved binding YAML by stem. SQL/CSV demos: crm-payroll-access.postgres / .csv. For Mongo/REST/SOQL use get_adapter_guide(source_id) instead.',
    { binding_name: z.string().describe('Binding stem, e.g. crm-payroll-access.postgres') },
    async ({ binding_name: bindingName }) => {
      const result = await getBinding(String(bindingName));
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'introspect_source',
    'Read source schema for agent mapping (tables/columns only — read-only)',
    {
      source_id: z.string().describe('Profile source id, e.g. warehouse_pg'),
      schema_name: z.string().optional(),
    },
    async ({ source_id: sourceId, schema_name: schemaName }) => {
      const result = await introspectSource(String(sourceId), schemaName ? String(schemaName) : undefined);
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'sample_source',
    'Read a few raw rows from a source table/collection/object — no playbook or binding required',
    {
      source_id: z.string().describe('Profile source id from list_sources'),
      resource: z
        .string()
        .optional()
        .describe('Table, MongoDB collection, Salesforce object, or REST path (optional for CSV)'),
      schema_name: z.string().optional().describe('SQL schema or MongoDB database when required'),
      limit: z.number().int().min(1).max(100).optional().describe('Row cap (default 5, max 100)'),
    },
    async ({ source_id: sourceId, resource, schema_name: schemaName, limit }) => {
      const result = await sampleSource(
        String(sourceId),
        resource ? String(resource) : undefined,
        schemaName ? String(schemaName) : undefined,
        typeof limit === 'number' ? limit : undefined,
      );
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'suggest_bindings',
    'Suggest playbook entity to table mappings from introspected schema',
    {
      playbook_id: z.string(),
      source_id: z.string(),
      schema_name: z.string().optional(),
    },
    async ({ playbook_id: playbookId, source_id: sourceId, schema_name: schemaName }) => {
      const result = await suggestBindings(
        String(playbookId),
        String(sourceId),
        schemaName ? String(schemaName) : undefined,
      );
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'propose_playbook',
    'Validate compact playbook JSON (entities with identifier/attributes, relationships/sources maps). Read save_instruction — save the same JSON via save_playbook, not a reformatted dump.',
    {
      playbook_id: z.string().describe('Must match "id" inside playbook_json'),
      playbook_json: z.string().describe('Compact JSON string — see AGENTS.md'),
    },
    async ({ playbook_id: playbookId, playbook_json: playbookJson }) => {
      const result = await proposePlaybook(String(playbookId), String(playbookJson));
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'save_playbook',
    'Save compact playbook JSON to playbooks/{playbook_id}.json — use the same JSON you passed to propose_playbook',
    {
      playbook_id: z.string(),
      playbook_json: z.string().describe('Same compact JSON you validated with propose_playbook'),
    },
    async ({ playbook_id: playbookId, playbook_json: playbookJson }) => {
      const result = await savePlaybook(String(playbookId), String(playbookJson));
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'propose_binding',
    'Validate declarative binding YAML (no SQL). Read save_instruction in response — save the SAME YAML via save_binding, never debug_compiled_binding_yaml',
    {
      playbook_id: z.string(),
      binding_yaml: z.string().describe('Compact YAML: source_id, entities (from/id/fields), relationships (object/link_column) only'),
    },
    async ({ playbook_id: playbookId, binding_yaml: bindingYaml }) => {
      const result = await proposeBinding(String(playbookId), String(bindingYaml));
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'test_binding',
    'Compile a sample query against a proposed or saved binding; optionally execute read-only',
    {
      playbook_id: z.string(),
      binding_name: z.string().optional(),
      binding_yaml: z.string().optional(),
      execute: z.boolean().optional(),
      entity: z.string().optional(),
      by_name: z.string().optional(),
      by_identifier: z.string().optional(),
      count_relationship: z.string().optional(),
      count_object_entity: z.string().optional(),
    },
    async (args) => {
      const payload: Record<string, unknown> = {
        playbook_id: args.playbook_id,
        binding_name: args.binding_name,
        binding_yaml: args.binding_yaml,
        execute: args.execute,
      };

      if (args.entity || args.by_name || args.by_identifier || args.count_relationship) {
        payload.sample_query = {
          playbook_id: args.playbook_id,
          resolve: {
            entity: args.entity,
            by_name: args.by_name,
            by_identifier: args.by_identifier,
          },
          count: args.count_relationship
            ? {
                relationship: args.count_relationship,
                object_entity: args.count_object_entity,
              }
            : undefined,
        };
      }

      const result = await testBinding(String(args.playbook_id), payload);
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'save_binding',
    'Save declarative binding YAML to bindings/{playbook_id}.{adapter_suffix}.yaml — use the same YAML you passed to propose_binding',
    {
      playbook_id: z.string(),
      adapter_suffix: z.string().describe('Source key from playbook sources map, e.g. postgres or csv'),
      binding_yaml: z.string().describe('Same compact YAML validated with propose_binding'),
    },
    async ({ playbook_id: playbookId, adapter_suffix: adapterSuffix, binding_yaml: bindingYaml }) => {
      const result = await saveBinding(String(playbookId), String(adapterSuffix), String(bindingYaml));
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  registerRoleTool(
    'plan_query',
    'Compile a structured federated query into plan IR',
    {
      playbook_id: z.string(),
      subject_id: z.string().optional(),
      binding_name: z.string().optional(),
      entity: z.string(),
      by_name: z.string().optional(),
      by_identifier: z.string().optional(),
      list_entity: z.boolean().optional(),
      sample_entity: z.boolean().optional(),
      limit: z.number().optional(),
      count_relationship: z.string().optional(),
      count_object_entity: z.string().optional(),
      list_relationship: z.string().optional(),
      list_limit: z.number().optional(),
    },
    async (args) => {
      const plan = await compilePlan(buildQueryRequestFromToolArgs(args));
      return { content: [{ type: 'text', text: JSON.stringify(plan, null, 2) }] };
    },
  );

  registerRoleTool(
    'execute_plan',
    'Execute a compiled plan IR via read-only adapters',
    { plan: z.record(z.unknown()) },
    async ({ plan }) => {
      const proof = await executePlan(plan as Record<string, unknown>);
      return { content: [{ type: 'text', text: JSON.stringify(proof, null, 2) }] };
    },
  );

  registerRoleTool(
    'list_entity',
    'List rows for a playbook entity (bounded browse; default limit 1000)',
    {
      playbook_id: z.string(),
      entity: z.string(),
      limit: z.number().optional(),
      binding_name: z.string().optional(),
      subject_id: z.string().optional(),
    },
    async (args) => {
      const proof = await runQuery(
        buildQueryRequestFromToolArgs({ ...args, list_entity: true }),
      );
      return { content: [{ type: 'text', text: JSON.stringify(proof, null, 2) }] };
    },
  );

  registerRoleTool(
    'sample_entity',
    'Return a small sample of rows for a playbook entity (default limit 5)',
    {
      playbook_id: z.string(),
      entity: z.string(),
      limit: z.number().optional(),
      binding_name: z.string().optional(),
      subject_id: z.string().optional(),
    },
    async (args) => {
      const proof = await runQuery(
        buildQueryRequestFromToolArgs({ ...args, sample_entity: true }),
      );
      return { content: [{ type: 'text', text: JSON.stringify(proof, null, 2) }] };
    },
  );

  registerRoleTool(
    'query_graph',
    'Compile and execute a federated read query in one step (proof envelope)',
    {
      playbook_id: z.string(),
      subject_id: z.string().optional(),
      binding_name: z.string().optional(),
      entity: z.string(),
      by_name: z.string().optional(),
      by_identifier: z.string().optional(),
      list_entity: z.boolean().optional().describe('Browse rows for entity instead of resolve lookup'),
      sample_entity: z.boolean().optional().describe('Small row sample instead of resolve lookup'),
      limit: z.number().optional(),
      count_relationship: z.string().optional(),
      count_object_entity: z.string().optional(),
    },
    async (args) => {
      const proof = await runQuery(buildQueryRequestFromToolArgs(args));
      return { content: [{ type: 'text', text: JSON.stringify(proof, null, 2) }] };
    },
  );

  registerRoleTool(
    'list_allowed_rows',
    'List row identifiers a subject may read under enforced relationship_access_rules',
    {
      playbook_id: z.string(),
      subject_id: z.string(),
      entity_name: z.string().optional(),
    },
    async (args) => {
      const result = await rebacAllowedRows({
        playbook_id: String(args.playbook_id),
        subject_id: String(args.subject_id),
        entity_name: args.entity_name ? String(args.entity_name) : undefined,
      });
      return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
    },
  );

  return server;
}
