import enUS from "./en-US.json";
import zhCN from "./zh-CN.json";

describe("translation resources", () => {
  it("keeps the en-US and zh-CN key sets identical", () => {
    expect(Object.keys(zhCN).sort()).toEqual(Object.keys(enUS).sort());
  });

  it.each([
    ["en-US", enUS],
    ["zh-CN", zhCN],
  ])("does not contain blank %s translations", (_locale, resource) => {
    expect(Object.values(resource).every((value) => value.trim().length > 0)).toBe(true);
  });
});
