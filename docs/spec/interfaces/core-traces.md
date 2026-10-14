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

Compilation and admission are separate observations. A plan-only invocation
does not record admission. Output delivery records the writer attempt, not the
operation outcome: a refusal can be delivered successfully, and a successful
operation can have failed output delivery. None of these observations rewrites
the retained Run outcome or proves an uncertain transaction committed.

Capture keeps the first 256 and newest 768 events, with explicit observed and
omitted counts. Per-phase aggregate counts and elapsed totals remain bounded by
the closed phase vocabulary. Normal detail omits individual internal phase
events while keeping their aggregates; detailed mode includes those events.
Counter exhaustion MUST stop capture with an explicit saturation flag.

Internal observations include actual source-read attempts, managed-output
validation and staging, and publication durability barriers. A platform adapter
that performs no separate directory-sync call does not invent a successful
directory-sync observation. These observations never retain source bytes or
backend error messages, and never change the publication truth boundary.

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

**Verification: PR-TEST-0247, PR-TEST-0263, PR-TEST-0312, PR-TEST-0404, PR-TEST-0788, PR-TEST-0793.**

**Verification: PR-TEST-0106, PR-TEST-0107, PR-TEST-0804, PR-TEST-0805, PR-TEST-0806.**

### PR-REQ-0398 - Non-authoritative journals and retention

The diagnostic journal uses a separate SQLite file and application marker. It
MUST NOT change the management database's durability, schema, ownership markers or
primary operation transactions. Existing foreign files, unsupported journal schemas and unsafe
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

The Store limit is one combined budget for phase documents and retained
Hook/helper events, not a separate allowance for each class. Table-backed events
are charged as UTF-8 JSON arrays of their stored scalar fields, including the
Run key; collection status and observed-count rows remain Run metadata. This
private accounting representation is not an export format. The Store maintains
derived scalar charges in indexes, so accounting does not reserialize all
retained text on each pass. Unknown event times
do not acquire invented expiry dates; they remain eligible for capacity eviction.
Reclamation preserves collection counters so an emptied retained window still
reports omissions instead of pretending the Run originally had no evidence.

Evidence writers coordinate separately from primary operations. Table-backed
reclamation uses admitted diagnostic transactions with at most 64 selected event
keys per transaction. It does not bootstrap or upgrade a Store, change WAL/FULL
durability, perform abandoned-session cleanup, or acquire runtime publication
coordination. Interrupted or unavailable maintenance is best effort; a later
write can continue reclamation without replaying the operation.

Phase evidence is persisted before attempting combined Store maintenance, so an
unavailable management database does not itself prevent retaining the trace.
The trace's `retention` observation is `not_checked`, `complete`, `incomplete` or
`unavailable`. It describes the policy attempt for that window, not the operation
outcome, a current global Store measurement, or a guarantee of continued
availability. A missing observation is `not_checked`, never inferred success.
If combined maintenance fails, the phase journal still attempts its own bound;
it MUST NOT label that narrower result as successful combined retention.
Conversely, an unavailable phase file does not by itself stop recording
Hook/helper events in a usable management database. Table-backed recording and
its narrower bound may continue, while combined maintenance remains unavailable.
Neither backend's maintenance failure changes the Run outcome or authorizes replay.

An individual document is limited to 524,288 bytes. Reading a damaged oversized
document MUST NOT allocate its advertised size. Stored records are validated as
typed data; parsing errors MUST NOT echo their contents. A journal failure or
missing record is explicitly unavailable, not a fabricated empty successful trace.

**Verification: PR-TEST-0755, PR-TEST-0756, PR-TEST-0757, PR-TEST-0758, PR-TEST-0759, PR-TEST-0760, PR-TEST-0762, PR-TEST-0763, PR-TEST-0764.**

**Verification: PR-TEST-0795, PR-TEST-0796, PR-TEST-0797, PR-TEST-0798, PR-TEST-0799, PR-TEST-0800, PR-TEST-0801, PR-TEST-0802, PR-TEST-0803.**

**Verification: PR-TEST-0807, PR-TEST-0808.**

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

**Verification: PR-TEST-0756, PR-TEST-0759, PR-TEST-0760, PR-TEST-0761, PR-TEST-0762, PR-TEST-0763, PR-TEST-0793.**

**Verification: PR-TEST-0796, PR-TEST-0800.**

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
