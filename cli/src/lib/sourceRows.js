import {
  buildServiceUrls,
  buildSourcePaths,
} from "./paths.js";
import { checkHealthUrl } from "./health.js";
import {
  buildReasoningAuthHeaders,
  listConfiguredSources,
  introspectConfiguredSource,
  summarizeValidationResponse,
} from "./reasoningApi.js";

// Ensure reasoning-service is reachable before listing or validating sources.
export async function ensureReasoningService(config) {
  const urls = buildServiceUrls(config);
  const reasoningHealthUrl = urls.reasoning;
  const isHealthy = await checkHealthUrl(reasoningHealthUrl, 4000);
  if (isHealthy) {
    return reasoningHealthUrl.replace(/\/health$/, "");
  }

  console.error("");
  console.error("Reasoning service is not running.");
  console.error("Start the stack in another terminal: anythinggraph start");
  process.exit(1);
}

// Validate one configured source and return a summary row.
export async function validateSourceEntry(reasoningBaseUrl, sourceEntry, authHeaders) {
  try {
    const introspection = await introspectConfiguredSource(
      reasoningBaseUrl,
      sourceEntry.source_id,
      authHeaders
    );
    return {
      source_id: sourceEntry.source_id,
      adapter: sourceEntry.adapter,
      validated: true,
      summary: summarizeValidationResponse(introspection),
    };
  } catch (validationError) {
    return {
      source_id: sourceEntry.source_id,
      adapter: sourceEntry.adapter,
      validated: false,
      summary: validationError.message,
    };
  }
}

// Fetch configured sources and validate each connection.
export async function fetchValidatedSourceRows(config, options) {
  const sourcePaths = buildSourcePaths(config.sourceRoot);
  const reasoningBaseUrl = await ensureReasoningService(config);
  const authHeaders = buildReasoningAuthHeaders(sourcePaths.envPath);
  const configuredSources = await listConfiguredSources(reasoningBaseUrl, authHeaders);
  const sourceList = Array.isArray(configuredSources) ? configuredSources : [];

  if (sourceList.length === 0) {
    return { reasoningBaseUrl, authHeaders, rows: [] };
  }

  if (options && options.skipValidate) {
    const rows = sourceList.map(function mapSource(sourceEntry) {
      return {
        source_id: sourceEntry.source_id,
        adapter: sourceEntry.adapter,
        validated: null,
        summary: null,
      };
    });
    return { reasoningBaseUrl, authHeaders, rows };
  }

  const rows = [];
  for (const sourceEntry of sourceList) {
    const row = await validateSourceEntry(reasoningBaseUrl, sourceEntry, authHeaders);
    rows.push(row);
  }

  return { reasoningBaseUrl, authHeaders, rows };
}

// Format one source row for numbered menus.
export function formatSourceRowLabel(row, menuNumber) {
  if (row.validated === null) {
    return `  ${menuNumber}) ${row.source_id} (${row.adapter})`;
  }

  const statusMarker = row.validated ? "ok" : "failed";
  return `  ${menuNumber}) ${row.source_id} (${row.adapter}) — ${statusMarker} — ${row.summary}`;
}
