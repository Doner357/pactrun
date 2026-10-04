---
title: E S0 Whole-Milestone Design and Acceptance Plan
---

# E S0 whole-milestone design and acceptance plan

**Status: Whole E milestone implementation authorized on 2026-09-26.**

The operator approved continuous execution through every E slice, integration,
verification, repair and documentation closeout, then local merge into develop.
The current implementation branch is feature/e-versioning-design, carrying the
verified executable-lifetime fix. The confirmed
[decision record](./e-versioning-redesign-decisions.md) governs product intent;
this plan supplies the concrete engineering baseline, not a second product Spec.

## Approved scope and boundaries

- Deliver the complete locally validated product 1.0.0-alpha.1 baseline and all
  eight 1.0-alpha.1 contracts. E completion is not formal 1.0.0 qualification.
- Apply the one-time clean reset to original development formats, schema ladders,
  compatibility dispatch and obsolete-only tests. Preserve shared product
  functionality and safety, not historical support. New baseline creation does
  not require a converter or preserving old object identity/service continuity.
- Actual deletion of pre-existing data/service resources remains separately
  scoped. This grants no reason to keep their obsolete readers. Isolated test
  roots can be created, reset and cleaned as part of validation.
- Use Scoop on Windows and Homebrew on Linux, with project-owned package source
  definitions and locally rehearsed artifacts. Do not build a custom installer,
  resident updater or independent alpha/beta/rc channel service.
- Target Windows 11 and qualified Linux x86-64 environments. No extra Windows 10,
  old-Linux, musl, macOS or ARM64 compatibility project is included. Declare only
  environments supported by actual evidence, not invented untested OS minima.
- Official-origin artifact hashes and exact build provenance are in scope;
  independent signing/certificate infrastructure and fixed LTS/SLA promises are
  not. Formal publication needs its own readiness/trust review.
- Local task commits and merge to develop are authorized. Preserve unrelated
  dirty files. No push, remote tag/Release/bucket/tap publication, Pages workflow
  work, deployment or broad document/page restructuring is authorized. Those are
  deferred to the operator's later publication/documentation phase.
- Necessary owning Specs, format definitions, test traceability and installation
  references remain E deliverables. Documentation deferral cannot remove them.

Slices are internal work boundaries, not new approvals. Resolve ordinary design,
implementation and verification failures autonomously. Pause only for a genuine
product conflict, additional authorization or unavailable external prerequisite.
Already approved product decisions must not be reopened as implementation choices.

## 1. Version representation and support boundaries

Use one crate-private value parser for the shared numeric/prerelease components,
with distinct ProductVersion and FormatVersion types. Product has three numeric
components; formats have two. Published baseline identifiers use ASCII decimal
components without leading zeroes, and optionally `-alpha.N`, `-beta.N` or
`-rc.N`, with positive N and no leading zeroes. No whitespace, `v` prefix,
wildcards or numeric JSON coercion in format fields. Bound inputs to 128 bytes
and numeric components to unsigned 64-bit values; reject overflow. Product
release tags use exactly `v` followed by the product identifier. Build metadata
is not emitted in published identifiers or used to choose between assets.

Comparison is numeric by components; prereleases precede the corresponding
formal version and alpha precedes beta precedes rc, with numeric N comparison.
Thus `1.1.0-alpha.1` can follow `1.0.0` only when prereleases are allowed.
Parsing an identifier is not a support grant. Unknown well-formed identifiers
produce unsupported-contract errors, not nearest-version fallback. Malformed
identifiers produce invalid-representation errors. Product SemVer parsing used
for release discovery must distinguish valid but non-published tag shapes;
ignore those as candidates rather than changing format grammar.

