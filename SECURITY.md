# Security Policy

## Supported versions

Only the latest release receives security fixes.

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's
[private vulnerability reporting](https://github.com/bekirhankarakurt/telemetry-tui-dashboard/security/advisories/new)
instead of opening a public issue. Include the version, steps to reproduce and,
if possible, a sample packet or recording that triggers the problem.

You can expect an acknowledgement within a week and a fix or an assessment
within 30 days.

## Scope notes

The dashboard receives untrusted UDP datagrams. The decoder validates length,
magic and CRC before reading fields, never indexes past a checked length, and
the tool contains no `unsafe` code. Source tracking is capped at 32 senders to
bound memory use. UDP is unauthenticated: a CRC detects corruption, not
tampering, so do not rely on the displayed data across untrusted networks.
