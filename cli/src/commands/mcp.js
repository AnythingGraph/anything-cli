import { loadConfig, resolveAnythingGraphHome } from "../lib/paths.js";

// Build the remote MCP JSON snippet for AI hosts.
export function buildMcpConfigJson(config) {
  const mcpUrl = `http://127.0.0.1:${config.ports.mcp}/mcp`;
  if (config.mcpAuthToken) {
    return JSON.stringify(
      {
        mcpServers: {
          "anythinggraph-cli": {
            url: mcpUrl,
            headers: {
              Authorization: `Bearer ${config.mcpAuthToken}`,
            },
          },
        },
      },
      null,
      2
    );
  }

  return JSON.stringify(
    {
      mcpServers: {
        "anythinggraph-cli": {
          url: mcpUrl,
        },
      },
    },
    null,
    2
  );
}

// Build Claude Desktop mcp-remote bridge config for local HTTP MCP.
export function buildClaudeBridgeConfigJson(config) {
  const mcpUrl = `http://127.0.0.1:${config.ports.mcp}/mcp`;
  const args = ["-y", "mcp-remote", mcpUrl, "--transport", "http-only"];
  if (config.mcpAuthToken) {
    args.push("--header", `Authorization: Bearer ${config.mcpAuthToken}`);
  }

  return JSON.stringify(
    {
      mcpServers: {
        "anythinggraph-cli": {
          command: "npx",
          args,
        },
      },
    },
    null,
    2
  );
}

// Parse mcp subcommand flags from argv.
function parseMcpOptions(args) {
  const options = {
    home: null,
    target: "cursor",
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--home" && args[index + 1]) {
      options.home = args[index + 1];
      index += 1;
    } else if (arg === "--target" && args[index + 1]) {
      options.target = args[index + 1];
      index += 1;
    }
  }

  return options;
}

// Print MCP configuration snippets for Cursor or Claude Desktop.
export function runMcpCommand(args) {
  const subcommand = args[0];
  const restArgs = args.slice(1);
  const options = parseMcpOptions(restArgs);
  const homeDirectory = resolveAnythingGraphHome(options.home);
  const config = loadConfig(homeDirectory);

  if (!config) {
    console.error("AnythingGraph is not onboarded yet. Run: anythinggraph onboard");
    process.exit(1);
  }

  if (subcommand !== "print-config") {
    console.log("Usage:");
    console.log("  anythinggraph mcp print-config [--target cursor|claude]");
    process.exit(1);
  }

  if (options.target === "claude") {
    console.log(buildClaudeBridgeConfigJson(config));
    return;
  }

  console.log(buildMcpConfigJson(config));
}
