---
title: Invocation Parameters
---

# Invocation Parameters

## Parameter model {#invocation-parameters-1}

### PR-REQ-0133 - Invocation-parameter lifecycle

Invocation Parameters MUST be typed, single-invocation values processed through
parse, validation, normalization, and binding. They MUST NOT persist as Instance
Inputs or become Managed Data.

**Verification: PR-TEST-0080, PR-TEST-0150, PR-TEST-0167, PR-TEST-0246, PR-TEST-0560, PR-TEST-0597, PR-TEST-0598.**

### PR-REQ-0134 - Parameter types {#pr-req-0134---initial-parameter-types}

The supported parameter types are integer, float, boolean and string. Each
parameter declaration MAY mark its value sensitive. Raw argument-vector passthrough MUST
NOT be part of this contract.

**Verification: PR-TEST-0080, PR-TEST-0592, PR-TEST-0597, PR-TEST-0598.**

### PR-REQ-0135 - Sensitive parameter channel

Pactrun MUST NOT record a sensitive parameter value as invocation metadata in Run
history. Hook-authored explanations follow the attribution and non-disclosure
boundaries in PR-REQ-0098 and PR-REQ-0351; this is not a general taint detector.
The CLI MUST
offer a value path that does not expose the value directly in command-line
arguments, such as prompt, file, or standard-input binding.

**Verification: PR-TEST-0116.**

### PR-REQ-0136 - Shared machinery is not shared identity

Actions and Snapshot operations MAY reuse parameter parsing and binding
machinery, but this MUST NOT turn them into the same domain operation.

**Verification: PR-TEST-0069, PR-TEST-0231, PR-TEST-0246, PR-TEST-0258, PR-TEST-0270.**

### PR-REQ-0273 - Primitive invocation-text lexical profile

A textual Invocation Parameter source MUST present one complete UTF-8 string to
the Application parameter boundary. This boundary MUST NOT trim whitespace,
normalize Unicode, apply shell or CLI quoting, or reuse an authoring frontend's
scalar grammar.

Integer text MUST match `-?(0|[1-9][0-9]*)`, then parse as its exact
mathematical value within the safe-integer range. Fraction and exponent forms,
a leading plus, invalid leading zeroes, and surrounding whitespace are invalid.
Negative zero normalizes to typed integer zero.

Float text MUST match
`-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`; an integral token is valid
when the declared target type is Float. Conversion MUST occur once to finite
IEEE-754 binary64 using round-to-nearest, ties-to-even. Overflow is invalid,
underflow to zero is valid, and negative zero normalizes to positive zero.
NaN, Infinity, a leading plus, invalid leading zeroes, `.5`, `1.`, and
surrounding whitespace are invalid.

Boolean text MUST be exactly lowercase ASCII `true` or `false`. String text is
the exact supplied Unicode scalar sequence, including an empty or
number-looking string.

This requirement defines reusable invocation semantics only. It does not define
CLI flags, positional arguments, prompting, files, standard input, escaping,
quoting, or shell syntax, and it is independent of `PackSourceYamlV1`.

**Verification: PR-TEST-0080.**
