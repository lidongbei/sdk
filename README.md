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
# 1. Init built-in plugins (offline, no network needed)
sdk plugin init           # install all 7 official plugins at once
# or pick specific ones:
sdk plugin init java node go

# 2. Install a version
sdk install node@22.16.0

# 3. Activate for this project
sdk use node@22.16.0

# 4. Verify
node --version   # v22.16.0
```

---

## Command reference

### Version management

| Command | Description |
|---------|-------------|
| `sdk install node@22.16.0` | Install a specific version |
| `sdk install node` | Install the version from `.sdk.toml` |
| `sdk uninstall node@22.16.0` | Uninstall a version |
| `sdk use node@22.16.0` | Set active version (project scope) |
| `sdk use node@22.16.0 --global` | Set active version (global scope) |
| `sdk use node@22.16.0 --session` | Set active version (current shell only) |
| `sdk unuse node` | Remove SDK from active config |
| `sdk unuse node --global` | Remove from global config |

### Information

| Command | Description |
|---------|-------------|
| `sdk list` | List all installed SDK versions |
| `sdk list node` | List installed versions of one SDK |
| `sdk current` | Show currently active versions |
| `sdk available node` | List versions available to install |
| `sdk search node` | Search for installable versions |
| `sdk env` | Show PATH/vars each active SDK exports |

### Plugin management

| Command | Description |
|---------|-------------|
| `sdk plugin init` | Install all 7 built-in plugins offline (embedded in binary) |
| `sdk plugin init java node` | Install specific built-in plugins offline |
| `sdk plugin add node` | Add a built-in plugin by name (same as `init`, offline) |
| `sdk plugin add node <url-or-path>` | Add a plugin from a git URL or local directory |
| `sdk plugin remove node` | Remove a plugin (aliases: `rm`, `uninstall`) |
| `sdk plugin update` | Update all plugins — git pull for git-managed, re-embed for built-in |
| `sdk plugin update node` | Update a specific plugin |
| `sdk plugin list` | List all installed plugins |
| `sdk plugin info node` | Show plugin metadata |

### Cache & Offline

| Command | Description |
|---------|-------------|
| `sdk cache list` | List cached download archives |
| `sdk cache clean` | Remove all cached archives |
| `sdk config set cache.offline true` | Enable offline mode (no network requests) |
| `sdk config set cache.mirror_dir /path` | Set local mirror directory for archives |
| `sdk config set cache.keep_downloads true` | Keep downloaded archives for offline reuse |

### Mirror download (build local mirror)

| Command | Description |
|---------|-------------|
| `sdk download node --lts` | Download LTS version(s) of node to local mirror |
| `sdk download node --all` | Download all available versions of node |
| `sdk download node -V 20.0.0,18.20.3` | Download specific version(s) |
| `sdk download node go --all` | Download all versions for multiple plugins |
| `sdk download --all` | Download all versions for every installed plugin |
| `sdk download node --all --dry-run` | Preview URLs without downloading |

Archives are saved flat to `mirror.local_dir/<plugin>/<filename>` and `versions.json` is generated automatically.
Only installed plugins are processed — uninstalled plugins are skipped with a warning.

### Utilities

| Command | Description |
|---------|-------------|
| `sdk exec node 22.16.0 -- node app.js` | Run a command with a specific SDK version |
| `sdk pin` | Pin active versions into project `.sdk.toml` |
| `sdk pin node` | Pin only the node version |
| `sdk upgrade` | Check for newer versions of active SDKs |
| `sdk upgrade --yes` | Auto-upgrade to latest versions |
| `sdk doctor` | Diagnose common issues |
| `sdk fix` | Scan and report broken/incomplete installs (dry run) |
| `sdk fix --yes` | Remove broken/incomplete installs |
| `sdk fix node --yes` | Remove broken installs for a specific SDK |
| `sdk config` | Show all configuration settings |
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

The 7 official plugins are **bundled inside the sdk binary** and can be installed completely offline:

```bash
sdk plugin init             # install all 7 plugins at once (offline)
sdk plugin init java node   # install specific plugins
sdk plugin add java         # same as init, install one plugin
```

Source: [**lidongbei/sdk-plugins**](https://github.com/lidongbei/sdk-plugins) — updated files are bundled on each sdk release.

To get the latest plugin files, upgrade sdk and run:

```bash
sdk plugin update        # re-embeds all built-in plugins from the new binary (offline)
```

If you need plugin changes before a new sdk release, install directly from source:

```bash
git clone https://github.com/lidongbei/sdk-plugins.git /tmp/sdk-plugins
sdk plugin add node /tmp/sdk-plugins/node
```

| Plugin | Description | Mirror env var |
|--------|-------------|----------------|
| `node` | Node.js runtime | `SDK_NODE_MIRROR` |
| `python` | Python (build-standalone) | `SDK_PYTHON_STANDALONE_MIRROR` |
| `go` | Go language | `SDK_GO_MIRROR` |
| `java` | Java (Eclipse Temurin) | `SDK_JAVA_MIRROR` |
| `maven` | Apache Maven | `SDK_MAVEN_MIRROR` |
| `gradle` | Gradle build tool | `SDK_GRADLE_MIRROR` |
| `rust` | Rust (via rustup) | `SDK_RUSTUP_MIRROR` |

**Offline / intranet deployment:**

```bash
# Point to your internal mirrors
export SDK_NODE_MIRROR=https://intranet-mirror/nodejs
export SDK_GO_MIRROR=https://intranet-mirror/golang

