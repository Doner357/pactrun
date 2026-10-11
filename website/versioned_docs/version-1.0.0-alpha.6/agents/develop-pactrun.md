# Modify Pactrun

In a source checkout, start with CONTRIBUTING.md and applicable workspace
instructions. Website maintenance commands are in website/README.md; focused and
opt-in test entry points are in tests/README.md; delivery tooling is described in
tools/README.md.

Use the [Spec topics](../spec/index.md) and [vocabulary](../spec/core/vocabulary.md)
to find the owning rules. Read their conditions, exceptions, formats and linked
tests, then inspect the actual code. Preserve stable requirement/test identities
when reorganizing sources. For a behavior change, define the intended contract
before implementation; a passing test does not establish an unapproved meaning.
