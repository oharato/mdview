import { describe, expect, it } from "vitest";
import { compareVersions, currentVersion, detectInstallKind } from "../src/update.js";

describe("compareVersions", () => {
  it("数値として比較する", () => {
    expect(compareVersions("0.10.0", "0.9.9")).toBeGreaterThan(0);
    expect(compareVersions("0.9.9", "0.10.0")).toBeLessThan(0);
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
  });

  it("v 接頭辞とプリリリースを無視する", () => {
    expect(compareVersions("v1.2.3", "1.2.3")).toBe(0);
    expect(compareVersions("1.2.3-beta.1", "1.2.3")).toBe(0);
    expect(compareVersions("1.2", "1.2.0")).toBe(0);
  });
});

describe("currentVersion", () => {
  it("package.json のバージョンを返す", () => {
    expect(currentVersion()).toMatch(/^\d+\.\d+\.\d+/);
  });
});

describe("detectInstallKind", () => {
  it("このリポジトリでは git と判定する", () => {
    const kind = detectInstallKind();
    expect(kind.kind).toBe("git");
    if (kind.kind === "git") expect(kind.root).toContain("mdview");
  });
});
