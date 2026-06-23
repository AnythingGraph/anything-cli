import { loadConfig, resolveAnythingGraphHome } from "../lib/paths.js";
import { stopSupervisor } from "../lib/supervisor.js";
import { uninstallDaemon } from "../lib/daemon.js";

// Parse stop-specific CLI flags from argv.
function parseStopOptions(args) {
  const options = {
    home: null,
    uninstallDaemon: false,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--home" && args[index + 1]) {
      options.home = args[index + 1];
      index += 1;
    } else if (arg === "--uninstall-daemon") {
      options.uninstallDaemon = true;
    }
  }

  return options;
}

// Stop the supervised stack and optionally remove the OS daemon.
export function runStopCommand(args) {
  const options = parseStopOptions(args);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  const config = loadConfig(homeDirectory);

  if (!config) {
    console.error("AnythingGraph is not onboarded yet. Run: anythinggraph onboard");
    process.exit(1);
  }

  stopSupervisor(homeDirectory);
  console.log("Stopped AnythingGraph services.");

  if (options.uninstallDaemon) {
    const removed = uninstallDaemon();
    if (removed) {
      console.log("Removed background gateway daemon.");
    }
  }
}
