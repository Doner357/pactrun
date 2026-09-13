---
title: Development and Verification Policy
---

# Development and Verification Policy

**Status: Normative contributor policy.**

**Audience:** Human contributors and development agents.

**Scope:** Pactrun implementation, tests, and normative documentation.

## Authority model

Canonical normative Markdown defines Pactrun architecture, semantics,
invariants, protocols, and observable behavior. Automated verification enforces
that contract, and implementation must conform to both. Rendered sites,
generated indexes, source comments, tests, and implementation details are not
independent semantic sources.

When implementation or tests disagree with canonical documentation, the
implementation or tests are incorrect unless an explicitly approved design
change first modifies the normative contract. A green suite never grants
permission to rewrite requirements around generated behavior.

## Normative and informative content

Normative content defines domain invariants, identity and lifecycle rules,
state transitions, persistence and atomicity guarantees, execution, recovery,
concurrency, protocols, canonical encodings, supported CLI contracts, and
observable failures. Informative material includes rationale, history,
tutorials, diagrams, examples, and non-contractual implementation notes.

Every independently verifiable product requirement must have a stable
`PR-REQ-NNNN` identifier. IDs are numeric, globally unique, and independent of
file location, implementation package, or temporary classification. Moving or
rewriting a requirement does not change its ID unless an approved design change
retires and replaces the requirement.

Stable requirement and test IDs are durable references, not contiguous
sequence assertions. IDs MUST be unique, every reference MUST resolve, and a
retired ID MUST NOT be reused or cause unrelated IDs to be renumbered merely to
close a numeric gap. Traceability tooling MUST accept legitimate gaps.

Contributor-process rules do not use `PR-REQ` identifiers. Informative prose
does not need artificial requirements or tests.

Product requirements and verification metadata belong only in docs/spec.
Development guides and design syntheses link to owning rules without creating
additional product requirements. Compatibility pages at old paths contain no
independent normative definitions. See the [authority map](../spec/index.md).

Usage documentation is deferred to a separately scoped phase. User and Pack
Author pages retain placeholders, including operation-oriented agent guides.
Development guides may be completed now. General usage guides must not expose
internal test traceability or become a second specification. Agent development
navigation is published as text, not as human website pages.

This documentation scope change does not change product semantics, Frozen
formats, requirement IDs, or the implementation approval state of milestones.

## Verification and traceability

Every mechanically verifiable normative requirement must have at least one real
automated verification artifact before implementation of that requirement can
be considered complete. The relationship may be many-to-many and must optimize
semantic coverage rather than test-count symmetry.

Normative tests use stable `PR-TEST-NNNN` identifiers and declare the real
requirements they verify:

```rust
// Test-ID: PR-TEST-NNNN
// Verifies: PR-REQ-NNNN
#[test]
fn behavior_under_test() {
    // ...
}
```

Normative requirement sections list their associated real test IDs.

### Specification-only transitional state

A mechanically verifiable normative requirement MAY temporarily say:

```text
Verification: Pending automated coverage.
```

only when implementation of that requirement has not started and the task is
establishing, extracting, restructuring, or refining the canonical
specification. The pending state MUST remain visible and traceable. Placeholder
test IDs are prohibited.

### Implementation completion

Once implementation of a requirement begins, the implementation task MUST NOT
be considered complete while an affected mechanically verifiable requirement
remains covered only by `Pending automated coverage`. Before completion, the
following relationship MUST exist:

```text
normative requirement
        <->
real automated verification
         |
         v
implementation
```

Reporting the missing test as a pending gap does not satisfy implementation
completion policy. A failing normative regression test MUST NOT be removed to
obtain a green build, a requirement MUST NOT be weakened to fit implementation,
and tests MUST NOT preserve accidental generated behavior. Semantic conflicts
MUST be reported rather than silently resolved.

Only docs/spec is scanned for authoritative requirement definitions. Numbered
product-rule definitions outside that tree are an error, not silently ignored.
Drafts, archives, proposals, agent guides, and generated copies must remain
outside Spec. A forwarding page contains references, never duplicate definitions.
Existing unnumbered normative constraints must also be reviewed; the ID scanner
cannot prove their preservation or the semantic sufficiency of test coverage.

Stable IDs, rather than file names or line numbers, form the durable
bidirectional relationship. Generated source links and indexes may improve
presentation but must derive from canonical documentation and test metadata.

### Automated traceability enforcement

