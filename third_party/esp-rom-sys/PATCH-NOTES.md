# Vendored esp-rom-sys (patched)

This is a verbatim copy of `esp-rom-sys 0.1.3` from crates.io with **one** change in
`src/lib.rs`: the `strcasecmp` shim called `to_ascii_lowercase()` on a `c_char` (`i8`),
which only exists on `u8`/`char`. The Xtensa `esp` toolchain's rustc rejects this and it
breaks every firmware build. We cast to `u8` first.

Wired in via the workspace root `Cargo.toml` `[patch.crates-io]`. Drop this once an
upstream esp-rom-sys release ships the fix and the dependency tree picks it up.
