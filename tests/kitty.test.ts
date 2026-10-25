import { describe, expect, it } from "vitest";
import { encodeKitty, parseKittyResponse } from "../src/kitty.js";
import { encodeIterm2, detectIterm2 } from "../src/iterm.js";

function splitCommands(buf: Buffer): { control: string; payload: string }[] {
  const text = buf.toString("latin1");
  const out: { control: string; payload: string }[] = [];
  const re = /\x1b_G([^;]*);([^\x1b]*)\x1b\\/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    out.push({ control: match[1]!, payload: match[2]! });
  }
  return out;
}

describe("encodeKitty", () => {
  const png = Buffer.alloc(12_000, 7); // 4096 base64 文字を超えるサイズ

  it("制御キーを付けて最初のチャンクを送る", () => {
    const commands = splitCommands(encodeKitty(png, { cols: 40, rows: 10, holdCursor: true }));
    expect(commands[0]!.control).toBe("a=T,f=100,C=1,c=40,r=10,q=2,m=1");
  });

  it("4096 文字ずつに分割し、最後だけ m=0 にする", () => {
    const commands = splitCommands(encodeKitty(png, { cols: 10, rows: 5 }));
    const payloadSize = 4096;
    expect(commands.length).toBeGreaterThan(1);
    for (const command of commands.slice(0, -1)) {
      expect(command.payload.length).toBe(payloadSize);
    }
    for (const command of commands.slice(1, -1)) {
      expect(command.control).toBe("m=1");
    }
    expect(commands.at(-1)!.control).toBe("m=0");
    expect(commands.at(-1)!.payload.length).toBeLessThanOrEqual(payloadSize);
  });

  it("連結すると元の PNG に戻る", () => {
    const commands = splitCommands(encodeKitty(png, { cols: 4, rows: 2 }));
    const decoded = Buffer.from(commands.map((c) => c.payload).join(""), "base64");
    expect(decoded.equals(png)).toBe(true);
  });

  it("image id を付けると i= が入り、C=1 は holdCursor のときだけ付く", () => {
    const withId = splitCommands(encodeKitty(png, { imageId: 42, cols: 10, rows: 5 }));
    expect(withId[0]!.control).toBe("a=T,f=100,i=42,c=10,r=5,q=2,m=1");
    const hold = splitCommands(encodeKitty(png, { imageId: 42, holdCursor: true }));
    expect(hold[0]!.control).toBe("a=T,f=100,i=42,C=1,q=2,m=1");
  });

  it("空のバッファでは何も出力しない", () => {
    expect(encodeKitty(Buffer.alloc(0), { cols: 1, rows: 1 })).toHaveLength(0);
  });
});

describe("parseKittyResponse", () => {
  it("OK 応答を検出する", () => {
    expect(parseKittyResponse("\x1b_Gi=31;OK\x1b\\")).toBe(true);
  });
  it("エラー応答は非対応とみなす", () => {
    expect(parseKittyResponse("\x1b_Gi=31;EINVAL:bad data\x1b\\")).toBe(false);
  });
  it("応答が無ければ null", () => {
    expect(parseKittyResponse("\x1b[?62;c")).toBeNull();
  });
});

describe("encodeIterm2", () => {
  const png = Buffer.from([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4]);

  it("サイズと base64 を含む", () => {
    const out = encodeIterm2(png, { name: "図.png" }).toString("latin1");
    expect(out.startsWith("\x1b]1337;File=inline=1;size=8;")).toBe(true);
    expect(out).toContain("preserveAspectRatio=1");
    expect(out.endsWith("\x07")).toBe(true);
    const payload = out.slice(out.indexOf(":") + 1, -1);
    expect(Buffer.from(payload, "base64").equals(png)).toBe(true);
  });

  it("TERM_PROGRAM から iTerm2 対応を推定する", () => {
    const original = process.env.TERM_PROGRAM;
    process.env.TERM_PROGRAM = "iTerm.app";
    expect(detectIterm2()).toBe(true);
    process.env.TERM_PROGRAM = "vscode";
    expect(detectIterm2()).toBe(true);
    process.env.TERM_PROGRAM = "Apple_Terminal";
    expect(detectIterm2()).toBe(false);
    if (original === undefined) delete process.env.TERM_PROGRAM;
    else process.env.TERM_PROGRAM = original;
  });
});
