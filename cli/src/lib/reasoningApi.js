import { loadDotEnvFile } from "./paths.js";

// Build Authorization header when bearer tokens are enabled.
export function buildReasoningAuthHeaders(envFilePath) {
  if (envFilePath) {
    loadDotEnvFile(envFilePath);
  }

  const authDisabled = (process.env.AG_AUTH_DISABLED || "").trim().toLowerCase();
  if (authDisabled === "1" || authDisabled === "true" || authDisabled === "yes") {
    return {};
  }

  const adminTokens = process.env.AG_ADMIN_TOKENS || "";
  const firstToken = adminTokens.split(",")[0]?.trim();
  if (!firstToken) {
    return {};
  }

  return {
    Authorization: `Bearer ${firstToken}`,
  };
}

// Parse JSON response body or throw when the reasoning API returns an error.
async function parseReasoningResponse(response, routePath) {
  const responseText = await response.text();
  let responseBody = null;
  if (responseText) {
    try {
      responseBody = JSON.parse(responseText);
    } catch (_parseError) {
      responseBody = { raw: responseText };
    }
  }

  if (!response.ok) {
    if (response.status === 404) {
      throw new Error(
        `Reasoning API endpoint not found (${routePath}). Rebuild and restart: anythinggraph start --rebuild-rust`
      );
    }

    const message =
      (responseBody && (responseBody.error || responseBody.message)) ||
      responseText ||
      `HTTP ${response.status}`;
    throw new Error(typeof message === "string" ? message : JSON.stringify(message));
  }

  return responseBody;
}

// GET JSON from the reasoning API and return parsed body or throw with message.
export async function getReasoningJson(reasoningBaseUrl, routePath, headers) {
  const response = await fetch(`${reasoningBaseUrl}${routePath}`, {
    method: "GET",
    headers: {
      ...headers,
    },
  });

  return parseReasoningResponse(response, routePath);
}

// POST JSON to the reasoning API and return parsed body or throw with message.
export async function postReasoningJson(reasoningBaseUrl, routePath, body, headers) {
  const response = await fetch(`${reasoningBaseUrl}${routePath}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      ...headers,
    },
    body: JSON.stringify(body),
  });

  return parseReasoningResponse(response, routePath);
}

// Validate a connection before saving profile + .env entries.
export async function validateSourceConnection(reasoningBaseUrl, connectionPayload, headers) {
  return postReasoningJson(reasoningBaseUrl, "/sources/validate", connectionPayload, headers);
}

// Reload playbooks, bindings, and profile after profile changes.
export async function reloadReasoningCatalog(reasoningBaseUrl, headers) {
  return postReasoningJson(reasoningBaseUrl, "/catalog/reload", {}, headers);
}

// List configured profile sources from the reasoning API (no secrets).
export async function listConfiguredSources(reasoningBaseUrl, headers) {
  return getReasoningJson(reasoningBaseUrl, "/sources", headers);
}

// Introspect one configured source to confirm connectivity and schema.
export async function introspectConfiguredSource(reasoningBaseUrl, sourceId, headers) {
  return postReasoningJson(reasoningBaseUrl, `/sources/${encodeURIComponent(sourceId)}/introspect`, {}, headers);
}

// Summarize introspection response for CLI output.
export function summarizeValidationResponse(responseBody) {
  if (!responseBody || !responseBody.schema) {
    return "Connected successfully.";
  }

  const schema = responseBody.schema;
  if (Array.isArray(schema.tables)) {
    return `Found ${schema.tables.length} table(s).`;
  }
  if (Array.isArray(schema.objects)) {
    return `Found ${schema.objects.length} Salesforce object(s).`;
  }
  if (Array.isArray(schema.collections)) {
    return `Found ${schema.collections.length} collection(s).`;
  }
  if (Array.isArray(schema.columns)) {
    return `Found ${schema.columns.length} CSV column(s).`;
  }
  if (Array.isArray(schema.endpoints)) {
    return `Found ${schema.endpoints.length} REST endpoint(s).`;
  }

  return "Connected successfully.";
}
