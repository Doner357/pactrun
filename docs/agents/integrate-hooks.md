# Integrate a Hook

Use the [Shell helper reference](../package-authors/reference/shell-loader.md).
For direct protocol clients, read the author-facing
[message reference](../package-authors/reference/hook-protocol.md) and
[service authority/transition guide](../package-authors/reference/service-fields.md).

Track granted Session authority, explicit output registration, cancellation,
risk acknowledgment, and completion. A normal process exit alone is not proof
of durable success. Keep Secrets out of diagnostics and never simulate
ServiceStorage with Inputs or Workspace.
