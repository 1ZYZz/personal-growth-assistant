import { READ_TOOL_NAMES, type AgentBridgeProbe } from "@pga/contracts";
import { BRIDGE_VERSION, serve } from "./server.js";
import { readAllowedTools, readSnapshot, requireSessionToken } from "./snapshot.js";

async function main() {
  const command = process.argv[2] ?? "probe";
  if (command === "probe" || (command === "runtime" && (process.argv[3] ?? "probe") === "probe")) {
    const probe: AgentBridgeProbe = {
      bridgeVersion: BRIDGE_VERSION,
      nodeVersion: process.version,
      transport: "stdio",
      databaseAccess: false,
      tools: [...READ_TOOL_NAMES],
    };
    process.stdout.write(`${JSON.stringify(probe)}\n`);
    return;
  }
  if (command === "mcp") {
    requireSessionToken(process.env);
    await serve(readSnapshot(process.env), readAllowedTools(process.env));
    return;
  }
  throw new Error(`Unknown command: ${command}`);
}

void main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
