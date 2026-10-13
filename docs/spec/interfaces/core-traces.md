---
title: Core Diagnostic Traces
---

# Core Diagnostic Traces

Core traces explain Pactrun's own phases and timings. They are non-authoritative
diagnostic evidence, not Run outcomes, recovery permissions, backups or captured
terminal streams.

### PR-REQ-0397 - Bounded typed phase evidence

An enabled invocation MUST record typed begin/end phase observations with local
sequence numbers and monotonic elapsed microseconds. A successful phase does not
prove that its containing operation committed. Dropping an unfinished phase
records an unknown result, never success. Timing totals may overlap for nested
phases and MUST NOT be presented as exclusive wall-clock fractions.

Capture keeps the first 256 and newest 768 events, with explicit observed and
omitted counts. Per-phase aggregate counts and elapsed totals remain bounded by
the closed phase vocabulary. Normal detail omits individual internal phase
events while keeping their aggregates; detailed mode includes those events.
Counter exhaustion MUST stop capture with an explicit saturation flag.

Trace records contain only closed operation/phase/result classifications, opaque
identities, counters and timestamps. They MUST NOT contain paths, arguments,
Input/Secret values, raw operating-system messages, Hook free text or terminal
bytes. A trace ID is not a Run ID or proof of Run acceptance. Association with a
Run requires the durable acceptance notification. Historical absence is not
filled from current state.

Producer product version and build target are recorded at capture. Source commit
and manifest identity are included only when supplied as qualified build metadata;
missing build provenance is not inferred from the current checkout or exporter.

**Verification: PR-TEST-0753, PR-TEST-0754, PR-TEST-0755, PR-TEST-0760, PR-TEST-0762, PR-TEST-0763, PR-TEST-0764.**

**Verification: PR-TEST-0788.**

### PR-REQ-0398 - Non-authoritative journals and retention

The diagnostic journal uses a separate SQLite file and application marker. It
MUST NOT change the management database's durability, schema, ownership or
transactions. Existing foreign files, unsupported journal schemas and unsafe
filesystem objects are refused, not repaired or overwritten. Reads MUST NOT
create journal files or directories. Journal writes are best effort, use bounded
contention waits, and MUST NOT change an operation's result or cause replay.

Directory qualification is iterative and limited to 256 ancestors. Paths beyond
that bound are unavailable for diagnostics; they do not change Store selection.

Retention limits measure retained serialized diagnostic document bytes, not
physical database size. Expired documents are removed first; oldest documents
are then removed to meet capacity, preferring the current update. Reclamation
does not promise secure erasure or immediate filesystem shrinkage. Limits can
apply to older alpha diagnostics without retaining a parallel policy. Diagnostic
retention MUST NOT delete Runs, Artifacts, Inputs, service data or authoritative
recovery/deletion facts. Trace references do not pin retained documents.

An individual document is limited to 524,288 bytes. Reading a damaged oversized
document MUST NOT allocate its advertised size. Stored records are validated as
typed data; parsing errors MUST NOT echo their contents. A journal failure or
missing record is explicitly unavailable, not a fabricated empty successful trace.

**Verification: PR-TEST-0755, PR-TEST-0756, PR-TEST-0757, PR-TEST-0758, PR-TEST-0759, PR-TEST-0760, PR-TEST-0762, PR-TEST-0764.**

### PR-REQ-0399 - Invocation capture and journal placement

Help, version and diagnostic/configuration inspection MUST NOT create diagnostic
files or perform retention. Ordinary invocations capture their configuration,
resolution, command and output phases; execution adapters add their own phases.
Capture MUST not perform file/database I/O on execution producer threads.

Unassociated invocation evidence uses the user journal. Its Windows location is
`%LOCALAPPDATA%/<entry>-diagnostics/diagnostics.sqlite3`; Linux uses
`$XDG_STATE_HOME/<entry>-diagnostics/diagnostics.sqlite3`, falling back to
`$HOME/.local/state/<entry>-diagnostics/diagnostics.sqlite3` only when the state
base is absent. Normal/test entries remain separate. These locations MUST NOT
overlap the selected Store. Unavailable user paths disable persistence with an
explicit failure status; they MUST NOT be replaced with the selected Store.

After durable Run acceptance, the invocation may publish its complete captured
window to `<store>/database/diagnostics.sqlite3`. This MUST NOT create a missing
Store or its database directory. Store policy applies to associated Run evidence;
startup policy applies to unassociated invocations. Neither path captures raw
Hook text or replaces the existing Hook/helper diagnostic collections.

The producer closes explicitly. The writer may publish bounded intermediate
windows and a final window; shutdown waits are bounded. An unclosed stored window
has an unknown tail. Inspection distinguishes capture closure, omissions and
persistence availability. A local diagnostic ID alone does not guarantee that
its document was successfully persisted or remains within retention.

Normal machine responses may contain `diagnostic_id` identifying the capture
attempt. Human failures show the same identity. This is not a persistence receipt:
the producer may still be draining when a response is written, and collection or
retention can leave the record unavailable. Inspect the retained document for
closure and omissions; never infer operation success from the reference.

**Verification: PR-TEST-0756, PR-TEST-0759, PR-TEST-0760, PR-TEST-0761, PR-TEST-0762, PR-TEST-0763.**

## Inspection

```text
pactrun diagnostic list [--startup] [--limit <1..100>]
pactrun diagnostic show <diagnostic-id> [--startup]
pactrun diagnostic show --run <complete-run-id>
```

List returns a bounded newest-first window (20 by default) and `has_more` when
older entries are outside that window. Show requires a full diagnostic identity
or a full Run identity, not a guessed prefix. Startup inspection does not load
the selected Store or runtime configuration; it remains usable when those are
unavailable. These commands do not capture their own inspection into the journal.
Missing records are unavailable, never evidence that an operation did not run.
