# Releasing

Releases are built by GitHub Actions from a version tag
(`.github/workflows/release.yml`).

## Steps

1. Set the version in the workspace `Cargo.toml`:

   ```toml
   [workspace.package]
   version = "0.2.0"
   ```

   Every crate and the desktop app take their version from here.
2. Run the full checks locally ([Building and testing](building.md)),
   commit, and merge to `main`.
3. Tag the commit and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

   A tag with a suffix, such as `v0.2.0-rc.1`, makes a pre-release.
4. The workflow checks that the tag matches the workspace version, then
   creates a **draft** release with generated notes and attaches:

   | Platform | Desktop app | Command-line tools |
   |---|---|---|
   | macOS, Apple silicon | `.dmg`, `.app` | `i18n-harness-cli-<version>-macos-arm64.tar.gz` |
   | macOS, Intel | `.dmg`, `.app` | `…-macos-x64.tar.gz` |
   | Windows x64 | `.msi`, `-setup.exe` (NSIS) | `…-windows-x64.zip` |
   | Linux x64 | `.deb`, `.AppImage` | `…-linux-x64.tar.gz` |

   Each command-line archive contains `harness`, `harness-lab`, the
   licence files and the README, with a `.sha256` checksum next to it.
5. Download and try the installers, edit the notes, and **publish the
   draft** by hand.

## Signing

- **macOS** builds are signed and notarised when the repository has the
  secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` and
  `APPLE_TEAM_ID`. Without them the builds are unsigned.
- **Windows** builds are unsigned.

For unsigned builds, users follow the steps in
[Install the app](../getting-started/install.md). Mention in the release
notes whether a build is signed.

## The book

The book is published from `main` by `.github/workflows/book.yml`
whenever `book/` changes, independently of releases. Keep it in step
with the release: update screenshots and pages in the same pull request
as the feature.
