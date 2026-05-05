# Security Policy

## Supported Versions

| Version | Supported |
|---|---|
| 0.1.x | Yes |

## Reporting a Vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Report through GitHub Security Advisories for the repository, or email
[kenneth@reflective.se](mailto:kenneth@reflective.se).

You should receive a response within 48 hours.

## Security Notes

- Connector observations must carry provenance.
- Source-specific terms, rate limits, and compliance constraints are part of the
  port design.
- Secrets and credentials do not belong in the library API or committed config.
- Stub providers are for deterministic tests only and must not be confused with
  production integrations.

## Operator Responsibility

Operators are responsible for credential storage, rate-limit handling, ToS
review, audit logging, and approving any production provider implementation.
