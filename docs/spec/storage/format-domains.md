---
title: Format Domains and Support
---

# Format Domains and Support

## Independent version domains

The [formal product compatibility policy](./compatibility.md)
defines same-Major compatibility. Format-domain ownership determines which
representation a particular interface supports.

### PR-REQ-0077 - Separate version domains

There is one product version and exactly eight independent format/protocol
domains: Pack source, Revision canonical, Hook Protocol, Snapshot content/integrity,
Snapshot bundle, Pack distribution, CLI machine interface and Persistence.
Results and event streams share the CLI version; runtime closure, Session subviews,
error catalogs and selector discriminators do not create additional domains.

Format values MUST be strings Major.Minor[-prerelease], without Patch. Published
prereleases use alpha.N, beta.N or rc.N with positive canonical decimal N. Numeric
components have no leading zeroes, fit u64, and complete identifiers are bounded
to 128 ASCII bytes. No whitespace, numeric coercion or build suffix is accepted.
Products use the corresponding three-component published SemVer profile.
Schema names such as `HookV1` and `ServiceAuthorityV2` identify structures; their
suffixes do not select a format. Reader selection uses the explicit version strings.

Each domain owns its implemented readers and writers. A matching Major or an
ordered newer identifier MUST NOT be interpreted as automatic support. Format
Minor does not imply minimum product Minor. Pack source and Pack distribution
write `1.0-alpha.2`; the CLI machine interface writes `1.0-alpha.3`. Persistence
writes `1.0-alpha.4` and accepts the exact `1.0-alpha.1`, `1.0-alpha.2` and
`1.0-alpha.3` Store schemas
through its supported upgrade path.
Revision, Hook, Snapshot integrity and Snapshot bundle remain `1.0-alpha.1`.
Changes follow the owning contract rather than synchronizing
unrelated domains. Unsupported formats are refused in their necessary scope.

**Verification: PR-TEST-0183, PR-TEST-0338, PR-TEST-0369, PR-TEST-0385, PR-TEST-0538,
PR-TEST-0557, PR-TEST-0618, PR-TEST-0619, PR-TEST-0625.**

### PR-REQ-0078 - Storage compatibility and admission {#pr-req-0078---persistence-migrations}

