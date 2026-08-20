import { describe, expect, it } from "vitest";
import { resolveTheme } from "./useTheme";

describe("resolveTheme", () => {
  it("honors an explicit light/dark choice", () => {
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("follows the OS when theme is system", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
});
