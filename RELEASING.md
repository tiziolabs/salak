# Releasing

A release publishes, on the GitHub page of the project:

| File | Built on |
| --- | --- |
| `salak_<version>-1_amd64.deb` | Linux, with `cargo deb -p salak-gtk --profile dist` |
| `Salak_<version>_x64-setup.exe` | Windows, with `scripts\package-windows.ps1` |
| `salak-<version>-windows-x64-portable.zip` | Windows, same script |
| `salak-<version>.tar.gz` and `.asc` | Linux, with `git archive`, signed |
| Source code (zip and tar.gz) | GitHub, from the tag |

The signed tarball is the one Debian downloads and checks (`debian/watch`):
see "Signing" below.

The version is written in two places: `[workspace.package]` of the root `Cargo.toml`, for `salak-core` and
`salak-gtk`, and `[package]` of `crates/salak-tauri/Cargo.toml`, which is a workspace of its own. The Tauri
configuration and the packages take it from there.

## 1. Prepare

1. Set the same `version` in `[workspace.package]` of the root `Cargo.toml` and in `crates/salak-tauri/Cargo.toml`
   (also the `version` of its `salak-core` dependency), then run `cargo build -p salak-gtk` at the root and
   `cargo build` in `crates/salak-tauri` to update both `Cargo.lock`. Both applications and `salak-core` share
   this version.
2. Replace `Unreleased` with the date in `CHANGELOG.md`.
3. Check the version and the date at the top of
   `crates/salak-gtk/data/salak.1` (`.TH`), and add the release to
   `<releases>` in `crates/salak-gtk/data/com.tiziolabs.salak.metainfo.xml`.
4. Commit, tag with a signature and push:

   ```sh
   git commit -am "chore: release 0.1.0"
   git tag -s v0.1.0 -m "Salak 0.1.0"
   git push origin main v0.1.0
   ```

## 2. Build

On Linux, preferably the oldest release to support (see the README):

```sh
git checkout v0.1.0
cargo deb -p salak-gtk --profile dist
# → target/debian/salak_0.1.0-1_amd64.deb
git archive --prefix=salak-0.1.0/ -o target/salak-0.1.0.tar.gz v0.1.0
gpg --armor --detach-sign target/salak-0.1.0.tar.gz
# → target/salak-0.1.0.tar.gz and target/salak-0.1.0.tar.gz.asc
```

On Windows:

```powershell
git fetch --tags
git checkout v0.1.0
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
# → dist\Salak_0.1.0_x64-setup.exe
# → dist\salak-0.1.0-windows-x64-portable.zip
```

The Linux build (`salak-gtk`) and the Windows build (`salak-tauri`) are separate workspaces, with their own
`Cargo.lock` and `target/`.

## 3. Publish

On GitHub, **Releases › Draft a new release**:

1. Choose the tag `v0.1.0`, and the title `Salak 0.1.0`.
2. Paste the section of `CHANGELOG.md` as description.
3. Attach the `.deb`, the signed tarball and its `.asc`, the installer and
   the portable zip. The source archives are added by GitHub.
4. **Publish release**.

With the [GitHub CLI](https://cli.github.com), the same from both machines:

```sh
# Linux: create the release as a draft, with the .deb
gh release create v0.1.0 --draft --title "Salak 0.1.0" --notes "…" \
  target/debian/salak_0.1.0-1_amd64.deb target/salak-0.1.0.tar.gz target/salak-0.1.0.tar.gz.asc
# Windows: add the Windows files, then publish
gh release upload v0.1.0 dist\Salak_0.1.0_x64-setup.exe dist\salak-0.1.0-windows-x64-portable.zip
gh release edit v0.1.0 --draft=false
```

## Signing

Tags and the source tarball are signed with the OpenPGP key of the
maintainer, published on <https://keys.openpgp.org> and on GitHub. Its public
part is also in the Debian package (`debian/upstream/signing-key.asc`), so
a new key must be announced there before it signs a release.

Once per machine:

```sh
git config --global user.signingkey <fingerprint>
git config --global tag.gpgSign true
```

Check a release with `git tag -v v0.1.0` and
`gpg --verify salak-0.1.0.tar.gz.asc salak-0.1.0.tar.gz`.

## Dependencies

`salak-core` and `salak-gtk` are meant to be packaged by Debian, which builds
offline from its own packaged crates. So:

- Before raising the version of a dependency of these crates, check that
  Debian unstable has it: `https://packages.debian.org/sid/librust-<crate>-dev`.
  The reference is unstable, where new packages enter Debian, not a stable
  release: see P2 of `docs/debian-packaging.md`.
- `gtk4`, `libadwaita`, `sourceview5`, `pulldown-cmark`, `glib` and `gio` are
  excluded from dependabot minor and major updates for that reason; patch
  updates are accepted.
- A new dependency needs a strong reason and must already be in Debian.
- Do not use system library features newer than GTK 4.16 and libadwaita 1.6
  (the `v4_16` and `v1_6` features of `salak-gtk`). Raising them is possible
  up to GTK 4.18 and libadwaita 1.7 (Debian 13), together with the minimum
  versions written in the README and the CHANGELOG.

`salak-tauri` is not packaged by Debian and is free of these rules.
