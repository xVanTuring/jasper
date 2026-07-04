# jasper-core

Pure-logic core for the [Joplin](https://joplinapp.org/) sync format, used by
[Jasper](https://github.com/jasper-note/jasper) — a lightweight Joplin-compatible client.

- `model` — `Note` / `Folder` / `Resource` / `Tag` / `NoteTag` plus `ItemType` / `MarkupLanguage` enums
- `parser` — parses Joplin `<32hex>.md` sync items (title / body / trailing `key:value` metadata block)
- `serialize` — writes items back: edits preserve unknown metadata verbatim, refresh timestamps only
- `library` — in-memory index: notebook tree, notes, resources, tags, full-text search, CRUD

No IO anywhere — the crate compiles unchanged to `wasm32` targets (it powers both the
Jasper server and its browser-only WASM demo). The data format itself is documented in
[`docs/joplin-data-format.md`](https://github.com/jasper-note/jasper/blob/main/docs/joplin-data-format.md).

## Features

- `serde` (off by default) — adds `Serialize`/`Deserialize` to the model types
  (used by the plugin ABI).

## License

MIT OR Apache-2.0, at your option.
