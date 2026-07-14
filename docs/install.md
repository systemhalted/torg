# Installing torg

`torg` ships as a prebuilt binary for macOS (Apple Silicon and Intel) and Linux (x86-64 and
ARM64), attached to each [GitHub release](https://github.com/systemhalted/textr-org/releases).
Pick whichever route fits your platform.

## macOS — Homebrew (recommended)

```sh
brew install systemhalted/tap/torg
```

Homebrew downloads the right binary for your Mac and keeps it updated with `brew upgrade`. It
also clears macOS's quarantine flag for you, so there's nothing else to do.

> The tap must exist for this to work — see [`releasing.md`](releasing.md) if `brew` reports
> the formula is missing.

## Debian / Ubuntu — `.deb`

Download the `.deb` for your architecture from the latest release and install it:

```sh
# x86-64
curl -LO https://github.com/systemhalted/textr-org/releases/latest/download/torg-x86_64-unknown-linux-gnu.deb
sudo apt install ./torg-x86_64-unknown-linux-gnu.deb

# ARM64 (e.g. Raspberry Pi, Ampere, Graviton)
curl -LO https://github.com/systemhalted/textr-org/releases/latest/download/torg-aarch64-unknown-linux-gnu.deb
sudo apt install ./torg-aarch64-unknown-linux-gnu.deb
```

`apt` pulls in nothing else — the binary is statically self-contained apart from libc. Remove
it later with `sudo apt remove torg`.

## Any platform — download the binary directly

Each release has a `torg-<version>-<target>.tar.gz` (plus a `.sha256` to verify it). Extract
`torg` and drop it on your `PATH`:

```sh
# example: Apple Silicon Mac, release v0.1.0
curl -LO https://github.com/systemhalted/textr-org/releases/download/v0.1.0/torg-v0.1.0-aarch64-apple-darwin.tar.gz
shasum -a 256 -c torg-v0.1.0-aarch64-apple-darwin.tar.gz.sha256   # optional, after downloading the .sha256
tar xzf torg-v0.1.0-aarch64-apple-darwin.tar.gz
sudo mv torg /usr/local/bin/
```

Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`.

### macOS Gatekeeper

The macOS binaries are **not code-signed**, so a directly-downloaded `torg` is quarantined the
first time you run it ("cannot be opened because the developer cannot be verified"). Clear the
flag once:

```sh
xattr -d com.apple.quarantine /usr/local/bin/torg
```

Installing via **Homebrew avoids this entirely** — it's only the manual-download path that trips
Gatekeeper.

## From source

With a Rust toolchain (1.96+):

```sh
git clone https://github.com/systemhalted/textr-org
cd textr-org
cargo install --path crates/tui   # installs `torg` into ~/.cargo/bin
```

## Verify it works

```sh
torg notes.org
```

opens the editor on `notes.org` (creating it on first save). See [`usage.md`](usage.md) for the
key bindings.
