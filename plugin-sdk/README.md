# jasper-plugin-sdk

SDK for writing [Jasper](https://github.com/xVanTuring/jasper) plugins in Rust,
compiled to `wasm32-unknown-unknown` and run inside the host's wasmi sandbox.

The crate's **minor version tracks the plugin API spec version** (SDK 0.2.x ⇔
`apiVersion = "0.2"` in `manifest.toml`). The full contract lives in
[`docs/plugin-spec.md`](https://github.com/xVanTuring/jasper/blob/main/docs/plugin-spec.md).

## Quick start

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
jasper-plugin-sdk = "0.2"
```

```rust
use jasper_plugin_sdk as sdk;
use sdk::core::model::Note;

fn before_save(mut note: Note) -> Result<Note, String> {
    note.body = note.body.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
    Ok(note)
}

sdk::register! { before_save: before_save }
```

Build with `cargo build --release --target wasm32-unknown-unknown`, then zip
`manifest.toml` + `plugin.wasm` (+ assets) into a `.jplug` package.

## What you get

- `register!` — wires your code to the wasm ABI (`plugin_dispatch`); the three
  slots `before_save`, `storage`, `command` can be combined freely
- `host` — typed wrappers over `host_call`: `log`, `now_ms` (the sandbox has no
  clock), `settings_get`/`settings_set` (needs `settings` capability),
  `http_request` (needs `host:http` capability)
- `storage::Storage` — implement this trait to provide a storage backend
  (declared via `[[contributes.storage]]` in the manifest)
- `sdk::core` — re-export of [`jasper-core`](https://crates.io/crates/jasper-core)
  model types crossing the ABI

## Gotchas

- Don't pull dependencies that drag in `wasm-bindgen` (e.g. `chrono` with default
  features) — the sandbox is plain wasm, not a JS environment. The SDK registers
  a `getrandom` error stub so `wasm32-unknown-unknown` links.
- Reference plugins live in
  [`plugins-examples/`](https://github.com/xVanTuring/jasper/tree/main/plugins-examples)
  (before-save hook, storage providers, editor command).

## License

MIT OR Apache-2.0, at your option.
