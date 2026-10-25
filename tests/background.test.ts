import { describe, expect, it } from "vitest";
import { parseBackground, resolveBackground } from "../src/images.js";

describe("resolveBackground", () => {
  it("auto はアルファを保持できる方式では合成しない", () => {
    expect(resolveBackground("auto", false, "kitty")).toBeNull();
    expect(resolveBackground("auto", false, "iterm")).toBeNull();
    expect(resolveBackground("auto", false, "blocks")).toBeNull();
  });

  it("auto + Sixel は背景色に合成する（アルファ非対応）", () => {
    expect(resolveBackground("auto", false, "sixel")).toEqual([12, 12, 12]);
    expect(resolveBackground("auto", true, "sixel")).toEqual([255, 255, 255]);
  });

  it("none は常に合成しない", () => {
    expect(resolveBackground("none", false, "sixel")).toBeNull();
  });

  it("明示色はすべての方式で合成する", () => {
    expect(resolveBackground("#ff0000", false, "kitty")).toEqual([255, 0, 0]);
    expect(resolveBackground("#0f0", false, "blocks")).toEqual([0, 255, 0]);
  });
});

describe("parseBackground", () => {
  it("auto は明暗で色を変える", () => {
    expect(parseBackground("auto", false)).toEqual([12, 12, 12]);
    expect(parseBackground("auto", true)).toEqual([255, 255, 255]);
    expect(parseBackground("none", false)).toBeNull();
  });
});
