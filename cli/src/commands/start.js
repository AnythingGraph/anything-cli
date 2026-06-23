import { loadConfig, resolveAnythingGraphHome } from "../lib/paths.js";
import { startSupervisor } from "../lib/supervisor.js";

// Parse start-specific CLI flags from argv.
function parseStartOptions(args) {
  const options = {
    foreground: true,
    rebuildRust: false,
    home: null,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--foreground" || arg === "-f") {
      options.foreground = true;
    } else if (arg === "--rebuild-rust") {
      options.rebuildRust = true;
    } else if (arg === "--home" && args[index + 1]) {
      options.home = args[index + 1];
      index += 1;
    }
  }

  return options;
}

// Load config and start the full local stack.
export async function runStartCommand(args) {
  const options = parseStartOptions(args);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  const config = loadConfig(homeDirectory);

  if (!config) {
    console.error("AnythingGraph is not onboarded yet. Run: anythinggraph onboard");
    process.exit(1);
  }

  config.home = homeDirectory;
  await startSupervisor(config, { foreground: options.foreground });
}
