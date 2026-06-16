export type McpAuthRole = 'admin' | 'user';

// Parse comma-separated bearer tokens from environment variables.
function parseTokenList(rawValue: string | undefined): string[] {
  if (!rawValue) {
    return [];
  }
  return rawValue
    .split(',')
    .map(function (part) {
      return part.trim();
    })
    .filter(function (part) {
      return part.length > 0;
    });
}

const adminTokens = parseTokenList(process.env.AG_ADMIN_TOKENS);
const userTokens = parseTokenList(process.env.AG_USER_TOKENS);

// True when AG_AUTH_DISABLED is set (local dev — no bearer token required).
function isAuthDisabled(): boolean {
  const rawValue = process.env.AG_AUTH_DISABLED?.trim().toLowerCase();
  return rawValue === '1' || rawValue === 'true' || rawValue === 'yes';
}

// True when at least one auth token is configured for MCP or reasoning-service.
export function isAuthRequired(): boolean {
  if (isAuthDisabled()) {
    return false;
  }
  return adminTokens.length > 0 || userTokens.length > 0;
}

// Resolve MCP caller role from bearer token (matches reasoning-service auth).
export function resolveRoleFromToken(bearerToken: string | undefined): McpAuthRole | null {
  if (!isAuthRequired()) {
    return 'admin';
  }

  const token = bearerToken?.trim();
  if (!token) {
    return null;
  }

  if (adminTokens.includes(token)) {
    return 'admin';
  }
  if (userTokens.includes(token)) {
    return 'user';
  }

  return null;
}

// Extract bearer token from Authorization header value.
export function extractBearerToken(authorizationHeader: string | undefined): string | undefined {
  if (!authorizationHeader) {
    return undefined;
  }

  const trimmed = authorizationHeader.trim();
  if (trimmed.toLowerCase().startsWith('bearer ')) {
    return trimmed.slice(7).trim();
  }

  return undefined;
}

// Default token for stdio MCP when AG_MCP_AUTH_TOKEN is set on the server process.
export function getDefaultAuthToken(): string | undefined {
  const fromEnv = process.env.AG_MCP_AUTH_TOKEN?.trim();
  return fromEnv || undefined;
}

// Build Authorization header for reasoning-service HTTP calls.
export function buildAuthHeaders(authToken: string | undefined): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };

  if (authToken) {
    headers.Authorization = 'Bearer ' + authToken;
  }

  return headers;
}

// User-facing MCP tools (read/query only).
export const USER_MCP_TOOLS = [
  'health_check',
  'list_playbooks',
  'get_playbook_context',
  'list_entity',
  'sample_entity',
  'plan_query',
  'execute_plan',
  'query_graph',
  'list_allowed_rows',
];

// Admin-only MCP tools (authoring + schema discovery).
export const ADMIN_MCP_TOOLS = [
  'list_sources',
  'get_adapter_guide',
  'list_bindings',
  'get_binding',
  'introspect_source',
  'sample_source',
  'suggest_bindings',
  'propose_binding',
  'test_binding',
  'save_binding',
  'propose_playbook',
  'save_playbook',
];

// Return true when the tool is available for the given role.
export function isToolAllowedForRole(toolName: string, role: McpAuthRole): boolean {
  if (role === 'admin') {
    return USER_MCP_TOOLS.includes(toolName) || ADMIN_MCP_TOOLS.includes(toolName);
  }

  return USER_MCP_TOOLS.includes(toolName);
}