sdk config set cache.offline true          # disable all network requests
sdk config set cache.mirror_dir /mnt/sdk   # local archive directory
```

**Local mirror profile (`sdk mirror use local`) and HTTP mirror server (`sdk mirror use http-server`):**

Plugins support a `local` profile (reads from filesystem) and an `http-server` profile (fetches from a local HTTP server).
When `local` is selected, `http.download_file` and `http.get` read/copy from the local path directly — no HTTP request is made.

**Configure the local mirror directory** (defaults to `~/.sdk/downloads/`):

```bash
sdk config set mirror.local_dir /mnt/usb/sdk-mirror   # custom path (e.g. USB drive / NAS)
sdk config set mirror.local_dir                        # reset to default (~/.sdk/downloads/)
```

**Switch all plugins to local/http-server mirror:**

```bash
sdk mirror use local          # apply local profile to all plugins
sdk mirror use local node     # apply only to node plugin
sdk mirror use http-server    # apply http-server profile to all plugins
sdk mirror use default node   # revert to official mirror
sdk mirror list node          # show all available profiles
```

The `{local_dir}` placeholder in profile vars is expanded to the configured path at runtime.
The `{http_server}` placeholder in profile vars is expanded to the configured URL at runtime.
Official plugins (node, go, java, python, maven, gradle, rust) use:

```
{local_dir}/node/   {local_dir}/go/   {local_dir}/java/   ...
```

**Local mirror directory structure:**

Files are stored **flat** (no subdirectories). Each plugin folder contains the archive files and optionally a `versions.json` for `sdk available`:

```
~/.sdk/downloads/
  node/
    node-v22.16.0-linux-x64.tar.gz
    node-v20.11.0-linux-x64.tar.gz
    node-v20.11.0-darwin-arm64.tar.gz
  go/
    go1.22.0.linux-amd64.tar.gz
    go1.21.5.linux-amd64.tar.gz
    versions.json           ← ["1.22.0", "1.21.5", ...]
  java/
    OpenJDK21U-jdk_x64_linux_hotspot_21.0.2_13.tar.gz
  maven/
    apache-maven-3.9.6-bin.tar.gz
    versions.json           ← ["3.9.6", "3.8.8", ...]
  gradle/
    gradle-8.6-bin.zip
  rust/
    rustup-init              ← Linux/macOS
    rustup-init.exe          ← Windows
  python/
    cpython-3.12.0+20240107-x86_64-unknown-linux-gnu-install_only.tar.gz
    versions.json           ← ["3.12.0", "3.11.5", ...]
