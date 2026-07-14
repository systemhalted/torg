# Releasing torg

Releases are cut by pushing a version tag. GitHub Actions
([`.github/workflows/release.yml`](../.github/workflows/release.yml)) does the rest — it
cross-builds the binaries, packages them, and publishes a GitHub Release.

## Cut a release

1. Bump the version in the workspace manifest (`Cargo.toml`, `[workspace.package] version`) and
   commit it on `main`.
2. Tag and push:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

   The tag must match `v*.*.*`. The workflow reads the version from the tag name.

3. Watch the run under the repo's **Actions** tab. When it finishes, the release at
   `releases/tag/v0.2.0` carries:
   - `torg-v0.2.0-<target>.tar.gz` (+ `.sha256`) for all four targets,
   - `torg-<target>.deb` for both Linux architectures,
   - `torg.rb`, the rendered Homebrew formula.

## What runs on each push

[`ci.yml`](../.github/workflows/ci.yml) runs `cargo test --workspace` and
`cargo clippy --all-targets -- -D warnings` on every push to `main` and every PR, so tagged
commits are already known-green.

## One-time setup for Homebrew

The `brew install systemhalted/tap/torg` route needs a **tap repository** the release workflow
can push the formula to. Until it exists, every release still attaches `torg.rb` as an asset —
the automatic push is simply skipped.

1. Create a public repo named **`homebrew-tap`** under the same owner
   (`systemhalted/homebrew-tap`). An empty repo is fine; the workflow creates `Formula/torg.rb`.
2. Create a token that can write to it — a fine-grained PAT scoped to `homebrew-tap` with
   **Contents: read and write** (a classic PAT with `repo` also works).
3. Add it to **this** repo as an Actions secret named **`HOMEBREW_TAP_TOKEN`**
   (Settings → Secrets and variables → Actions → New repository secret).

On the next tagged release the `homebrew` job renders `torg.rb` from
[`packaging/homebrew/torg.rb.tmpl`](../packaging/homebrew/torg.rb.tmpl) with the correct
version and per-arch `sha256`, commits it to `homebrew-tap/Formula/torg.rb`, and `brew install`
starts working.

## Notes and limitations

- **The workflows only run on GitHub's runners** — they can't be exercised locally. The first
  real tag is the smoke test; check the Actions logs if an asset is missing.
- **macOS binaries are unsigned.** Homebrew installs are unaffected; direct downloads need a
  one-time `xattr -d com.apple.quarantine` (documented in [`install.md`](install.md)). Signing
  and notarization would need an Apple Developer account and is deliberately out of scope.
- **Cross-built Linux ARM64 binaries are left unstripped** (the x86 host `strip` can't process
  an ARM64 ELF); they are functionally identical, just a little larger.
