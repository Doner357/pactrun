# Develop Pactrun

This is project navigation, not personal developer preferences or permission to
commit, push, deploy, change machines, or perform service side effects.

1. Read applicable repository instructions and CONTRIBUTING.md in a checkout.
2. Read [the Spec map](../spec/index.md) and choose a [task path](../development/reading-paths.md).
3. Check [the roadmap](../development/implementation-roadmap.md) and [M6.5 handoff](../development/next-milestone.md).
4. Read actual owning rules, including unnumbered constraints, definitions,
   exceptions, and the exact format/version scope. Use [the glossary](../spec/glossary.md)
   to find a term's owner, not as a replacement definition.
5. Follow [verification policy](../development/development-and-verification.md).
   Preserve requirement/test IDs and the bidirectional evidence graph.
6. Report changed rules or state explicitly that semantics are unchanged;
   distinguish Passed, Failed, Not run, Partially run, and Blocked verification.

The shared full verification entry is `cargo xtask ci`. Respect the authorized
execution environment. Tests enforce contracts; a passing suite does not permit
weakening a rule. Stop for a real product-semantic conflict or an unapproved
compatibility commitment, not for an ordinary reversible implementation detail.