| Domain | Proposed representation | Baseline implementation boundary |
| --- | --- | --- |
| Pack source | Required YAML `source_format: "1.0-alpha.1"` | Strict acquisition/authoring frontend; combine current Source V3 capabilities and existing simple defaults. Authors do not specify Revision canonical version. |
| Revision canonical | Required JSON `format_version: "1.0-alpha.1"` | Strict canonical codec; full current V3 semantics, including service declarations and shell launch. Runtime closure is governed by this version, not a ninth domain. |
| Hook Protocol | Hook declaration and session_start/session_ready `protocol_version: "1.0-alpha.1"` | Compiler selects the declared supported version; one baseline transport/Session codec includes current V2 authority plus all existing operation kinds. |
| Snapshot content/integrity | Manifest `format_version: "1.0-alpha.1"` | Strict canonical manifest codec retains current V2 managed bindings and service content. |
| Snapshot bundle | `bundle.json`: existing kind, string `bundle_version`, string `integrity_format`, integrity_digest | Independent bounded Stored-ZIP decoder; envelope version and contained manifest version must match their declarations. |
| Pack distribution | `pactrun-distribution.json`: existing kind, string `format_version`; add string `revision_format` | Container decoder checks the inner Core version explicitly before publication; preserve portable metadata's non-identity boundary. |
| CLI machine interface | `format: "pactrun.cli"`, `format_version: "1.0-alpha.1"`; retain existing result fields and event `type` discriminators | One shared envelope version for JSON results, JSONL events and errors. No independently versioned event format. |
| Persistence | Singleton metadata row with `format_version TEXT`, exactly `1.0-alpha.1` | Root admission verifies application marker, metadata and exact schema before interpretation or writable opening. SQLite integer pragmas are bootstrap markers, not a public second format version. |

Every read/write/execute capability table initially contains **only** its real
baseline entry. Diagnostic output reports product version and the eight actual
reader/writer/Hook support entries from the same typed definitions used at
admission. No range such as "all 1.x" is inferred from ordering. Future codecs
are added only for actual contracts; dispatch stays at source, canonical,
transport, Session, CLI and persistence boundaries, never in business operations.

Unsupported diagnostics carry domain, encountered version, supported versions,
affected operation and a next action. Errors that occur before reading a usable
version identify that absence rather than inventing a version. No Secrets,
credentials or unbounded input echoing. Preserve typed errors through the CLI.

### Hook selection and framing

An exact declared protocol is selected during side-effect-free compilation.
No capability declaration alone proves implementation support, no peer chooses
a more permissive protocol, and no timeout/mismatch triggers retry at another
version. After admission, both sides must confirm the selected version and
session ID before authority-bearing requests are serviced.

Proposed preamble in each direction:
`ASCII("pactrun.hook-protocol\0") || U16BE(version_byte_length) || ASCII(version)`.
The version length is 1..128; the baseline value is `1.0-alpha.1`.
Frames retain `U32BE(json_byte_length) || UTF8(strict_JSON)` and the existing
16 MiB cap. Reject truncated/oversized preambles before allocating a frame.
Session data retains exact authority, cancellation, ACK, completion and recovery
rules; direct and shell-loader Hooks use the same selected contract. A Hook
failure after accepted Run creation remains an execution outcome, not a claim
that no Run existed. Preflight rejects unsupported declarations before a Run,
lease or Hook is created; runtime mismatch cannot undo acceptance history.

## 2. Canonical bytes, identity and formalization

Keep strict JSON, exact JCS, lexical/path rules, SHA-256, ordered collections
and opaque payload bytes. Replace numeric format fields with the exact string
inside canonical Core/manifest bytes. Do not hash producer product version or
transport ZIP metadata into content identity.

Proposed Revision digest frame:
`ASCII("pactrun.revision-content-digest\0") || ASCII("revision-core\0") ||
U64BE(core_length) || core_JCS || ASCII("runtime-content-closure\0") ||
U64BE(runtime_length) || runtime_JCS`.
Proposed Snapshot digest frame:
`ASCII("pactrun.snapshot-integrity-digest\0") ||
ASCII("snapshot-integrity-manifest\0") || U64BE(manifest_length) || manifest_JCS`.
Remove the redundant old integer version in these frames; the canonical
payload already carries its exact format identity. Preserve distinct domain
labels and length framing. This is an explicitly identity-changing reset,
not permission to reinterpret old digests. Raw payload blob hashes stay unchanged.

