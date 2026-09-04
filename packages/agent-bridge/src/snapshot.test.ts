import { describe, expect, it } from "vitest";
import { readAllowedTools, readSnapshot, requireSessionToken } from "./snapshot.js";

describe("agent bridge boundary", () => {
  it("accepts only a bounded typed snapshot", () => {
    const snapshot = readSnapshot({
      PGA_TASK_SNAPSHOT_JSON: JSON.stringify({
        generatedAtUtc: "2026-09-03T08:00:00Z",
        tasks: [
          {
            id: "task-1",
            title: "Write the report",
            date: "2026-09-03",
            status: "todo",
            priority: "p1",
          },
        ],
      }),
    });
    expect(snapshot.tasks[0]?.id).toBe("task-1");
  });

  it("rejects malformed snapshots and weak session tokens", () => {
    expect(() => readSnapshot({ PGA_TASK_SNAPSHOT_JSON: "{}" })).toThrow();
    expect(() => requireSessionToken({ PGA_SESSION_TOKEN: "short" })).toThrow();
    expect(() => requireSessionToken({ PGA_SESSION_TOKEN: "x".repeat(32) })).not.toThrow();
  });

  it("allows only recognized tools requested through the bounded session scope", () => {
    expect(
      readAllowedTools({ PGA_ALLOWED_SCOPE: JSON.stringify(["get_tasks", "get_watch_fields"]) }),
    ).toEqual(["get_tasks", "get_watch_fields"]);
    expect(() =>
      readAllowedTools({ PGA_ALLOWED_SCOPE: JSON.stringify(["get_tasks", "run_command"]) }),
    ).toThrow("unavailable tool");
  });
});
