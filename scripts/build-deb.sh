#!/usr/bin/env bash
set -euo pipefail
umask 022

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
build_dir=${1:-"${project_root}/build-package"}
output_dir=${2:-"${project_root}/dist"}

project_version=$(sed -nE \
    's/^[[:space:]]*VERSION[[:space:]]+([0-9]+\.[0-9]+\.[0-9]+).*$/\1/p' \
    "${project_root}/CMakeLists.txt" | head -1)
debian_version=$(sed -nE \
    '1s/^plasma-visual-screensaver \(([^)]+)-[0-9]+\).*$/\1/p' \
    "${project_root}/packaging/debian/changelog")
metainfo_version=$(sed -nE \
    's/.*<release version="([^"]+)".*/\1/p' \
    "${project_root}/data/metainfo/org.kde.plasmavisualscreensaver.metainfo.xml" | head -1)
if [[ -z "${project_version}" || "${debian_version}" != "${project_version}" ||
      "${metainfo_version}" != "${project_version}" ]]; then
    echo "Release versions disagree: CMake=${project_version:-missing}, Debian=${debian_version:-missing}, AppStream=${metainfo_version:-missing}" >&2
    exit 1
fi

shellcheck \
    "${project_root}/scripts/build-deb.sh" \
    "${project_root}/packaging/debian/postinst" \
    "${project_root}/packaging/debian/postrm"

cmake -S "${project_root}" -B "${build_dir}" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DBUILD_TESTING=ON
cmake --build "${build_dir}" --parallel
ctest --test-dir "${build_dir}" --output-on-failure

qmllint_path=$(command -v qmllint || true)
if [[ -z "${qmllint_path}" && -x /usr/lib/qt6/bin/qmllint ]]; then
    qmllint_path=/usr/lib/qt6/bin/qmllint
fi
if [[ -z "${qmllint_path}" ]]; then
    echo "qmllint was not found" >&2
    exit 1
fi
"${qmllint_path}" \
    "${project_root}/qml/Screensaver.qml" \
    "${project_root}/qml/Settings.qml" \
    "${project_root}"/qml/visuals/*.qml

appstreamcli validate --no-net \
    "${project_root}/data/metainfo/org.kde.plasmavisualscreensaver.metainfo.xml"
desktop-file-validate \
    "${project_root}/data/applications/org.kde.plasmavisualscreensaver.desktop" \
    "${project_root}/data/autostart/org.kde.plasmavisualscreensaver.desktop"

mkdir -p "${output_dir}"
find "${output_dir}" -maxdepth 1 -type f \
    \( -name 'plasma-visual-screensaver_*.deb' -o -name 'plasma-visual-screensaver_*.deb.sha256' \) \
    -delete
cpack --config "${build_dir}/CPackConfig.cmake" -G DEB -B "${output_dir}"

for package in "${output_dir}"/plasma-visual-screensaver_*.deb; do
    package_name=$(basename -- "${package}")
    (
        cd -- "${output_dir}"
        sha256sum "${package_name}" > "${package_name}.sha256"
    )
    dpkg-deb --info "${package}"
    dpkg-deb --contents "${package}"
    lintian --display-info --pedantic --fail-on error "${package}"
done

echo
echo "Release artifacts:"
find "${output_dir}" -maxdepth 1 -type f -printf '%f\n' | sort
