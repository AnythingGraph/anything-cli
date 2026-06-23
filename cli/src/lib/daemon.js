import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

export const DAEMON_LABEL = "com.anythinggraph.gateway";

// Return the platform identifier used for daemon installation.
export function getPlatformKind() {
  if (process.platform === "darwin") {
    return "macos";
  }
  if (process.platform === "linux") {
    return "linux";
  }
  return "unsupported";
}

// Resolve the path to the globally linked anythinggraph binary when possible.
export function resolveAnythingGraphBinaryPath() {
  const whichResult = spawnSync("which", ["anythinggraph"], { encoding: "utf8" });
  if (whichResult.status === 0 && whichResult.stdout.trim()) {
    return whichResult.stdout.trim();
  }
  return process.argv[1];
}

// Build launchd ProgramArguments for the gateway daemon.
export function buildDaemonStartCommand(config) {
  const binaryPath = resolveAnythingGraphBinaryPath();
  return {
    binaryPath,
    args: ["start", "--foreground", "--home", config.home],
  };
}

// Install a launchd user agent on macOS.
export function installMacOsDaemon(config) {
  const launchAgentsDirectory = path.join(os.homedir(), "Library", "LaunchAgents");
  fs.mkdirSync(launchAgentsDirectory, { recursive: true });

  const plistPath = path.join(launchAgentsDirectory, `${DAEMON_LABEL}.plist`);
  const logDirectory = path.join(config.home, "logs");
  fs.mkdirSync(logDirectory, { recursive: true });

  const startCommand = buildDaemonStartCommand(config);
  const plistXml = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${DAEMON_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>${startCommand.binaryPath}</string>
    <string>start</string>
    <string>--foreground</string>
    <string>--home</string>
    <string>${config.home}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>${path.join(logDirectory, "gateway.stdout.log")}</string>
  <key>StandardErrorPath</key>
  <string>${path.join(logDirectory, "gateway.stderr.log")}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>${process.env.PATH || ""}</string>
  </dict>
</dict>
</plist>
`;

  fs.writeFileSync(plistPath, plistXml, "utf8");

  spawnSync("launchctl", ["bootout", `gui/${process.getuid()}`, plistPath], { stdio: "ignore" });
  const loadResult = spawnSync("launchctl", ["bootstrap", `gui/${process.getuid()}`, plistPath], {
    encoding: "utf8",
  });

  if (loadResult.status !== 0) {
    throw new Error(loadResult.stderr || "launchctl bootstrap failed");
  }

  return plistPath;
}

// Unload the macOS launchd user agent.
export function uninstallMacOsDaemon() {
  const plistPath = path.join(os.homedir(), "Library", "LaunchAgents", `${DAEMON_LABEL}.plist`);
  if (!fs.existsSync(plistPath)) {
    return false;
  }

  spawnSync("launchctl", ["bootout", `gui/${process.getuid()}`, plistPath], { stdio: "ignore" });
  fs.unlinkSync(plistPath);
  return true;
}

// Install a systemd user service on Linux.
export function installLinuxDaemon(config) {
  const systemdUserDirectory = path.join(os.homedir(), ".config", "systemd", "user");
  fs.mkdirSync(systemdUserDirectory, { recursive: true });

  const unitPath = path.join(systemdUserDirectory, "anythinggraph-gateway.service");
  const logDirectory = path.join(config.home, "logs");
  fs.mkdirSync(logDirectory, { recursive: true });

  const startCommand = buildDaemonStartCommand(config);
  const unitText = `[Unit]
Description=Anything CLI gateway (reasoning-service + MCP)
After=network.target

[Service]
Type=simple
ExecStart=${startCommand.binaryPath} start --foreground --home ${config.home}
Restart=always
RestartSec=3
Environment=PATH=${process.env.PATH || ""}

[Install]
WantedBy=default.target
`;

  fs.writeFileSync(unitPath, unitText, "utf8");

  spawnSync("systemctl", ["--user", "daemon-reload"], { stdio: "inherit" });
  spawnSync("systemctl", ["--user", "enable", "anythinggraph-gateway.service"], { stdio: "inherit" });
  const startResult = spawnSync("systemctl", ["--user", "restart", "anythinggraph-gateway.service"], {
    encoding: "utf8",
  });

  if (startResult.status !== 0) {
    throw new Error(startResult.stderr || "systemctl restart failed");
  }

  return unitPath;
}

// Disable and remove the Linux systemd user service.
export function uninstallLinuxDaemon() {
  const unitPath = path.join(os.homedir(), ".config", "systemd", "user", "anythinggraph-gateway.service");
  if (!fs.existsSync(unitPath)) {
    return false;
  }

  spawnSync("systemctl", ["--user", "disable", "--now", "anythinggraph-gateway.service"], { stdio: "ignore" });
  fs.unlinkSync(unitPath);
  spawnSync("systemctl", ["--user", "daemon-reload"], { stdio: "ignore" });
  return true;
}

// Install the OS-specific background daemon for the gateway.
export function installDaemon(config) {
  const platformKind = getPlatformKind();
  if (platformKind === "macos") {
    const plistPath = installMacOsDaemon(config);
    console.log(`Installed macOS LaunchAgent: ${plistPath}`);
    return;
  }
  if (platformKind === "linux") {
    const unitPath = installLinuxDaemon(config);
    console.log(`Installed systemd user service: ${unitPath}`);
    return;
  }

  throw new Error(
    "Background daemon install is supported on macOS and Linux only. On Windows, use WSL2 or run `anythinggraph start` in a terminal."
  );
}

// Remove the OS-specific background daemon.
export function uninstallDaemon() {
  const platformKind = getPlatformKind();
  if (platformKind === "macos") {
    return uninstallMacOsDaemon();
  }
  if (platformKind === "linux") {
    return uninstallLinuxDaemon();
  }
  return false;
}
