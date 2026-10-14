# Security Policy

## Reporting a vulnerability

Please **do not** open a public issue for security vulnerabilities.

Instead, report them privately via GitHub's
[private vulnerability reporting](https://github.com/trygantry/gantry/security/advisories/new)
or by email to **hi@gantryapp.com**.

Include:

- A description of the vulnerability and its impact
- Steps to reproduce
- Affected version(s)

We aim to acknowledge reports within 72 hours and to provide a remediation timeline
after triage.

## Scope notes

Gantry is local-first: it reads and writes files in the user's project folder. Of
particular interest are reports involving:

- Path traversal or writes outside the `.gantry/` vault
- Markdown/YAML/JSON parsing leading to unintended file writes or code execution
- Corruption of user-authored markdown during write-back

## Supported versions

Until a `1.0` release, only the latest `main` and the most recent release receive
security fixes.
