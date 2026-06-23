import fs from "node:fs";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import {
  buildServiceEnvironment,
  buildServiceUrls,
  buildSourcePaths,
  loadDotEnvFile,
} from "./paths.js";
import { ensureNpmDependencies } from "./prerequisites.js";
import { waitForPort } from "./health.js";
import { ensureRustBinaries } from "./rustBinaries.js";

// Return the PID file path used by the CLI supervisor.
export function getSupervisorPidFilePath(homeDirectory) {
  return path.join(homeDirectory, "run", "supervisor.pid");
}

// Return the JSON state file listing child process IDs.
export function getSupervisorStateFilePath(homeDirectory) {
  return path.join(homeDirectory, "run", "supervisor-state.json");
}

// Free a local TCP port by terminating listeners when possible.
export function freePort(portNumber) {
  const lookup = spawnSync("lsof", ["-ti", `:${portNumber}`], { encoding: "utf8" });
  if (lookup.status !== 0 || !lookup.stdout.trim()) {
    return;
  }

  const processIds = lookup.stdout.trim().split(/\s+/);
  for (const processId of processIds) {
    spawnSync("kill", ["-TERM", processId], { stdio: "ignore" });
  }

  spawnSync("sleep", ["1"], { shell: true, stdio: "ignore" });

  const secondLookup = spawnSync("lsof", ["-ti", `:${portNumber}`], { encoding: "utf8" });
  if (secondLookup.status === 0 && secondLookup.stdout.trim()) {
    const remainingIds = secondLookup.stdout.trim().split(/\s+/);
    for (const processId of remainingIds) {
      spawnSync("kill", ["-KILL", processId], { stdio: "ignore" });
    }
  }
}

// Spawn reasoning-service with prefixed logs.
function startRustService(config, serviceLabel, binaryPath, childRecords, logDirectory) {
  const serviceEnvironment = buildServiceEnvironment(config);
  const logFilePath = path.join(logDirectory, `${serviceLabel}.log`);

  const logStream = fs.createWriteStream(logFilePath, { flags: "a" });
  const childProcess = spawn(binaryPath, [], {
    env: serviceEnvironment,
    stdio: ["ignore", "pipe", "pipe"],
  });

  childProcess.stdout.on("data", function prefixStdout(chunk) {
    const lines = chunk.toString().split("\n").filter(Boolean);
    for (const line of lines) {
      const formattedLine = `[${serviceLabel}] ${line}\n`;
      logStream.write(formattedLine);
      process.stdout.write(formattedLine);
    }
  });
  childProcess.stderr.on("data", function prefixStderr(chunk) {
    const lines = chunk.toString().split("\n").filter(Boolean);
    for (const line of lines) {
      const formattedLine = `[${serviceLabel}] ${line}\n`;
      logStream.write(formattedLine);
      process.stderr.write(formattedLine);
    }
  });

  childRecords.push({
    label: serviceLabel,
    pid: childProcess.pid,
    command: binaryPath,
  });

  return childProcess;
}

// Spawn the ag-cli MCP HTTP service with prefixed logs.
function startMcpService(config, childRecords, logDirectory) {
  const sourcePaths = buildSourcePaths(config.sourceRoot);
  const serviceEnvironment = buildServiceEnvironment(config);
  const logFilePath = path.join(logDirectory, "mcp-http.log");
  const logStream = fs.createWriteStream(logFilePath, { flags: "a" });

  const childProcess = spawn("npm", ["run", "dev:http"], {
    cwd: sourcePaths.mcpDirectory,
    env: serviceEnvironment,
    stdio: ["ignore", "pipe", "pipe"],
  });

  childProcess.stdout.on("data", function prefixStdout(chunk) {
    const lines = chunk.toString().split("\n").filter(Boolean);
    for (const line of lines) {
      const formattedLine = `[mcp-http] ${line}\n`;
      logStream.write(formattedLine);
      process.stdout.write(formattedLine);
    }
  });
  childProcess.stderr.on("data", function prefixStderr(chunk) {
    const lines = chunk.toString().split("\n").filter(Boolean);
    for (const line of lines) {
      const formattedLine = `[mcp-http] ${line}\n`;
      logStream.write(formattedLine);
      process.stderr.write(formattedLine);
    }
  });

  childRecords.push({
    label: "mcp-http",
    pid: childProcess.pid,
    command: "npm run dev:http",
  });

  return childProcess;
}

