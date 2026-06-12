import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { createThinMcpServer } from './serverCore.js';

async function main() {
  const server = createThinMcpServer();
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error('[anythinggraph-thin-mcp] stdio listening (Rust reasoning layer)');
}

main().catch((error) => {
  console.error('[anythinggraph-thin-mcp] fatal:', error);
  process.exit(1);
});
