# torg on iOS / iPadOS — a spike

This is a **proof-of-concept**, not a shippable app. It exists to de-risk one thing: that
torg's headless Rust core (`torg-core`) can drive a native SwiftUI frontend across an FFI
boundary — the payoff of the "shared core, thin frontends" architecture, applied to mobile.

The SwiftUI app renders a torg outline and lets you fold subtrees and cycle a heading's TODO.
Every structural operation is computed by `torg-core` (via the [`torg-ffi`](../crates/ffi)
crate and [UniFFI](https://mozilla.github.io/uniffi-rs/)); the Swift layer only renders and
handles touch. A segmented control flips the same document between the Org and Markdown
providers to show they share the commands.

## What's here

| Path | What it is |
|------|------------|
| `build-rust.sh` | Builds `torg-ffi` for iOS and assembles `TorgFFI.xcframework` + the Swift bindings. |
| `project.yml` | [XcodeGen](https://github.com/yonaskolb/XcodeGen) config for a reproducible `TorgSpike.xcodeproj`. |
| `TorgSpike/` | The SwiftUI app (`TorgSpikeApp.swift`, `ContentView.swift`). |
| `Generated/`, `Headers/`, `TorgFFI.xcframework/` | Build products (git-ignored). |

## Build and run (macOS)

Prerequisites: **Xcode** (with command-line tools), a Rust toolchain, and — for the reproducible
project — [XcodeGen](https://github.com/yonaskolb/XcodeGen) (`brew install xcodegen`).

```sh
cd ios
./build-rust.sh          # builds the xcframework + Generated/torg_ffi.swift
xcodegen generate        # writes TorgSpike.xcodeproj
open TorgSpike.xcodeproj # ⌘R to run in the iPad simulator
```

No XcodeGen? Create a new **iOS App** in Xcode and add: the `TorgSpike/` Swift files,
`Generated/torg_ffi.swift`, and `TorgFFI.xcframework` (General → Frameworks, "Do Not Embed" —
it's a static library).

## How the bridge works

`torg-ffi` annotates a tiny API with UniFFI proc-macros (`crates/ffi/src/lib.rs`):

```rust
#[uniffi::export] pub fn outline(text: String, markdown: bool) -> Vec<HeadingInfo>;
#[uniffi::export] pub fn cycle_todo(text: String, line: u32, markdown: bool) -> String;
```

`build-rust.sh` compiles that crate as a static library for the iOS device and simulator, then
runs `uniffi-bindgen` to emit `torg_ffi.swift` (idiomatic Swift wrappers) and a C
header/modulemap. Xcode links the xcframework; the Swift app just calls `outline(text:markdown:)`
and `cycleTodo(...)`.

## Scope and next steps

- **In scope:** the parse/edit round-trip across FFI, folding, both formats.
- **Not yet:** persistence/Files integration, a real text editor surface, the full command set,
  and a touch-native interaction model (the terminal's keyboard chords don't transfer).
- **Open design question:** how much of the TUI's state tier (buffers, fold state, format
  detection in `crates/tui`) to lift into a UI-agnostic session layer in the core so both the
  terminal and mobile frontends share it, versus reimplementing per frontend.
