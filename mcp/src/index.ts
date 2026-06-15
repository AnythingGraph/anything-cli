import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { getDefaultAuthToken, isAuthRequired, resolveRoleFromToken } from './auth.js';
import { createThinMcpServer } from './serverCore.js';

async function main() {
  const authToken = getDefaultAuthToken();
  const role = resolveRoleFromToken(authToken);

  if (isAuthRequired() && !role) {
    console.error(
      '[anythinggraph-thin-mcp] missing valid AG_MCP_AUTH_TOKEN — token must match AG_ADMIN_TOKENS or AG_USER_TOKENS',
    );
    process.exit(1);
  }

  const server = createThinMcpServer({
    role: role || 'admin',
    authToken,
  });
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error(`[anythinggraph-thin-mcp] stdio listening (role=${role || 'admin'})`);
}

main().catch((error) => {
  console.error('[anythinggraph-thin-mcp] fatal:', error);
  process.exit(1);
});
