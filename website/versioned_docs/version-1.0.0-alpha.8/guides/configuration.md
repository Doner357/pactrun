---
title: Inspect Pactrun configuration
---

# Inspect Pactrun configuration

Pactrun separates user presentation preferences from Store diagnostic defaults.
Configuration does not grant Hook access or authorize sensitive exports. Keep
Input values and credentials out of configuration files.

## Validate candidate files

Use explicit paths to check files before placing them at their default locations:

```text
pactrun config validate --user-file ./user.toml --store-file ./store.toml
pactrun --format json config show --user-file ./user.toml --store-file ./store.toml
```

Both files must exist when explicitly selected. The commands do not copy or
install them, open the management database, initialize a Store, or run cleanup.
Each effective setting reports its value and whether it came from a default,
user file, Store file or command-line override. Invalid files fail rather than
silently selecting defaults; error messages do not echo their contents.

Example user file:

```toml
version = 1

[presentation]
verbosity = "normal"
phase_messages = true

[startup_diagnostics]
level = "normal"
retention_days = 30
max_bytes = 67108864
```

Example Store file:

```toml
version = 1

[diagnostics]
level = "normal"
retain_hook_text = true
retention_days = 30
max_bytes = 268435456
```

## Discover defaults

Without explicit file selectors, `config show` uses the platform user config
location and `pactrun.toml` under the selected Store. Missing files use built-in
defaults. Selecting a Store still follows [data locations](./data-location.md);
a configuration file cannot move or select it. Pactrun does not search the
current working directory or a Pack for configuration.

## Override one invocation

Place global selectors before the command. They can be combined with the
ordinary output-format selector:

```text
pactrun --config ./user.toml --verbosity detailed config show
pactrun --format json --diagnostic-level detailed config show
pactrun --no-phase-messages instance list
```

Execution commands additionally accept either `--retain-hook-text` or
`--no-retain-hook-text`. Omit both to use the Store default. Repeating or
combining them is an error. An explicit selection works in either direction;
it never rewrites the Store configuration or changes live Hook output.

Settings are fixed for an invocation. Editing a file while a Hook is running
affects subsequent invocations, not that Hook's diagnostic collection. Quiet
presentation reduces optional stage chatter, but preserves raw Hook bytes,
failure diagnostics, uncertainty/retry warnings and recovery information.
JSON/JSONL records are independent of Human presentation preferences.

Help and version queries do not read configuration. If a default file is invalid,
use `config validate` with explicit candidate files to inspect replacements
without opening or repairing the Store. Do not specify both `--config` and
`--user-file` in the same inspection command.

See the [configuration reference](../pactrun-users/reference/configuration.md) for exact
platform paths, setting ownership and allowed ranges. Hook-authored text can
contain sensitive information even when it is labeled as diagnostic output.