Promotion does not edit existing bytes, digests, Snapshot IDs, bindings or
references. New formal writers eventually emit `1.0`. Old `1.0-alpha.1` objects
keep that identifier and their existing identity. A promotion review may add
an explicit accepted reader entry for that precise prerelease representation,
with its original digest algorithm and formal-equivalent behavior proved by
vectors and operation tests. No wildcard "all alpha" acceptance, no treating
different byte encodings as the same digest, and no rebuilding solely because
the producer product was a prerelease. New identical-looking content written
under a different format label can have a different digest; it does not replace
an installed Revision or rebind an Instance automatically.

E freezes baseline vectors and a promotion-review procedure, not a hypothetical
formal codec. Later promotion must compare canonical components, defaults,
identity, Hook authority, integrity and stored references before granting support.

Error owners become semantic names (for example `hook_protocol` and
`revision_core`) with an explicit old-to-new catalog mapping in the Spec change;
retain error code meaning and typed ownership, not historical owner versions.
Keep Migration selector `mp1-` and its hash domain as a product-owned encoding
discriminator unless an actual encoding change is needed. It is not an extra
release domain. Keep selector validation against real edges and availability;
never turn a selector into authority, a pin or a Revision identity.

## 3. Persistence, coordination and recovery

Construct one fresh schema from the effective V11 model, not a replay of eleven
historical schemas. Preserve tables/constraints for immutable content and
references, Instances/state versions, active and retained bindings, Runs/pins,
owner admissions, recovery guards/consequence versions, ServiceStorage and
targets, Snapshots, lifecycle/GC, metadata and diagnostics. Remove historical
ALTER chains, upgrade-only classification and artificial downgrade fixtures.
Keep a checked-in complete DDL and compare the actual sqlite_master/PRAGMA
table, index, foreign-key and constraint shape to it in contract tests.

New roots use `application_id = 0x50414354`, `user_version = 0`, and exactly one
`pactrun_metadata` row keyed by integer 1 with the string `format_version`.
The zero integer is not sufficient for acceptance: the metadata row and full
schema distinguish this baseline from pristine or foreign SQLite files.
Existing V1..V11 databases are refused read-only with fresh-root guidance;
they are never relabeled or modified. Missing/malformed metadata on a populated
database is not permission to initialize. Empty bootstrap and complete baseline
are the only accepted states; interrupted bootstrap must roll back atomically.

Admission order:

1. Validate the supplied root and supported filesystem without creating paths.
   Existing roots are inspected read-only, including schema/format, before
   Pactrun staging directories, owner leases or cleanup. The approved read-only
   SQLite coordination exception does not permit changing existing database/WAL
   content or ignoring committed WAL data. Trace callers
   of StagingSession::prepare, not just SQLite opening: the current adapter can
   receive an already-created Session.
2. For supported writable admission, acquire existing content/collection
   coordination in its established order, prepare a Session, then open a write
   transaction and revalidate format/schema under serialization. An advisory
   precheck alone never grants write authority. New-root initialization uses
   a separately typed creation path and validates concurrent first-open races.
3. Retain admitted-writer Session leases, Instance mutation guards/CAS, durable
   Run pins, consequence versions and publication transactions. Readers do not
   create coordination state. Acquire no service-owned data locks for updates.
4. Check owner liveness by the existing OS lease contract. Unknown/live owners
   are not dead because a version differs. Recovery follows its existing
   operation evidence and ACK boundaries; never seize, force-unlock, replay a
   Hook, or clear a guard to make an update succeed.

All baseline executables use identical lock locations and ownership records.
E introduces **no in-place representation migration**: opening a supported root
does not rewrite it. Concurrent process tests use separately installed copies
of the real baseline and unknown-format negative fixtures, not invented future
codecs. Future private representation changes must prove compatibility with
active owners and service access before being admitted; otherwise defer that
change, not the same-Major compatibility obligation. A mechanism that globally
blocks old operations until users run an upgrade is not an acceptable future
same-Major design.

