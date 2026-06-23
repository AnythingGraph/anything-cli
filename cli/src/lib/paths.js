import fs from "node:fs";
import os from "node:os";
import path from "node:path";

export const DEFAULT_PORTS = {
  reasoning: 8787,
  mcp: 3334,
};

export const DEFAULT_GITHUB_REPO = "https://github.com/AnythingGraph/anything-cli.git";

// Resolve the AnythingGraph home directory (~/.anythinggraph by default).
export function resolveAnythingGraphHome(overrideHome) {
  if (overrideHome && overrideHome.trim()) {
    return path.resolve(overrideHome.trim());
  }
  if (process.env.ANYTHINGGRAPH_HOME && process.env.ANYTHINGGRAPH_HOME.trim()) {
    return path.resolve(process.env.ANYTHINGGRAPH_HOME.trim());
  }
  return path.join(os.homedir(), ".anythinggraph");
}

// Ensure standard subdirectories exist under the home folder.
export function ensureHomeLayout(homeDirectory) {
  const directories = [
    homeDirectory,
    path.join(homeDirectory, "bin"),
    path.join(homeDirectory, "run"),
    path.join(homeDirectory, "logs"),
    path.join(homeDirectory, "data"),
  ];

  for (const directoryPath of directories) {
    fs.mkdirSync(directoryPath, { recursive: true });
  }
}

// Return the default config file path for a home directory.
export function getConfigFilePath(homeDirectory) {
  return path.join(homeDirectory, "config.json");
}

// Load saved CLI configuration or return null when not onboarded yet.
export function loadConfig(homeDirectory) {
  const configFilePath = getConfigFilePath(homeDirectory);
  if (!fs.existsSync(configFilePath)) {
    return null;
  }

  const rawText = fs.readFileSync(configFilePath, "utf8");
  const config = JSON.parse(rawText);

  // Migrate configs saved before the ag-cli-only CLI rewrite.
  if (!config.ports || config.ports.reasoning === undefined) {
    config.ports = { ...DEFAULT_PORTS };
  }

  return config;
}

// Persist CLI configuration to disk.
export function saveConfig(homeDirectory, config) {
  ensureHomeLayout(homeDirectory);
  const configFilePath = getConfigFilePath(homeDirectory);
  fs.writeFileSync(configFilePath, JSON.stringify(config, null, 2) + "\n", "utf8");
}

// Build service directory paths from the anything-cli checkout root.
export function buildSourcePaths(sourceRoot) {
  return {
    sourceRoot: path.resolve(sourceRoot),
    cargoToml: path.join(sourceRoot, "Cargo.toml"),
    mcpDirectory: path.join(sourceRoot, "mcp"),
    reasoningServiceDirectory: path.join(sourceRoot, "reasoning-service"),
    startAllScript: path.join(sourceRoot, "start-all.sh"),
    envExamplePath: path.join(sourceRoot, ".env.example"),
    envPath: path.join(sourceRoot, ".env"),
    playbooksDirectory: path.join(sourceRoot, "playbooks"),
    bindingsDirectory: path.join(sourceRoot, "bindings"),
    profilesDirectory: path.join(sourceRoot, "profiles"),
  };
}

// Validate that a directory looks like an anything-cli (ag-cli) checkout.
export function isValidSourceRoot(candidatePath) {
  if (!candidatePath || !fs.existsSync(candidatePath)) {
    return false;
  }

  const paths = buildSourcePaths(candidatePath);
  return (
    fs.existsSync(paths.cargoToml) &&
    fs.existsSync(paths.mcpDirectory) &&
    fs.existsSync(paths.reasoningServiceDirectory) &&
    fs.existsSync(paths.startAllScript)
  );
}

// Load key=value pairs from a .env file into process.env when not already set.
export function loadDotEnvFile(envFilePath) {
  if (!envFilePath || !fs.existsSync(envFilePath)) {
    return;
  }

  const rawText = fs.readFileSync(envFilePath, "utf8");
  for (const line of rawText.split("\n")) {
    const trimmedLine = line.trim();
    if (!trimmedLine || trimmedLine.startsWith("#")) {
      continue;
    }

    const equalsIndex = trimmedLine.indexOf("=");
    if (equalsIndex === -1) {
      continue;
    }

    const key = trimmedLine.slice(0, equalsIndex).trim();
    let value = trimmedLine.slice(equalsIndex + 1).trim();

    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }

    if (key && process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

// Build environment variables passed to reasoning-service and MCP.
export function buildServiceEnvironment(config) {
  const ports = config.ports || DEFAULT_PORTS;
  const sourceRoot = config.sourceRoot;

  return {
    ...process.env,
    AG_WORKSPACE_ROOT: sourceRoot,
    AG_REASONING_HOST: "127.0.0.1",
    AG_REASONING_PORT: String(ports.reasoning),
    AG_REASONING_URL: `http://127.0.0.1:${ports.reasoning}`,
    AG_MCP_HOST: "127.0.0.1",
    AG_MCP_PORT: String(ports.mcp),
    AG_AUTH_DISABLED: process.env.AG_AUTH_DISABLED || "1",
    ...(config.mcpAuthToken ? { AG_MCP_AUTH_TOKEN: config.mcpAuthToken } : {}),
  };
}

// Human-readable URLs for a running ag-cli stack.
export function buildServiceUrls(config) {
  const ports = config.ports || DEFAULT_PORTS;
  return {
    reasoning: `http://127.0.0.1:${ports.reasoning}/health`,
    mcp: `http://127.0.0.1:${ports.mcp}/mcp`,
  };
}
