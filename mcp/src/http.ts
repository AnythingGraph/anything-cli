import { randomUUID } from 'node:crypto';
import type { Request, Response } from 'express';
import { createMcpExpressApp } from '@modelcontextprotocol/sdk/server/express.js';
import { StreamableHTTPServerTransport } from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import { isInitializeRequest } from '@modelcontextprotocol/sdk/types.js';
import { createThinMcpServer } from './serverCore.js';

const transportsBySessionId = new Map<string, StreamableHTTPServerTransport>();

function getHttpHost(): string {
  return process.env.AG_MCP_HOST?.trim() || '127.0.0.1';
}

function getHttpPort(): number {
  const rawPort = process.env.AG_MCP_PORT?.trim() || '3334';
  return Number.parseInt(rawPort, 10);
}

function getHttpPath(): string {
  return process.env.AG_MCP_PATH?.trim() || '/mcp';
}

async function handleMcpPost(request: Request, response: Response): Promise<void> {
  const sessionIdHeader = request.headers['mcp-session-id'];
  const sessionId = typeof sessionIdHeader === 'string' ? sessionIdHeader : undefined;

  try {
    let transport: StreamableHTTPServerTransport | undefined;

    if (sessionId && transportsBySessionId.has(sessionId)) {
      transport = transportsBySessionId.get(sessionId);
    } else if (!sessionId && isInitializeRequest(request.body)) {
      transport = new StreamableHTTPServerTransport({
        sessionIdGenerator: () => randomUUID(),
        onsessioninitialized: (newSessionId) => {
          if (transport) {
            transportsBySessionId.set(newSessionId, transport);
          }
        },
      });

      transport.onclose = () => {
        const closedSessionId = transport?.sessionId;
        if (closedSessionId) {
          transportsBySessionId.delete(closedSessionId);
        }
      };

      const server = createThinMcpServer();
      await server.connect(transport);
      await transport.handleRequest(request, response, request.body);
      return;
    } else {
      response.status(400).json({
        jsonrpc: '2.0',
        error: { code: -32000, message: 'Bad Request: No valid session ID provided' },
        id: null,
      });
      return;
    }

    if (!transport) {
      response.status(400).json({
        jsonrpc: '2.0',
        error: { code: -32000, message: 'Bad Request: Unknown MCP session' },
        id: null,
      });
      return;
    }

    await transport.handleRequest(request, response, request.body);
  } catch (error) {
    console.error('[anythinggraph-thin-mcp-http] POST error:', error);
    if (!response.headersSent) {
      response.status(500).json({
        jsonrpc: '2.0',
        error: { code: -32603, message: 'Internal error' },
        id: null,
      });
    }
  }
}

async function main() {
  const app = createMcpExpressApp();
  const mcpPath = getHttpPath();

  app.post(mcpPath, (request, response) => {
    void handleMcpPost(request, response);
  });

  app.get(mcpPath, (_request, response) => {
    response.status(405).json({ error: 'Use POST for MCP Streamable HTTP' });
  });

  const host = getHttpHost();
  const port = getHttpPort();
  app.listen(port, host, () => {
    console.error(`[anythinggraph-thin-mcp-http] listening on http://${host}:${port}${mcpPath}`);
  });
}

main().catch((error) => {
  console.error('[anythinggraph-thin-mcp-http] fatal:', error);
  process.exit(1);
});
