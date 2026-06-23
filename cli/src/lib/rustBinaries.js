import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { buildSourcePaths } from "./paths.js";
import { commandExists, runCommand } from "./prerequisites.js";

// Rust services started by the ag-cli supervisor.
export const RUST_AG_CLI_SERVICES = [
  { label: "reasoning-service", packageName: "reasoning-service", pathKey: "reasoning" },
];

// Return the directory where installed Rust binaries are stored.
export function getRustBinDirectory(homeDirectory) {
  return path.join(homeDirectory, "bin");
}

// Return the executable file name for a Rust crate on this OS.
export function getRustBinaryFileName(packageName) {
  if (process.platform === "win32") {
    return `${packageName}.exe`;
  }
  return packageName;
}

// Return true when a path exists and is executable (or a .exe on Windows).
function isExecutableBinary(binaryPath) {
  if (!fs.existsSync(binaryPath)) {
    return false;
  }

  if (process.platform === "win32") {
    return true;
  }

  try {
    fs.accessSync(binaryPath, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

// Return the cargo release output path for one workspace package.
export function getCargoReleaseBinaryPath(workspaceRoot, packageName) {
  return path.join(workspaceRoot, "target", "release", getRustBinaryFileName(packageName));
}

// Return installed binary paths keyed by service pathKey when all are present.
export function getInstalledRustBinaryPaths(homeDirectory) {
  const binDirectory = getRustBinDirectory(homeDirectory);
  const binaryPaths = {};

  for (const service of RUST_AG_CLI_SERVICES) {
    const binaryPath = path.join(binDirectory, getRustBinaryFileName(service.packageName));
    if (!isExecutableBinary(binaryPath)) {
      return null;
    }
    binaryPaths[service.pathKey] = binaryPath;
  }

  return binaryPaths;
}

// Copy one built binary into ~/.anythinggraph/bin and mark it executable.
function installBinaryToHome(homeDirectory, sourceBinaryPath, packageName) {
  const binDirectory = getRustBinDirectory(homeDirectory);
  fs.mkdirSync(binDirectory, { recursive: true });

  const destinationPath = path.join(binDirectory, getRustBinaryFileName(packageName));
  fs.copyFileSync(sourceBinaryPath, destinationPath);

  if (process.platform !== "win32") {
    fs.chmodSync(destinationPath, 0o755);
  }

  return destinationPath;
}

// Install reasoning-service from workspace target/release into home bin.
function installBuiltBinariesFromWorkspace(homeDirectory, workspaceRoot) {
  const binaryPaths = {};

  for (const service of RUST_AG_CLI_SERVICES) {
    const builtPath = getCargoReleaseBinaryPath(workspaceRoot, service.packageName);
    if (!fs.existsSync(builtPath)) {
      throw new Error(
        `Expected release binary was not built: ${builtPath}\n` +
          "Try: anythinggraph start --rebuild-rust"
      );
    }

    binaryPaths[service.pathKey] = installBinaryToHome(
      homeDirectory,
      builtPath,
      service.packageName
    );
  }

  return binaryPaths;
}

// Remove installed Rust binaries from the home bin directory.
export function removeInstalledRustBinaries(homeDirectory) {
  const binDirectory = getRustBinDirectory(homeDirectory);
  for (const service of RUST_AG_CLI_SERVICES) {
    const binaryPath = path.join(binDirectory, getRustBinaryFileName(service.packageName));
    if (fs.existsSync(binaryPath)) {
      fs.unlinkSync(binaryPath);
    }
  }
}

// Compile reasoning-service from the anything-cli checkout.
export function buildRustBinariesFromSource(workspaceRoot) {
  if (!commandExists("cargo")) {
    throw new Error(
      "Rust / cargo is required to build reasoning-service. Install from https://rustup.rs/"
    );
  }

  console.log("Building reasoning-service (first run may take several minutes)...");
  console.log("  cargo build --release -p reasoning-service");
  runCommand("cargo build --release -p reasoning-service", { cwd: workspaceRoot });
}

// Pull latest anything-cli source before a forced rebuild.
function pullSourceCheckoutBeforeRebuild(sourceRoot) {
  const gitDirectory = path.join(sourceRoot, ".git");
  if (!fs.existsSync(gitDirectory)) {
    return;
  }

  console.log(`Updating git checkout: ${sourceRoot}`);
  runCommand(`git -C "${sourceRoot}" pull --ff-only`);
}

// Ensure reasoning-service exists in ~/.anythinggraph/bin — reuse or build from source.
export async function ensureRustBinaries(config, options) {
  const homeDirectory = config.home;
  const sourcePaths = buildSourcePaths(config.sourceRoot);
  const rebuildRequested = Boolean(options && options.rebuildRust);

  fs.mkdirSync(getRustBinDirectory(homeDirectory), { recursive: true });

  if (rebuildRequested) {
    removeInstalledRustBinaries(homeDirectory);
    pullSourceCheckoutBeforeRebuild(sourcePaths.sourceRoot);
  } else {
    const existingPaths = getInstalledRustBinaryPaths(homeDirectory);
    if (existingPaths) {
      return existingPaths;
    }
  }

  buildRustBinariesFromSource(sourcePaths.sourceRoot);
  return installBuiltBinariesFromWorkspace(homeDirectory, sourcePaths.sourceRoot);
}
