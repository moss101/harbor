# Harbor — Security Policy & Contact

## Reporting

Report vulnerabilities via a **private GitHub security advisory** against
`github.com/moss101/harbor` (Security → Advisories → New draft advisory).
An operator-bound security email address and PGP key will be published
here before GA; until then, GitHub advisories are the only monitored
channel.

Please include: affected platform and build identity (the app shows the
version and git commit in About), reproduction steps, and impact
assessment. You will receive an acknowledgement within 5 business days.

## Scope notes

Of particular interest:

- Any egress that occurs in Local Only mode without explicit user
  authorization (this would be a critical integrity break).
- Plaintext private-workspace content at rest inside the app container.
- Bypass of per-effect user authority, stale-write protection, or run
  replay integrity.
- Signed-catalog verification bypasses (wrong hash accepted, epoch
  rollback).
- FFI boundary issues that corrupt the JSON dispatch contract.

Out of scope: store-side processes, the operator signing environment, and
denial-of-service of the developer's own infrastructure.

## Safe harbor

We will not pursue action against good-faith research that respects user
data, avoids service degradation, and reports privately.

## Hardening claims are evidence-bound

Security-relevant claims in release materials map to gates in
`evidence/releases/<version>/` (network capture reconciliation, plaintext
inspection, catalog verification tests, optional-capability zero-surface
proof). The evidence bundle for each release identifies the exact build it
was produced from.
