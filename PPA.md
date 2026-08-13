# Automatic updates through Launchpad

Launchpad builds this project from a signed Debian source package and publishes
the result in a Personal Package Archive (PPA). After users add the PPA once,
newer package versions arrive through Discover, Software Updater, or normal
`apt upgrade` operations. Personal settings remain in the user's KDE config
directory and are not replaced by package upgrades.

The public stable archive is
[`ppa:stantonmatt/plasma-visual-screensaver`](https://launchpad.net/~stantonmatt/+archive/ubuntu/plasma-visual-screensaver).
For a newly initialized archive, wait until its package page reports the
Resolute build as **Published** before using the installation commands below.

## One-time maintainer setup

These steps are already complete for the public stable archive. They are kept
here as recovery and maintainer-transfer documentation.

1. Create or claim a Launchpad account at <https://launchpad.net/+login> and
   note its account name from the `launchpad.net/~ACCOUNT` profile URL.
2. Publish the maintainer's public OpenPGP key to Ubuntu's key server:

   ```bash
   gpg --keyserver hkps://keyserver.ubuntu.com \
     --send-keys F78C76E2C1BE76E07CF81202B4468B1BCFFD55F5
   ```

3. In the Launchpad profile, open **OpenPGP keys**, import fingerprint
   `F78C76E2C1BE76E07CF81202B4468B1BCFFD55F5`, decrypt the confirmation email,
   and follow its confirmation link.
4. Create a public PPA named `plasma-visual-screensaver`. Either use the
   Launchpad web interface or install `ppa-dev-tools` and run:

   ```bash
   sudo snap install ppa-dev-tools
   ppa create plasma-visual-screensaver
   ```

Launchpad accepts only signed source packages. It does not accept the prebuilt
`.deb` from GitHub Releases.

## Build and inspect a PPA source package

Install the packaging tools once:

```bash
sudo apt install appstream build-essential cmake debhelper devscripts dput \
  extra-cmake-modules libkf6config-dev libkf6idletime-dev \
  liblayershellqtinterface-dev lintian ninja-build qt6-base-dev \
  qt6-declarative-dev
```

Then run:

```bash
./scripts/build-ppa-source.sh
```

The script exports the matching upstream Git tag without `debian/`, overlays
the current `debian/` packaging, builds and extracts the 3.0 (quilt) source
package, builds a local binary package, runs the test suite, applies Debian
hardening flags, runs error-level Lintian checks, and writes verified artifacts
to `dist/ppa/`.

For CI validation of the current commit before a release tag exists:

```bash
PPA_SOURCE_REF=HEAD PPA_BUILD_BINARY=0 ./scripts/build-ppa-source.sh
```

## Upload a release

The newest entries in all three locations must describe the same upstream
version:

- top-level `CMakeLists.txt` project version;
- `data/metainfo/org.kde.plasmavisualscreensaver.metainfo.xml`; and
- `debian/changelog`.

Use a PPA version such as `0.4.0-1ppa1~resolute1`. Increase `ppa1` when
re-uploading changed packaging for the same app version; Launchpad never
accepts the same source version twice. The PPA version is deliberately newer
than the corresponding standalone `0.4.0-1` GitHub package, allowing existing
users to migrate without a forced downgrade.

Build, sign, lint, and upload with:

```bash
export PPA_TARGET=ppa:stantonmatt/plasma-visual-screensaver
export PPA_SIGNING_KEY=F78C76E2C1BE76E07CF81202B4468B1BCFFD55F5
./scripts/publish-ppa.sh
```

To validate signing and `dput` configuration without sending anything:

```bash
PPA_DRY_RUN=1 ./scripts/publish-ppa.sh
```

Launchpad emails an acceptance or rejection notice. An accepted source upload
then enters separate build and publication stages; do not announce the update
until the Resolute build is successfully published.

## User installation and migration

After the archive reports the Resolute build as **Published**, users install
with:

```bash
sudo add-apt-repository ppa:stantonmatt/plasma-visual-screensaver
sudo apt update
sudo apt install plasma-visual-screensaver
```

The final command upgrades an existing GitHub `.deb` installation in place.
Afterward, Discover and Ubuntu's normal update tools automatically offer newer
PPA builds. The service uses the upgraded executable at the next login; users
can restart it immediately with:

```bash
plasma-visual-screensaver --quit
plasma-visual-screensaver --background
```

To leave the update channel without uninstalling the application:

```bash
sudo add-apt-repository --remove ppa:stantonmatt/plasma-visual-screensaver
sudo apt update
```
