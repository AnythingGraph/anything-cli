import {
  loadConfig,
  resolveAnythingGraphHome,
  buildSourcePaths,
  buildServiceUrls,
} from "../lib/paths.js";
import { checkHealthUrl } from "../lib/health.js";
import {
  buildReasoningAuthHeaders,
  listConfiguredSources,
  introspectConfiguredSource,
  summarizeValidationResponse,
} from "../lib/reasoningApi.js";
import { getProfileFilePath } from "../lib/profileStore.js";

// Parse flags for the sources list command.
function parseSourcesOptions(args) {
  const options = {
    home: null,
    json: false,
    skipValidate: false,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--home" && args[index + 1]) {
      options.home = args[index + 1];
      index += 1;
    } else if (arg === "--json") {
      options.json = true;
    } else if (arg === "--no-validate") {
      options.skipValidate = true;
    }
  }

  return options;
}

// Ensure reasoning-service is reachable before listing sources.
async function ensureReasoningService(config) {
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
async function validateSourceEntry(reasoningBaseUrl, sourceEntry, authHeaders) {
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

// List configured sources, optionally validating each connection.
export async function runSourcesCommand(args) {
  const options = parseSourcesOptions(args);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  const config = loadConfig(homeDirectory);

  if (!config) {
    console.error("AnythingGraph is not onboarded yet. Run: anythinggraph onboard");
    process.exit(1);
  }

  config.home = homeDirectory;
  const sourcePaths = buildSourcePaths(config.sourceRoot);
  const profileFilePath = getProfileFilePath(config.sourceRoot);
  const reasoningBaseUrl = await ensureReasoningService(config);
  const authHeaders = buildReasoningAuthHeaders(sourcePaths.envPath);

  const configuredSources = await listConfiguredSources(reasoningBaseUrl, authHeaders);
  const sourceList = Array.isArray(configuredSources) ? configuredSources : [];

  if (sourceList.length === 0) {
    console.log("No sources configured.");
    console.log(`Profile: ${profileFilePath}`);
    console.log("Add one with: anythinggraph source add");
    return;
  }

  let rows = sourceList.map(function mapConfiguredSource(sourceEntry) {
    return {
      source_id: sourceEntry.source_id,
      adapter: sourceEntry.adapter,
      validated: null,
      summary: null,
    };
  });

  if (!options.skipValidate) {
    rows = [];
    for (const sourceEntry of sourceList) {
      const row = await validateSourceEntry(reasoningBaseUrl, sourceEntry, authHeaders);
      rows.push(row);
    }
  }

  if (options.json) {
    console.log(JSON.stringify(rows, null, 2));
    return;
  }

  console.log("sources:");
  console.log(`Profile: ${profileFilePath}`);
  console.log("");

  const idColumnWidth = Math.max(
    "source_id".length,
    ...rows.map(function measureId(row) {
      return row.source_id.length;
    })
  );
  const adapterColumnWidth = Math.max(
    "adapter".length,
    ...rows.map(function measureAdapter(row) {
      return row.adapter.length;
    })
  );

  if (options.skipValidate) {
    console.log(
      `${"source_id".padEnd(idColumnWidth)}  ${"adapter".padEnd(adapterColumnWidth)}`
    );
    for (const row of rows) {
      console.log(`${row.source_id.padEnd(idColumnWidth)}  ${row.adapter.padEnd(adapterColumnWidth)}`);
    }
    console.log("");
    console.log("Run without --no-validate to test each connection.");
    return;
  }

  console.log(
    `${"source_id".padEnd(idColumnWidth)}  ${"adapter".padEnd(adapterColumnWidth)}  status`
  );

  for (const row of rows) {
    const statusMarker = row.validated ? "ok" : "failed";
    const statusText = row.validated ? row.summary : row.summary;
    console.log(
      `${row.source_id.padEnd(idColumnWidth)}  ${row.adapter.padEnd(adapterColumnWidth)}  ${statusMarker} — ${statusText}`
    );
  }

  const validatedCount = rows.filter(function countValidated(row) {
    return row.validated;
  }).length;

  console.log("");
  console.log(`${validatedCount}/${rows.length} source(s) validated.`);
}
