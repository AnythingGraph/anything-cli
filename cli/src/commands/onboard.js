import readline from "node:readline";
import path from "node:path";
import {
  DEFAULT_GITHUB_REPO,
  DEFAULT_PORTS,
  buildSourcePaths,
  ensureHomeLayout,
  loadConfig,
  saveConfig,
  buildServiceUrls,
  resolveAnythingGraphHome,
} from "../lib/paths.js";
import {
  checkPrerequisites,
  ensureGitCheckout,
  installNodeServices,
} from "../lib/prerequisites.js";
import { installDaemon } from "../lib/daemon.js";
import { ensureRustBinaries } from "../lib/rustBinaries.js";

// Parse onboard-specific CLI flags from argv.
function parseOnboardOptions(args) {
  const options = {
    installDaemon: false,
    rebuildRust: false,
    yes: false,
    home: null,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--install-daemon") {
      options.installDaemon = true;
    } else if (arg === "--rebuild-rust") {
      options.rebuildRust = true;
    } else if (arg === "--yes" || arg === "-y") {
      options.yes = true;
    } else if (arg === "--home" && args[index + 1]) {
      options.home = args[index + 1];
      index += 1;
    } else if (arg === "--source" || arg === "--clone") {
      console.warn(`Warning: ${arg} is ignored — onboard always clones from ${DEFAULT_GITHUB_REPO}`);
      if (arg === "--source" && args[index + 1] && !args[index + 1].startsWith("-")) {
        index += 1;
      }
    }
  }

  return options;
}

// Ask a yes/no question in the terminal.
function askYesNo(questionText, defaultYes) {
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  const suffix = defaultYes ? " [Y/n] " : " [y/N] ";

  return new Promise(function resolveAnswer(resolve) {
    rl.question(`${questionText}${suffix}`, function handleAnswer(answerText) {
      rl.close();
      const normalized = answerText.trim().toLowerCase();
      if (!normalized) {
        resolve(defaultYes);
        return;
      }
      resolve(normalized === "y" || normalized === "yes");
    });
  });
}

// Resolve the git checkout path used for onboarding (~/.anythinggraph/source).
function resolveSourceRoot(homeDirectory) {
  const cloneDirectory = path.join(homeDirectory, "source");
  ensureGitCheckout(cloneDirectory, DEFAULT_GITHUB_REPO);
  return cloneDirectory;
}

// Run the onboarding wizard and write ~/.anythinggraph/config.json.
export async function runOnboardCommand(args) {
  const options = parseOnboardOptions(args);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  ensureHomeLayout(homeDirectory);

  console.log("AnythingGraph onboard");
  console.log(`Home: ${homeDirectory}`);
  console.log(`Git:  ${DEFAULT_GITHUB_REPO}`);
  console.log("");

  const missing = checkPrerequisites();
  if (missing.length > 0) {
    console.error("Missing required tools:");
    for (const item of missing) {
      console.error(`  - ${item}`);
    }
    process.exit(1);
  }

  const sourceRoot = resolveSourceRoot(homeDirectory);
  const sourcePaths = buildSourcePaths(sourceRoot);

  console.log("");
  console.log("Installing ag-cli MCP dependencies (first run may take a few minutes)...");
  installNodeServices(sourcePaths);

  const config = {
    version: 1,
    home: homeDirectory,
    sourceRoot,
    ports: { ...DEFAULT_PORTS },
    mcpAuthToken: null,
    installedAt: new Date().toISOString(),
  };

  saveConfig(homeDirectory, config);
  config.home = homeDirectory;

  console.log("");
  console.log("Building reasoning-service binary...");
  await ensureRustBinaries(config, { rebuildRust: options.rebuildRust });

  const urls = buildServiceUrls(config);
  console.log("");
  console.log("Onboarding complete.");
  console.log(`  Config:  ${path.join(homeDirectory, "config.json")}`);
  console.log(`  Source:  ${sourceRoot}`);
  console.log("");
  console.log("Next steps:");
  console.log(`  cp ${path.join(sourceRoot, ".env.example")} ${path.join(sourceRoot, ".env")}`);
  console.log("  # edit .env — set AG_SQL_DSN and other credentials");
  console.log("  anythinggraph start              # run ag-cli in this terminal");
  console.log("  anythinggraph start --foreground # same, explicit foreground mode");
  if (!options.installDaemon) {
    console.log("  anythinggraph onboard --install-daemon");
  }
  console.log("  anythinggraph doctor");
  console.log("  anythinggraph mcp print-config");
  console.log("");
  console.log("When running, connect your agent to:");
  console.log(`  MCP endpoint: ${urls.mcp}`);

  if (options.installDaemon) {
    console.log("");
    installDaemon(config);
    console.log("Background gateway daemon installed.");
  } else if (options.yes) {
    console.log("");
    console.log("Tip: run `anythinggraph onboard --install-daemon` to keep the stack running in the background.");
  } else {
    const shouldInstallDaemon = await askYesNo("Install background daemon now?", false);
    if (shouldInstallDaemon) {
      installDaemon(config);
      console.log("Background gateway daemon installed.");
    }
  }
}