Program selection changes only later launches. Keep old executable files
available to active processes/descendants; changing a default never changes an
accepted Plan's executable/Hook authority. Recovery after owner loss uses the
persisted operation contract, not whichever codec has the highest version.
No installer action accesses the database or claims to repair recovery state.

## 4. Removal and preservation inventory

This is a symbol/responsibility inventory, not permission to delete whole files
by suffix. S1 records requirement/test mappings before removal; S2-S5 move
shared code and replace tests before removing obsolete declarations.

| Inspected location or family | Remove/consolidate | Preserve and prove |
| --- | --- | --- |
| src/revision_content.rs; src/domain/revision_content.rs | Numeric V1/V2/V3 support dispatch and retired variants | One typed baseline plus narrow future dispatch boundary; immutable identity and validation |
| src/revision_core_v1/, src/revision_core_v2.rs, src/revision_core_v3.rs; corresponding domain modules | Separate old canonical codecs and V3 delegation through V2/V1 | Strict JSON/JCS, runtime closure, current service semantics, shell launch, hashing and validated types in unversioned implementation modules |
| src/authoring.rs, src/authoring_v2.rs, src/authoring/source_acquisition.rs | Source V1/V2/V3 selection and projection of one old numeric Core to another | Simple source defaults, secure acquisition, service declarations, explicit transitions and Source V3 shell support |
| src/hook/protocol.rs, protocol_v2.rs, versioned_protocol.rs | Old integer preambles, old-only Session decoders and dual message variants | Transport limits, cancellation, all five operation families, V2 service authority, ACK order, completion and shell-loader/session behavior |
| src/snapshot_integrity.rs; Snapshot domain types | Integrity V1/V2 dispatch | V2 content closure, exact producer validation, cryptographic integrity and bounded streaming |
| src/snapshot_bundle.rs, src/pack_transport.rs, src/pack_zip.rs | Numeric envelope versions and implicit inner-version assumptions | Hostile ZIP preflight, cancellation, bounded acquisition, portable metadata, atomic no-clobber publication |
| src/persistence/sqlite_revision_store.rs; persistence_schema_v2..v11_additions.sql | SCHEMA_LADDER, historical DDL constants, old schema classes and bootstrap-by-upgrade | Fresh consolidated schema, strict shape checking, read-only refusal, WAL/transaction safety and runtime publication |
| src/persistence/sqlite_v9.rs; sqlite_v6/v7/v8.rs; upgrade fixtures | upgrade_storage, original-schema upgrade-only code and synthetic rollback constructors | Move any remaining shared production helper before removing its old home |
| src/persistence/sqlite_v5.rs | Legacy-schema handling and upgrade-only quiescence scans | open_writer_database, current writer admission, owner liveness, consequence-version helpers and Run validation remain production responsibilities |
| src/application.rs and src/cli.rs storage upgrade path | Legacy upgrade command, help and upgrade-required guidance | Actionable unsupported Persistence diagnostics; Pack-defined Migration remains a product operation |
| src/cli/presentation.rs and execution/transport projections | pactrun.cli.v1 and separately versioned event labels; integer version projections | Typed identity, success/failure/partial results, payload-safe stdout, JSONL terminal events and output-failure cancellation |
| tests/vectors/, tests/oracles/, xtask/src/ format verifiers | Old-format acceptance vectors/codec runners once new baseline coverage replaces them | Independent Node/Rust byte and digest agreement; negative parsing/ZIP/ownership cases; stable surviving test IDs |
| Error catalog and Migration selectors | Historical error-owner suffixes and catalog-as-release-version implication | Exact owner/code mapping, semantic error coverage; product-owned selector discriminator/hash separation |
| docs/spec/contracts/, docs/spec/persistence/, docs/spec/catalog.md, website/sidebars.ts | Old active-format promises and redundant live navigation after Spec transition | Stable requirement identities, current single owning definition, working links and clearly historical milestone evidence |