// Write supervisor PID and child process metadata to disk.
function writeSupervisorState(homeDirectory, childRecords) {
  const pidFilePath = getSupervisorPidFilePath(homeDirectory);
  const stateFilePath = getSupervisorStateFilePath(homeDirectory);
  fs.writeFileSync(pidFilePath, String(process.pid), "utf8");
  fs.writeFileSync(stateFilePath, JSON.stringify({ pid: process.pid, children: childRecords }, null, 2), "utf8");
}

// Remove supervisor state files from disk.
function clearSupervisorState(homeDirectory) {
  const pidFilePath = getSupervisorPidFilePath(homeDirectory);
  const stateFilePath = getSupervisorStateFilePath(homeDirectory);
  if (fs.existsSync(pidFilePath)) {
    fs.unlinkSync(pidFilePath);
  }
  if (fs.existsSync(stateFilePath)) {
    fs.unlinkSync(stateFilePath);
  }
}

// Read the saved supervisor state when present.
export function readSupervisorState(homeDirectory) {
  const stateFilePath = getSupervisorStateFilePath(homeDirectory);
  if (!fs.existsSync(stateFilePath)) {
    return null;
  }
  return JSON.parse(fs.readFileSync(stateFilePath, "utf8"));
}

// Stop all processes recorded by a previous supervisor run.
export function stopSupervisor(homeDirectory) {
  const pidFilePath = getSupervisorPidFilePath(homeDirectory);
  const state = readSupervisorState(homeDirectory);

  if (state && Array.isArray(state.children)) {
    for (const childRecord of state.children) {
      if (!childRecord.pid) {
        continue;
      }
      spawnSync("kill", ["-TERM", String(childRecord.pid)], { stdio: "ignore" });
    }
  }

  if (fs.existsSync(pidFilePath)) {
    const supervisorPid = Number(fs.readFileSync(pidFilePath, "utf8").trim());
    if (supervisorPid) {
      spawnSync("kill", ["-TERM", String(supervisorPid)], { stdio: "ignore" });
    }
  }

  clearSupervisorState(homeDirectory);
}

// Start the ag-cli stack (reasoning-service + MCP) and optionally keep the supervisor alive.
export async function startSupervisor(config, options) {
  const sourcePaths = buildSourcePaths(config.sourceRoot);
  const logDirectory = path.join(config.home, "logs");
  fs.mkdirSync(logDirectory, { recursive: true });

  loadDotEnvFile(sourcePaths.envPath);

  stopSupervisor(config.home);

  const ports = config.ports;
  for (const portNumber of [ports.reasoning, ports.mcp]) {
    freePort(portNumber);
  }

  ensureNpmDependencies(sourcePaths.mcpDirectory, "ag-cli MCP");

  const rustBinaries = await ensureRustBinaries(config, options);

  const childRecords = [];
  const childProcesses = [];

  console.log("Starting reasoning-service...");
  childProcesses.push(
    startRustService(
      config,
      "reasoning-service",
      rustBinaries.reasoning,
      childRecords,
      logDirectory
    )
  );

  await waitForPort(ports.reasoning, 180000);

  console.log("Starting MCP HTTP...");
  childProcesses.push(startMcpService(config, childRecords, logDirectory));

  await waitForPort(ports.mcp, 120000);

  writeSupervisorState(config.home, childRecords);

  const urls = buildServiceUrls(config);
  console.log("");
  console.log("Anything CLI (ag-cli) is running:");
  console.log(`  reasoning-service: ${urls.reasoning}`);
  console.log(`  MCP endpoint:      ${urls.mcp}`);
  console.log("");

  if (!fs.existsSync(sourcePaths.envPath) && fs.existsSync(sourcePaths.envExamplePath)) {
    console.log("Tip: copy .env.example to .env in your checkout and set AG_SQL_DSN for Postgres demos.");
    console.log("");
  }

  if (options && options.foreground) {
    console.log("Press Ctrl+C to stop all services.");
    console.log("");

    function shutdownAllServices() {
      for (const childProcess of childProcesses) {
        if (childProcess.pid) {
          spawnSync("kill", ["-TERM", String(childProcess.pid)], { stdio: "ignore" });
        }
      }
      clearSupervisorState(config.home);
      process.exit(0);
    }

    process.on("SIGINT", shutdownAllServices);
    process.on("SIGTERM", shutdownAllServices);

    await new Promise(function keepSupervisorAlive() {
      /* keep process running until signal */
    });
  }
}
