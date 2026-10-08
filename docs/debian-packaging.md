# Packaging plan: Salak in Debian and Ubuntu

This document tracks the work needed to get Salak (the GTK application,
`salak-gtk`) into Debian unstable, then into Ubuntu, with a PPA meanwhile.
It is self-contained: read sections 1 to 4 once, then take any task of
section 6 whose dependencies are done, and tick it in section 5 when it is.

Status: in progress, written on 2026-10-07 from an audit of the
`docs/gtk-migration-plan` branch (commit `6bfdfe9`). It supersedes phase 9
and the decisions D1 and D2 of [gtk-migration-plan.md](gtk-migration-plan.md)
where they disagree (see P2 and P3).

## 1. Goals

1. **Salak in Debian unstable**, as a source package `salak` built by Debian
   from the upstream tarball with Debian's own Rust crates, then in testing
   and the next stable release (forky).
2. **Salak in Ubuntu**, by automatic sync from Debian into the development
   release after the upload to unstable.
3. **A PPA** for the Ubuntu releases that are already out and have recent
   enough system libraries, until the sync reaches them.
4. **The GitHub `.deb` keeps working** for Debian 13 users, built with
   cargo-deb as today.

Non-goals: Debian 12, Ubuntu 24.04 and older (GTK and libadwaita too old,
see P5), trixie-backports, salak-tauri in any distribution.

## 2. Audit findings (2026-10-07)

Versions checked on sources.debian.org on 2026-10-07:

| Crate | Salak asks | trixie | forky / sid |
| --- | --- | --- | --- |
| `gtk4` | `0.9` | 0.9.6 | 0.11.5 |
| `libadwaita` | `0.7` | 0.7.2 | 0.9.2 |
| `sourceview5` | `0.9` | 0.9.1 | 0.11.2 |
| `pulldown-cmark` | `0.10` | 0.10.3 | 0.13.3 |
| `glib` | `0.20` (through gtk4) | 0.20.9 | 0.22.10 |
| `dunce` | `1` (Windows only) | 1.0.5 | 1.0.5 |
| `tauri`, `tauri-plugin-dialog` | salak-tauri | absent | absent |

Blocking:

- **B1. The crate versions are trixie's, not sid's.** New packages enter
  through unstable; `"0.9"` means `>=0.9, <0.10` for Cargo, so Salak does not
  build in sid.
- **B2. salak-tauri is a workspace member.** Cargo resolves the dependencies
  of the whole workspace even with `-p salak-gtk`, so an offline build with
  Debian's registry fails on `tauri`, which Debian does not have.
- **B3. No Debian source package exists.** The cargo-deb `.deb` is a binary
  package; the archive needs a source package with `debian/`, an ITP bug
  and a sponsor.

To fix before asking for a sponsor:

- **R1.** README and CHANGELOG claim Ubuntu 24.04 support; it has GTK 4.14
  and libadwaita 1.5, under the `v4_16` and `v1_6` features the code needs.
- **R2.** README requires GTK 4.18 and libadwaita 1.7, while the code only
  asks for 4.16 and 1.6.
- **R3.** `[profile.release]` sets `strip = true` (no `-dbgsym` package),
  `opt-level = "s"`, `lto`, `codegen-units = 1` and `panic = "abort"`;
  Debian would have to patch them out.
- **R4.** No licence nor author is recorded for the icons, `icon.ico`,
  the banner and `logo.svg`, and the generation of the PNGs from the SVG is
  not documented.
- **R5.** Release tags are annotated but not signed; uscan cannot check them.

Improvements:

- **I1.** metainfo: the screenshot is the banner, from `main`; `<releases>`
  only knows 0.1.0 (the Tauri release); no `vcs-browser` URL, `<branding>`
  nor `<supports>`.
- **I2.** Desktop file: `inode/directory` is missing from `MimeType` although
  Salak browses folders.
