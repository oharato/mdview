import { describe, expect, it } from "vitest";
import { KITTY_TEST_ID, kittySelfTestSequence, parseKittySelfTest } from "../src/selftest.js";

function commands(sequence: string): { control: string; payload: string }[] {
  const out: { control: string; payload: string }[] = [];
  const re = /\x1b_G([^;\x1b]*)(?:;([^\x1b]*))?\x1b\\/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(sequence)) !== null) out.push({ control: m[1]!, payload: m[2] ?? "" });
  return out;
}

describe("kittySelfTestSequence", () => {
  const seq = kittySelfTestSequence();
  const cmds = commands(seq);

  it("複数チャンクで送信する（大きい画像と同じ経路を通す）", () => {
    expect(cmds.length).toBeGreaterThanOrEqual(3); // 2 チャンク + delete
    expect(cmds[0]!.control).toContain("a=t");
    expect(cmds[0]!.control).toContain("f=100");
    expect(cmds[0]!.control).toContain(`i=${KITTY_TEST_ID}`);
    expect(cmds[0]!.control).toContain("m=1");
  });

  it("ペイロードは 4096 文字以内で、連結すると PNG に戻る", () => {
    const chunks = cmds.filter((c) => c.payload.length > 0);
    for (const chunk of chunks) expect(chunk.payload.length).toBeLessThanOrEqual(4096);
    const png = Buffer.from(chunks.map((c) => c.payload).join(""), "base64");
    expect(png.subarray(1, 4).toString("latin1")).toBe("PNG");
  });

  it("最後にテスト画像を削除する", () => {
    const last = cmds.at(-1)!;
    expect(last.control).toContain("a=d");
    expect(last.control).toContain(`i=${KITTY_TEST_ID}`);
  });
});

describe("parseKittySelfTest", () => {
  it("WezTerm 形式（I=, i=）の OK を検出する", () => {
    expect(parseKittySelfTest(`\x1b_GI=${KITTY_TEST_ID},i=${KITTY_TEST_ID};OK\x1b\\`)).toBe(true);
  });
  it("kitty 形式（i= のみ）の OK を検出する", () => {
    expect(parseKittySelfTest(`\x1b_Gi=${KITTY_TEST_ID};OK\x1b\\`)).toBe(true);
  });
  it("クエリ応答 (i=31) とは区別する", () => {
    expect(parseKittySelfTest("\x1b_Gi=31;OK\x1b\\")).toBe(false);
  });
  it("応答が無ければ false", () => {
    expect(parseKittySelfTest("")).toBe(false);
    expect(parseKittySelfTest("\x1b[?62;c")).toBe(false);
  });
});