Retain minimum retired-input rejection fixtures (old integer versions, old
database markers and old wire bytes) without retaining old interpreters. Do
not retain full historical reader suites merely to reject them. Rework mixed
tests so domain safety coverage survives removal of the old-format branches.
Do not delete pre-E PR-TEST-0605..0615 assertions about ownership, authority,
retained lifetime, byte opacity and pre-acceptance behavior. Rebase their source
fixtures and preserve stable IDs where the same requirement is still enforced.

Old Spec pages with incoming links become clearly non-normative historical
redirect/explanation pages where needed; active requirements move once, never
duplicate their PR-REQ headings. Historical implementation records remain
history, not current support promises. New-baseline conformance runner output
must not call a retired V1/V2/V3 suite "current support".

## 5. Native package-source integration and release artifacts

Scoop/Homebrew own downloading, verification, extraction, program inventory,
activation and removal. Pactrun never self-updates. Keep management roots and
service resources outside package-manager program/persist directories; package
scripts must not create, move, migrate or delete that data.

Produce normal and test package definitions with non-conflicting entrypoints.
Small per-install launch wrappers supply a separate default management root only
when the operator has not explicitly supplied one. They forward argument vectors,
stdio, cancellation and exit status without shell expression interpolation.
Installing a test package does not take over the normal command. Standalone
versioned executable archives remain available as engineering artifacts.

Release-source metadata maps selected product Major plus prerelease eligibility
to a highest-precedence candidate. Formal-only has no implicit prerelease fallback;
preview includes formal versions and can later advance to a higher-Minor prerelease.
Exact versions use immutable published source definitions. Native hold/pin and
explicit activation are distinct from obtaining an artifact. Updates do not
downgrade or cross Major; explicit version switch can select an older executable,
whose data support is then checked by Pactrun, not an automatic converter.

Saved selection is a native package-source concern, not a new Pactrun data domain.
The implementation will qualify Git source-ref selection for both managers:
per-install source clones retain selected Major/formal-or-preview or exact refs.
Changing a source selection alone changes metadata, not the active executable.
If Homebrew follows origin/HEAD rather than the currently checked-out branch,
the source-selection operation must explicitly maintain and verify that native
tracking reference. Do not silently claim a branch checkout alone implements it.
Only narrow source-selection conveniences are allowed; they delegate program
management to the native tools instead of becoming another installer.

Shared publication tooling computes eligible versions from version identifiers,
not upload time, and emits both managers' metadata from the same validated inputs.
No automatic cross-Major transition or prerelease downgrade is permitted. Public
publication is excluded: use local Git sources and loopback fixture downloads for
the full matrix, plus identifiable Windows ZIP/Linux tar.gz artifacts, SHA-256,
byte sizes, source/build provenance and a reviewed publication procedure.

The [executable-lifetime correction](../executable-lifetime-fix-status.md) is
already integrated locally and protects active Linux helper calls across package
removal. Preserve it and rerun the real manager cases with the E candidate. Scoop
may safely defer while a process runs; verify the requested operation completes
after it ends. Explicit cleanup/uninstall must not be waved away as unsafe user
behavior when they are part of the supported normal workflow.

## 6. Platform and stage gates

Proposed targets are x86_64-pc-windows-msvc and x86_64-unknown-linux-gnu.
Keep bundled SQLite. Inspect actual binary imports, loader dependencies and
GNU symbol versions rather than inferring the minimum from Rust's target name.
Build Linux artifacts against the proposed glibc floor, not the newer test
host by accident. Verify Windows runtime dependencies on a clean supported OS;
do not make an already-installed development runtime an undocumented prerequisite.
No macOS, ARM64 or musl release artifact is promised.

