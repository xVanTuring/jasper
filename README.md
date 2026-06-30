# Joplin Web

**English** · [中文](README.zh-CN.md)

[![Deploy WASM demo to Pages](https://github.com/xVanTuring/joplin-web/actions/workflows/pages.yml/badge.svg)](https://github.com/xVanTuring/joplin-web/actions/workflows/pages.yml)

> 🌐 **Live demo (runs entirely in your browser via WASM, no server):** https://xvanturing.github.io/joplin-web/

A lightweight, **read-write** [Joplin](https://joplinapp.org/)-compatible client: a local **Rust (axum) server + browser SPA** — no Electron, Tauri, or WebView. It reads and writes your existing Joplin sync library directly, so edits are picked up by Joplin on its next sync.

![Reading view](docs/screenshots/01-reading.png)

## Why

- **Tiny & fast** — the Rust backend stays resident at ~10 MB and starts almost instantly. No 200 MB Electron runtime.
- **Cross-platform** — a single binary on macOS / Windows / Linux; the UI is just a browser tab.
- **Your data, your files** — operates directly on the Joplin sync format (`<id>.md` items + `.resource/` blobs). Nothing is locked into a new database.

## Features

### 📂 Reads & writes real Joplin libraries
- Data sources: **local folder** or **WebDAV** (Nextcloud, etc.).
- Three-pane UI: notebook tree (nested, with note counts) · note list · reader/editor.
- Full-text search across note titles and bodies.
- Create a brand-new library, or connect to an existing one — chosen in a first-run setup wizard. Switch data source at any time from ⚙.

### ✍️ Editing
- CodeMirror 6 editor with Markdown highlighting, lazy-loaded so it never bloats first paint.
- **Autosave** while you type; create / update / delete notes.
- Write-back preserves every metadata field verbatim and only refreshes the timestamps — minimal diff, friendly to Joplin's conflict handling.

### 🎨 Rich Markdown rendering
- Code highlighting, tables, task lists, blockquotes.
- **Math** via KaTeX (inline `$…$` and block `$$…$$`).
- Embedded images and attachments via Joplin's `:/resourceId` links.
- HTML notes are sanitized (DOMPurify) and shown as-is.

### 🖼️ Resources & image upload
- **Paste**, **drag-and-drop**, or use the **📎 attach** button in the editor — the file is uploaded as a Joplin resource and a `:/id` reference is inserted at the cursor.
- **Resource manager** panel (🖼 in the top bar): thumbnails, type & size, **reference counts**, rename, delete, and **one-click orphan cleanup** for resources no note links to.

![Resource manager](docs/screenshots/03-resources.png)

### ⚡ Incremental cache
- A local SQLite cache stores each item's content + modification time, scoped per data source.
- On startup it only fetches **new or changed** items — over WebDAV the second launch makes **zero** `GET` requests for unchanged notes, just one directory listing.

### 📝 Editor & 🔎 Search

| Editor (paste / drag / attach) | Full-text search |
| --- | --- |
| ![Editor](docs/screenshots/02-editor.png) | ![Search](docs/screenshots/04-search.png) |

## Quick start

### From source

```bash
# Backend — serves the API and the built UI on http://127.0.0.1:27583
cd server && cargo run                       # uses saved config, or the setup wizard
cd server && cargo run -- /path/to/JoplinDir # bootstrap with a local folder
cd server && cargo run -- https://host/dav/  # or a WebDAV URL

# Frontend
cd web && pnpm install
cd web && pnpm build      # produces web/dist, served by the backend at :27583
cd web && pnpm dev        # or hot-reload dev server on :5173 (proxies /api)
```

Then open **http://127.0.0.1:27583/**. On first run the wizard lets you pick *existing library* / *new library* × *local* / *WebDAV*.

### Single binary

The frontend can be **embedded into the executable** (via [rust-embed](https://crates.io/crates/rust-embed)) so the whole app ships as one self-contained file — no separate `web/dist` at runtime:

```bash
cd web && pnpm build                                   # build web/dist first
cd server && cargo build --release --features embed     # → server/target/release/joplin-lite (~5 MB)
```

Copy that single binary anywhere and run it. The `embed` feature is opt-in; without it the backend serves `web/dist` from disk as before (so plain `cargo run` still works without building the frontend).

### Docker

```bash
docker compose up --build   # then open http://localhost:27583/
```

The image is a single embedded binary on `debian-slim` (frontend baked in). The config directory is a mounted volume so your data-source settings and cache persist.

#### Pre-built image from GHCR

Pushed images are published to the GitHub Container Registry by [`.github/workflows/docker.yml`](.github/workflows/docker.yml):

```bash
docker run -p 27583:27583 -v joplin-config:/config \
  ghcr.io/<owner>/joplin-lite:latest      # then open http://localhost:27583/
```

`main` builds tag `latest` (+ `sha-…`); version tags (`v1.2.3`) get semver tags.

### Try it with demo content

`docs/gen-demo-library.py` generates a small sample library (the one in these screenshots):

```bash
python3 docs/gen-demo-library.py /tmp/joplin-demo
cd server && cargo run -- /tmp/joplin-demo
```

### Browser-only demo (WASM, no server)

The core (model / parser / serializer / index) is a dependency-light `joplin-core` crate that also compiles to WebAssembly. With a bundled sample library it powers a **read-only, server-less preview** that runs entirely in the browser tab — handy for a static demo (e.g. GitHub Pages).

```bash
# needs: rustup target add wasm32-unknown-unknown + wasm-pack
cd web && pnpm build:demo   # builds the wasm, then a static demo bundle into web/dist
```

It does not affect the native build: when not in demo mode the wasm import is tree-shaken out entirely.

![Browser WASM demo](docs/screenshots/05-wasm-demo.png)

## How it works

```
Browser SPA (Svelte 5 + CodeMirror)  ──HTTP──▶  Rust server (axum)  ──read/write──▶  Local folder / WebDAV
```

The server scans the sync directory, parses every `<id>.md` item into an in-memory index, and exposes a small JSON API (`/api/folders`, `/api/notes`, `/api/resources`, `/api/search`, …). The SPA renders Markdown client-side and rewrites `:/id` resource links to `/api/resources/id`.

## Compatibility & limitations

- Targets the **Joplin v3.x sync format** (sync target version 3), **unencrypted** libraries.
- Handles notes, notebooks, resources, tags, and note-tags; revisions and other internal types are skipped.
- This client does **not** participate in Joplin's sync lock protocol — fine for personal use; if both sides edit the same note, Joplin creates a conflict copy as usual.
- No authentication yet — bind to `127.0.0.1` (default), or put it behind your own auth before exposing on a LAN.
- WebDAV passwords are stored **in plaintext** in the local config DB.

## Tech stack

Rust · axum · rusqlite · rayon · ureq — Svelte 5 (runes) · Vite · TypeScript · CodeMirror 6 · markdown-it · KaTeX · highlight.js · DOMPurify.
