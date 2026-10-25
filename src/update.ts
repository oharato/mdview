import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const REPOSITORY = "oharato/mdview";
const GIT_URL = `https://github.com/${REPOSITORY}.git`;

export interface LatestRelease {
  version: string;
  url: string;
}

export type InstallKind =
  | { kind: "git"; root: string }
  | { kind: "package"; manager: "pnpm" | "npm" }
  | { kind: "unknown" };

/** パッケージのルート（dist/ の 1 つ上）。 */
export function packageRoot(): string {
  return resolve(import.meta.dirname, "..");
}

/** package.json の version。 */
export function currentVersion(): string {
  try {
    const raw = readFileSync(resolve(packageRoot(), "package.json"), "utf8");
    const parsed = JSON.parse(raw) as { version?: string };
    return parsed.version ?? "0.0.0";
  } catch {
    return "0.0.0";
  }
}

function normalize(version: string): number[] {
  return version
    .trim()
    .replace(/^v/, "")
    .split(/[.+-]/)
    .slice(0, 3)
    .map((part) => {
      const n = Number.parseInt(part, 10);
      return Number.isFinite(n) ? n : 0;
    });
}

/** セマンティックバージョンの比較（a > b なら正、等しければ 0、a < b なら負）。 */
export function compareVersions(a: string, b: string): number {
  const pa = normalize(a);
  const pb = normalize(b);
  for (let i = 0; i < 3; i += 1) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

async function githubJson(path: string): Promise<unknown | null> {
  try {
    const res = await fetch(`https://api.github.com/repos/${REPOSITORY}${path}`, {
      headers: {
        Accept: "application/vnd.github+json",
        "User-Agent": `mdview/${currentVersion()}`,
      },
      signal: AbortSignal.timeout(10_000),
    });
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

/** GitHub の最新リリース（無ければ最新タグ）を取得する。 */
export async function fetchLatestRelease(): Promise<LatestRelease | null> {
  const release = (await githubJson("/releases/latest")) as
    | { tag_name?: string; html_url?: string }
    | null;
  if (release?.tag_name) {
    return {
      version: release.tag_name.replace(/^v/, ""),
      url: release.html_url ?? `https://github.com/${REPOSITORY}/releases`,
    };
  }

  const tags = (await githubJson("/tags?per_page=1")) as { name?: string }[] | null;
  const name = tags?.[0]?.name;
  if (name) {
    return {
      version: name.replace(/^v/, ""),
      url: `https://github.com/${REPOSITORY}/releases/tag/${name}`,
    };
  }
  return null;
}

function tryExec(command: string, args: string[]): string | null {
  try {
    return execFileSync(command, args, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return null;
  }
}

/** どの方法でインストールされているかを判定する。 */
export function detectInstallKind(): InstallKind {
  const root = packageRoot();

  // git クローンから symlink されている場合
  const gitRoot = tryExec("git", ["-C", root, "rev-parse", "--show-toplevel"]);
  if (gitRoot) {
    const remote = tryExec("git", ["-C", gitRoot, "remote", "get-url", "origin"]) ?? "";
    if (remote.replace(/\.git$/, "").endsWith(REPOSITORY)) return { kind: "git", root: gitRoot };
  }

  // node_modules 配下（pnpm/npm のグローバルインストール）
  if (root.includes("node_modules")) {
    const manager = root.includes(".pnpm") ? "pnpm" : "npm";
    return { kind: "package", manager };
  }

  return { kind: "unknown" };
}

function run(command: string, args: string[], cwd?: string): boolean {
  const result = spawnSync(command, args, { cwd, stdio: "inherit" });
  return !result.error && result.status === 0;
}

export interface UpdateResult {
  ok: boolean;
  message: string;
}

/** 自己アップデートを実行する。 */
export async function runSelfUpdate(): Promise<UpdateResult> {
  const current = currentVersion();
  const latest = await fetchLatestRelease();
  if (!latest) {
    return { ok: false, message: "最新バージョンを取得できませんでした（ネットワークを確認してください）" };
  }
  if (compareVersions(latest.version, current) <= 0) {
    return { ok: true, message: `最新です (v${current})` };
  }

  const install = detectInstallKind();

  if (install.kind === "git") {
    process.stderr.write(`v${current} → v${latest.version} に更新します (${install.root})\n`);
    if (!run("git", ["-C", install.root, "pull", "--ff-only"])) {
      return { ok: false, message: "git pull に失敗しました（ローカル変更やブランチを確認してください）" };
    }
    if (!run("pnpm", ["install", "--frozen-lockfile"], install.root)) {
      return { ok: false, message: "pnpm install に失敗しました" };
    }
    if (!run("pnpm", ["build"], install.root)) {
      return { ok: false, message: "ビルドに失敗しました" };
    }
    return { ok: true, message: `v${latest.version} に更新しました` };
  }

  if (install.kind === "package") {
    const spec = `github:${REPOSITORY}#v${latest.version}`;
    process.stderr.write(`v${current} → v${latest.version} に更新します (${install.manager} -g)\n`);
    const args =
      install.manager === "pnpm" ? ["add", "-g", spec] : ["install", "-g", spec];
    if (!run(install.manager, args)) {
      return { ok: false, message: `${install.manager} による更新に失敗しました` };
    }
    return { ok: true, message: `v${latest.version} に更新しました` };
  }

  return {
    ok: false,
    message:
      "インストール方法を判定できませんでした。手動で更新してください:\n" +
      `  git -C <clone> pull && pnpm install && pnpm build\n` +
      `  pnpm add -g ${GIT_URL}`,
  };
}

/** 更新があるかだけを確認する。 */
export async function checkForUpdate(): Promise<{
  current: string;
  latest: LatestRelease | null;
  hasUpdate: boolean;
  install: InstallKind;
}> {
  const current = currentVersion();
  const latest = await fetchLatestRelease();
  return {
    current,
    latest,
    hasUpdate: latest ? compareVersions(latest.version, current) > 0 : false,
    install: detectInstallKind(),
  };
}

export function describeInstall(kind: InstallKind): string {
  if (kind.kind === "git") return `git clone (${kind.root})`;
  if (kind.kind === "package") return `${kind.manager} global`;
  return "不明";
}