Source inspection confirms Linux uses openat2, handle-relative publication and
filesystem checks. The current adapter explicitly accepts ext4, XFS, Btrfs and
ZFS; E does not add tmpfs, overlay or network-filesystem support. Test each
claimed filesystem or explicitly separate implemented acceptance from verified
release support. Preserve Windows NTFS protections and the existing narrow
platform-adapter unsafe-code boundary, rather than expanding application unsafe.

Read-only environment preflight on 2026-09-25 found Windows 11 build 26200 and
configured-remote Linux x86-64 kernel 7.0.14-15-pve / glibc 2.41. These are not
evidence for the proposed Linux 6.8/glibc 2.39 floor, all Windows 11 releases,
or every accepted filesystem. Minimum-environment execution/VM access is a
known prerequisite to the broader release claim, not an implementation test
failure. Use the current authorized environments for engineering now; request
authorization/access before using another host or changing the test host OS.
No successful external platform-document lookup is claimed in this review.

| Gate | Required evidence | Not implied |
| --- | --- | --- |
| E alpha baseline | S1-S8 complete; baseline contracts and real operations; isolated Windows/Linux installer install/update/switch/uninstall rehearsal; source-qualified full CI; artifact manifest and failure tests | Public release, lower-environment qualification, actual-use completion or formal compatibility certification |
| Beta | Intended feature scope settled; eight contracts reviewed; retained baseline vectors; representative actual-use cases and known issue triage; platform qualification plan executed for promised targets | Automatic format relabeling or rebuilding prerelease objects |
| RC | No planned contract changes; eight required domains formal; baseline-to-candidate acceptance matrix; actual candidate artifact installation/update/switch/rollback-refusal tests on declared minimum environments; source/tag/build consistency and trust review | Publication approval or arbitrary data downgrade |
| Formal | RC evidence plus actual-use signoff, no unresolved required automated coverage, all formal defaults, prerelease-data acceptance dispositions, maintenance statement and explicit publication approval | Automatic movement of test data or interruption of services |

E supplies the machinery and alpha-candidate evidence; beta, RC and formal
advancement are later explicit readiness decisions, not invented E work to
implement hypothetical versions. Public download and minimum-environment gaps
must remain visible in release readiness, not be called passed at E closeout.

## 7. Owning Spec and traceability transition

S1 applies the approved semantic transition before changing executable bytes.
Keep a per-requirement disposition ledger under the E delivery record with
old owner, new owner, unchanged/revised/retired reason and real test mapping.
No new requirement or test identifiers are allocated by this draft.

| Owning contract | Required reconciliation |
| --- | --- |
| foundations/product-versioning-and-compatibility.md | PR-REQ-0329..0333: replace superseded independent-major, 0.1.0-only and user-required storage-upgrade assumptions; add accepted scope and stage/release boundaries |
| foundations/resources-and-versioning.md | PR-REQ-0077/0078 and 0079..0085: exactly eight domains, string identifiers, same-Major obligations, immutable identities and no automatic rewriting |
| contracts/pack-source-yaml-v1/v2/v3.md and revision-core-format-v1/v2/v3.md | Consolidate into one current source and canonical contract; move shared rules rather than referencing retired codecs; retain shell, service and Migration semantics |
| contracts/hook-protocol-v1/v2.md and shell-loader.md | One baseline framing/Session authority contract; preserve operation-specific safety and launch identity; explicit support selection |
| contracts/snapshot-integrity-format-v1/v2.md, snapshot-bundle.md, pack-distribution.md | New bytes/framing and contained-format declarations; exact compatibility, integrity and container safety remain owned here |
| contracts/cli-machine-interface.md and error-taxonomy-v1.md | One CLI version, shared results/events/error representation, unversioned semantic owners; preserve PR-REQ-0358..0368 as applicable with existing coverage |
| persistence/persistence-schema-v1..v11 documentation and extracted DDL tests | One complete new schema/metadata/admission contract; archive only real existing historical pages; no invented V1 file |
| execution/execution-and-concurrency.md and recovery-and-reconciliation.md | Retain Run acceptance, liveness, guard and recovery contracts; add only necessary program-lifetime coordination cross-reference |
| behavior/command-and-output-reference.md; release-readiness.md; new installer reference | Remove legacy upgrade command; specify exact installer grammar/data boundary and platform/trust/maintenance claims |
| catalog, navigation, conformance entry points and developer handoff | One owning definition per PR-REQ, no broken links, no current support claim for retired formats, accurate evidence status |

