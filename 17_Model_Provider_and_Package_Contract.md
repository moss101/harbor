# Harbor model provider and package contract

## Model references
`InstalledPackage` points to Harbor-verified local files. `SystemManaged` identifies an OS/provider-owned model that may have no weight files. `RemoteEndpoint` identifies a user-configured remote model. Router logic operates on explicit capabilities rather than assuming every provider supports generation, embeddings, tools or vision.

## Provider contract
Every provider exposes availability, capabilities, load progress, cancellable load/generate/embed operations and cleanup ownership. Unsupported operations return `CapabilityUnavailable`; absence is not an error that permits silent provider substitution. Cancelled/failed load must release mapped files, GPU allocations and temporary state.

## Package install
`harbor.model/v3` supports multiple data files with roles and SHA-256. Required files install into a staging directory, every size/hash is checked, license/policy is accepted, then the package is atomically made visible. Interrupted or partial installs are not loadable. Downloaded executable code and `trust_remote_code` behavior are forbidden.

## Catalog trust
Catalog metadata has monotonic epoch, key ID, not-before/not-after and signature. The client ships a root keyring and accepts rotation only through signed metadata. Revoked/expired keys and lower epochs are rejected. A last-known-good catalog may be used offline for at most 30 days after its expiry policy; model packages already installed remain usable if their own integrity metadata is valid.

## Benchmark identity
A benchmark result is keyed by weight/package hash, runtime build hash, backend, context size, quantization, GPU offload parameters, thread settings, OS version and normalized device hardware class. Results from another identity are estimates, not measured facts.


## Reference validation
Installed packages require a nonempty complete weight-file set, license acceptance, source revision, storage classification and per-file normalized relative path/size/SHA-256. System-managed references require provider and provider-model identity and cannot carry installed files. Remote references require provider/model identity and a local endpoint-profile reference, never inline credentials.

Installer semantic checks reject absolute or escaping paths, separators/aliases invalid on the target OS, duplicate normalized or case-colliding paths, reserved device names, symlinks and hardlinks. Directory traversal checks use the actual granted destination handle, not string prefix tests. Catalog sources require signing key and monotonic epoch; private sources require encrypted workspace storage. Every required shard and declared role must be present before publication. The global schema bound is not device admission: apply lower disk/memory/package ceilings before allocation.

Each system provider has its own M4 activation gate. A qualified GGUF path is always available on declared custom-model devices. Lower-memory devices are capability-limited and cannot depend on an unqualified optional system provider to satisfy core GA.
