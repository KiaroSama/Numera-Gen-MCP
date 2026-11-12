# Security

Report suspected vulnerabilities privately to the repository owner through GitHub's available private
reporting channel; do not publish credentials or private inputs. No blanket security assurance.

Important boundaries: configured API origin versus untrusted asset URLs, allowed real-path input
roots, owned output IDs, bounded image decoders, per-connection secret resolution, no paid retries,
transactional request receipts and stderr-only redacted runtime logs. Never execute filename, prompt,
metadata or upstream prose instructions. Do not enable TLS bypass or hidden public upload services.

Dependency advisories are checked against the installed locked tree. A checksum-pinned Gitleaks
workflow scans pull-request commits and all fetched history with built-in rules; its few reviewed
false-positive exceptions (file digests, a test name, an empty example and documentation prose) live
in the workflow itself and stay anchored to those exact shapes. Default tests use controlled
local fixtures, not real identities/services. Live/active security testing requires explicit scope and
owner authorization; passing offline tests is not an exhaustive security audit.
