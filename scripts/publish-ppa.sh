#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
output_dir=${PPA_OUTPUT_DIR:-"${project_root}/dist/ppa"}
ppa_target=${PPA_TARGET:?Set PPA_TARGET to ppa:LAUNCHPAD_NAME/PPA_NAME}
signing_key=${PPA_SIGNING_KEY:?Set PPA_SIGNING_KEY to the registered OpenPGP fingerprint}
source_name=$(dpkg-parsechangelog -l"${project_root}/debian/changelog" -SSource)
package_version=$(dpkg-parsechangelog -l"${project_root}/debian/changelog" -SVersion)
upstream_version=${package_version#*:}
upstream_version=${upstream_version%%-*}

if [[ ! "${ppa_target}" =~ ^ppa:[a-z0-9][a-z0-9+.-]*/[a-z0-9][a-z0-9+.-]*$ ]]; then
    echo "Invalid PPA target: ${ppa_target}" >&2
    exit 1
fi
if [[ "${PPA_ALLOW_DIRTY:-0}" != "1" ]] &&
   [[ -n "$(git -C "${project_root}" status --porcelain)" ]]; then
    echo "Refusing to publish from a dirty working tree" >&2
    exit 1
fi

"${project_root}/scripts/build-ppa-source.sh" "${output_dir}"

source_changes="${output_dir}/${source_name}_${package_version}_source.changes"
if [[ ! -f "${source_changes}" ]]; then
    echo "Expected source changes file was not created: ${source_changes}" >&2
    exit 1
fi

debsign -k"${signing_key}" "${source_changes}"

checksum_file="${source_name}_${package_version}.SHA256SUMS"
mapfile -t artifacts < <(find "${output_dir}" -maxdepth 1 -type f \
    \( -name "${source_name}_${upstream_version}.orig.tar.*" \
       -o -name "${source_name}_${package_version}*" \
       -o -name "${source_name}-dbgsym_${package_version}*" \) \
    ! -name '*.SHA256SUMS' -printf '%f\n' | sort)
(
    cd -- "${output_dir}"
    sha256sum "${artifacts[@]}" > "${checksum_file}"
)

if [[ "${PPA_DRY_RUN:-0}" == "1" ]]; then
    dput --simulate --lintian "${ppa_target}" "${source_changes}"
else
    dput --lintian "${ppa_target}" "${source_changes}"
fi
