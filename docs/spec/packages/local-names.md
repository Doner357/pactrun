---
title: Local Names and References
---

# Local Names and References

Names belong to the user of a Store. They identify installed Packages and
Revisions without changing Package IDs, content digests, or existing Instance
bindings. Pack authors provide declarations and explanations, not local names.

### PR-REQ-0371 - Local names and two-part identity resolution

A Package and a Revision MAY each have one local name. Package names are unique
within a Store; Revision names are unique within their Package. Unnamed objects
remain addressable by ID. Names MUST contain 1 to 31 ASCII characters, start with an ASCII letter or digit and
contain only ASCII letters, digits, hyphens, underscores and periods. Names are
case-sensitive exact strings: no case folding, trimming or normalization occurs.
This public naming restriction does not restrict Unicode descriptions or other
text. Names have no special semantics, including `stable` and `latest`.

A Revision reference MUST have the form `<package>:<revision>`, with exactly one
colon. Each component may match a user name or an ID. Package IDs use lowercase
hexadecimal; Revision IDs use the lowercase digest digits without `sha256:` in
this reference syntax. ID prefixes require at least eight digits and MUST NOT
exceed the full ID length. The name length limit does not restrict IDs or the
combined reference. Full Package IDs (32 digits) and Revision digests (64 digits)
cannot be names. Short hexadecimal names can also match ID prefixes; neither
interpretation takes precedence, and complete IDs remain an unambiguous fallback.

A complete ID pair identifies an exact identity without a catalog lookup.
The command determines whether it must currently be installed; for example,
deleting an already absent exact Revision remains idempotent.

Resolution MUST consider each matching Package and match the Revision component
within that Package. The resulting exact Revision identities are deduplicated.
Zero matching identities means unresolved, one means resolved, and multiple
distinct identities mean ambiguous. A complete reference can disambiguate a non-unique Package component.
Package-only operations MUST instead resolve a unique Package. Resolution MUST
NOT select by installation time, name preference, or insertion order.

Names are references, not rebinding instructions. Accepted operations and Instance
bindings retain exact identities when a name changes or is removed.

**Verification: PR-TEST-0469, PR-TEST-0656, PR-TEST-0657, PR-TEST-0658, PR-TEST-0659, PR-TEST-0660, PR-TEST-0661, PR-TEST-0662, PR-TEST-0664, PR-TEST-0669, PR-TEST-0670, PR-TEST-0677, PR-TEST-0678.**

### PR-REQ-0372 - Local naming mutations and installation time

An installation MAY request a Package name, a Revision name, both, or neither.
Requested names and the installed Revision MUST commit together. Repeating an
existing assignment is idempotent. Installation MUST NOT rename an already named
object or take a name from another object. A conflict leaves no newly installed
Revision or partially changed name. Unreferenced staged content is not an
installed Revision and remains subject to ordinary storage housekeeping.

`package rename <package> <name>` and `revision rename <package>:<revision> <name>`
set or replace the selected object's name. `package unname <package>` and
`revision unname <package>:<revision>` remove only its name. Conflicts MUST NOT
overwrite another object's name. Mutation targets retain the resolved exact
identity rather than re-resolving a name during execution.

Each locally installed Revision records its successful installation time. A
repeat installation, metadata update or rename MUST preserve this time. Removing
and reinstalling a Revision creates a new installation time. Imported Packs MUST
NOT carry local installation times or user names to another Store. Earlier
installations whose time was not recorded use an explicit unknown value; upgrade
time MUST NOT substitute for installation time. Time is neither Revision identity
nor authority to select a newer Revision.

**Verification: PR-TEST-0662, PR-TEST-0663, PR-TEST-0664, PR-TEST-0665, PR-TEST-0677.**

### PR-REQ-0376 - Revision catalog order and local facts

Revision lists MUST order by installation time descending, with unknown times
last. Equal times use Package ID and Revision digest ascending as a deterministic
tie-breaker. Pagination MUST carry the time and exact identity of the last row,
not re-query that row to recover its position. A deleted cursor row therefore
does not invalidate continuation. Consumers copy the returned continuation token;
it is not a Revision reference or a name.

Names, installation facts, declarations and metadata MUST come from one read
snapshot for all output formats. Machine results expose local names and a nullable
decimal-string `installed_at_unix_ms`. Human timestamps include their time zone;
unknown times MUST NOT look like installation at the Unix epoch. A presented
short or named reference MUST resolve uniquely in that snapshot, otherwise the
display uses complete IDs. `--no-trunc` displays the full identity reference.

**Verification: PR-TEST-0678.**

