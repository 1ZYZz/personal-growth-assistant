import { FluentProvider, webLightTheme } from "@fluentui/react-components";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { taskApi, type TaskItem } from "./api";
import { formatDate } from "./date";
import i18n, { i18nReady } from "./i18n";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return {
    ...original,
    taskApi: {
      list: vi.fn().mockResolvedValue([]),
      create: vi.fn(),
      createRecurring: vi.fn(),
      setStatus: vi.fn(),
      runtimeInfo: vi.fn().mockResolvedValue({
        appVersion: "0.0.1",
        dataDirectory: "C:\\test",
        databaseEngine: "SQLite",
        recurrenceEngine: "RRule.rs",
        timezoneDatabase: "IANA",
      }),
    },
  };
});

function renderApp() {
  return render(
    <FluentProvider theme={webLightTheme}>
      <App />
    </FluentProvider>,
  );
}

describe("task workspace", () => {
  beforeEach(async () => {
    localStorage.clear();
    vi.mocked(taskApi.list).mockResolvedValue([]);
    await i18nReady;
    await i18n.changeLanguage("zh-CN");
  });

  it("shows a usable empty Today view and opens task capture", async () => {
    const user = userEvent.setup();
    renderApp();

    await waitFor(() => expect(screen.getByRole("heading", { name: "今日计划" })).toBeVisible());
    expect(await screen.findByText("今天还没有任务")).toBeVisible();
    await user.click(screen.getByRole("button", { name: /新建任务/ }));
    expect(screen.getByRole("dialog", { name: "创建任务" })).toBeVisible();
    expect(screen.getByLabelText("任务名称")).toBeVisible();
  });

  it("switches the workspace navigation to English", async () => {
    const user = userEvent.setup();
    renderApp();
    await waitFor(() => expect(screen.getByRole("heading", { name: "今日计划" })).toBeVisible());

    await user.click(screen.getByRole("button", { name: "切换到英文" }));
    expect(screen.getByRole("heading", { name: "Today's plan" })).toBeVisible();
    expect(screen.getByRole("button", { name: /Calendar/ })).toBeVisible();
    expect(document.documentElement.lang).toBe("en-US");
  });

  it("keeps long calendar titles readable and the copy action accessible", async () => {
    const user = userEvent.setup();
    const longTitle = "学习脑清除系统并阅读相关论文";
    const task: TaskItem = {
      id: "019-task",
      seriesId: null,
      occurrenceKey: null,
      title: longTitle,
      notes: null,
      priority: "p2",
      estimatedMinutes: 45,
      completionCriteria: null,
      status: "todo",
      timeMode: "floating",
      scheduledDate: formatDate(new Date()),
      scheduledLocal: `${formatDate(new Date())}T17:15:00`,
      scheduledUtc: null,
      tzid: null,
      dstAdjusted: false,
      isVirtual: false,
      version: 1,
      seriesVersion: null,
      progressPercent: null,
      progressNote: null,
      blockedReason: null,
      snoozedUntilUtc: null,
    };
    vi.mocked(taskApi.list).mockResolvedValue([task]);
    localStorage.setItem("pga-onboarding", "done");
    renderApp();

    await user.click(await screen.findByRole("button", { name: "日历" }));
    expect(await screen.findByText(longTitle)).toHaveAttribute("title", longTitle);
    expect(screen.getByRole("button", { name: "复制任务" })).toBeVisible();
  });
});
