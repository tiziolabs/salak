# Releasing

A release publishes, on the GitHub page of the project:

| File | Built on |
| --- | --- |
| `salak_<version>-1_amd64.deb` | Linux, with `cargo deb -p salak-gtk` |
| `Salak_<version>_x64-setup.exe` | Windows, with `scripts\package-windows.ps1` |
| `salak-<version>-windows-x64-portable.zip` | Windows, same script |
| Source code (zip and tar.gz) | GitHub, from the tag |

The version is only written in the root `Cargo.toml` (`[workspace.package]`): the Tauri configuration and the
packages take it from there.

## 1. Prepare

1. Set `version` in `[workspace.package]` of the root `Cargo.toml`, then run `cargo build -p salak-gtk` to update
   `Cargo.lock`. Both applications and `salak-core` share this version.
2. Replace `Unreleased` with the date in `CHANGELOG.md`.
3. Commit, tag and push:

   ```sh
   git commit -am "chore: release 0.1.0"
   git tag -a v0.1.0 -m "Salak 0.1.0"
   git push origin main v0.1.0
   ```

## 2. Build

On Linux, preferably the oldest release to support (see the README):

```sh
git checkout v0.1.0
cargo deb -p salak-gtk
# → target/debian/salak_0.1.0-1_amd64.deb
```

On Windows:

```powershell
git fetch --tags
git checkout v0.1.0
powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
# → dist\Salak_0.1.0_x64-setup.exe
# → dist\salak-0.1.0-windows-x64-portable.zip
```

The Linux build (`salak-gtk`) and the Windows build (`salak-tauri`) are separate:
never build both in one command, they write the same `target/release/salak`.

## 3. Publish

On GitHub, **Releases › Draft a new release**:

1. Choose the tag `v0.1.0`, and the title `Salak 0.1.0`.
2. Paste the section of `CHANGELOG.md` as description.
3. Attach the `.deb`, the installer and the portable zip. The source archives
   are added by GitHub.
4. **Publish release**.

With the [GitHub CLI](https://cli.github.com), the same from both machines:

```sh
# Linux: create the release as a draft, with the .deb
gh release create v0.1.0 --draft --title "Salak 0.1.0" --notes "…" \
  target/debian/salak_0.1.0-1_amd64.deb
# Windows: add the Windows files, then publish
gh release upload v0.1.0 dist\Salak_0.1.0_x64-setup.exe dist\salak-0.1.0-windows-x64-portable.zip
gh release edit v0.1.0 --draft=false
```
