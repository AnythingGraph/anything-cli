import {
  loadConfig,
  resolveAnythingGraphHome,
  buildSourcePaths,
} from "../lib/paths.js";
import { getProfileFilePath } from "../lib/profileStore.js";
import {
  fetchValidatedSourceRows,
} from "../lib/sourceRows.js";

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
  const profileFilePath = getProfileFilePath(config.sourceRoot);
  const { rows } = await fetchValidatedSourceRows(config, {
    skipValidate: options.skipValidate,
  });

  if (rows.length === 0) {
    console.log("No sources configured.");
    console.log(`Profile: ${profileFilePath}`);
    console.log("Add one with: anythinggraph source add");
    return;
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
    console.log(
      `${row.source_id.padEnd(idColumnWidth)}  ${row.adapter.padEnd(adapterColumnWidth)}  ${statusMarker} — ${row.summary}`
    );
  }

  const validatedCount = rows.filter(function countValidated(row) {
    return row.validated;
  }).length;

  console.log("");
  console.log(`${validatedCount}/${rows.length} source(s) validated.`);
}
