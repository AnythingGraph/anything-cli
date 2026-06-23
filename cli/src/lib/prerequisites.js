import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { isValidSourceRoot } from "./paths.js";

// Return true when a command exists on PATH.
export function commandExists(commandName) {
  const lookup = spawnSync("which", [commandName], { encoding: "utf8" });
  return lookup.status === 0;
}

// Run a shell command synchronously and return stdout.
export function runCommand(commandText, options) {
  const result = spawnSync(commandText, {
    shell: true,
    encoding: "utf8",
    stdio: options && options.silent ? "pipe" : "inherit",
    env: options && options.env ? options.env : process.env,
    cwd: options && options.cwd ? options.cwd : process.cwd(),
  });

  if (result.status !== 0) {
    const stderrText = result.stderr ? result.stderr.trim() : "";
    throw new Error(
      `Command failed (${result.status}): ${commandText}${stderrText ? `\n${stderrText}` : ""}`
    );
  }

  return result.stdout ? result.stdout.trim() : "";
}

// Install npm dependencies for a package when node_modules is missing.
export function ensureNpmDependencies(packageDirectory, label) {
  const nodeModulesPath = path.join(packageDirectory, "node_modules");
  if (fs.existsSync(nodeModulesPath)) {
    return;
  }

  console.log(`Installing npm dependencies for ${label}...`);
  runCommand(`npm install --prefix "${packageDirectory}"`, { silent: false });
}

// Check required tools and return a list of missing items.
export function checkPrerequisites() {
  const missing = [];

  if (!commandExists("node")) {
    missing.push("Node.js (https://nodejs.org/)");
  }
  if (!commandExists("npm")) {
    missing.push("npm (bundled with Node.js)");
  }
  if (!commandExists("cargo")) {
    missing.push("Rust / cargo (https://rustup.rs/)");
  }
  if (!commandExists("git")) {
    missing.push("git (https://git-scm.com/)");
  }

  return missing;
}

// Print prerequisite errors and exit when something is missing.
export function requirePrerequisites() {
  const missing = checkPrerequisites();
  if (missing.length === 0) {
    return;
  }

  console.error("Missing required tools:");
  for (const item of missing) {
    console.error(`  - ${item}`);
  }
  process.exit(1);
}

// Clone the anything-cli repository into the target directory.
export function clonePlatformRepository(targetDirectory, repositoryUrl) {
  fs.mkdirSync(path.dirname(targetDirectory), { recursive: true });
  if (fs.existsSync(targetDirectory)) {
    throw new Error(`Target directory already exists: ${targetDirectory}`);
  }

  console.log(`Cloning anything-cli from ${repositoryUrl} ...`);
  console.log(`  → ${targetDirectory}`);
  runCommand(`git clone "${repositoryUrl}" "${targetDirectory}"`);
}

// Clone anything-cli from GitHub, or pull when ~/.anythinggraph/source already exists.
export function ensureGitCheckout(targetDirectory, repositoryUrl) {
  if (fs.existsSync(targetDirectory)) {
    if (!isValidSourceRoot(targetDirectory)) {
      throw new Error(
        `${targetDirectory} exists but is not a valid anything-cli checkout.\n` +
          "Remove that directory and run `anythinggraph onboard` again."
      );
    }

    console.log(`Updating git checkout: ${targetDirectory}`);
    runCommand(`git -C "${targetDirectory}" pull --ff-only`);
    return;
  }

  clonePlatformRepository(targetDirectory, repositoryUrl);
}

// Install npm dependencies for the ag-cli MCP service.
export function installNodeServices(sourcePaths) {
  ensureNpmDependencies(sourcePaths.mcpDirectory, "ag-cli MCP");
}
