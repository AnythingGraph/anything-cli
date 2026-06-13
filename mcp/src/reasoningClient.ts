const DEFAULT_REASONING_URL = 'http://127.0.0.1:8787';

// Resolve reasoning-service base URL from environment.
export function getReasoningBaseUrl(): string {
  const fromEnv = process.env.AG_REASONING_URL?.trim();
  return fromEnv || DEFAULT_REASONING_URL;
}

// Parse JSON or throw with response body for easier agent debugging.
async function parseJsonResponse(response: Response, actionLabel: string): Promise<Record<string, unknown>> {
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${actionLabel} failed: ${response.status} ${text}`);
  }
  return response.json() as Promise<Record<string, unknown>>;
}

// Call reasoning-service GET /health.
export async function reasoningHealthCheck(): Promise<Record<string, unknown>> {
  const response = await fetch(`${getReasoningBaseUrl()}/health`);
  return parseJsonResponse(response, 'reasoning health');
}

// Fetch playbook context summary from Rust core.
export async function getPlaybookContext(playbookId: string): Promise<Record<string, unknown>> {
  const response = await fetch(
    `${getReasoningBaseUrl()}/playbooks/${encodeURIComponent(playbookId)}/context`,
  );
  return parseJsonResponse(response, 'get playbook context');
}

// List configured data sources from profile.
export async function listSources(): Promise<Record<string, unknown>> {
  const response = await fetch(`${getReasoningBaseUrl()}/sources`);
  return parseJsonResponse(response, 'list sources');
}

// List loaded binding file stems.
export async function listBindings(): Promise<Record<string, unknown>> {
  const response = await fetch(`${getReasoningBaseUrl()}/bindings`);
  return parseJsonResponse(response, 'list bindings');
}

// Fetch one binding by stem name.
export async function getBinding(bindingName: string): Promise<Record<string, unknown>> {
  const response = await fetch(
    `${getReasoningBaseUrl()}/bindings/${encodeURIComponent(bindingName)}`,
  );
  return parseJsonResponse(response, 'get binding');
}

// Introspect tables/columns/foreign keys for a configured source.
export async function introspectSource(
  sourceId: string,
  schemaName?: string,
): Promise<Record<string, unknown>> {
  const response = await fetch(
    `${getReasoningBaseUrl()}/sources/${encodeURIComponent(sourceId)}/introspect`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ schema_name: schemaName }),
    },
  );
  return parseJsonResponse(response, 'introspect source');
}

// Suggest playbook entity to table mappings from source schema.
export async function suggestBindings(
  playbookId: string,
  sourceId: string,
  schemaName?: string,
): Promise<Record<string, unknown>> {
  const response = await fetch(
    `${getReasoningBaseUrl()}/playbooks/${encodeURIComponent(playbookId)}/suggest-bindings`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ source_id: sourceId, schema_name: schemaName }),
    },
  );
  return parseJsonResponse(response, 'suggest bindings');
}

// Validate and compile proposed binding YAML without saving.
export async function proposeBinding(
  playbookId: string,
  bindingYaml: string,
): Promise<Record<string, unknown>> {
  const response = await fetch(
    `${getReasoningBaseUrl()}/playbooks/${encodeURIComponent(playbookId)}/propose-binding`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ binding_yaml: bindingYaml }),
    },
  );
  return parseJsonResponse(response, 'propose binding');
}

// Test a binding with optional live execution.
export async function testBinding(
  playbookId: string,
  payload: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const response = await fetch(
    `${getReasoningBaseUrl()}/playbooks/${encodeURIComponent(playbookId)}/test-binding`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    },
  );
  return parseJsonResponse(response, 'test binding');
}

// Save a validated binding YAML for a playbook.
export async function saveBinding(
  playbookId: string,
  adapterSuffix: string,
  bindingYaml: string,
): Promise<Record<string, unknown>> {
  const response = await fetch(
    `${getReasoningBaseUrl()}/playbooks/${encodeURIComponent(playbookId)}/save-binding`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        adapter_suffix: adapterSuffix,
        binding_yaml: bindingYaml,
      }),
    },
  );
  return parseJsonResponse(response, 'save binding');
}

// Compile a structured query request into plan IR.
export async function compilePlan(queryRequest: Record<string, unknown>): Promise<Record<string, unknown>> {
  const response = await fetch(`${getReasoningBaseUrl()}/plan`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(queryRequest),
  });
  return parseJsonResponse(response, 'compile plan');
}

// Execute a compiled plan and return proof envelope.
export async function executePlan(plan: Record<string, unknown>): Promise<Record<string, unknown>> {
  const response = await fetch(`${getReasoningBaseUrl()}/execute`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ plan }),
  });
  return parseJsonResponse(response, 'execute plan');
}

// Compile and execute in one HTTP call.
export async function runQuery(queryRequest: Record<string, unknown>): Promise<Record<string, unknown>> {
  const response = await fetch(`${getReasoningBaseUrl()}/query`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(queryRequest),
  });
  return parseJsonResponse(response, 'query');
}

// List row ids a subject may read under enforced ReBAC.
export async function rebacAllowedRows(payload: {
  playbook_id: string;
  subject_id: string;
  entity_name?: string;
}): Promise<Record<string, unknown>> {
  const response = await fetch(`${getReasoningBaseUrl()}/rebac/allowed-rows`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  return parseJsonResponse(response, 'rebac allowed rows');
}
