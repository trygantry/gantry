# Gantry schema compatibility and migration policy

`schema-manifest.json` is the machine-readable index of published mobile governance
contracts. Every persisted object declares `schema_version`; readers validate that
version before trusting identity, policy, or decision data.

## Compatibility rules

- Gantry 0.4 uses exact-version, fail-closed readers. An unknown version is never treated
  as current and never silently coerced.
- Governance records are append-only identities. A future migration must write a new
  verified record or a copy-on-write migrated release tree; it may not rewrite signed-off
  meaning in place.
- Derived artifacts and cache entries may be deleted and regenerated. They are never an
  authority for approval, waiver, or event history.
- A version change requires its JSON Schema, manifest entry, fixtures, verification code,
  CLI documentation, and a migration test in the same release.
- Backward-compatible readers may be added for an older version, but writers emit only
  the current version. Removing a reader requires a major Gantry release and a documented
  migration path.

## Version changes

- Patch-level implementation fixes do not change `schema_version` when serialized meaning
  is unchanged.
- Additive optional fields require a reviewed minor schema version and an older-reader
  behavior test.
- Removed, renamed, or semantically changed fields require a major schema version plus an
  explicit migration command before the new writer becomes the default.

Until a migration exists, the safe response to an unsupported record is to stop and ask
the operator to use a compatible Gantry version.

Operators can verify the active vault and migration registry without changing records:

```bash
gantry vault check
gantry vault migrate --dry-run
```

Durable multi-file writes use checksummed transaction journals. After an interrupted
process, `gantry vault check --repair` rolls a complete journal forward before schema and
event verification. Transaction recovery is not a schema migration and never changes the
meaning of a published record.
