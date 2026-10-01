# Authentik alpha.2 retirement regression fixture

This disposable Linux evaluation Pack is derived from the user-supplied
2026-09-30 black-box Pack. It is not a production recipe or an Authentik support,
security, upgrade, backup or TLS qualification.

The PostgreSQL and Redis containers explicitly run under the invoking non-root
Pactrun user's numeric UID/GID, recorded in the generated Compose contract. This
prevents those images from handing newly initialized bind-mounted directories to
a different host owner. Configuration or UID/GID drift refuses restart rather
than silently changing an existing deployment. Existing foreign-owned data is
not converted or repaired. This fixture does not add privilege elevation,
automatic chown, or permissive chmod to Pactrun.

The Authentik worker retains the original fixture's root container user; Docker
daemon access is privileged host capability. Passing this fixture is not a
general rootless-container or least-privilege certification. Verify actual
initialized data and ordinary-user retirement, not only Compose generation.

Keep credentials synthetic and private; use fresh isolated stores and loopback
ports. Do not modify or rerun the original evidence archive. The harness in
`tools/verify_authentik_alpha2.py` owns the exact scoped execution and receipts.
