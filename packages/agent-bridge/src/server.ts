import { McpServer } from "@modelcontextprotocol/server";
import { StdioServerTransport } from "@modelcontextprotocol/server/stdio";
import { z } from "zod";
import type { AgentSnapshot, ReadToolName } from "@pga/contracts";

export const BRIDGE_VERSION = "1.1.1";

const statusSchema = z.enum(["todo", "in_progress", "snoozed", "blocked", "completed", "canceled"]);

function toolResult(output: Record<string, unknown>) {
  return {
    content: [{ type: "text" as const, text: JSON.stringify(output) }],
    structuredContent: output,
  };
}

export function createServer(
  snapshot: AgentSnapshot,
  allowedTools: ReadToolName[] = ["get_tasks"],
): McpServer {
  const server = new McpServer(
    { name: "personal-growth-assistant", version: BRIDGE_VERSION },
    { capabilities: { tools: {} } },
  );
  const allowed = new Set<ReadToolName>(allowedTools);

  if (allowed.has("get_user_profile")) {
    server.registerTool(
      "get_user_profile",
      {
        title: "Read de-identified user preferences",
        description:
          "Return only locale, timezone, and clock-format preferences in this job snapshot.",
        inputSchema: z.object({}).strict(),
        outputSchema: z.object({
          available: z.boolean(),
          profile: z
            .object({
              locale: z.enum(["zh-CN", "en-US"]),
              timezone: z.string(),
              clockFormat: z.enum(["12h", "24h"]),
            })
            .nullable(),
        }),
        annotations: { readOnlyHint: true, destructiveHint: false, openWorldHint: false },
      },
      async () =>
        toolResult({
          available: Boolean(snapshot.userProfile),
          profile: snapshot.userProfile ?? null,
        }),
    );
  }

  if (allowed.has("get_tasks")) {
    server.registerTool(
      "get_tasks",
      {
        title: "Read personal tasks",
        description: "Return only the bounded task snapshot authorized for this job.",
        inputSchema: z
          .object({
            from: z.iso.date().optional(),
            to: z.iso.date().optional(),
            statuses: z.array(statusSchema).max(6).optional(),
            includeCompleted: z.boolean().default(false),
            limit: z.number().int().min(1).max(500).default(200),
          })
          .strict(),
        outputSchema: z.object({
          generatedAtUtc: z.string(),
          tasks: z.array(
            z.object({
              id: z.string(),
              title: z.string(),
              date: z.string(),
              time: z.string().nullable().optional(),
              status: statusSchema,
              priority: z.enum(["p0", "p1", "p2", "p3"]),
            }),
          ),
        }),
        annotations: { readOnlyHint: true, destructiveHint: false, openWorldHint: false },
      },
      async ({ from, to, statuses, includeCompleted, limit }) => {
        const tasks = snapshot.tasks
          .filter(
            (task) =>
              (!from || task.date >= from) &&
              (!to || task.date <= to) &&
              (!statuses || statuses.includes(task.status)) &&
              (includeCompleted || !["completed", "canceled"].includes(task.status)),
          )
          .slice(0, limit);
        return toolResult({ generatedAtUtc: snapshot.generatedAtUtc, tasks });
      },
    );
  }

  if (allowed.has("get_task_events")) {
    server.registerTool(
      "get_task_events",
      {
        title: "Read task events",
        description: "Return bounded task event metadata from this job snapshot.",
        inputSchema: z
          .object({
            taskId: z.string().max(128).optional(),
            fromUtc: z.iso.datetime({ offset: true }).optional(),
            toUtc: z.iso.datetime({ offset: true }).optional(),
            limit: z.number().int().min(1).max(500).default(200),
          })
          .strict(),
        outputSchema: z.object({ events: z.array(z.object({}).passthrough()) }),
        annotations: { readOnlyHint: true, destructiveHint: false, openWorldHint: false },
      },
      async ({ taskId, fromUtc, toUtc, limit }) => {
        const events = (snapshot.taskEvents ?? [])
          .filter(
            (event) =>
              (!taskId || event.taskId === taskId) &&
              (!fromUtc || event.occurredAtUtc >= fromUtc) &&
              (!toUtc || event.occurredAtUtc <= toUtc),
          )
          .slice(0, limit);
        return toolResult({ events });
      },
    );
  }

  if (allowed.has("get_watch_fields")) {
    server.registerTool(
      "get_watch_fields",
      {
        title: "Read enabled watch fields",
        description: "Return bounded search configuration authorized for this job.",
        inputSchema: z.object({ limit: z.number().int().min(1).max(100).default(50) }).strict(),
        outputSchema: z.object({ fields: z.array(z.object({}).passthrough()) }),
        annotations: { readOnlyHint: true, destructiveHint: false, openWorldHint: false },
      },
      async ({ limit }) => toolResult({ fields: (snapshot.watchFields ?? []).slice(0, limit) }),
    );
  }

  if (allowed.has("get_news_history")) {
    server.registerTool(
      "get_news_history",
      {
        title: "Read verified news history",
        description:
          "Return only stored event fingerprints, feedback, titles, and canonical links.",
        inputSchema: z
          .object({
            fieldId: z.string().max(128).optional(),
            sinceUtc: z.iso.datetime({ offset: true }).optional(),
            limit: z.number().int().min(1).max(500).default(200),
          })
          .strict(),
        outputSchema: z.object({ items: z.array(z.object({}).passthrough()) }),
        annotations: { readOnlyHint: true, destructiveHint: false, openWorldHint: false },
      },
      async ({ fieldId, sinceUtc, limit }) => {
        const items = (snapshot.newsHistory ?? [])
          .filter(
            (item) =>
              (!fieldId || item.fieldId === fieldId) &&
              (!sinceUtc || !item.publishedAtUtc || item.publishedAtUtc >= sinceUtc),
          )
          .slice(0, limit);
        return toolResult({ items });
      },
    );
  }

  if (allowed.has("get_learning_state")) {
    server.registerTool(
      "get_learning_state",
      {
        title: "Read learning state",
        description: "Return bounded mastery and review timing from this job snapshot.",
        inputSchema: z
          .object({
            goalId: z.string().max(128).optional(),
            limit: z.number().int().min(1).max(500).default(200),
          })
          .strict(),
        outputSchema: z.object({ nodes: z.array(z.object({}).passthrough()) }),
        annotations: { readOnlyHint: true, destructiveHint: false, openWorldHint: false },
      },
      async ({ goalId, limit }) => {
        const nodes = (snapshot.learningState ?? [])
          .filter((node) => !goalId || node.goalId === goalId)
          .slice(0, limit);
        return toolResult({ nodes });
      },
    );
  }

  return server;
}

export async function serve(snapshot: AgentSnapshot, allowedTools: ReadToolName[]): Promise<void> {
  const server = createServer(snapshot, allowedTools);
  const transport = new StdioServerTransport(process.stdin, process.stdout, {
    maxBufferSize: 1024 * 1024,
  });
  await server.connect(transport);
}