Repository tooling and CI MUST enforce requirement-to-test traceability. During
the current pre-implementation specification stage, tooling MAY report pending
verification as a legitimate transitional state. Once corresponding
implementation exists, CI MUST fail implementation-completion checks when
required automated verification is missing.

Traceability checks MUST cover at least:

1. duplicate `PR-REQ` identifiers;
2. duplicate `PR-TEST` identifiers;
3. requirement references to nonexistent tests;
4. test references to nonexistent requirements;
5. dangling references after deletion or renaming;
6. mechanically verifiable implemented requirements without required automated
   verification.

Numeric continuity is intentionally not a traceability invariant.

## Test the real contract

A test must exercise the level at which the requirement makes its promise. A
unit test for an internal value object is insufficient when the contract spans
admission, persistence, a Hook, or concurrent mutation.

## Executable system tests

`tests/system.rs` is the executable-level CLI boundary. It uses one isolated
storage root per scenario under `target/system-tests/`, a materialized copy of
the integration-test executable as a real Hook, and a new `pactrun` process for
each product operation and durable inspection. The harness does not call
crate-private application or persistence APIs, inspect SQLite, or use
production failpoints.

Each scenario has an overall deadline and each CLI process has a shorter
deadline. The harness drains both output streams while the child runs and owns
cleanup of the CLI and recorded Hook process trees, so a hung CLI, Hook, or
Hook descendant cannot strand the test runner. The Hook marker is written at
worker process entry before transport discovery. It provides independent
negative evidence for plan and pre-acceptance rejection scenarios; an empty
Run list alone is not sufficient evidence.

Run the focused system boundary and retained M3 real-CLI regression suite with:

```text
cargo xtask system-test
```

`cargo test --workspace --all-features` already includes both targets. System
scenarios use semantic CLI fields rather than stdout goldens, preserve raw-byte
assertions only where the public contract exposes bytes, and leave platform
console, signal, containment, and native-path cases to platform-specific tests.

The common system matrix currently has an explicit boundary rather than one
monolithic end-to-end test: `PR-TEST-0142` through `PR-TEST-0147` establish the
startup, installation, Input/Secret, read-only, acceptance, and durable Run
foundation; `PR-TEST-0148` through `PR-TEST-0159` cover parameter acquisition,
output publication, transport failure and timeout, late completion, redaction,
terminal-channel separation, recovery risk, pinned binding views, Mutate
conflict, and explicit owner-loss reconciliation. `PR-TEST-0165` through
`PR-TEST-0175` close the remaining executable matrix evidence for read-only
orphan inspection, pre-acceptance ordering, empty/Unicode protected parameters,
durable success inspection, multi-output publication, malformed protocol
terminalization, rejected late completion, exact mutation conflict, recovery
token behavior, reconcile idempotence, and Secret/Protected-source redaction.
Existing real-CLI platform regressions remain the containment and
interactive-Hook evidence. The platform counterparts `PR-TEST-0160` through
`PR-TEST-0164` add Linux parameter-stdin SIGINT, ordered interpreter
eligibility, executable-bit, and byte-path coverage plus Windows
isolated-console Ctrl+C and native-image/batch-suffix no-fallback coverage.
`PR-TEST-0176` and `PR-TEST-0177` add accepted late completion after a real
POSIX SIGINT and Windows console Ctrl+C; `PR-TEST-0163` uses a deterministic
named-pipe consumption barrier before cancellation. Each platform still runs
the common matrix; the native and console assertions are deliberately
platform-specific rather than stdout goldens.

Use the lowest-cost level that genuinely proves the contract:

- domain unit tests for pure validation, identities, legal transitions,
  continuity, protection, and compiler invariants;
- persistence integration tests for transactions, uniqueness, durability,
  publication atomicity, exact references, pins, and state versions;
- execution integration tests for Resolver-to-Hook behavior;
- concurrency tests for Observe and Mutate interaction, stale plans, versions,
  pins, and conflicts;
- crash and failure-injection tests for risk boundaries, durable directives,
  interruption, reconciliation, no-replay behavior, and manual recovery.

Negative requirements need regression protection. Tests must demonstrate that
Pactrun does not take tempting but forbidden actions, such as invalidating an
incomplete Instance, selecting an ambiguous Revision, replaying a Hook after a
crash, declassifying a Secret implicitly, accepting multiple Migration writers,
or treating an ephemeral Plan as a replay log.

Property-based or fuzz verification should be used for general invariants over
large state spaces, including canonical identity, equivalent normalization,
writer uniqueness, transition state machines, Secret monotonicity, malformed
protocol input, and operation sequences. Golden vectors are required when
compatibility depends on exact bytes or digests.

