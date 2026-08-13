#!/usr/bin/env bash
set -euo pipefail
umask 022

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output_dir=${1:-"${project_root}/dist/ppa"}
mkdir -p -- "${output_dir}"
output_dir=$(cd -- "${output_dir}" && pwd)

source_name=$(dpkg-parsechangelog -l"${project_root}/debian/changelog" -SSource)
package_version=$(dpkg-parsechangelog -l"${project_root}/debian/changelog" -SVersion)
upstream_version=${package_version#*:}
upstream_version=${upstream_version%%-*}
source_ref=${PPA_SOURCE_REF:-"v${upstream_version}"}
project_version=$(sed -nE \
    's/^[[:space:]]*VERSION[[:space:]]+([0-9]+\.[0-9]+\.[0-9]+).*$/\1/p' \
    "${project_root}/CMakeLists.txt" | head -1)
metainfo_version=$(sed -nE \
    's/.*<release version="([^"]+)".*/\1/p' \
    "${project_root}/data/metainfo/org.kde.plasmavisualscreensaver.metainfo.xml" | head -1)

if [[ "${source_name}" != "plasma-visual-screensaver" ]]; then
    echo "Unexpected Debian source package: ${source_name}" >&2
    exit 1
fi
if [[ -z "${project_version}" || "${upstream_version}" != "${project_version}" ||
      "${metainfo_version}" != "${project_version}" ]]; then
    echo "Release versions disagree: CMake=${project_version:-missing}, Debian=${upstream_version:-missing}, AppStream=${metainfo_version:-missing}" >&2
    exit 1
fi
if ! git -C "${project_root}" rev-parse --verify --quiet "${source_ref}^{commit}" >/dev/null; then
    echo "Upstream source ref does not exist: ${source_ref}" >&2
    exit 1
fi

build_root=$(mktemp -d /tmp/plasma-ppa-source.XXXXXX)
cleanup()
{
    if [[ "${build_root}" == /tmp/plasma-ppa-source.* && -d "${build_root}" ]]; then
        rm -rf -- "${build_root}"
    fi
}
trap cleanup EXIT

source_dir="${build_root}/${source_name}-${upstream_version}"
orig_tarball="${build_root}/${source_name}_${upstream_version}.orig.tar.gz"

git -C "${project_root}" archive --format=tar \
    --prefix="${source_name}-${upstream_version}/" \
    "${source_ref}" -- . ':(exclude)debian' \
    | gzip -9n > "${orig_tarball}"
tar -xzf "${orig_tarball}" -C "${build_root}"
cp -a -- "${project_root}/debian" "${source_dir}/debian"

(
    cd -- "${source_dir}"
    dpkg-buildpackage --build=source --source-option=--include-binaries \
        -sa -us -uc
)

source_changes="${build_root}/${source_name}_${package_version}_source.changes"
source_dsc="${build_root}/${source_name}_${package_version}.dsc"
lintian --display-info --pedantic --fail-on error "${source_changes}"

verify_dir="${build_root}/verify-source"
dpkg-source --extract "${source_dsc}" "${verify_dir}" >/dev/null

if [[ "${PPA_BUILD_BINARY:-1}" == "1" ]]; then
    host_architecture=$(dpkg-architecture -qDEB_HOST_ARCH)
    (
        cd -- "${source_dir}"
        dpkg-buildpackage --build=binary -us -uc
    )
    binary_changes="${build_root}/${source_name}_${package_version}_${host_architecture}.changes"
    lintian --display-info --pedantic --fail-on error "${binary_changes}"
fi

find "${output_dir}" -maxdepth 1 -type f \
    \( -name "${source_name}_${package_version}*" \
       -o -name "${source_name}-dbgsym_${package_version}*" \) -delete

artifact_names=()
while IFS= read -r artifact; do
    artifact_name=$(basename -- "${artifact}")
    cp -- "${artifact}" "${output_dir}/${artifact_name}"
    artifact_names+=("${artifact_name}")
done < <(find "${build_root}" -maxdepth 1 -type f \
    \( -name "${source_name}_${upstream_version}.orig.tar.*" \
       -o -name "${source_name}_${package_version}*" \
       -o -name "${source_name}-dbgsym_${package_version}*" \) \
    -print | sort)

(
    cd -- "${output_dir}"
    sha256sum "${artifact_names[@]}" \
        > "${source_name}_${package_version}.SHA256SUMS"
)

echo
echo "PPA source artifacts:"
printf '%s\n' "${artifact_names[@]}" \
    "${source_name}_${package_version}.SHA256SUMS"
