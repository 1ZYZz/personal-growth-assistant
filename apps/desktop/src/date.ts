export function formatDate(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function parseDate(value: string): Date {
  const [year, month, day] = value.split("-").map(Number);
  return new Date(year, month - 1, day);
}

export function addDays(value: string, days: number): string {
  const date = parseDate(value);
  date.setDate(date.getDate() + days);
  return formatDate(date);
}

export function startOfWeek(value: string, startsOn: "monday" | "sunday"): string {
  const date = parseDate(value);
  const day = date.getDay();
  const offset = startsOn === "monday" ? (day + 6) % 7 : day;
  date.setDate(date.getDate() - offset);
  return formatDate(date);
}

export function taskDate(task: {
  scheduledDate: string | null;
  scheduledLocal: string | null;
}): string {
  return task.scheduledDate ?? task.scheduledLocal?.slice(0, 10) ?? "";
}

export function taskTime(task: { scheduledLocal: string | null; timeMode: string }): string | null {
  if (task.timeMode === "all_day" || !task.scheduledLocal) return null;
  return task.scheduledLocal.slice(11, 16);
}
