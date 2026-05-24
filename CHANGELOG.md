# Changelog

All notable changes to this project will be documented in this file.

## [0.5.0] - 2026-05-24

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
