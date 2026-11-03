# Publishing Plasma Visual Screensaver

This project publishes an installable Debian package for Kubuntu 26.04. The
package includes the application, desktop launcher, icon, AppStream metadata,
documentation, and KDE autostart entry. User settings are never packaged or
deleted during upgrades.

Install the README's build dependencies, including the distribution's `rustc`
and `cargo` (1.93 or newer), before running the release helper. Both package
formats record the Rust standard library's source version in `Static-Built-Using`.

## GitLab setup

The public project is hosted at
<https://gitlab.com/StantonMatt/plasma-wayland-screensaver>.
`.gitlab-ci.yml` uses an Ubuntu 26.04 image on the `ci-public` Docker runner.
Enable Issues and the package registry, keep `main` protected from force pushes,
and require successful MR pipelines before merging. A full-history secret scan
must precede making a private mirror public.

Releases use the job's `CI_JOB_TOKEN`; no personal token is stored in CI.
Generic package downloads and release links must remain publicly accessible.

## Prepare a release

1. Choose a semantic version, for example `0.21.0`.
2. Update `VERSION` in the top-level `CMakeLists.txt`.
3. Add a matching newest entry to `packaging/debian/changelog`. Increment the
   revision after the dash when rebuilding the same upstream version.
4. Add a matching newest `<release>` entry to
   `data/metainfo/org.kde.plasmavisualscreensaver.metainfo.xml` with the release
   date in `YYYY-MM-DD` format.
5. Update README or release-facing documentation for behavior changes.
6. Build exactly what the release workflow will build:

   ```bash
   ./scripts/build-deb.sh
   ```

7. Install the package on a Kubuntu 26.04 Plasma Wayland session and complete
   the manual checklist in the README.
8. Run the repository's required review-fix loop until a fresh review reports
   no actionable findings.
9. Commit and push the release preparation to `main`.

## Publish the release

Create and push an annotated tag whose name exactly matches the CMake version:

```bash
git tag -a v0.21.0 -m "Plasma Visual Screensaver 0.21.0"
git push origin v0.21.0
```

The **release** GitLab job then:

- verifies that the tag and project versions match;
- builds on Ubuntu 26.04 using the `ci-public` runner;
- runs unit tests and metadata/QML validation;
- generates the `.deb` and SHA-256 checksum; and
- uploads both files to the generic package registry, then creates a GitLab
  Release with commit notes and public asset links.

After the GitLab release is verified, publish the matching signed source
package to the automatic-update PPA using the process in [PPA.md](PPA.md).
GitLab distributes a directly installable `.deb`; Launchpad independently
rebuilds the source and provides the repository metadata used by automatic
updates.

Publication happens only after build and validation succeed. A failed upload can
leave a package without a Release. Retries reuse byte-identical uploads and
refuse to replace differing bytes; inspect a conflict before retrying. Never
move a tag for an already published release; increment the version instead.

For migration of an existing tag that predates `.gitlab-ci.yml`, run a manual
web pipeline on the reviewed migration branch with `RELEASE_TAG=v0.21.0`.
The release job archives and builds that exact tag, checks its CMake version,
and publishes it with the current release helper. Release dates preserve the
original tag timestamp so backfilling an older tag cannot make it latest.
Keep `RELEASE_TAG` empty for
normal manual build/test pipelines. Do not republish until the release path has
been verified. Downloads are available at:

```text
https://gitlab.com/StantonMatt/plasma-wayland-screensaver/-/releases/permalink/latest
https://gitlab.com/api/v4/projects/88430057/packages/generic/plasma-visual-screensaver/<tag>/<filename>
```

## Verify the published release

On a clean Kubuntu 26.04 machine:

```bash
sha256sum --check plasma-visual-screensaver_*.deb.sha256
sudo apt install ./plasma-visual-screensaver_*.deb
plasma-visual-screensaver --settings
```

Confirm that it appears in the application launcher, Preview works on every
monitor, and the background process starts after logging out and back in.

## Current distribution scope

The generated package targets the architecture of the GitLab runner (`amd64`)
and Kubuntu 26.04. Supporting other Ubuntu releases, Debian, ARM64, Flatpak, or
a signed APT repository should be treated as separate tested distribution
targets rather than assuming binary compatibility.

## If moving back to GitHub

`.github/` is the inactive GitHub configuration and must stay untouched during
this migration. If GitHub is restored, keep `.github/workflows` in sync by hand
before enabling Actions again. The original workflows retain GitHub release
creation and baseline downloads. Restore `origin` to the SSH GitHub remote
(`git@github.com:StantonMatt/plasma-wayland-screensaver.git`); keep GitLab as a
second remote.

Build with `-DPVS_RELEASE_FORGE=github` to resolve updates through
`https://api.github.com/repos/StantonMatt/plasma-wayland-screensaver/releases/latest`
instead of GitLab's URL-encoded project releases API. `gitlab` is the default;
change that CMake default to `github` for the preserved release workflows.
The standalone baseline downloader also accepts `RELEASE_FORGE=github` and
`GITHUB_REPOSITORY`. Restore homepage/issue URLs, Debian upstream metadata and
`debian/watch` when moving the current host back. The previous watch file is
available in Git history.

For Actions, keep the default `GITHUB_TOKEN` read-only; the preserved tag
workflow requests `contents: write` to create the Release. Enable Issues and
private vulnerability reporting as appropriate. The old public release URL is
<https://github.com/StantonMatt/plasma-wayland-screensaver/releases/latest>.
