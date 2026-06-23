import net from "node:net";

// Return a promise that resolves when a TCP port accepts connections.
export function waitForPort(portNumber, timeoutMilliseconds) {
  const deadline = Date.now() + timeoutMilliseconds;

  return new Promise(function waitLoop(resolve, reject) {
    const socket = new net.Socket();

    socket.setTimeout(1000);
    socket.once("error", function handlePortError() {
      socket.destroy();
      if (Date.now() >= deadline) {
        reject(new Error(`Port ${portNumber} did not open within ${timeoutMilliseconds}ms`));
        return;
      }
      setTimeout(function retryPortCheck() {
        waitForPort(portNumber, deadline - Date.now()).then(resolve).catch(reject);
      }, 1000);
    });
    socket.once("timeout", function handlePortTimeout() {
      socket.destroy();
      if (Date.now() >= deadline) {
        reject(new Error(`Port ${portNumber} did not open within ${timeoutMilliseconds}ms`));
        return;
      }
      setTimeout(function retryPortCheck() {
        waitForPort(portNumber, deadline - Date.now()).then(resolve).catch(reject);
      }, 1000);
    });
    socket.connect(portNumber, "127.0.0.1", function handlePortOpen() {
      socket.destroy();
      resolve();
    });
  });
}

// Fetch a health URL and return true when the response is OK.
export async function checkHealthUrl(healthUrl, timeoutMilliseconds) {
  const controller = new AbortController();
  const timeoutHandle = setTimeout(function abortHealthRequest() {
    controller.abort();
  }, timeoutMilliseconds || 5000);

  try {
    const response = await fetch(healthUrl, { signal: controller.signal });
    return response.ok;
  } catch (_error) {
    return false;
  } finally {
    clearTimeout(timeoutHandle);
  }
}

// Probe ag-cli service health endpoints for doctor/status output.
export async function probeAllServices(config) {
  const ports = config.ports;
  const checks = [
    { label: "reasoning-service", url: `http://127.0.0.1:${ports.reasoning}/health` },
    { label: "mcp (HTTP)", url: `http://127.0.0.1:${ports.mcp}/mcp` },
  ];

  const results = [];
  for (const check of checks) {
    const isHealthy = await checkHealthUrl(check.url, 4000);
    results.push({ label: check.label, url: check.url, healthy: isHealthy });
  }
  return results;
}
