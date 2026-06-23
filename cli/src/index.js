import { runOnboardCommand } from "./commands/onboard.js";
import { runStartCommand } from "./commands/start.js";
import { runStopCommand } from "./commands/stop.js";
import { runStatusCommand, runDoctorCommand } from "./commands/status.js";
import { runMcpCommand } from "./commands/mcp.js";
import { runSourceAddCommand } from "./commands/sourceAdd.js";
import { runSourcesCommand } from "./commands/sources.js";
import { CLI_VERSION } from "./lib/version.js";

// Print top-level CLI help text.
function printHelp() {
  console.log(`AnythingGraph CLI v${CLI_VERSION}`);
  console.log("");
  console.log("Usage:");
  console.log("  anythinggraph onboard [--install-daemon] [--rebuild-rust] [--yes]");
  console.log("  anythinggraph start [--foreground] [--rebuild-rust] [--home PATH]");
  console.log("  anythinggraph stop [--uninstall-daemon] [--home PATH]");
  console.log("  anythinggraph status [--home PATH]");
  console.log("  anythinggraph doctor [--home PATH]");
  console.log("  anythinggraph source add [--home PATH]");
  console.log("  anythinggraph sources [--home PATH] [--json] [--no-validate]");
  console.log("  anythinggraph mcp print-config [--target cursor|claude] [--home PATH]");
  console.log("  anythinggraph gateway install");
  console.log("");
  console.log("Quick start (OpenClaw-style):");
  console.log("  npm install -g @anythinggraph/cli@latest");
  console.log("  anythinggraph onboard --install-daemon");
  console.log("");
  console.log("Onboard always clones from https://github.com/AnythingGraph/anything-cli");
  console.log("  into ~/.anythinggraph/source (git pull on re-run).");
}

// Route argv to the matching subcommand handler.
export async function runCli(argv) {
  const args = argv.slice(2);
  const command = args[0];

  if (!command || command === "--help" || command === "-h") {
    printHelp();
    return;
  }

  if (command === "--version" || command === "-v") {
    console.log(CLI_VERSION);
    return;
  }

  const commandArgs = args.slice(1);

  if (command === "onboard") {
    await runOnboardCommand(commandArgs);
    return;
  }

  if (command === "gateway") {
    if (commandArgs[0] === "install") {
      const { loadConfig, resolveAnythingGraphHome, ensureHomeLayout } = await import("./lib/paths.js");
      const { installDaemon } = await import("./lib/daemon.js");
      const homeDirectory = resolveAnythingGraphHome(null);
      ensureHomeLayout(homeDirectory);
      const config = loadConfig(homeDirectory);
      if (!config) {
        console.error("AnythingGraph is not onboarded yet. Run: anythinggraph onboard");
        process.exit(1);
      }
      config.home = homeDirectory;
      installDaemon(config);
      return;
    }
    await runStartCommand(commandArgs);
    return;
  }

  if (command === "start") {
    await runStartCommand(commandArgs);
    return;
  }

  if (command === "stop") {
    runStopCommand(commandArgs);
    return;
  }

  if (command === "status") {
    await runStatusCommand(commandArgs);
    return;
  }

  if (command === "doctor") {
    await runDoctorCommand(commandArgs);
    return;
  }

  if (command === "sources") {
    await runSourcesCommand(commandArgs);
    return;
  }

  if (command === "source") {
    if (commandArgs[0] === "add") {
      await runSourceAddCommand(commandArgs.slice(1));
      return;
    }
    console.error("Usage: anythinggraph source add [--home PATH]");
    process.exit(1);
  }

  if (command === "mcp") {
    runMcpCommand(commandArgs);
    return;
  }

  console.error(`Unknown command: ${command}`);
  printHelp();
  process.exit(1);
}
