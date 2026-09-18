---
title: Revision Core Format V3
---

# Revision Core Format V3

**Status: Frozen normative Package contract specification.**

The operator approved the [Shell Loader baseline](../../development/design-notes/shell-adapter-loader-baseline.md).
This contract does not amend Frozen Core V1/V2 or Hook Protocol V1/V2.
The [format review](../../development/design-notes/shell-loader-format-activation-review.md)
records the conformance, platform and scoped integration evidence. Freeze changes
status only; an identity-affecting change now requires a new Core format.
It does not authorize a product release or Git integration.

<!-- spec-navigation:start -->
Read [Core V2](./revision-core-format-v2.md) for unchanged service semantics and
the [implementation record](../../development/shell-adapter-loader-status.md)
for actual availability. This document owns only the explicit V3 delta.
<!-- spec-navigation:end -->

### PR-REQ-0348 - Explicit shell-loader identity and launch

V3 retains all Core V2 fields and validation, changes `format_version` to 3,
and adds exactly this alternative at every Hook launch position:

```text
{ kind: shell_loader,
  shell: sh | bash | powershell_7 | windows_powershell_5_1,
  command: HostExecutableName,
  script: ContentId }
```

All fields are required; unknown fields are rejected. The script MUST reference
an owned regular runtime file; its executable bit does not control launch.
Existing Hook args, I/O, protocol selection and service authorities retain their
meanings. All declaration fields bear identity. Canonicalization retains V2's
rules; the revision digest frame uses independent U32BE version 3 with the same
component labels and lengths. The current Pactrun executable path is a host fact,
not a Core field or identity input. Core V1/V2 MUST reject shell_loader.

POSIX supports sh and bash; Windows supports powershell_7 and
windows_powershell_5_1. Other host/shell pairs MUST fail admission, not fall back.
Host executable resolution and admission revalidation retain the exact selected
path rules. Loader execution MUST use the running Pactrun executable without
requiring an installed PATH alias or copying Pactrun into the Pack.

**Verification: PR-TEST-0486, PR-TEST-0487, PR-TEST-0492, PR-TEST-0493, PR-TEST-0510.**

## YAML projection

Pack Source YAML V3 explicitly uses plain raw `source_format: 3` and otherwise
retains YAML V2's closed schema, defaults, scalar handling and source acquisition.
Its revision projection inserts `format_version: 3`. There is no inferred upgrade
of a V1/V2 source or installed Revision, and no interpreter argument override on
the shell_loader alternative. YAML remains Candidate, not a public compatibility
promise. Availability is tracked in the [implementation record](../../development/shell-adapter-loader-status.md).
