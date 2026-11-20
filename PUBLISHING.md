# Publishing Plasma Visual Screensaver

This project publishes an installable Debian package for Kubuntu 26.04. The
package includes the application, desktop launcher, icon, AppStream metadata,
documentation, and KDE autostart entry. User settings are never packaged or
deleted during upgrades.

Install the README's build dependencies, including the distribution's `rustc`
and `cargo` (1.93 or newer), before running the release helper. Both package
formats record the Rust standard library's source version in `Static-Built-Using`.

## GitHub setup

The public project is hosted at
<https://github.com/StantonMatt/plasma-wayland-screensaver>.
GitHub Actions uses hosted Ubuntu 26.04 runners. The build workflow runs on
`main` pushes, pull requests and manual dispatches. Keep `main` protected from
force pushes and require successful checks before merging.

The default `GITHUB_TOKEN` is read-only; only the release publication job has
`contents: write`. GitLab is a read-only mirror with CI/CD disabled.

## Prepare a release

1. Choose a semantic version, for example `0.24.0`.
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
git tag -a v0.24.0 -m "Plasma Visual Screensaver 0.24.0"
git push origin v0.24.0
```

The **Publish release** GitHub workflow then:

- verifies that the tag and project versions match;
- builds on a hosted Ubuntu 26.04 runner;
- runs unit tests and metadata/QML validation;
- builds and lints the offline Debian binary package; and
- publishes the CPack `.deb` and SHA-256 checksum as GitHub Release assets.

Pull requests run the same release build and retain its artifacts for 14 days,
without publishing a release. No new tag is needed to validate release builds.

After the GitHub release is verified, publish the matching signed source
package to the automatic-update PPA using the process in [PPA.md](PPA.md).
GitHub distributes a directly installable `.deb`; Launchpad independently
rebuilds the source and provides the repository metadata used by automatic
updates.

Publication happens only after build and validation succeed. Never move a tag
for an already published release; increment the version instead. If publication
fails, inspect the release and any uploaded assets before retrying; the workflow
refuses to overwrite an existing release.

To publish an existing tag with no GitHub release, manually dispatch **Publish
release** with `release_tag` set to that tag. It checks out and builds that exact
tag with the selected workflow. Keep `release_tag` empty for a build without
publication. Downloads are available at:

```text
https://github.com/StantonMatt/plasma-wayland-screensaver/releases/latest
https://github.com/StantonMatt/plasma-wayland-screensaver/releases/download/<tag>/<filename>
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

The generated package targets the architecture of the GitHub runner (`amd64`)
and Kubuntu 26.04. Supporting other Ubuntu releases, Debian, ARM64, Flatpak, or
a signed APT repository should be treated as separate tested distribution
targets rather than assuming binary compatibility.

## Release source selection

GitHub is the default for the application's update lookup and the standalone
cadence baseline downloader. `-DPVS_RELEASE_FORGE=gitlab` and
`RELEASE_FORGE=gitlab` retain compatibility with historical GitLab releases.
Keep `origin` pointed at GitHub and `gitlab` at the read-only mirror.