- **I3.** Man page: the version `Salak 0.1.0` is hard-coded; EXIT STATUS does
  not mention 0.
- **I4.** The crate name `salak` is taken on crates.io (a configuration
  loader). No impact while `salak-gtk` is not published (P3).
- **I5.** Tests run without a display; say so where the packager will look.

Already fine: `desktop-file-validate` and `appstreamcli validate --pedantic`
pass, the man page passes the lintian groff check, the 59 tests of
salak-core and salak-gtk pass without a display, no vendored code, every
dependency of salak-core and salak-gtk is in Debian, MIT OR Apache-2.0
licensing with `LICENSE-*` linked into salak-core.

## 3. Decisions

### P1. Target: Debian unstable, Ubuntu by sync, and a PPA

Decided on 2026-10-07. Salak goes to Debian unstable. Ubuntu gets it by the
automatic sync of its development release from Debian (component universe).
Meanwhile, a Launchpad PPA serves the supported Ubuntu releases that meet P5:
on 2026-10-07, 26.04 LTS and 26.10 (released this month). 25.10 reached its
end of life in July 2026; 24.04 and 22.04 are too old (P5). No
trixie-backports.

### P2. Follow the crate versions of sid; MSRV 1.92

Decided on 2026-10-07, replaces the first consequence of D1 of the migration
plan. `gtk4 0.11`, `libadwaita 0.9`, `sourceview5 0.11`, `pulldown-cmark
0.13`. They require Rust 1.92, so `salak-gtk` sets `rust-version = "1.92"`.

- sid has rustc 1.96; Ubuntu 26.04 has 1.93.
- Debian 13 users building from source, and the GitHub `.deb`, use the
  rustc of trixie-backports (1.95) or rustup. The system libraries of
  trixie (GTK 4.18, libadwaita 1.7) still suffice: gtk4-rs 0.11 supports
  older GTK through its version features.
- The dependency rule of `RELEASING.md` and dependabot becomes: follow
  **sid**.

### P3. A standalone source package, maintained on Salsa

