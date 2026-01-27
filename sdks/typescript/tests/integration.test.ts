import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { type ChildProcess } from "node:child_process";
import { SandboxAgent } from "../src/client.ts";
import { spawnSandboxAgent, isNodeRuntime } from "../src/spawn.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Check for binary in common locations
function findBinary(): string | null {
  if (process.env.SANDBOX_AGENT_BIN) {
    return process.env.SANDBOX_AGENT_BIN;
  }

  // Check cargo build output (run from sdks/typescript/tests)
  const cargoPaths = [
    resolve(__dirname, "../../../target/debug/sandbox-agent"),
    resolve(__dirname, "../../../target/release/sandbox-agent"),
  ];

  for (const p of cargoPaths) {
    if (existsSync(p)) {
      return p;
    }
  }

  return null;
}

const BINARY_PATH = findBinary();
const SKIP_INTEGRATION = !BINARY_PATH && !process.env.RUN_INTEGRATION_TESTS;

// Set env var if we found a binary
if (BINARY_PATH && !process.env.SANDBOX_AGENT_BIN) {
  process.env.SANDBOX_AGENT_BIN = BINARY_PATH;
}

describe.skipIf(SKIP_INTEGRATION)("Integration: spawn (local mode)", () => {
  it("spawns server and connects", async () => {
    const handle = await spawnSandboxAgent({
      enabled: true,
      log: "silent",
      timeoutMs: 30000,
    });

    try {
      expect(handle.baseUrl).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
      expect(handle.token).toBeTruthy();

      const client = await SandboxAgent.connect({
        baseUrl: handle.baseUrl,
        token: handle.token,
      });

      const health = await client.getHealth();
      expect(health.status).toBe("ok");
    } finally {
      await handle.dispose();
    }
  });

  it("SandboxAgent.start spawns automatically", async () => {
    const client = await SandboxAgent.start({
      spawn: { log: "silent", timeoutMs: 30000 },
    });

    try {
      const health = await client.getHealth();
      expect(health.status).toBe("ok");

      const agents = await client.listAgents();
      expect(agents.agents).toBeDefined();
      expect(Array.isArray(agents.agents)).toBe(true);
    } finally {
      await client.dispose();
    }
  });

  it("lists available agents", async () => {
    const client = await SandboxAgent.start({
      spawn: { log: "silent", timeoutMs: 30000 },
    });

    try {
      const agents = await client.listAgents();
      expect(agents.agents).toBeDefined();
      // Should have at least some agents defined
      expect(agents.agents.length).toBeGreaterThan(0);
    } finally {
      await client.dispose();
    }
  });
});

describe.skipIf(SKIP_INTEGRATION)("Integration: connect (remote mode)", () => {
  let serverProcess: ChildProcess;
  let baseUrl: string;
  let token: string;

  beforeAll(async () => {
    // Start server manually to simulate remote server
    const handle = await spawnSandboxAgent({
      enabled: true,
      log: "silent",
      timeoutMs: 30000,
    });
    serverProcess = handle.child;
    baseUrl = handle.baseUrl;
    token = handle.token;
  });

  afterAll(async () => {
    if (serverProcess && serverProcess.exitCode === null) {
      serverProcess.kill("SIGTERM");
      await new Promise<void>((resolve) => {
        const timeout = setTimeout(() => {
          serverProcess.kill("SIGKILL");
          resolve();
        }, 5000);
        serverProcess.once("exit", () => {
          clearTimeout(timeout);
          resolve();
        });
      });
    }
  });

  it("connects to remote server", async () => {
    const client = await SandboxAgent.connect({
      baseUrl,
      token,
    });

    const health = await client.getHealth();
    expect(health.status).toBe("ok");
  });

  it("handles authentication", async () => {
    const client = await SandboxAgent.connect({
      baseUrl,
      token,
    });

    const health = await client.getHealth();
    expect(health.status).toBe("ok");
  });

  it("rejects invalid token on protected endpoints", async () => {
    const client = await SandboxAgent.connect({
      baseUrl,
      token: "invalid-token",
    });

    // Health endpoint may be open, but listing agents should require auth
    await expect(client.listAgents()).rejects.toThrow();
  });
});

describe("Runtime detection", () => {
  it("detects Node.js runtime", () => {
    expect(isNodeRuntime()).toBe(true);
  });
});