```

`versions.json` format (simple array of version strings, newest first):
```json
["1.22.0", "1.21.5", "1.20.14"]
```

Plugins that use `versions.json` for `sdk available`: **go**, **maven**, **python**, **node** (local/http-server only).
Plugins that don't need it (use existing API or bundled list): java, gradle, rust.

**HTTP mirror server profile (`sdk mirror use http-server`):**

Instead of a local filesystem path, you can serve the mirror directory over HTTP (e.g. for sharing across machines on a LAN):

```bash
# Start a local HTTP server on your downloads directory
cd ~/.sdk/downloads && python3 -m http.server 8080

# Configure the server URL
sdk config set mirror.http_server http://192.168.1.100:8080

# Switch all plugins to use the HTTP mirror server
sdk mirror use http-server
sdk mirror use http-server node    # only node
```

The `{http_server}` placeholder in profile vars is expanded to the configured URL at runtime.
The `SDK_FLAT_MIRROR=1` env var is automatically set when using the `http-server` profile, so hooks use the flat file structure (same as `local`).

**Configure the local mirror directory** (defaults to `~/.sdk/downloads/`):

```bash
sdk config set mirror.local_dir /mnt/usb/sdk-mirror   # custom path (e.g. USB drive / NAS)
sdk config set mirror.local_dir                        # reset to default (~/.sdk/downloads/)
```

**Summary of mirror config keys:**

| Key | Default | Description |
|-----|---------|-------------|
| `mirror.local_dir` | `~/.sdk/downloads/` | Base dir for `local` profile |
| `mirror.http_server` | *(empty)* | Base URL for `http-server` profile |

**Define a custom `local` / `http-server` profile in your plugin's `metadata.lua`:**

```lua
PLUGIN = {
  name    = "node",
  version = "1.0.0",
  mirrors = {
    { name="default",     description="Official",              vars={SDK_NODE_MIRROR="https://nodejs.org/dist"} },
    { name="china",       description="NPMMIRROR (China CDN)", vars={SDK_NODE_MIRROR="https://registry.npmmirror.com/-/binary/node"} },
    -- {local_dir} expands to mirror.local_dir config value (default: ~/.sdk/downloads/)
    { name="local",       description="Local directory",       vars={SDK_NODE_MIRROR="{local_dir}/node"} },
    -- {http_server} expands to mirror.http_server config value; SDK_FLAT_MIRROR=1 enables flat path logic
    { name="http-server", description="Local HTTP server",     vars={SDK_NODE_MIRROR="{http_server}/node", SDK_FLAT_MIRROR="1"} },
  },
}
```

Then in the hook, use `is_flat()` to choose between flat and hierarchical URL structure:

```lua
local function is_flat(path)
  return path:sub(1, 4) ~= "http" or os.getenv("SDK_FLAT_MIRROR") == "1"
end

function PLUGIN:PreInstall(ctx)
  local mirror = os.getenv("SDK_NODE_MIRROR") or "https://nodejs.org/dist"
  local filename = "node-v" .. ctx.version .. "-linux-x64.tar.xz"
  local url
  if is_flat(mirror) then
    url = mirror .. "/" .. filename          -- flat: mirror/filename
  else
    url = mirror .. "/v" .. ctx.version .. "/" .. filename  -- hierarchical
  end
  return { version = ctx.version, url = url }
end
```

---

## Plugin compatibility

`sdk` uses the same Lua plugin format as vfox. Any plugin from the [vfox plugin registry](https://github.com/version-fox/vfox-plugins) works directly:

```bash
sdk add node    https://github.com/version-fox/vfox-nodejs
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
    node/            ← Lua plugin
    python/
  cache/
    node/
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