Decided on 2026-10-07, replaces D2 and T9.1 of the migration plan. Nothing is
published on crates.io. Debian gets a source package `salak` built with
`dh-cargo` from the upstream tarball, its `debian/` directory in a personal
repository on [Salsa](https://salsa.debian.org), Timothe Tizio as
maintainer. A sponsor is found through debian-mentors (mentors.debian.net
and the RFS process). `debian/` is never added to the upstream repository.

### P4. salak-tauri is excluded from the root workspace

Decided on 2026-10-07. It stays in `crates/salak-tauri/` with its own
`[workspace]` and `Cargo.lock`; the root workspace only holds salak-core and
salak-gtk, so their lock file and resolution never see Tauri.

### P5. Minimum system: GTK 4.16 and libadwaita 1.6

The code uses the `v4_16` and `v1_6` features. The documentation states
these numbers and names Debian 13 and Ubuntu 25.04 or later as examples. If
a later change needs GTK 4.18 or libadwaita 1.7 API, raise the feature and
the documentation together.

### P6. Signed tags and a signed release tarball

Decided on 2026-10-07. A new OpenPGP key signs the release tags and a
`salak-<version>.tar.gz` made with `git archive`, attached to the GitHub
release with its `.asc`. `debian/watch` downloads that tarball and checks it
with the key, shipped as `debian/upstream/signing-key.asc`.

### P7. The application id stays `com.tiziolabs.salak`

Decided on 2026-10-07: the domain `tiziolabs.com` belongs to the author.

### P8. The desktop file does not claim folders

Decided on 2026-10-08, closes I2 and T3.2. `inode/directory` stays out of
`MimeType`: on desktops without a `mimeapps.list` default, as often under
sway, `xdg-open` on a folder could pick Salak instead of the file manager.
Folders are opened from Salak itself or with `salak DIR`.

## 4. Conventions

- Upstream tasks follow section 4 of the migration plan: conventional
  commits, `cargo fmt`, `cargo clippy -p <crate> -- -D warnings` and
  `cargo test -p salak-core -p salak-gtk` pass, `CHANGELOG.md` updated.
- Debian work is checked in a **Debian unstable container** (podman or
  docker `debian:sid`), never only on the development machine (Ubuntu 26.04).
- Every version number written here is checked on sources.debian.org or
  launchpad.net, with the date.
- When a task changes a decision, update section 3 and the migration plan.

## 5. Progress

| Task | Content | Status |
| --- | --- | --- |
| T1.1 | Exclude salak-tauri from the workspace (B2, P4) | done |
| T1.2 | Move to the crate versions of sid (B1, P2) | done |
| T1.3 | Update the migration plan, README, RELEASING.md, dependabot | done |
| T2.1 | Fix the stated minimum systems (R1, R2, P5) | done |
| T2.2 | Release profile Debian can use (R3) | done |
| T2.3 | Copyright of every file (R4) | done upstream; `debian/copyright` in T4.2 |
| T2.4 | OpenPGP key, signed tags and tarball (R5, P6) | procedure written; key to create |
| T3.1 | metainfo (I1) | done; online check after the v0.2.0 tag |
| T3.2 | Desktop file (I2) | dropped, see P8 |
| T3.3 | Man page (I3) | todo |
| T3.4 | Packager notes (I5) | todo |
| T4.1 | Upstream release 0.2.0 | todo |
| T4.2 | Debian source package on Salsa | todo |
| T4.3 | ITP bug | todo |
| T4.4 | Upload to mentors.debian.net and RFS | todo |
| T4.5 | Follow-up until unstable and testing | todo |
| T5.1 | Launchpad PPA | todo |
| T5.2 | Ubuntu sync | todo |

## 6. Tasks

### Phase 1: blocking (upstream)

#### T1.1 Exclude salak-tauri from the workspace

- **Depends on:** nothing.
- **Steps:**
  1. Root `Cargo.toml`: `members = ["crates/salak-core", "crates/salak-gtk"]`
     and `exclude = ["crates/salak-tauri"]`.
  2. `crates/salak-tauri/Cargo.toml`: add an empty `[workspace]`, replace the
     `*.workspace = true` fields by their values (or a `[workspace.package]`
     of its own), and copy `[profile.release]` there (profiles are only read
     from the workspace root).
  3. Generate `crates/salak-tauri/Cargo.lock`; regenerate the root one, which
     must no longer contain `tauri`.
  4. Update `scripts/package-windows.ps1`, the Tauri `beforeBuildCommand`
     paths if any, `.gitignore` (`crates/salak-tauri/target`), dependabot
     (second `directory`) and the build commands of README and RELEASING.md.
     The "never build both in one command" warning goes away: the two
     binaries no longer share `target/`.
- **Done when:** `grep -c tauri Cargo.lock` prints 0, `cargo build -p
  salak-gtk` and `cargo build --release` in `crates/salak-tauri` both work.

#### T1.2 Move to the crate versions of sid

- **Depends on:** T1.1 (smaller lock file to update).
- **Steps:**
  1. Check sid again on sources.debian.org for `rust-gtk4`,
     `rust-libadwaita`, `rust-sourceview5`, `rust-pulldown-cmark`, and their
     highest version features (`v4_*`, `v1_*`).
  2. `salak-gtk`: `gtk4 0.11`, `libadwaita 0.9`, `sourceview5 0.11`,
     `rust-version = "1.92"`; keep `v4_16` and `v1_6` (P5).
  3. `salak-core`: `pulldown-cmark 0.13`, `default-features = false`; adapt
     the event handling of `markdown.rs` and of `buffer.rs` (changes between
     0.10 and 0.13: `Tag`/`TagEnd`, metadata blocks, definition lists,
     `math` options; read the upstream changelog).
  4. Fix the breaking changes of gtk-rs 0.10 and 0.11 (follow their release
     notes); salak-tauri keeps its own versions, it is no longer affected.
  5. Run the parity smoke test of T8.1 of the migration plan under GNOME and
     sway.
- **Done when:** the tests and clippy pass, and in a `debian:sid` container
  the build works with Debian's crates only: install `dh-cargo`,
  `librust-gtk4-dev`, `librust-libadwaita-dev`, `librust-sourceview5-dev`,
  `librust-pulldown-cmark-dev`, then build with the registry
  `/usr/share/cargo/registry` (`/usr/share/cargo/bin/cargo prepare-debian
  target --link-from-system`, then `cargo build --offline`).

#### T1.3 Update the documentation of the dependency policy

- **Depends on:** T1.2.
- **Steps:**
  1. Migration plan: rewrite the consequences of D1 and D2, and replace
     phase 9 by a link to this file.
  2. `RELEASING.md`, "Dependencies": the reference is Debian **unstable**;
     remove the trixie pin.
  3. `.github/dependabot.yml`: the comment says unstable; keep the ignore
     rules (a dependency must still wait for Debian).
  4. README: Rust 1.92 or later; on Debian 13, `apt install -t
     trixie-backports rustc cargo` or rustup.
- **Done when:** no document mentions the trixie crate versions as a rule.

### Phase 2: before asking for a sponsor (upstream)

#### T2.1 Fix the stated minimum systems

- **Depends on:** nothing.
- **Steps:** README ("Building"), CHANGELOG ("Unreleased"), user guide if it
  says it: "GTK 4.16 and libadwaita 1.6, for instance Debian 13 or
  Ubuntu 25.04 and later". Remove Ubuntu 24.04.
- **Done when:** the numbers match the features of `crates/salak-gtk/Cargo.toml`.

#### T2.2 Release profile Debian can use

- **Depends on:** T1.1.
- **Steps:**
  1. Root `Cargo.toml`: keep `[profile.release]` at Cargo's defaults (or
     only `lto = "thin"`); move the size settings to a `[profile.dist]` that
     `inherits = "release"`, with `strip`, `opt-level = "s"`, `lto`,
     `codegen-units = 1`, `panic = "abort"`.
  2. cargo-deb: build with `cargo deb -p salak-gtk --profile dist` and check
     that the binary asset path follows the profile (cargo-deb rewrites
     `target/release/`; fix the asset path otherwise).
  3. RELEASING.md and README: the GitHub `.deb` uses `--profile dist`.
  4. Repeat the size measure of section 7 of the migration plan.
