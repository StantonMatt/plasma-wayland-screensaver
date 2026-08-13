# Publishing Plasma Visual Screensaver

This project publishes an installable Debian package for Kubuntu 26.04. The
package includes the application, desktop launcher, icon, AppStream metadata,
documentation, and KDE autostart entry. User settings are never packaged or
deleted during upgrades.

## One-time GitHub setup

1. Make the repository public when it is ready for general access. Standard
   GitHub-hosted runners are free for public repositories.
2. Enable **Issues** so the bug-report URL in the AppStream metadata works.
3. Keep the repository's default `GITHUB_TOKEN` permission read-only. The tag
   workflow requests `contents: write` only for its release job.
4. Optionally protect `main` and require the **Kubuntu 26.04 package** check.
5. Optionally enable immutable releases. The workflow uploads all artifacts as
   part of the release creation, so it is compatible with that mode.

The repository is intentionally not made public by the build scripts. Changing
visibility is a maintainer decision.

## Prepare a release

1. Choose a semantic version, for example `0.4.0`.
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
git tag -a v0.4.0 -m "Plasma Visual Screensaver 0.4.0"
git push origin v0.4.0
```

The **Publish release** workflow then:

- verifies that the tag and project versions match;
- builds on GitHub's Ubuntu 26.04 runner;
- runs unit tests and metadata/QML validation;
- generates the `.deb` and SHA-256 checksum; and
- creates a GitHub Release with generated release notes and both files.

If any step fails, no release is published. Fix the issue, delete the failed
local and remote tag, complete review again, then recreate the tag on the fixed
commit. Never move a tag for an already published release; increment the Debian
package revision or project version instead.

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
