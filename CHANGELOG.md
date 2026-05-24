# Changelog

All notable changes to this project will be documented in this file.

## [0.5.6] - 2026-05-24

### Changed
- Removed all references to vfox from the codebase and documentation
- Lua runtime globals renamed: `VFOX_NAVIGATOR` → `SDK_NAVIGATOR`
- Lua module renamed: `vfox.strings` → `sdk.strings`
- User-Agent changed from `vfox/<version>` to `sdk/<version>`
- Internal env var renamed: `VFOX_PYTHON_USE_UV_BUILD` → `SDK_PYTHON_USE_UV_BUILD`
- Default plugin registry URL cleared (was pointing to vfox registry)
- README rewritten: removed vfox comparison table, updated plugin compatibility section
- `sdk` is now a fully independent tool



### Changed
- `sdk plugin update` now updates built-in plugins (installed via `sdk plugin init`)
  by re-extracting the embedded Lua files from the current binary — no network needed
- Plugins with `.git` still use `git pull` as before
- Plugins with no `.git` that are not built-in: explicitly reported as skipped

## [0.5.4] - 2026-05-24

### Added
- `sdk fix [sdk] [--yes]` — scan installed versions and remove broken ones:
  - Detects linked versions (`.link` file) whose target path no longer exists
  - Detects incomplete installs where the runtime directory is missing
  - Default is dry-run mode (reports what would be removed)
  - Pass `--yes` to actually delete broken version directories



### Changed
- `sdk plugin init` is now **fully offline** — plugin Lua files are embedded in the
  SDK binary at compile time via `include_str!`; no network access required
- Removed `tempfile` dependency (no longer needed)
- `assets/plugins/` directory added to the SDK repository: contains the bundled Lua
  files for all 7 official plugins (java, node, python, go, gradle, maven, rust)
- To update bundled plugins: copy new Lua files into `assets/plugins/` and rebuild



### Added
- **Built-in plugin registry**: all 7 official plugins (`java`, `node`, `python`, `go`,
  `gradle`, `maven`, `rust`) are now known built-ins
- **`sdk plugin add <name>`** (without source): installs a built-in plugin directly from
  the official GitHub repository using sparse checkout — no need to know the URL
- **`sdk plugin init [names...]`**: batch-initialize built-in plugins; omit names to
  install all 7 at once, or specify names to install only those (e.g. `sdk plugin init java node`)
- Already-installed plugins are skipped with an `ℹ` notice

## [0.5.1] - 2026-05-24

### Added (SDK)
- `PreInstallResult` now supports a `fallback_url` field: when a mirror download fails archive
  validation, the SDK automatically retries with the fallback URL and prints a clear message

### Changed (SDK)
- Archive validation: if the primary URL returns an invalid archive, the fallback URL is tried
  before reporting an error

### Changed (Plugins — java)
- **Adoptium source**: `sdk available java` now shows full patch versions (`17.0.19 LTS`)
  instead of just major version numbers; achieved by querying `/assets/latest/{major}/hotspot`
  per version
- **Adoptium china mirror** (`SDK_JAVA_MIRROR` set to a hierarchical HTTP mirror):
  `sdk available java` now lists only the versions actually present in the mirror directory
  (avoids showing versions that would 404 on download)
- **Adoptium `pre_install`**: correctly extracts major version when called with full version
  string (e.g. `sdk install java@17.0.19`); installed version now reflects actual semver
  returned by API
- **Zulu source**: `pre_install` sets `fallback_url` to the official Azul CDN when a mirror is
  configured, enabling automatic fallback if the mirror fails
- Removed `zulu-china` mirror profile — Huawei Cloud no longer serves Zulu JDK files at the
  expected URL; use `sdk mirror java china` (Adoptium/Tsinghua) for China-accessible Java
- `china` mirror profile description updated to "recommended for users in China"

### Changed (Plugins — gradle)
- Fixed `\u2014` Unicode escape in `metadata.lua` (Lua does not support `\u` escapes) — the
  Tencent and Huawei mirror profiles previously caused a Lua syntax error on load
- `sdk available gradle` now shows stable versions sorted descending by version number; RC /
  milestone versions are grouped at the end of the list

### Changed (Plugins — go)
- Removed `ustc` mirror profile — USTC's Go mirror redirects to `dl.google.com`, which is
  blocked in mainland China
- Fixed `aliyun` mirror profile: added `SDK_GO_FLAT=1` (Aliyun uses flat directory structure
  without a `/dl/` subdirectory)
- Added `SDK_GO_API` variable to separate version-listing endpoint from download mirror;
  `sdk available go` now uses `SDK_GO_API` (defaults to `https://go.dev`) regardless of
  whether the download mirror is flat



### Added
- `sdk plugin` subcommand for plugin management (`add`, `remove`, `update`, `list`, `info`)
  - Old top-level `sdk add` / `sdk remove` / `sdk update` / `sdk info` kept as hidden aliases for backward compatibility
  - `sdk plugin update [name]` supports updating a single plugin by name
  - `sdk plugin list` lists all installed plugins
- `sdk mirror` now launches an interactive TUI (Select picker) when run in a TTY
  - Step 1: select plugin (or "All plugins"), showing current active profile
  - Step 2: select profile with `✓` marker on the current selection
  - Falls back to plain text output in non-TTY environments (pipes, scripts)
- All command aliases are now visible in `sdk -h` output (`[aliases: i]`, `[aliases: ls]`, etc.)
- `sdk config --help` now lists all 13 valid configuration keys with descriptions
- `sdk config` output includes a usage hint at the bottom

### Fixed
- Shell hook (`sdk hook bash` / `sdk hook zsh`): `PROMPT_COMMAND` / `precmd` hook is now
  registered in **every new shell**, not just the first one. Previously, `__SDK_INITIALIZED`
  was exported and inherited by sub-shells (e.g. VS Code integrated terminals), causing the
  hook to be skipped and `sdk use` to have no effect on `PATH`.

### Changed
- `sdk doctor`: local plugins (added via local path, no `.git`) now show an `ℹ` info line
  instead of a `✗` warning — not having `.git` is expected for local installs
- `sdk doctor`: PATH check now distinguishes between "no SDKs activated" and "global SDK
  active but not yet on PATH (open a new terminal or re-eval the hook)"
- Command examples updated throughout: `nodejs` → `node`, version `20.0.0` → `22.16.0`
- README command reference table reorganised to reflect `sdk plugin` subcommand

## [0.4.0] - 2026-05-24

- TBD: describe changes