Preserve existing IDs when obligations survive, including after their owner page
moves. Retire an obsolete-only requirement/test with explicit reason rather
than reusing its ID for unrelated behavior. Allocate genuinely new IDs only
during the approved Spec transition using the repository census. A test may
cover several requirements. Do not mark a new versioning obligation complete
because a documentation test sees a test-name comment: require real behavior
and forbidden-behavior assertions, then verify both directions of traceability.

## 8. Entire-milestone execution sequence

Slices are internal dependency boundaries, **not** repeated approval or delivery
gates. After one approval, continue through integration, repairs and closeout.

| Slice | Depends on | Work and completion criterion |
| --- | --- | --- |
| S0 | Current task | Whole design, impact inventory, decision/authorization closure; obtain one implementation approval |
| S1 | S0 approval | Reconcile owning Specs and requirement ledger; lock exact baseline bytes, CLI/installer behavior and fixture design; no vague format placeholders entering implementation |
| S2 | S1 | Product/format value types and support diagnostics; consolidate source/Core and domain types; independent canonical/digest vectors and secure acquisition tests |
| S3 | S2 | Fresh Persistence schema and admission; remove development storage upgrades; preserve writer/owner/CAS/recovery/publication semantics; crash and concurrency tests |
| S4 | S2, S3 | Consolidate Hook, Snapshot, bundle and distribution boundaries; exercise real operations and all rejection paths without historical readers |
| S5 | S2-S4 | One machine envelope/version report; error owner/selector catalog alignment; replace fixtures/verifiers, remove remaining obsolete dispatch/tests/docs and verify retained assertions |
| S6 | S3-S5 | Native package-source/launch integration, saved selection, artifact tooling and isolated platform rehearsal; no public deployment or actual data cleanup |
| S7 | S2-S6 | Full integrated source-qualified verification, focused Windows/platform checks, repair failures and rerun affected checks/full gate when warranted |
| S8 | S7 | Final traceability/removal review, rendered documentation checks, source/evidence reconciliation and delivery record; leave Git/publication actions unexecuted unless separately authorized |

Do not use work size, a new session, or a slice boundary to reduce this scope.
Pause only for an actual product-contract conflict, new authorization or
external prerequisite. Preserve the approved plan and evidence across sessions.

## 9. Acceptance matrix

The matrix defines evidence targets, not tests claimed implemented today.

