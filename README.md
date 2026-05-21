# sdk

A fast, cross-platform SDK version manager — a Rust reimplementation of [vfox](https://github.com/version-fox/vfox) with one key optimization: **no symlinks in project directories**.

## How it works

| Original vfox | sdk (this project) |
|---|---|
| `use node@20` → writes `.vfox.toml` **+ creates `.vfox/sdk/node` symlink** | `use node@20` → writes **only** `.sdk.toml` |
| Activation reads symlink | Activation reads `.sdk.toml` → resolves `~/.sdk/cache/` directly |

All runtimes are stored in `~/.sdk/cache/`. Projects only ever contain a single `.sdk.toml` file.

## Installation

```bash
cargo install --path .
```

## Shell activation

Add this to your shell profile so `sdk` automatically sets env vars when you `cd` into a project:

**Bash** (`~/.bashrc`):
```bash
eval "$(sdk activate bash)"
```

**Zsh** (`~/.zshrc`):
```zsh
eval "$(sdk activate zsh)"
```

**Fish** (`~/.config/fish/config.fish`):
```fish
sdk activate fish | source
```

**PowerShell** (`$PROFILE`):
```powershell
Invoke-Expression (sdk activate pwsh)
```

**Nushell** (`config.nu`):
```nu
sdk activate nu | save -f ~/.sdk-init.nu
source ~/.sdk-init.nu
```

## Usage

```
sdk <COMMAND>

Commands:
  install      Install a SDK version          e.g. sdk install nodejs@20.0.0
  use          Set active version             e.g. sdk use nodejs@20.0.0
  uninstall    Uninstall a SDK version        e.g. sdk uninstall nodejs@20.0.0
  unuse        Remove SDK from active config
  list         List installed SDK versions
  current      Show currently active versions
  available    List available versions        e.g. sdk available nodejs
  search       Search plugin registry         e.g. sdk search node
  add          Add a plugin                   e.g. sdk add nodejs https://github.com/version-fox/vfox-nodejs
  remove       Remove a plugin
  update       Update all installed plugins
  info         Show info about an SDK plugin
  exec         Run command with specific SDK  e.g. sdk exec nodejs 22.16.0 -- node --version
  completions  Generate shell completion script
```

### Scope flags for `use`

```bash
sdk use nodejs@20.0.0           # project scope (writes .sdk.toml in current dir)
sdk use nodejs@20.0.0 --global  # global scope  (~/.sdk/.sdk.toml)
sdk use nodejs@20.0.0 --session # session scope  (current shell only)
```

## Directory layout

```
~/.sdk/
  .sdk.toml          ← global version config
  config.yaml        ← user settings
  plugin/
    nodejs/          ← Lua plugin (compatible with vfox plugins)
  cache/
    nodejs/
      v-20.0.0/
        nodejs-20.0.0/   ← actual runtime (no symlinks!)

<project>/
  .sdk.toml          ← ONLY artifact in project directory
```

## Shell completions

Generate and install tab-completions for your shell:

**Bash** (add to `~/.bashrc`):
```bash
eval "$(sdk completions bash)"
# or write to a file for faster startup:
sdk completions bash > ~/.local/share/bash-completion/completions/sdk
```

**Zsh** (add to `~/.zshrc`):
```zsh
sdk completions zsh > "${fpath[1]}/_sdk"
```

**Fish**:
```fish
sdk completions fish > ~/.config/fish/completions/sdk.fish
```

**PowerShell** (add to `$PROFILE`):
```powershell
sdk completions powershell | Out-String | Invoke-Expression
```

## Plugin compatibility

`sdk` uses the same Lua plugin format as vfox. You can install any plugin from the [vfox plugin registry](https://github.com/version-fox/vfox-plugins):

```bash
sdk add nodejs https://github.com/version-fox/vfox-nodejs
sdk add python https://github.com/version-fox/vfox-python
sdk add java https://github.com/version-fox/vfox-java
```

## License

Apache-2.0
