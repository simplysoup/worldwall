# 2026-10-01 — Arch/AUR package increment

## Scope and ownership

- Stage / requirements: S11-AC1 packaging smoke follow-up; WW-12.
- Outcome: add an x86_64 Arch `PKGBUILD`/`.SRCINFO` and build a transferable
  package without publishing it or changing the active desktop/session.
- Owner: Pi coding agent. This increment does not request or record maintainer
  acceptance; the historical S11 acceptance is unchanged.
- Source revision packaged: `fe42865000d71802231bd64baed12ca8e4e247ce`
  (`0.1.0.r20260924.gfe42865`). No runtime source files changed.

## Implemented

- Added `packaging/aur/PKGBUILD`, generated `packaging/aur/.SRCINFO`, a
  package-specific installed quick-start, and build-output ignores.
- The package installs `worldwall` and `worldwallctl` into `/usr/bin`, an
  example config and quick-start under `/usr/share/doc/worldwall/`, and the MIT
  license. It does not install artwork, registries, a private config, a user
  service, or compositor/session settings.
- Declared `wayland` and `vulkan-icd-loader` runtime dependencies; a GPU-specific
  Vulkan driver remains operator-provided. The recipe runs Cargo tests and a
  locked release build. It removes GCC's `-flto=auto` from C dependency flags
  because those GCC LTO objects failed to link with rust-lld, and remaps Rust
  build paths out of the resulting binaries.
- Added Arch package instructions to [the release runbook](../release.md#arch-linux-package).

## Verification

| Item | Command/artifact | Result |
| --- | --- | --- |
| PKGBUILD syntax and metadata | `bash -n packaging/aur/PKGBUILD`; `makepkg --printsrcinfo --dir packaging/aur` compared byte-for-byte with `.SRCINFO` | pass |
| Package build and Rust checks | `WORLDWALL_LOCAL_REPO="$(realpath ../..)" makepkg --noconfirm --force` from `packaging/aur/` | pass; 132 application tests passed, 3 Vulkan adapter tests ignored, and 78 CLI tests passed |
| Artifact inspection | `pacman -Qip` and `bsdtar -tf target/worldwall-aur-packages/worldwall-0.1.0.r20260924.gfe42865-1-x86_64.pkg.tar.zst` | pass; expected binaries/docs/license only; no artwork or scene fixtures |
| Build-path inspection | extracted packaged `worldwall` binary and searched strings for this checkout's makepkg source path / `$srcdir` | no matches |
| Documentation / Python tools | `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v` | pass; 284 Markdown files and 117 tests |
| Diff hygiene | `git diff --check`; `bash -n packaging/aur/PKGBUILD` | pass |

Package artifact: `target/worldwall-aur-packages/worldwall-0.1.0.r20260924.gfe42865-1-x86_64.pkg.tar.zst`
SHA-256: `63d8a350162133e19bfdfced07a63722b1f0b2b048f0bd578e908e45c405b63d`

The build used the pinned commit from the local checkout (`WORLDWALL_LOCAL_REPO`)
and downloaded locked Cargo dependencies. An initial unauthenticated GitHub
source fetch asked for credentials; anonymous HTTPS/AUR source availability is
not verified. The package archive itself is built and available locally for
transfer.

## Limitations and next action

- `pacman -Qip` and archive contents were inspected; the package was **not
  installed** on this machine, and no Wayland/Vulkan hardware run was attempted.
- The recipe is not published to the AUR. Public AUR availability requires a
  publicly fetchable source revision and a separate authorized AUR publication.
- The built archive is x86_64 only. Transfer it to an Arch Linux system with
  `wayland`, `vulkan-icd-loader`, and an appropriate Vulkan driver, then install
  with `sudo pacman -U ./worldwall-*.pkg.tar.zst`.
- No new stage gate is claimed accepted. Keep the existing S11 ledger state and
  historical acceptance unchanged.
