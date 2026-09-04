import { InMemoryTransport, type JSONRPCMessage } from "@modelcontextprotocol/server";
import { afterEach, describe, expect, it } from "vitest";
import { createServer } from "./server.js";

describe("MCP bridge contract", () => {
  const transports: InMemoryTransport[] = [];

  afterEach(async () => {
    await Promise.all(transports.map((transport) => transport.close()));
    transports.length = 0;
  });

  it("negotiates MCP and exposes only bounded read tools", async () => {
    const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
    transports.push(clientTransport, serverTransport);
    const server = createServer(
      {
        generatedAtUtc: "2026-09-03T08:00:00Z",
        tasks: [
          {
            id: "task-1",
            title: "Review roadmap",
            date: "2026-09-03",
            status: "in_progress",
            priority: "p1",
          },
          {
            id: "task-2",
            title: "Archived decision",
            date: "2026-09-03",
            status: "canceled",
            priority: "p3",
          },
        ],
        watchFields: [
          {
            id: "field-1",
            name: "AI tools",
            includeKeywords: ["agent"],
            excludeKeywords: [],
            regions: ["global"],
            languages: ["en"],
          },
        ],
      },
      ["get_tasks", "get_watch_fields"],
    );
    await server.connect(serverTransport);
    await clientTransport.start();

    const nextMessage = () =>
      new Promise<JSONRPCMessage>((resolve) => {
        clientTransport.onmessage = resolve;
      });
    let response = nextMessage();
    await clientTransport.send({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-06-18",
        capabilities: {},
        clientInfo: { name: "pga-contract-test", version: "1.0.0" },
      },
    });
    expect(await response).toMatchObject({
      id: 1,
      result: { serverInfo: { name: "personal-growth-assistant" } },
    });

    await clientTransport.send({ jsonrpc: "2.0", method: "notifications/initialized" });
    response = nextMessage();
    await clientTransport.send({ jsonrpc: "2.0", id: 2, method: "tools/list" });
    const tools = await response;
    expect(tools).toMatchObject({ id: 2 });
    const toolNames =
      "result" in tools && tools.result && "tools" in tools.result
        ? (tools.result.tools as { name: string }[]).map((tool) => tool.name)
        : [];
    expect(toolNames).toEqual(["get_tasks", "get_watch_fields"]);

    response = nextMessage();
    await clientTransport.send({
      jsonrpc: "2.0",
      id: 3,
      method: "tools/call",
      params: { name: "get_watch_fields", arguments: { limit: 10 } },
    });
    expect(await response).toMatchObject({
      id: 3,
      result: {
        structuredContent: {
          fields: [
            {
              id: "field-1",
              name: "AI tools",
              includeKeywords: ["agent"],
              excludeKeywords: [],
              regions: ["global"],
              languages: ["en"],
            },
          ],
        },
      },
    });

    await server.close();
  });
});
