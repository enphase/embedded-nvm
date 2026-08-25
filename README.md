# embedded-nvm

A collection of crates for embedded nonvolatile memory (NVM) operations with a focus on simplifying management of device settings.

See the individual crate READMEs for detailed documentation.

- [**`versioned-postcard`**](versioned-postcard/README.md): versioned, migratable [postcard](https://crates.io/crates/postcard)
  records for `no_std`. Each version declares its predecessor and the migration from it; the version is stored as a leading varint tag.

- [**`embedded-nvm-settings`**](embedded-nvm-settings/README.md): an `NvmSettings` object holding an in-memory cache of your settings
  with read/modify access that is synchronous and ISR-safe, plus a deferred async commit that keeps
  flash off the hot path. The storage backend and serialization format are both traits, with
  built-in adapters for [sequential-storage](https://crates.io/crates/sequential-storage),
  `versioned-postcard`, and vanilla postcard.
