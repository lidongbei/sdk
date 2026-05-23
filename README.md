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
# 1. Clone the official plugin collection
git clone https://github.com/lidongbei/sdk-plugins.git /tmp/sdk-plugins

# 2. Add a plugin (from local path)
sdk add node /tmp/sdk-plugins/node

# 3. Install a version
sdk install node@20.0.0

# 4. Activate for this project
sdk use node@20.0.0

# 5. Verify
node --version   # v20.0.0
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
| `sdk add node <url-or-path>` | Install a plugin from a git URL or local directory |
| `sdk remove node` | Remove a plugin |
| `sdk update` | Update all installed plugins |

### Cache & Offline

| Command | Description |
|---------|-------------|
| `sdk cache list` | List cached download archives |
| `sdk cache clean` | Remove all cached archives |
| `sdk config set cache.offline true` | Enable offline mode (no network requests) |
| `sdk config set cache.mirror_dir /path` | Set local mirror directory for archives |
| `sdk config set cache.keep_downloads true` | Keep downloaded archives for offline reuse |

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

## Official plugins

[**lidongbei/sdk-plugins**](https://github.com/lidongbei/sdk-plugins) — self-hosted plugin collection with custom mirror support:

| Plugin | Description | Mirror env var |
|--------|-------------|----------------|
| `node` | Node.js runtime | `SDK_NODE_MIRROR` |
| `python` | Python (build-standalone) | `SDK_PYTHON_STANDALONE_MIRROR` |
| `go` | Go language | `SDK_GO_MIRROR` |
| `java` | Java (Eclipse Temurin) | `SDK_JAVA_MIRROR` |
| `maven` | Apache Maven | `SDK_MAVEN_MIRROR` |
| `gradle` | Gradle build tool | `SDK_GRADLE_MIRROR` |
| `rust` | Rust (via rustup) | `SDK_RUSTUP_MIRROR` |

```bash
git clone https://github.com/lidongbei/sdk-plugins.git /tmp/sdk-plugins
sdk add node    /tmp/sdk-plugins/node
sdk add python  /tmp/sdk-plugins/python
sdk add go      /tmp/sdk-plugins/go
sdk add java    /tmp/sdk-plugins/java
sdk add maven   /tmp/sdk-plugins/maven
sdk add gradle  /tmp/sdk-plugins/gradle
sdk add rust    /tmp/sdk-plugins/rust
```

**Offline / intranet deployment:**

```bash
# Point to your internal mirrors
export SDK_NODE_MIRROR=https://intranet-mirror/nodejs
export SDK_GO_MIRROR=https://intranet-mirror/golang

sdk config set cache.offline true          # disable all network requests
sdk config set cache.mirror_dir /mnt/sdk   # local archive directory
```

**Local mirror profile (`sdk mirror use local`):**

Plugins can define a `local` mirror profile that points to a local directory instead of an HTTP URL. When selected, `http.download_file` and `http.get` will read/copy from the local path directly — no HTTP request is made.

Define it in your plugin's `metadata.lua`:

```lua
PLUGIN = {
  name    = "node",
  version = "1.0.0",
  mirrors = {
    { name="default", description="Official",        vars={SDK_NODE_MIRROR="https://nodejs.org/dist"} },
    { name="china",   description="NPMMIRROR (China CDN)", vars={SDK_NODE_MIRROR="https://npmmirror.com/mirrors/node"} },
    { name="local",   description="Local directory", vars={SDK_NODE_MIRROR="/opt/sdk-mirror/node"} },
  },
}
```

Then in your plugin hook, build paths with `os.getenv`:

```lua
function PLUGIN:PreInstall(ctx)
  local mirror = os.getenv("SDK_NODE_MIRROR") or "https://nodejs.org/dist"
  local url = mirror .. "/v" .. ctx.version .. "/node-v" .. ctx.version .. "-linux-x64.tar.gz"
  -- When mirror is a local path, http.download_file copies the file directly.
  return { version = ctx.version, url = url }
end
```

Switch mirror:

```bash
sdk mirror use local node     # use local directory for node plugin
sdk mirror use default node   # revert to official mirror
sdk mirror list node          # show all available profiles
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
