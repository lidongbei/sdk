# sdk

A fast, cross-platform SDK version manager — Rust reimplementation of [vfox](https://github.com/version-fox/vfox) with one key optimization: **no symlinks in project directories**.

[![CI](https://github.com/lidongbei/sdk/actions/workflows/ci.yml/badge.svg)](https://github.com/lidongbei/sdk/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/lidongbei/sdk)](https://github.com/lidongbei/sdk/releases/latest)

## How it works

| vfox | sdk |
|------|-----|
| `use node@20` → writes `.vfox.toml` **+ creates `.vfox/sdk/node` symlink** | `use node@20` → writes **only** `.sdk.toml` |
| Activation reads the symlink | Activation reads `.sdk.toml` → resolves `~/.sdk/cache/` directly |

All runtimes live in `~/.sdk/cache/`. Projects contain only a single `.sdk.toml` file — no symlinks, no hidden directories.

---

## Installation

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/lidongbei/sdk/main/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/lidongbei/sdk/main/install.ps1 | iex
```

### From source

```bash
cargo install --git https://github.com/lidongbei/sdk
```

---

## Shell hook (auto-switching)

Add the hook to your shell profile so sdk automatically activates the right version when you enter a project directory.

**Bash** (`~/.bashrc`):
```bash
eval "$(sdk hook bash)"
```

**Zsh** (`~/.zshrc`):
```zsh
eval "$(sdk hook zsh)"
```

**Fish** (`~/.config/fish/config.fish`):
```fish
sdk hook fish | source
```

**PowerShell** (`$PROFILE`):
```powershell
Invoke-Expression (& sdk hook pwsh)
```

**Nushell** (`config.nu`):
```nu
sdk hook nu | save -f ~/.sdk-init.nu
source ~/.sdk-init.nu
```

The hook runs on every prompt change. It reads `.sdk.toml` (project → global) and updates `PATH` for the active SDK versions. Moving to a directory without a version config cleanly restores your original `PATH`.

---

## Quick start

```bash
# 1. Install a plugin
sdk add nodejs https://github.com/version-fox/vfox-nodejs

# 2. Install a version
sdk install nodejs@22.16.0

# 3. Activate for this project
sdk use nodejs@22.16.0

# 4. Verify
node --version   # v22.16.0
```

---

## Command reference

### Version management

| Command | Description |
|---------|-------------|
| `sdk install nodejs@22.16.0` | Install a specific version |
| `sdk install nodejs` | Install the version from `.sdk.toml` |
| `sdk uninstall nodejs@22.16.0` | Uninstall a version |
| `sdk use nodejs@22.16.0` | Set active version (project scope) |
| `sdk use nodejs@22.16.0 --global` | Set active version (global scope) |
| `sdk use nodejs@22.16.0 --session` | Set active version (current shell only) |
| `sdk unuse nodejs` | Remove SDK from active config |
| `sdk unuse nodejs --global` | Remove from global config |

### Information

| Command | Description |
|---------|-------------|
| `sdk list` | List all installed SDK versions |
| `sdk list nodejs` | List installed versions of one SDK |
| `sdk current` | Show currently active versions |
| `sdk available nodejs` | List versions available to install |
| `sdk search node` | Search the vfox plugin registry |
| `sdk info nodejs` | Show plugin metadata |
| `sdk env` | Show PATH/vars each active SDK exports |

### Plugins

| Command | Description |
|---------|-------------|
| `sdk add nodejs <url>` | Install a plugin from a git URL |
| `sdk remove nodejs` | Remove a plugin |
| `sdk update` | Update all installed plugins |

### Utilities

| Command | Description |
|---------|-------------|
| `sdk exec nodejs 22.16.0 -- node app.js` | Run a command with a specific SDK version |
| `sdk pin` | Pin active versions into project `.sdk.toml` |
| `sdk pin nodejs` | Pin only the nodejs version |
| `sdk upgrade` | Check for newer versions of active SDKs |
| `sdk upgrade --yes` | Auto-upgrade to latest versions |
| `sdk doctor` | Diagnose common issues |
| `sdk config` | Show user configuration |
| `sdk config get proxy.url` | Read a config key |
| `sdk config set proxy.url http://proxy:8080` | Write a config key |
| `sdk hook bash` | Print shell activation script |
| `sdk completions bash` | Print shell completion script |

---

## Scope

Three config scopes are merged in priority order (highest first):

| Scope | File | Flag |
|-------|------|------|
| Session | `$TMPDIR/sdk-session-*/\.sdk.toml` | `--session` |
| Project | `.sdk.toml` in nearest parent directory | *(default)* |
| Global | `~/.sdk/.sdk.toml` | `--global` |

---

## Shell completions

```bash
# Bash  (add to ~/.bashrc)
eval "$(sdk completions bash)"

# Zsh
sdk completions zsh > "${fpath[1]}/_sdk"

# Fish
sdk completions fish > ~/.config/fish/completions/sdk.fish

# PowerShell  (add to $PROFILE)
sdk completions powershell | Out-String | Invoke-Expression
```

---

## Plugin compatibility

`sdk` uses the same Lua plugin format as vfox. Any plugin from the [vfox plugin registry](https://github.com/version-fox/vfox-plugins) works directly:

```bash
sdk add nodejs  https://github.com/version-fox/vfox-nodejs
sdk add python  https://github.com/version-fox/vfox-python
sdk add java    https://github.com/version-fox/vfox-java
sdk add golang  https://github.com/version-fox/vfox-go
sdk add rust    https://github.com/version-fox/vfox-rust
```

---

## Directory layout

```
~/.sdk/
  .sdk.toml          ← global version config
  config.yaml        ← user settings (proxy, etc.)
  plugin/
    nodejs/          ← Lua plugin
    python/
  cache/
    nodejs/
      v-22.16.0/
        node-v22.16.0-linux-x64/   ← actual runtime files (no symlinks)
    python/
      v-3.12.0/

<your-project>/
  .sdk.toml          ← ONLY sdk artifact in project directory
```

---

## License

Apache-2.0
