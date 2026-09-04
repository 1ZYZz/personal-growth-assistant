import { readFileSync, statSync } from "node:fs";
import { z } from "zod";
import { READ_TOOL_NAMES, type AgentSnapshot, type ReadToolName } from "@pga/contracts";

const taskSchema = z.object({
  id: z.string().min(1).max(128),
  title: z.string().min(1).max(500),
  date: z.iso.date(),
  time: z.string().max(32).nullable().optional(),
  status: z.enum(["todo", "in_progress", "snoozed", "blocked", "completed", "canceled"]),
  priority: z.enum(["p0", "p1", "p2", "p3"]),
});

const snapshotSchema = z.object({
  generatedAtUtc: z.iso.datetime({ offset: true }),
  tasks: z.array(taskSchema).max(1000),
  userProfile: z
    .object({
      locale: z.enum(["zh-CN", "en-US"]),
      timezone: z.string().min(1).max(128),
      clockFormat: z.enum(["12h", "24h"]),
    })
    .optional(),
  taskEvents: z
    .array(
      z.object({
        id: z.string().min(1).max(128),
        taskId: z.string().min(1).max(128),
        eventType: z.string().min(1).max(128),
        occurredAtUtc: z.iso.datetime({ offset: true }),
      }),
    )
    .max(2000)
    .optional(),
  watchFields: z
    .array(
      z.object({
        id: z.string().min(1).max(128),
        name: z.string().min(1).max(200),
        description: z.string().max(1000).nullable().optional(),
        includeKeywords: z.array(z.string().max(100)).max(100),
        excludeKeywords: z.array(z.string().max(100)).max(100),
        regions: z.array(z.string().max(100)).max(50),
        languages: z.array(z.string().max(32)).max(20),
      }),
    )
    .max(100)
    .optional(),
  newsHistory: z
    .array(
      z.object({
        id: z.string().min(1).max(128),
        fieldId: z.string().min(1).max(128),
        eventFingerprint: z.string().min(1).max(256),
        title: z.string().min(1).max(500),
        canonicalUrl: z.url().max(2048),
        publishedAtUtc: z.iso.datetime({ offset: true }).nullable().optional(),
        isRead: z.boolean(),
        isSaved: z.boolean(),
        isNotInterested: z.boolean(),
      }),
    )
    .max(2000)
    .optional(),
  learningState: z
    .array(
      z.object({
        id: z.string().min(1).max(128),
        goalId: z.string().min(1).max(128),
        title: z.string().min(1).max(500),
        mastery: z.number().min(0).max(100),
        nextReviewAtUtc: z.iso.datetime({ offset: true }).nullable().optional(),
      }),
    )
    .max(2000)
    .optional(),
});

export function readSnapshot(environment: NodeJS.ProcessEnv): AgentSnapshot {
  const snapshotPath = environment.PGA_SNAPSHOT_PATH;
  const encoded = snapshotPath
    ? (() => {
        if (statSync(snapshotPath).size > 1024 * 1024) {
          throw new Error("PGA snapshot file exceeds the 1 MiB safety limit");
        }
        return readFileSync(snapshotPath, "utf8");
      })()
    : environment.PGA_TASK_SNAPSHOT_JSON;
  if (!encoded) {
    return { generatedAtUtc: new Date(0).toISOString(), tasks: [] };
  }
  if (Buffer.byteLength(encoded, "utf8") > 1024 * 1024) {
    throw new Error("PGA_TASK_SNAPSHOT_JSON exceeds the 1 MiB safety limit");
  }
  return snapshotSchema.parse(JSON.parse(encoded)) as AgentSnapshot;
}

export function requireSessionToken(environment: NodeJS.ProcessEnv): void {
  const token = environment.PGA_SESSION_TOKEN;
  if (!token || token.length < 32) {
    throw new Error("A one-time PGA_SESSION_TOKEN of at least 32 characters is required");
  }
}

export function readAllowedTools(environment: NodeJS.ProcessEnv): ReadToolName[] {
  const encoded = environment.PGA_ALLOWED_SCOPE;
  if (!encoded) return ["get_tasks"];
  if (Buffer.byteLength(encoded, "utf8") > 4096) {
    throw new Error("PGA_ALLOWED_SCOPE exceeds the 4 KiB safety limit");
  }
  const parsed: unknown = JSON.parse(encoded);
  const values = Array.isArray(parsed)
    ? parsed
    : typeof parsed === "object" && parsed && "allowed_tools" in parsed
      ? (parsed as { allowed_tools: unknown }).allowed_tools
      : undefined;
  if (!Array.isArray(values) || !values.every((value) => typeof value === "string")) {
    throw new Error("PGA_ALLOWED_SCOPE must contain an allowed_tools string array");
  }
  const known = new Set<string>(READ_TOOL_NAMES);
  if (values.some((value) => !known.has(value))) {
    throw new Error("PGA_ALLOWED_SCOPE requests an unavailable tool");
  }
  return [...new Set(values)] as ReadToolName[];
}
