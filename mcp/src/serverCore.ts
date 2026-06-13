import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { z } from 'zod';
import {
  compilePlan,
  executePlan,
  getBinding,
  getPlaybookContext,
  introspectSource,
  listBindings,
  listSources,
  proposeBinding,
  reasoningHealthCheck,
  rebacAllowedRows,
  runQuery,
  saveBinding,
  suggestBindings,
  testBinding,
} from './reasoningClient.js';

const THIN_MCP_INSTRUCTIONS = [
  'AnythingGraph thin reasoning layer — MCP front-end over Rust reasoning-service.',
  'Onboarding workflow for external agents:',
  '1) health_check → list_sources → introspect_source(source_id)',
  '2) get_playbook_context(playbook_id) → suggest_bindings(playbook_id, source_id)',
  '3) propose_binding(playbook_id, binding_yaml) → test_binding(..., execute=true)',
  '4) save_binding(playbook_id, adapter_suffix, binding_yaml)',
  '5) query_graph(playbook_id, ...) — binding auto-routed from entity_sources + bindings when binding_name omitted.',
  '6) list_allowed_rows(playbook_id, subject_id) — discover visible row ids under enforced ReBAC.',
  'When relationship_access_rules.implementation_status is enforced, pass subject_id or resolve the subject entity.',
  'Playbook JSON may include a bindings map: source keys → binding file stems (see get_playbook_context).',
  'Binding files live in bindings/ as {playbook_id}.{adapter_suffix}.yaml (use list_bindings for loaded stems).',
  'Federated playbooks route entities to sources via entity_sources + bindings; omit binding_name on query_graph to auto-route.',
  'Declarative bindings: set from/id_field/fields and subject_link_column; Rust compiles SQL.',
  'Set AG_REASONING_URL (default http://127.0.0.1:8787).',
].join('\n');

// Create MCP server wired to Rust reasoning-service HTTP API.
export function createThinMcpServer(): McpServer {
  const server = new McpServer(
    {
      name: 'anythinggraph-thin',
      version: '0.2.0',
    },
    {
      instructions: THIN_MCP_INSTRUCTIONS,
    },
  );

  server.tool(
    'health_check',
    'Ping Rust reasoning-service',
    {},
    async () => {
      const result = await reasoningHealthCheck();
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'list_sources',
    'List configured data sources from profiles/local.yaml',
    {},
    async () => {
      const result = await listSources();
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'list_bindings',
    'List loaded binding file stems in bindings/',
    {},
    async () => {
      const result = await listBindings();
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'get_binding',
    'Load one binding YAML by stem name',
    {
      binding_name: z.string().describe('Binding stem, e.g. simple-crm-access.postgres'),
    },
    async ({ binding_name: bindingName }) => {
      const result = await getBinding(bindingName);
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'get_playbook_context',
    'Load playbook schema summary (entities and relationships) from Rust core',
    {
      playbook_id: z.string().describe('Playbook id, e.g. simple-crm-access'),
    },
    async ({ playbook_id: playbookId }) => {
      const result = await getPlaybookContext(playbookId);
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'introspect_source',
    'Read Postgres schema (tables, columns, foreign keys) for agent mapping',
    {
      source_id: z.string().describe('Profile source id, e.g. warehouse_pg'),
      schema_name: z.string().optional().describe('Postgres schema, default public'),
    },
    async ({ source_id: sourceId, schema_name: schemaName }) => {
      const result = await introspectSource(sourceId, schemaName);
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'suggest_bindings',
    'Suggest playbook entity to table mappings from introspected schema',
    {
      playbook_id: z.string(),
      source_id: z.string(),
      schema_name: z.string().optional(),
    },
    async ({ playbook_id: playbookId, source_id: sourceId, schema_name: schemaName }) => {
      const result = await suggestBindings(playbookId, sourceId, schemaName);
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'propose_binding',
    'Validate and compile binding YAML for a playbook without saving',
    {
      playbook_id: z.string(),
      binding_yaml: z.string().describe('Binding YAML text (declarative or full SQL)'),
    },
    async ({ playbook_id: playbookId, binding_yaml: bindingYaml }) => {
      const result = await proposeBinding(playbookId, bindingYaml);
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'test_binding',
    'Compile a sample query against a proposed or saved binding; optionally execute',
    {
      playbook_id: z.string(),
      binding_name: z.string().optional(),
      binding_yaml: z.string().optional(),
      execute: z.boolean().optional().describe('Run live query when true'),
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

      const result = await testBinding(args.playbook_id, payload);
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'save_binding',
    'Save validated binding YAML as bindings/{playbook_id}.{adapter_suffix}.yaml',
    {
      playbook_id: z.string(),
      adapter_suffix: z.string().describe('File suffix, e.g. postgres'),
      binding_yaml: z.string(),
    },
    async ({ playbook_id: playbookId, adapter_suffix: adapterSuffix, binding_yaml: bindingYaml }) => {
      const result = await saveBinding(playbookId, adapterSuffix, bindingYaml);
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  server.tool(
    'plan_query',
    'Compile a structured federated query into plan IR',
    {
      playbook_id: z.string(),
      subject_id: z.string().optional(),
      binding_name: z.string().optional().describe('Binding file stem; defaults to playbook default_binding'),
      entity: z.string().describe('Entity to resolve, e.g. crm_user'),
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
      return {
        content: [{ type: 'text', text: JSON.stringify(plan, null, 2) }],
      };
    },
  );

  server.tool(
    'execute_plan',
    'Execute a compiled plan IR via SQL/SOQL adapters',
    {
      plan: z.record(z.unknown()).describe('Plan object returned by plan_query'),
    },
    async ({ plan }) => {
      const proof = await executePlan(plan);
      return {
        content: [{ type: 'text', text: JSON.stringify(proof, null, 2) }],
      };
    },
  );

  server.tool(
    'query_graph',
    'Compile and execute a federated query in one step (proof envelope)',
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
      return {
        content: [{ type: 'text', text: JSON.stringify(proof, null, 2) }],
      };
    },
  );

  server.tool(
    'list_allowed_rows',
    'List row identifiers a subject may read under enforced relationship_access_rules',
    {
      playbook_id: z.string(),
      subject_id: z.string().describe('Access subject identifier, e.g. crm_user.user_id value'),
      entity_name: z.string().optional().describe('Optional entity filter; omit for all entities'),
    },
    async (args) => {
      const result = await rebacAllowedRows({
        playbook_id: args.playbook_id,
        subject_id: args.subject_id,
        entity_name: args.entity_name,
      });
      return {
        content: [{ type: 'text', text: JSON.stringify(result, null, 2) }],
      };
    },
  );

  return server;
}