Line, branch, and function coverage are supporting signals only. The primary
question is whether every Pactrun promise has executable evidence that fails
when the promise is violated.

## Change discipline

During ordinary implementation, refactoring, bug fixing, testing, or
maintenance, normative behavior is fixed. Contributors may implement, refactor,
add or strengthen tests, improve informative text, repair presentation links,
and report ambiguity. They must not weaken, delete, reinterpret, or relabel a
requirement to accommodate an implementation or make a suite green.

Semantic design changes use this order:

1. identify affected requirement IDs;
2. update canonical normative documentation;
3. obtain explicit semantic approval;
4. update or replace verification;
5. update implementation;
6. run the complete relevant suite.

Tests enforce but do not define semantics. If a test conflicts with a still
authoritative requirement, fix the test or implementation. If a design change
is approved, update the requirement first.

Before removing or weakening a normative test, identify every requirement it
protects. Removal is allowed only when the requirement was explicitly retired,
equal or stronger coverage replaces it, or remaining tests already provide
adequate protection. A failing regression test must not be deleted merely to
restore a green build.

A bug fix must identify the violated requirement, add or strengthen a
reproduction, demonstrate the bad behavior when practical, fix the code, and
run relevant verification. If no requirement establishes the intended
behavior, report the specification gap instead of silently inventing semantics.

A new externally observable feature is complete only after its normative
behavior, stable requirement IDs, real verification, bidirectional
traceability, implementation, and documentation impact are complete.

## Specification conflict reporting

When two requirements, implementation, and tests cannot all agree, report:

```text
Specification conflict detected.

Requirements:
- PR-REQ-NNNN

Conflicting implementation or tests:
- path or PR-TEST-NNNN

Reason:
<the incompatible semantics>

Action:
No semantic choice was made silently. A design decision is required.
```

Do not guess which authoritative behavior should be discarded.

## Documentation language and encoding

English Markdown under `docs/` is the canonical source. Its natural-language
headings, prose, labels, explanations, and comments must be English. Future
files explicitly belonging to the `zh-Hant` locale may use Traditional Chinese
as their primary language, but remain translations rather than independent
semantic sources.

Documentation must be valid UTF-8 and may use the full Unicode repertoire,
including typographic punctuation, arrows, mathematical symbols, box-drawing
characters, emoji, proper names, and Unicode literals needed for encoding or
interoperability examples. English-language policy must not be enforced with a
blanket non-ASCII ban. For example, `Ready ✅` is valid English UTF-8 content.

Pactrun must correctly support UTF-8 where its external contracts accept text;
user data, Package metadata, Inputs, and outputs must not be restricted to
ASCII. Repository-owned source code, identifiers, comments, test names, and
source-embedded fixtures must nevertheless be ASCII by default. Prefer Unicode
escapes, byte construction, or dedicated fixtures for test data. A literal
non-ASCII source-code exception is allowed only when indispensable to the
behavior under test, must be narrowly scoped, and must have an ASCII explanation
of its necessity.

## Prohibited anti-patterns

- **Test-after-generation:** preserving accidental generated behavior without
  grounding it in requirements.
- **Specification laundering:** changing the specification to match an
  unapproved implementation.
- **Coverage theater:** creating low-value tests solely for a metric or nominal
  one-test-per-requirement symmetry.
- **Brittle links:** using source line numbers as the canonical traceability
  relationship.
- **Silent semantic decisions:** selecting one materially different
  interpretation of ambiguous text without design approval.

## Change Completion Checklist

- [ ] Relevant normative documentation was read first.
- [ ] No known requirement is contradicted.
- [ ] New normative behavior has stable requirement IDs.
- [ ] Specification-only requirements use `Pending automated coverage` only
      while their implementation has not started.
- [ ] Every implemented mechanically verifiable requirement has real automated
      verification; no implementation task is complete with only a pending gap.
- [ ] New normative tests have stable IDs and declare verified requirements.
- [ ] Documentation references real test IDs only.
- [ ] Tests exercise the real contract and its negative behavior.
- [ ] Transaction, concurrency, crash, and recovery behavior is tested at the
      level required by the promise.
- [ ] No requirement or regression protection was weakened for convenience.
- [ ] Traceability and relevant suites pass.
- [ ] Documentation impact is explicitly reported as `none` or as a list of
      affected requirements and pages.

Different implementations may vary internally. They are acceptable only while
they remain inside the same documented and verified semantic envelope.
