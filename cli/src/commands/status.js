import { loadConfig, resolveAnythingGraphHome, buildServiceUrls } from "../lib/paths.js";
import { probeAllServices } from "../lib/health.js";
import { readSupervisorState } from "../lib/supervisor.js";
import { checkPrerequisites } from "../lib/prerequisites.js";
import { getInstalledRustBinaryPaths, getRustBinDirectory } from "../lib/rustBinaries.js";

// Parse shared CLI flags that accept --home.
function parseHomeOption(args) {
  const options = { home: null };
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--home" && args[index + 1]) {
      options.home = args[index + 1];
      index += 1;
    }
  }
  return options;
}

// Print configured URLs and health for each service.
export async function runStatusCommand(args) {
  const options = parseHomeOption(args);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  const config = loadConfig(homeDirectory);

  if (!config) {
    console.error("AnythingGraph is not onboarded yet. Run: anythinggraph onboard");
    process.exit(1);
  }

  config.home = homeDirectory;
  const supervisorState = readSupervisorState(homeDirectory);

  console.log("AnythingGraph status");
  console.log(`Home:   ${homeDirectory}`);
  console.log(`Source: ${config.sourceRoot}`);
  console.log(`Supervisor PID: ${supervisorState ? supervisorState.pid : "(not running)"}`);
  console.log("");

  const results = await probeAllServices(config);
  for (const result of results) {
    const marker = result.healthy ? "ok" : "down";
    console.log(`  [${marker}] ${result.label.padEnd(16)} ${result.url}`);
  }
}

// Run prerequisite and health checks for troubleshooting.
export async function runDoctorCommand(args) {
  const options = parseHomeOption(args);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  const config = loadConfig(homeDirectory);

  console.log("AnythingGraph doctor");
  console.log("");

  const missing = checkPrerequisites();
  if (missing.length === 0) {
    console.log("[ok] Prerequisites: node, npm, cargo, git");
  } else {
    console.log("[!!] Missing prerequisites:");
    for (const item of missing) {
      console.log(`     - ${item}`);
    }
  }

  if (!config) {
    console.log("[!!] Not onboarded — run: anythinggraph onboard");
    process.exit(1);
  }

  config.home = homeDirectory;
  console.log(`[ok] Config: ${homeDirectory}/config.json`);
  console.log(`[ok] Source: ${config.sourceRoot}`);

  const rustBinaryPaths = getInstalledRustBinaryPaths(homeDirectory);
  if (rustBinaryPaths) {
    console.log(`[ok] Rust binaries: ${getRustBinDirectory(homeDirectory)}`);
  } else {
    console.log(
      `[!!] Rust binaries missing in ${getRustBinDirectory(homeDirectory)} — run: anythinggraph onboard or anythinggraph start`
    );
  }

  const results = await probeAllServices(config);
  let healthyCount = 0;
  for (const result of results) {
    if (result.healthy) {
      healthyCount += 1;
    }
    const marker = result.healthy ? "ok" : "!!";
    console.log(`[${marker}] ${result.label}: ${result.url}`);
  }

  console.log("");
  if (healthyCount === results.length) {
    console.log("All ag-cli services look healthy.");
  } else {
    console.log("Some services are down. Try: anythinggraph start");
    console.log("Set credentials in .env (copy from .env.example) before querying Postgres or other sources.");
  }
}
