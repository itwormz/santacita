# Desktop UI

Tauri + Svelte/TypeScript frontend of the SantaCita desktop application (Rust).
It shows the main window: profiles and their settings (timings, search
behaviour, Telegram and webhook notifications, pairing), the live search state
and work journal, profile statistics with the timeline and office history, the
daily appointment budget, appointment notes, achievements and public cards, and
application updates. Everything it does goes through scoped Rust commands; the
browser, the search and the notifications run in the native part, and site
content is never loaded into this window.

Node is a build and test tool only, not a sidecar.

A debug build uses a separate application ID and runtime under
`.local/builds/runtime/rust-preview`, so it does not open the installed
application's profiles or credentials. Debug-only `CEH_PREVIEW_*` variables
(for example `CEH_PREVIEW_LEGACY_COPY`, which opens a prepared copy of older
data from `.local/builds/runtime/legacy-compat/<case>/rust`) exist for looking
at the interface and are ignored in release builds.

## Local macOS development

Use the repository's Rust toolchain and Node 26 (`.nvmrc`). From the repository root:

```sh
node desktop-ui/scripts/prepare.mjs
npm run test --prefix desktop-ui
cargo test -p ceh-desktop
```

The preparation script installs the exact frontend lockfile under
`.local/builds/work/rust-ui`, generates the existing app icon and checks/builds
the frontend. `desktop-ui/node_modules` is an ignored link to those dependencies.
The frontend output is `.local/builds/work/rust-ui-dist`.

To compile the native shell without launching it:

```sh
cargo check -p ceh-desktop --features shell,custom-protocol --locked
```

To create the separate debug `.app`, after preparing the frontend:

```sh
cd crates/ceh-desktop
node ../../desktop-ui/node_modules/@tauri-apps/cli/tauri.js build --debug --features shell --bundles app --ci
```

Cargo output follows the workspace target directory under `.local/builds`.
The app bundle is in `rust-target/debug/bundle/macos/SantaCita.app`.
The Tauri build hook always checks and rebuilds frontend assets before embedding
them; it does not install dependencies. Run the preparation script first on a
fresh checkout. The local macOS debug bundle uses an explicit ad-hoc bundle signature, binding its Info.plist
and resources; it is not a Developer ID signature or notarization. Verify it with
`codesign --verify --deep --strict` before native acceptance. An already running
debug build must be closed before replacing its bundle.

These instructions describe the local macOS debug build; other native targets and
release installers have separate acceptance gates.

Only the local main webview can call desktop commands. Site content is not loaded
into that privileged view. UI assets reuse the existing application fonts,
translations, icons and theme; their original licences remain applicable.
