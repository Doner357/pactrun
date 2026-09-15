---
title: Internal persistence
---

# Internal persistence

**Status: Informative navigation.**

V7 is current on the M6.5 feature branch, with explicit exact-V6 upgrade only.
V6 remains the integrated develop baseline until separately authorized Git
integration. Older schemas retain their historical and compatibility
obligations; their presence is not permission for an implicit upgrade chain.

- [Persistence Schema V2](./persistence-schema-v2.md): Historical internal contract; not an implicit upgrade path.
- [Persistence Schema V3](./persistence-schema-v3.md): Historical Instance and managed-binding representation.
- [Persistence Schema V4](./persistence-schema-v4.md): Historical M3 Run representation.
- [Persistence Schema V5](./persistence-schema-v5.md): Historical M4 contract, including writable admission and V4 bootstrap. That bootstrap is not an M5 production entry point.

- [Persistence Schema V6](./persistence-schema-v6.md): Historical exact DDL and V5 upgrade; the integrated M5/M6 Migration/recovery baseline.
- [Persistence Schema V7](./persistence-schema-v7.md): Current exact DDL, explicit V6 upgrade and implemented ServiceStorage custody/association representation.

Return to the [specification map](../index.md). For current runtime support and
next-milestone boundaries, see [the development entry](../../development/next-milestone.md).
