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
    '- Playbook JSON: entities/relationships/sources maps; optional access block. See ag-cli/AGENTS.md.',
    '- Binding YAML: source_id + entities (from, id, fields) + relationships (object, link_column) only.',
    '- NEVER include: lookup, operations, join, raw SQL/SOQL, adapter, version, playbook_id in bindings.',
    '- Before authoring: get_binding("crm-payroll-access.postgres") and get_binding("crm-payroll-access.csv") as templates.',
    '- propose_playbook / propose_binding validate only. save_* must use YOUR SAME compact input — NOT debug_compiled_binding_yaml.',
  ];

  const shared = [
    'AnythingGraph thin reasoning layer — MCP front-end over Rust reasoning-service.',
    'Full agent guide: ag-cli/AGENTS.md',
    'Profiles/credentials are configured manually in profiles/local.yaml (never written via MCP).',
    'Data sources are read-only at query time — MCP cannot insert/update/delete live records.',
    'Pass Authorization: Bearer <token> on MCP HTTP requests; token maps to admin or user role.',
  ];

  if (role === 'admin') {
    return [
      ...shared,
      ...compactAuthoring,
      'Admin onboarding workflow:',
      '1) health_check → list_sources → introspect_source(source_id)',
      '2) propose_playbook(playbook_id, compact JSON) → save_playbook(same JSON)',
      '3) suggest_bindings → propose_binding(minimal YAML) → test_binding → save_binding(same YAML, adapter_suffix=source key)',
      '4) Users query with query_graph(playbook_id, subject_id, ...).',
    ].join('\n');
  }

  return [
    ...shared,
    'User workflow:',
    '1) list_playbooks → get_playbook_context(playbook_id)',
    '2) query_graph(..., subject_id=...) for federated read queries',
    '3) list_allowed_rows(playbook_id, subject_id) under enforced ReBAC',
  ].join('\n');
}

// Create MCP server wired to Rust reasoning-service HTTP API.
export function createThinMcpServer(options: ThinMcpServerOptions): McpServer {
  const role = options.role;
  setReasoningAuthToken(options.authToken);

  const server = new McpServer(
    {
      name: 'anythinggraph-thin',
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
    'List configured data sources from profiles/local.yaml (no secrets returned)',
    {},
    async () => {
      const result = await listSources();
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
    'Load one binding YAML by stem — use crm-payroll-access.postgres and .csv as compact templates before authoring',
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
    'Validate compact playbook JSON (entities/relationships/sources maps). Read save_instruction — save the same JSON via save_playbook, not a reformatted dump.',
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
      count_relationship: z.string().optional(),
      count_object_entity: z.string().optional(),
      list_relationship: z.string().optional(),
      list_limit: z.number().optional(),
    },
    async (args) => {
      const queryRequest = {
        playbook_id: args.playbook_id,
        subject_id: args.subject_id,
        binding_name: args.binding_name,
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
      const plan = await compilePlan(queryRequest);
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
    'query_graph',
    'Compile and execute a federated read query in one step (proof envelope)',
    {
      playbook_id: z.string(),
      subject_id: z.string().optional(),
      binding_name: z.string().optional(),
      entity: z.string(),
      by_name: z.string().optional(),
      by_identifier: z.string().optional(),
      count_relationship: z.string().optional(),
      count_object_entity: z.string().optional(),
    },
    async (args) => {
      const queryRequest = {
        playbook_id: args.playbook_id,
        subject_id: args.subject_id,
        binding_name: args.binding_name,
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
      const proof = await runQuery(queryRequest);
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
