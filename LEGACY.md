# Legacy Node implementation

The Node.js/TypeScript implementation of mdview is frozen at **v0.6.1**.

- Branch: [`legacy/node`](https://github.com/oharato/mdview/tree/legacy/node)
- Tag: `node-v0.6.1`
- Status: **archived; no new features or dependency updates**

The Rust implementation at the repository root is the official version.

## Why keep it?

- It can run on macOS / other Unix environments where a Node.js + mdcat setup is available.
- It uses Pele for Mermaid, so its class/sequence diagram rendering can differ from the
  Rust `mermaid-rs-renderer` implementation.

## Install the legacy version

```bash
pnpm add -g github:oharato/mdview#node-v0.6.1
```

Or check out the branch and run its original installer:

```bash
git clone --branch legacy/node https://github.com/oharato/mdview.git
cd mdview
./install.sh
```

## Migration

For Linux, use the Rust single binary from GitHub Releases:

```bash
curl -fsSL https://github.com/oharato/mdview/releases/latest/download/mdview-x86_64-unknown-linux-gnu \
  -o ~/.local/bin/mdview
chmod +x ~/.local/bin/mdview
```

See [README.md](README.md) for the current feature set and known differences.
