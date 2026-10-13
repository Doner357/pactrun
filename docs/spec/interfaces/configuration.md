---
title: Pactrun Configuration
---

# Pactrun Configuration

Configuration supplies invocation defaults, not Hook authority, recovery
permission, or permission to export sensitive data. Store selection remains
owned by the existing executable/environment contract.

### PR-REQ-0393 - Explicit configuration ownership and discovery

The user TOML document owns `presentation` and `startup_diagnostics`. The Store
TOML document owns `diagnostics`. Both accept `version = 1`; omitted version is
1. Unknown keys, duplicate keys, invalid types and unsupported versions MUST be
refused. An empty document selects defaults. No current-directory or Pack
configuration discovery is permitted.

The default user file is `%APPDATA%/<entry>/config.toml` on Windows, or
`$XDG_CONFIG_HOME/<entry>/config.toml` on Linux, falling back to
`$HOME/.config/<entry>/config.toml` only when XDG_CONFIG_HOME is absent. `<entry>`
is `pactrun-test` only for the actual test executable, otherwise `pactrun`.
Configured platform bases MUST be absolute. The Store file is `pactrun.toml`
under the already selected Store root. It cannot select or relocate that Store.

An absent default file selects defaults; an unreadable or malformed existing
file MUST NOT silently select defaults. Explicit user/Store file selectors
require the selected file to exist and do not select a different Store.
Configuration reads MUST NOT initialize a Store, create directories, or run
retention. Symlink/reparse-point configuration files MUST be refused. Documents
are UTF-8 and limited to 65,536 bytes inclusive, including for stream reads.
Error classification MUST NOT echo source text or parsed values that might
accidentally contain secrets.

**Verification: PR-TEST-0738, PR-TEST-0741, PR-TEST-0742, PR-TEST-0743, PR-TEST-0744.**

### PR-REQ-0394 - Typed settings and bounded diagnostic defaults

The following table defines the complete initial setting vocabulary. Byte
counts are integers, not strings with suffixes. Unknown enum values are invalid.

| Scope | Key | Default | Valid values |
| --- | --- | --- | --- |
| User | `presentation.verbosity` | `normal` | `quiet`, `normal`, `detailed` |
| User | `presentation.phase_messages` | `true` | boolean |
| User | `startup_diagnostics.level` | `normal` | `normal`, `detailed` |
| User | `startup_diagnostics.retention_days` | `30` | integer 1..3650 |
| User | `startup_diagnostics.max_bytes` | `67108864` | integer 1048576..1073741824 |
| Store | `diagnostics.level` | `normal` | `normal`, `detailed` |
| Store | `diagnostics.retain_hook_text` | `true` | boolean |
| Store | `diagnostics.retention_days` | `30` | integer 1..3650 |
| Store | `diagnostics.max_bytes` | `268435456` | integer 1048576..1073741824 |

Retention settings apply to eligible diagnostic evidence, never to Run outcomes,
Input payloads, Artifacts, service state or unresolved recovery/deletion facts.
These defaults are policy choices, not measured storage/performance guarantees.
This vocabulary does not expose SQLite durability, process ownership, timeouts,
protocol limits, Input capacity or sensitive-export authorization.

**Verification: PR-TEST-0738, PR-TEST-0739, PR-TEST-0740.**

### PR-REQ-0395 - Invocation settings and effective source inspection

Each normal invocation MUST resolve one immutable settings value before applying
the settings. Explicit CLI values override the owning file's value; omitted CLI
values preserve that value. There is no Store policy-lock layer. No setting is
duplicated between user and Store documents, and the initial version defines no
setting-value environment overrides. Existing environment Store selection is
unchanged. Later file edits affect later invocations, not an active operation.

Effective inspection MUST identify each setting's value and source: `default`,
`user_file`, `store_file`, or `command_line`. A setting absent from a loaded file
still has source `default`. Inspection and validation MUST not create or upgrade
a Store or apply retention. File selection paths are not stored diagnostic text.

**Verification: PR-TEST-0738, PR-TEST-0739, PR-TEST-0741, PR-TEST-0744, PR-TEST-0745; Pending automated coverage.**

### Configuration inspection commands

```text
pactrun config show [--user-file <path>] [--store-file <path>]
pactrun config validate [--user-file <path>] [--store-file <path>]
```

Both commands resolve the same typed settings and return `valid: true` plus
`settings` on success. Every leaf setting has `value` and `source` members.
Invalid documents produce a nonzero result without modifying either file or
the selected Store. Explicit file selectors inspect candidate documents without
installing them or changing the selected Store. Human, JSON and JSONL results
use the ordinary CLI presentation contract. Machine configuration commands
require interface `1.0-alpha.4`; an explicit legacy selection is refused before
configuration acquisition with a null command in the legacy error envelope.