- **Done when:** `cargo build --release -p salak-gtk` gives an unstripped
  binary and the GitHub `.deb` keeps its size.

#### T2.3 Copyright of every file

- **Depends on:** T1.1.
- **Steps:**
  1. List every file that is not code: `crates/salak-gtk/data/icons/*`,
     `icons/*`, `crates/salak-tauri/ui/logo.svg`, `docs/assets/salak-banner.png`,
     help pages.
  2. For each one, record who made it and under which licence (MIT OR
     Apache-2.0 like the code, or CC-BY-SA-4.0 / CC0 for the artwork). If a
     file comes from elsewhere, record its source; replace it if its licence
     is not DFSG-free.
  3. Document how the PNGs and `icon.ico` are generated from `icons/salak.svg`
     (for instance `rsvg-convert -w 128 -h 128`), in a comment in
     `icons/README.md` or a script.
  4. Write a draft `debian/copyright` (DEP-5) in the Salsa repository of
     T4.2 from this list; upstream, mention the artwork licence in the
     README "License" section.
- **Done when:** every file of `git ls-files` is covered by a stanza of the
  draft.

#### T2.4 OpenPGP key, signed tags and tarball

- **Depends on:** nothing.
- **Steps:**
  1. Create an ed25519 key for `t.tizio@mailbox.org` with an expiry date
     (`gpg --quick-gen-key`), back it up with a revocation certificate,
     publish it on keys.openpgp.org and on GitHub.
  2. `git config user.signingkey <fingerprint>` and `tag.gpgSign true`.
  3. RELEASING.md: tags are made with `git tag -s`; after the tag, create
     `salak-<version>.tar.gz` with
     `git archive --prefix=salak-<version>/ -o … v<version>`, sign it with
     `gpg --armor --detach-sign`, attach both to the release.
  4. Export the public key (`gpg --export --export-options export-minimal
     --armor`) for `debian/upstream/signing-key.asc`.
