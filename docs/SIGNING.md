# Publisher signatures and trust

Approved publisher: **KiaroSama**. Product: **Numera Image-Gen MCP**.
The project has an approved RSA-3072 self-signed code-signing identity. Its public material is in
[signing/publisher.json](signing/publisher.json), [publisher-cert.der](signing/publisher-cert.der) and
[publisher-key.der](signing/publisher-key.der). These are public files, not signing credentials.

## Current delivery status

Public signed test manifests have been authenticated independently. They do **not** establish that
a native executable or release archive has been signed. Platform-native signing, protected hosted
custody, final package/extraction verification and release acceptance remain pending. Existing
unsigned CI archives are internal migration test artifacts, not completed signed distributions.
Do not install them over a working runtime or describe them as a verified Release.

## What the signature must prove

Windows executables require embedded Authenticode signatures; macOS executables require genuine
certificate-backed Mach-O signatures. An ad-hoc Apple signature is not an owner signature. Linux
standalone binaries use authenticated detached signatures or a signed manifest covering their exact
bytes. A detached manifest supplements, never replaces, supported Windows/macOS embedded signatures.
Both the MCP executable and optional developer executable must satisfy their target contract.

The native prepared-artifact verifier authenticates exact manifest bytes against its compiled public
key, checks target/version/source metadata and bounded regular member hashes, and requires the exact
approved logo. A certificate/key supplied by the artifact cannot replace the compiled pin. Final
platform verification must independently validate the native executable signatures, then repeat on
packaged/extracted and downloaded bytes before activation.

Public-key SHA-256:

```text
31ccb588ae7e529535dcccd94f06d8d39ae1b8847b7003e884ad8f44cd340c80
```

Certificate DER SHA-256:

```text
657dd7869e90e2bf9a49942d5098e1e3a82e22cf84add51aa958e3dfd0af937f
```

Compare these through an already authenticated project source/channel. A checksum and a replaceable
public key beside an untrusted download are not independent signer authentication.

## Deliberate trust limits

Self-signed means private, pinned identity, **not public OS trust**. It does not promise SmartScreen
or Gatekeeper acceptance and is not Apple Developer ID or notarization. No system trust store,
Root/TrustedPublisher store, Gatekeeper policy or owner's keychain is changed to manufacture a pass.
A signature's integrity result and the operating system's public-trust decision are separate facts.

The current certificate is valid from 2026-10-17T20:07:22Z through 2027-10-17T20:17:22Z. No external
cryptographic timestamp was requested. Verification requires current validity; expiry blocks final
signing/publication until an explicitly authorized identity decision. The self-signed identity has
no public CA revocation service. Withdrawal/rotation requires a reviewed pin change; failed or
ambiguous signing never triggers automatic key regeneration, unsigned fallback or another signer.

## Hosted custody boundary

Signing secrets are not in Git, examples, command arguments, logs, caches, fixtures or downloadable
artifacts. Keep the existing encrypted exportable identity in access-restricted local custody and
retain its recovery information. Never upload a whole credential registry or private custody folder.
Only exact separately authorized signing inputs may enter a reviewed protected GitHub environment.

A workflow job referencing a nonexistent environment can create it **without protection rules**.
Do not treat an environment name in YAML as evidence that the route is protected. Before activation,
verify the intended repository, exact SHA, trusted workflow/actions, allowed refs, protection rules,
explicitly selected inputs and private temporary-storage cleanup. Build/test jobs receive no signing
key. No untrusted PR code receives signing secrets. Missing custody/verifier/protection blocks final
artifacts and Release, while independent source/test work may continue.

No certificate purchase, paid signing service, new account, external timestamp authority, trust
installation or actual host registration is implicit in these instructions.

## Verification fixtures

[Public fixtures](../tests/fixtures/native-signing/README.md) cover authentic manifests, changed
version/binary bytes, unrelated signer, tampering and a genuinely signed but unapproved logo. Their
synthetic executable bytes are never run. DER certificates/public keys and raw signatures are binary;
JSON manifests are exact UTF-8 bytes excluded from formatting because whitespace is signed.
The native test execution and real platform-signature tests still need hosted evidence.

References: [Microsoft SignTool](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool),
[Apple Code Signing In Depth](https://developer.apple.com/library/archive/technotes/tn2206/_index.html),
[OpenSSL dgst](https://docs.openssl.org/3.6/man1/openssl-dgst/) and
[GitHub environments](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments).