| Contract/risk | Required positive and forbidden-behavior evidence | Level |
| --- | --- | --- |
| Parsing/support | Eight exact baselines; numeric ordering including alpha.9/alpha.10 and later-Minor prerelease; reject numeric fields, overflow, malformed/unknown identifiers, missing declarations; no guessed codec | Unit/property cases and public CLI fixtures |
| Identity | Rust/independent Node agree on bytes, frame and digest; source defaults normalize identically; format label changes alter identity; exact import/export preserves original bytes/digests; payload hashes unchanged | Golden cross-language vectors plus persistence round trips |
| Hook | All five operation families, direct/shell launch, service authority, ACK/recovery; wrong version/session, truncated preamble, unknown required fields, cancellation and output failure; no negotiation | Codec, subprocess and real CLI/Hook tests |
| Persistence | Pristine bootstrap, concurrent bootstrap, exact baseline reopening/read-only inspection; old/foreign/malformed schema refusal with unchanged database/committed WAL content; only SQLite read-coordination sidecars allowed, no Pactrun staging/lease creation before support | Real SQLite/filesystem tests and fault injection |
| Continuity/ownership | Two processes using same baseline; live/unknown owner, process crash and recovery; switch while Hook and native service continue; data remains accessible at the same path/handle and permissions; no forced unlock/restart/rebind | Deterministic process barriers, crash tests and native service witness on both platforms |
| Snapshot/Pack | Current managed/service content and metadata; wrong inner/outer version, forged digest, missing/extra/duplicate member, zip-slip, hostile size, cancellation; no partial publication | Transport/adversarial fixtures and real import/export |
| CLI | JSON and JSONL share version; preserve typed errors, partial outcomes, terminal event, raw payload stdout and broken-pipe behavior; no duplicate output or prose contamination | Unit plus CLI process tests |
| Installer selection | Paginated/out-of-order Releases, no formal candidate, exact pin, Major boundary, formal/prerelease transition, no automatic downgrade; select alone changes no executable | Pure selector tests plus fixture-server integration |
| Installer safety | Install/switch/update/default/uninstall; download truncation, digest/target mismatch, hostile paths, unknown record, disk/write failure, pointer crash, concurrent mutations, direct-product active lease; old selection and data untouched | Filesystem/fault injection plus packaged Windows/Linux subprocess rehearsal |
| Reset/removal | Old formats clearly refused; production graph no longer depends on retired codecs/schema ladders; all surviving functional assertions retained; Pack Migration still works | Symbol/reference audit, compile/Clippy, representative real workflows |
| Future promotion | Frozen alpha evidence includes originating version and exact bytes; documented per-representation acceptance checklist; no invented formal support | Vector/provenance tests and design review, later formal evidence explicitly deferred |
| Documentation/traceability | One owner per requirement, bidirectional PR-REQ/PR-TEST mapping, no affected mechanically verifiable implementation obligation left Pending, working navigation | Existing document and xtask conformance tests, remote site typecheck/build |

Use [development and verification](../development-and-verification.md): focused
checks during implementation, then complete remote `cargo xtask ci` for this
cross-cutting runtime milestone. Keep the persistent supported filesystem,
source hash manifest and intended runner/build paths; smoke-test actual CLI and
Hook before the long gate. Retain assertions, overflow checks, timeouts and
coverage. Add focused Windows evidence for native installer/NTFS/process cases.
Reassess ignored capacity tests if changed packaging/streaming makes their
coverage relevant; do not silently inherit ignores for affected guarantees.

Evidence records commands, target/runtime/filesystem, input hashes, tested
artifact hashes, counts, exclusions and failures. A later documentation-only
closeout can reuse unchanged runtime evidence with a source comparison, but
must rerun affected documentation checks. Source checkout identity alone does
not prove the executable came from it. Self-review is not independent review.

## 10. Closeout definition and current verification

E is complete only when the approved alpha scope above is implemented, tested,
documented and its removal ledger is closed. Requirements added for E runtime
cannot remain only Pending automated coverage. Every command supports success,
failure, partial result and output failure as appropriate. All retained unknowns
are explicitly later release gates, not unfinished approved implementation.
The delivery report separately states runtime, installer, platform, docs, Git,
data cleanup and publication status, including retained isolated resources.

This S0 preparation changes only informative design/navigation documentation.
It allocates no PR-REQ/PR-TEST IDs and does not change executable inputs, fixtures,
canonical DDL or active normative requirements. Verification results for this
draft are recorded in the task report; no E runtime test is claimed by writing
this plan. Existing unrelated dirty files, workflow and archives are preserved.

## Approved read-coordination clarification (2026-09-27)

The operator approved ordinary SQLite read coordination when inspecting and
refusing an unsupported store. New SQLite sidecars are not Pactrun migration,
object rewriting or deletion. The owning PR-REQ-0078 contract preserves existing
database and committed WAL bytes and prohibits ignoring WAL frames, deleting
sidecars as "temporary", creating Pactrun sessions/content guards before support,
or affecting services. PR-TEST-0623 retains unsupported-store coverage including
WAL mode; PR-TEST-0639 additionally preserves an existing committed WAL and a
live writer. No obsolete format reader or automatic conversion is introduced.