- **Done when:** `git tag -v` and `gpg --verify` succeed on a test tag.

### Phase 3: improvements (upstream)

#### T3.1 metainfo

- **Depends on:** T4.1 for the release entry.
- **Steps:**
  1. A real screenshot of the GTK window (light mode, a folder and a
     document), in `docs/assets/`, referenced from a tag, not `main`.
  2. `<releases>`: one entry per release, newest first, with a short
     `<description>`.
  3. `<url type="vcs-browser">`, `<url type="contribute">` if any.
  4. `<branding>` with a light and a dark colour taken from the icon
     (`#FBF3E4` and `#3A2216`).
  5. `<supports><control>keyboard</control><control>pointing</control></supports>`.
- **Done when:** `appstreamcli validate --pedantic` passes, with network
  checks this time (no `--no-net`).

#### T3.2 Desktop file

- **Depends on:** nothing.
- **Steps:** `MimeType=text/markdown;inode/directory;`. Check that Salak does
  not become the default folder handler of GNOME Files (it is added to "Open
  With" only, since the mimeapps defaults win); drop it otherwise.
- **Done when:** `desktop-file-validate` passes and the check above is done.

#### T3.3 Man page

- **Depends on:** nothing.
- **Steps:** EXIT STATUS lists 0; the version and date of `.TH` are part of
  the release checklist in RELEASING.md.
- **Done when:** the groff check of T8.2 of the migration plan is clean.

#### T3.4 Packager notes

- **Depends on:** T1.2.
- **Steps:** a short "Packaging" section in the README: dependencies of
  salak-gtk only, `cargo test -p salak-core -p salak-gtk` needs no display,
  data files to install and where (same list as `[package.metadata.deb]`),
  salak-tauri is not for Linux distributions.
- **Done when:** the section exists.

### Phase 4: Debian

#### T4.1 Upstream release 0.2.0

- **Depends on:** phases 1 and 2, T3.1 to T3.4 if ready.
- **Steps:** follow RELEASING.md, with the signed tag and tarball of T2.4.
- **Done when:** the release has the `.deb`, the Windows files and the signed
  tarball.

#### T4.2 Debian source package on Salsa

- **Depends on:** T4.1, T2.3.
- **Steps:**
  1. Create a Salsa account and a repository `salak` in the personal
     namespace, with the DEP-14 layout (`debian/latest`, `upstream/latest`,
     `pristine-tar`) through `gbp import-orig --uscan`.
  2. `debian/control`: source `salak`, section `text`, priority `optional`,
     `Build-Depends: debhelper-compat (= 13), dh-sequence-cargo`, and the
     `librust-*-dev` of the dependencies (with the `v4_16`, `v1_6` feature
     packages), `pkgconf`; `Rules-Requires-Root: no`;
     `Standards-Version` current; `Vcs-Git` and `Vcs-Browser` to Salsa;
     binary `salak`, `Depends: ${misc:Depends}, ${shlibs:Depends}`,
     `Built-Using: ${cargo:Built-Using}`, `Static-Built-Using:
     ${cargo:Static-Built-Using}`.
  3. `debian/rules` with `dh $@ --buildsystem cargo` and the build of the
     `salak-gtk` package only.
  4. `debian/salak.install`, `debian/salak.manpages` from
     `crates/salak-gtk/data/`.
  5. `debian/copyright` from T2.3, `debian/watch` (version 4, the signed
     tarball of the GitHub release, `pgpsigurlmangle`),
     `debian/upstream/signing-key.asc`, `debian/upstream/metadata`,
     `debian/changelog` (`UNRELEASED`, `Closes: #<ITP>`), `debian/salsa-ci.yml`.
  6. `debian/tests/control`: an autopkgtest running the tests of salak-core
     and salak-gtk, plus a smoke test `salak --version`.
  7. Build in a clean sid chroot (`sbuild` or `pbuilder`), then run
     `lintian -EvIL +pedantic` and fix every error and warning that applies.
- **Done when:** sbuild, lintian (no error, no warning) and autopkgtest pass,
  and Salsa CI is green.

#### T4.3 ITP bug

- **Depends on:** nothing (can be done early to claim the name).
- **Steps:** `reportbug wnpp`, type ITP, package `salak`, with the short and
  long descriptions of the metainfo, licence, URL, and a line on why Salak is
  useful in Debian (Markdown reader without a web engine). Put the bug number
  in `debian/changelog`.
- **Done when:** the bug has a number.

#### T4.4 Upload to mentors.debian.net and RFS

- **Depends on:** T4.2, T4.3.
- **Steps:**
  1. Account on mentors.debian.net, upload the source package with `dput
     mentors`.
  2. File the RFS bug against `sponsorship-requests` with the template the
     site generates; also ask on the debian-mentors list, and the Rust
     team (debian-rust list, `#debian-rust` on OFTC) for a reviewer who knows
     dh-cargo.
  3. Answer every review by a new upload; note the requests below.
- **Done when:** a sponsor uploads the package to unstable.

#### T4.5 Follow-up until unstable and testing

- **Depends on:** T4.4.
- **Steps:** the package waits in the NEW queue for the ftp-masters; once
  accepted, watch tracker.debian.org/pkg/salak (piuparts, autopkgtest, reproducible
  builds) until migration to testing.
- **Done when:** `salak` is in testing (forky).

### Phase 5: Ubuntu

#### T5.1 Launchpad PPA

- **Depends on:** T4.2 (same `debian/` directory).
- **Steps:**
  1. Check, for each supported release (26.04 LTS, 26.10), its GTK,
     libadwaita and GtkSourceView versions, its rustc, and whether it has
     `librust-gtk4-dev` 0.11, `librust-libadwaita-dev` 0.9,
     `librust-sourceview5-dev` 0.11 and `librust-pulldown-cmark-dev` 0.13.
     Write the result in section 3, P1.
  2. Where the crates are missing, Launchpad builds offline: build that
     release's package with the crates vendored into the orig tarball
     (`cargo vendor`), a `~ppa` version (`0.2.0-1~ppa1~26.04`), and never
     reuse this tarball for Debian.
  3. Create the PPA, upload with `dput ppa:<user>/salak`, test the
     installation on each release, document it in the README.
- **Done when:** `sudo add-apt-repository ppa:<user>/salak && sudo apt install
  salak` works on 26.04 and 26.10.

#### T5.2 Ubuntu sync

- **Depends on:** T4.4.
- **Steps:** after the upload to unstable, the package is synced into the
  current Ubuntu development release before its Debian Import Freeze;
  otherwise ask for a sync (`requestsync`). Check on
  launchpad.net/ubuntu/+source/salak. Afterwards, keep the PPA only for the
  releases that do not have the package.
- **Done when:** `salak` is in an Ubuntu release, and the README says so.

## 7. Open questions

- ~~Licence of the artwork (T2.3)~~: answered on 2026-10-08, the icon and the
  banner are original works of the author, under MIT OR Apache-2.0 like the
  code. The banner has no other source than its PNG; it is not installed.
- Ubuntu crate versions (T5.1): they decide whether the PPA needs vendoring.