Pactrun supports the complete [persistence schema](../persistence/persistence-baseline.md).
Supported Store opening upgrades follow
[PR-REQ-0373](./store-opening.md#pr-req-0373---store-opening-and-supported-catalog-upgrades).
Other unsupported schemas have no implicit or explicit upgrade chain. Refusal MUST
NOT clear real data, rewrite identities, infer Run outcomes, or touch
service-owned resources. Pack-defined Revision Migration is independent of
storage-format compatibility.

The exact public version is the string in the singleton metadata row. SQLite's
application marker and private bootstrap marker alone do not prove support.
Opening an existing store MUST validate the exact supported metadata, table and
index manifest, column declarations, constraints, keys and references. Pristine
means zero ownership markers and no non-SQLite objects. Partial, foreign,
unmarked non-empty and unsupported stores MUST be refused without repair.

Support inspection precedes Pactrun staging/session and content-coordination
creation. SQLite MAY create or update its own read-coordination sidecars while
performing ordinary read-only inspection. This exception MUST NOT change existing
database or committed WAL content, ignore committed WAL frames, grant write
admission, rewrite objects, run cleanup, or interfere with service resources.
An existing WAL can contain committed data and MUST NOT be discarded as a
"temporary" file. This is read coordination, not a format conversion. Writer
admission MUST repeat qualification after acquiring the serialized transaction;
preflight is advisory only. Fresh bootstrap publishes the complete schema and
admission atomically. Interrupted bootstrap cannot expose a partially admitted
schema. Read-only opening cannot initialize or upgrade storage. Merely reopening
or updating the product MUST preserve existing identities, bytes, bindings,
non-terminal Runs and unresolved recovery obligations without reconciliation.

Any future internal migration requires an explicit supported contract and must
preserve those same obligations. No other storage conversion is defined.

**Verification: PR-TEST-0692, PR-TEST-0202, PR-TEST-0621, PR-TEST-0622, PR-TEST-0623, PR-TEST-0624, PR-TEST-0639, PR-TEST-0344.**

**Verification: PR-TEST-0687.**

### PR-REQ-0079 - Revision Core format ownership

Each published Revision Core format MUST define its closed semantic schema,
identity-affecting normalization, collection ordering, runtime-content
descriptor, canonical byte profile, hash framing, domain separation, and hash
algorithm.

The [Revision format](../packages/revision-format.md) defines the exact
representation and framing.

**Verification: PR-TEST-0044, PR-TEST-0045, PR-TEST-0331, PR-TEST-0333, PR-TEST-0493.**

### PR-REQ-0080 - Snapshot integrity format ownership

Each Snapshot integrity format MUST define its integrity-bearing fields,
SnapshotId binding, producer and provenance normalization, complete managed
binding representation, service-content roles, collection ordering, canonical
bytes, domain separation, and hash profile. It MUST hash a semantic manifest,
not archive bytes. `SnapshotIntegrityFormatV1` MUST apply semantic normalization
before RFC 8785 JCS encoding and use a fixed SHA-256 profile. The exact Frozen
contract is defined by
[Snapshot integrity format](../snapshots/integrity.md).

**Verification: PR-TEST-0013, PR-TEST-0015, PR-TEST-0016, PR-TEST-0018,
PR-TEST-0019.**

### PR-REQ-0081 - Bundle envelopes

Revision and Snapshot bundles MUST be self-describing, versioned envelopes that
identify their kind, format version, manifest, and content. Unsupported formats
MUST be rejected rather than guessed. Packaging or compression changes MUST NOT
change contained domain identity.

For Revision transport, this envelope is the user-facing distribution Pack in
[Pack distribution format](../packages/distribution.md), not a separately
managed Bundle object. Snapshot transport remains independent and unchanged.

**Verification: PR-TEST-0538, PR-TEST-0540, PR-TEST-0542.**

### PR-REQ-0082 - Hook Protocol version

The canonical Hook Protocol MUST have its own language-neutral version covering
Session establishment, authority, I/O transitions, typed contexts and outputs,
recovery-risk messages, cancellation, EOF, and protocol violations. SDKs and
helpers MUST remain adapters to that protocol.

**Verification: PR-TEST-0020, PR-TEST-0022, PR-TEST-0023, PR-TEST-0026, PR-TEST-0028, PR-TEST-0030, PR-TEST-0031, PR-TEST-0032, PR-TEST-0362, PR-TEST-0491.**

### PR-REQ-0240 - ServiceStorage format boundaries {#pr-req-0240---future-servicestorage-version-gates}

ServiceStorage declarations and Session authorities MUST follow their explicitly
versioned [Revision](../packages/revision-format.md) and
[Hook](../interfaces/hook-protocol.md) contracts. Implementers MUST NOT add them
retroactively to an older closed schema or authority union. The current
Revision and Hook contracts include these capabilities. An unsupported format
MUST NOT be interpreted as another version.

Revision Core and Hook Protocol remain independent version domains. A future
Revision Core format MUST NOT imply that every Hook uses the same-numbered or a
new Hook Protocol version. PR-REQ-0235 through PR-REQ-0248 own the resource
semantics. Their implemented declaration, authority and publication mechanisms
are owned by the current baselines and
[ServiceStorage execution](../instances/service-storage.md);
[retirement](../lifecycle/retirement.md) owns Cleanup receipts,
finalization and abandonment custody.

These contracts cover ServiceStorage-backed resources only; they do not define
identity or authority for external databases, Docker volumes, or other resource
types.

**Verification: PR-TEST-0331, PR-TEST-0369, PR-TEST-0385.**

### PR-REQ-0083 - Structured CLI version

Human-readable output MAY evolve for usability. Machine-readable CLI output
MUST be a versioned interface, and breaking changes MUST require an explicit
version change. Interactive Hook terminal streams MUST NOT be wrapped in the
structured output envelope.

**Verification: PR-TEST-0554, PR-TEST-0556, PR-TEST-0557.**
