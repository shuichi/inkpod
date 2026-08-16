#!/bin/zsh

set -euo pipefail

SCRIPT_DIR="${0:A:h}"
ROOT_DIR="${SCRIPT_DIR:h}"

APP_NAME="Inkpod"
BUILD_PRESET="${INKPOD_BUILD_PRESET:-macos-arm64-release}"
BUILD_TARGET="${INKPOD_BUILD_TARGET:-inkpod_macos_archive}"
BUILD_DIR="${INKPOD_BUILD_DIR:-${ROOT_DIR}/build/macos-arm64-release}"
SOURCE_APP="${INKPOD_SOURCE_APP:-${BUILD_DIR}/xcode-arm64-release-derived/Inkpod.xcarchive/Products/Applications/${APP_NAME}.app}"
OUTPUT_DIR="${INKPOD_OUTPUT_DIR:-${ROOT_DIR}/build/release/macos}"
PACKAGED_APP="${OUTPUT_DIR}/${APP_NAME}.app"
ENTITLEMENTS_FILE="${INKPOD_ENTITLEMENTS_FILE:-${ROOT_DIR}/apps/macos/App/Inkpod.entitlements}"

project_version() {
    sed -nE 's/^project\(inkpod VERSION ([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' \
        "${ROOT_DIR}/CMakeLists.txt" | head -n 1
}

default_build_number() {
    local count

    count="$(git -C "${ROOT_DIR}" rev-list --count HEAD 2>/dev/null || true)"
    if [[ -z "${count}" ]]; then
        count=1
    fi
    print -r -- "${count}"
}

VERSION="${INKPOD_VERSION:-$(project_version)}"
BUILD_NUMBER="${INKPOD_BUILD_NUMBER:-$(default_build_number)}"
DMG_FILE="${INKPOD_DMG_PATH:-${OUTPUT_DIR}/${APP_NAME}-${VERSION}-macOS-arm64.dmg}"
VOLUME_NAME="${INKPOD_VOLUME_NAME:-${APP_NAME}}"
CODESIGN_IDENTITY="${INKPOD_CODESIGN_IDENTITY:-${CODESIGN_IDENTITY:-Developer ID Application: Shuichi Kurabayashi (ETD7LJJGQZ)}}"
NOTARY_PROFILE="${INKPOD_NOTARY_PROFILE:-${NOTARY_PROFILE:-developer-id-notary}}"
NOTARY_TIMEOUT="${INKPOD_NOTARY_TIMEOUT:-${NOTARY_TIMEOUT:-30m}}"
SKIP_NOTARIZE="${INKPOD_SKIP_NOTARIZE:-${SKIP_NOTARIZE:-0}}"
GIT_REMOTE="${INKPOD_GIT_REMOTE:-origin}"
RELEASE_BRANCH="${INKPOD_RELEASE_BRANCH:-main}"
RELEASE_TAG="v${VERSION}"
GITHUB_REPOSITORY="${INKPOD_GITHUB_REPOSITORY:-}"
GITHUB_PRERELEASE="${INKPOD_GITHUB_PRERELEASE:-1}"
export CODESIGN_IDENTITY NOTARY_PROFILE

HEAD_COMMIT=""
LOCAL_DMG_SHA256=""
LOCAL_DMG_SIZE=""
PUBLISH_STATE_DIR=""
RELEASE_EXISTS=0
RELEASE_IS_DRAFT=""
RELEASE_URL=""
RELEASE_ASSET_COUNT=0
RELEASE_ASSET_SIZE=""
REMOTE_TAG_COMMIT=""

typeset -a TEMP_DIRS
TEMP_DIRS=()
typeset -a MOUNT_POINTS
MOUNT_POINTS=()

cleanup() {
    local temp_dir
    local mount_point

    for mount_point in "${MOUNT_POINTS[@]}"; do
        if [[ -n "${mount_point}" ]]; then
            hdiutil detach "${mount_point}" >/dev/null 2>&1 || true
        fi
    done

    for temp_dir in "${TEMP_DIRS[@]}"; do
        if [[ -n "${temp_dir}" && -d "${temp_dir}" ]]; then
            rm -rf -- "${temp_dir}"
        fi
    done
}

trap cleanup EXIT HUP INT TERM

log() {
    print -r -- "==> $*"
}

die() {
    print -ru2 -- "error: $*"
    exit 1
}

usage() {
    cat <<'USAGE'
Usage: ./scripts/macOS.sh <command>

Commands:
  verify      Run every automated Rust/macOS M12 release profile.
  build       Build a real arm64 Release xcarchive through CMake.
  package     Run build, stage the app bundle, set its version, and sign it.
  dmg         Run package and create a signed release DMG.
  notarize    Run dmg, submit it to Apple, staple the ticket, and assess it.
  release     Verify, sign, notarize, staple, and assess the release DMG.
  publish     Publish an existing notarized DMG to the matching GitHub Release.

Configuration environment variables:
  INKPOD_VERSION              Marketing version (default: CMake project version)
  INKPOD_BUILD_NUMBER         Decimal build number, 0-65535 (default: Git count)
  INKPOD_CODESIGN_IDENTITY    Developer ID Application identity
                              (default: Shuichi Kurabayashi / ETD7LJJGQZ)
  INKPOD_NOTARY_PROFILE       notarytool keychain profile
                              (default: developer-id-notary)
  INKPOD_NOTARY_TIMEOUT       notarytool timeout (default: 30m)
  INKPOD_SKIP_NOTARIZE=1      Rejected by release; use dmg for local packaging
  INKPOD_OUTPUT_DIR           Packaged app and DMG output directory
  INKPOD_DMG_PATH             Override the final DMG path
  INKPOD_GIT_REMOTE           Git remote used for publication (default: origin)
  INKPOD_RELEASE_BRANCH       Required synchronized branch (default: main)
  INKPOD_GITHUB_REPOSITORY    OWNER/REPO override (default: derive from remote)
  INKPOD_GITHUB_PRERELEASE    Create a prerelease when absent: 0 or 1 (default: 1)

Set INKPOD_CODESIGN_IDENTITY=- to create an ad-hoc signed app for local
package/dmg checks. Ad-hoc artifacts cannot be notarized.

publish never rebuilds or replaces a GitHub asset. It requires a clean branch
synchronized with its remote, verifies the existing notarized DMG, and uses tag
v<INKPOD_VERSION>. An identical existing asset is a no-op; different bytes are
rejected. A missing Release is created only after the remote tag is fixed to
the exact HEAD commit.
USAGE
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command was not found: $1"
}

validate_configuration() {
    [[ "$(uname -s)" == "Darwin" ]] || die "this release pipeline must run on macOS"
    [[ -n "${VERSION}" ]] || die "could not determine INKPOD_VERSION"
    [[ "${VERSION}" =~ '^[0-9]+(\.[0-9]+){1,2}$' ]] || \
        die "INKPOD_VERSION must contain two or three decimal components: ${VERSION}"
    [[ "${BUILD_NUMBER}" =~ '^[0-9]+$' ]] || \
        die "INKPOD_BUILD_NUMBER must contain only decimal digits: ${BUILD_NUMBER}"
    (( BUILD_NUMBER >= 0 && BUILD_NUMBER <= 65535 )) || \
        die "INKPOD_BUILD_NUMBER must be between 0 and 65535: ${BUILD_NUMBER}"
    [[ -n "${CODESIGN_IDENTITY}" ]] || die "INKPOD_CODESIGN_IDENTITY must not be empty"
    [[ "${SKIP_NOTARIZE}" == "0" || "${SKIP_NOTARIZE}" == "1" ]] || \
        die "INKPOD_SKIP_NOTARIZE must be 0 or 1"
}

validate_notarization_configuration() {
    [[ "${CODESIGN_IDENTITY}" != "-" ]] || \
        die "notarization requires INKPOD_CODESIGN_IDENTITY to name a Developer ID Application certificate"
    [[ -n "${NOTARY_PROFILE}" ]] || die "INKPOD_NOTARY_PROFILE must not be empty"
}

build_app() {
    require_command cmake
    require_command cargo
    require_command xcodebuild
    require_command xcrun

    log "Configuring ${BUILD_PRESET} (version ${VERSION}, build ${BUILD_NUMBER})"
    cmake --preset "${BUILD_PRESET}" \
        -DINKPOD_BUILD_NUMBER="${BUILD_NUMBER}"

    log "Building ${BUILD_TARGET}"
    cmake --build --preset "${BUILD_PRESET}" --target "${BUILD_TARGET}"

    [[ -d "${SOURCE_APP}" ]] || \
        die "CMake completed without producing ${SOURCE_APP}"
    [[ -x "${SOURCE_APP}/Contents/MacOS/${APP_NAME}" ]] || \
        die "the built app executable is missing or is not executable"

    log "Built ${SOURCE_APP}"
}

verify_release_candidate() {
    require_command cargo
    require_command cmake
    require_command ctest
    require_command xcodebuild
    require_command xcrun

    log "Running portable Rust formatting, lint, test, benchmark, and docs"
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features
    cargo bench --package inkpod-core --bench core_workflows -- --quick
    RUSTDOCFLAGS="-D warnings" cargo doc \
        --package inkpod-core --all-features --no-deps

    log "Running macOS unit, integration, ABI, parity, and source gates"
    cmake --preset macos-arm64-debug \
        -DINKPOD_BUILD_NUMBER="${BUILD_NUMBER}"
    cmake --build --preset macos-arm64-debug --target inkpod_macos_check
    ctest --preset macos-arm64-debug --output-on-failure

    log "Running launched-product UI and accessibility audit"
    cmake --build --preset macos-arm64-debug --target inkpod_macos_ui_test

    log "Running Metal API Validation and 200-cycle ownership soak"
    cmake --build --preset macos-arm64-debug --target inkpod_macos_metal_check

    log "Running the fixed-owner-thread suite under Thread Sanitizer"
    cmake --build --preset macos-arm64-debug --target inkpod_macos_tsan

    build_app
    log "All automated macOS M12 release profiles passed"
}

set_plist_string() {
    local plist="$1"
    local key="$2"
    local value="$3"
    local plist_buddy=/usr/libexec/PlistBuddy

    [[ -x "${plist_buddy}" ]] || die "PlistBuddy was not found"
    if "${plist_buddy}" -c "Print :${key}" "${plist}" >/dev/null 2>&1; then
        "${plist_buddy}" -c "Set :${key} ${value}" "${plist}"
    else
        "${plist_buddy}" -c "Add :${key} string ${value}" "${plist}"
    fi
}

codesign_item() {
    local item="$1"

    if [[ "${CODESIGN_IDENTITY}" == "-" ]]; then
        codesign --force --options runtime --sign - "${item}"
    else
        codesign --force --options runtime --timestamp \
            --sign "${CODESIGN_IDENTITY}" "${item}"
    fi
}

sign_app() {
    local app="$1"
    local code_item

    require_command codesign
    require_command file

    # Sign nested Mach-O code before enclosing bundles, then sign the app last.
    while IFS= read -r -d '' code_item; do
        if file -b "${code_item}" | grep -q 'Mach-O'; then
            codesign_item "${code_item}"
        fi
    done < <(find "${app}/Contents" -type f -print0)

    while IFS= read -r -d '' code_item; do
        codesign_item "${code_item}"
    done < <(find "${app}/Contents" -depth -type d \
        \( -name '*.framework' -o -name '*.xpc' -o -name '*.appex' -o -name '*.app' \) \
        -print0)

    if [[ "${CODESIGN_IDENTITY}" == "-" ]]; then
        codesign --force --options runtime --sign - \
            --entitlements "${ENTITLEMENTS_FILE}" "${app}"
    else
        codesign --force --options runtime --timestamp \
            --sign "${CODESIGN_IDENTITY}" \
            --entitlements "${ENTITLEMENTS_FILE}" "${app}"
    fi

    codesign --verify --deep --strict --verbose=2 "${app}"
}

package_app() {
    local stage_dir
    local staged_app
    local plist

    require_command ditto
    require_command xattr
    [[ -f "${ENTITLEMENTS_FILE}" ]] || \
        die "entitlements file was not found: ${ENTITLEMENTS_FILE}"

    mkdir -p "${OUTPUT_DIR}"
    stage_dir="$(mktemp -d "${OUTPUT_DIR}/.inkpod-package.XXXXXX")"
    TEMP_DIRS+=("${stage_dir}")
    staged_app="${stage_dir}/${APP_NAME}.app"

    log "Staging ${APP_NAME}.app"
    ditto --norsrc --noextattr "${SOURCE_APP}" "${staged_app}"
    xattr -cr "${staged_app}" 2>/dev/null || true

    plist="${staged_app}/Contents/Info.plist"
    [[ -f "${plist}" ]] || die "the staged app has no Info.plist"
    set_plist_string "${plist}" CFBundleShortVersionString "${VERSION}"
    set_plist_string "${plist}" CFBundleVersion "${BUILD_NUMBER}"

    log "Signing ${APP_NAME}.app with ${CODESIGN_IDENTITY}"
    sign_app "${staged_app}"

    if [[ -e "${PACKAGED_APP}" ]]; then
        rm -rf -- "${PACKAGED_APP}"
    fi
    mv "${staged_app}" "${PACKAGED_APP}"

    log "Packaged ${PACKAGED_APP}"
}

create_dmg() {
    local dmg_parent
    local stage_dir
    local dmg_root
    local staged_dmg

    require_command hdiutil
    require_command ditto

    dmg_parent="$(dirname "${DMG_FILE}")"
    mkdir -p "${dmg_parent}"
    stage_dir="$(mktemp -d "${dmg_parent}/.inkpod-dmg.XXXXXX")"
    TEMP_DIRS+=("${stage_dir}")
    dmg_root="${stage_dir}/root"
    staged_dmg="${stage_dir}/${APP_NAME}.dmg"
    mkdir -p "${dmg_root}"

    log "Preparing DMG contents"
    ditto --norsrc --noextattr "${PACKAGED_APP}" "${dmg_root}/${APP_NAME}.app"
    ln -s /Applications "${dmg_root}/Applications"

    log "Creating compressed DMG"
    hdiutil create \
        -volname "${VOLUME_NAME}" \
        -srcfolder "${dmg_root}" \
        -format UDZO \
        -ov \
        "${staged_dmg}"

    if [[ "${CODESIGN_IDENTITY}" != "-" ]]; then
        log "Signing the DMG with ${CODESIGN_IDENTITY}"
        codesign --force --timestamp --sign "${CODESIGN_IDENTITY}" "${staged_dmg}"
        codesign --verify --verbose=2 "${staged_dmg}"
    else
        log "Leaving the DMG unsigned because the app uses an ad-hoc identity"
    fi

    log "Verifying the DMG checksum"
    hdiutil verify "${staged_dmg}"
    mv -f "${staged_dmg}" "${DMG_FILE}"
    log "Created ${DMG_FILE}"
}

notarize_dmg() {
    local stage_dir
    local submission_file
    local submission_id
    local submission_status

    require_command xcrun
    require_command spctl
    validate_notarization_configuration
    [[ -f "${DMG_FILE}" ]] || die "DMG was not produced: ${DMG_FILE}"

    stage_dir="$(mktemp -d "${OUTPUT_DIR}/.inkpod-notary.XXXXXX")"
    TEMP_DIRS+=("${stage_dir}")
    submission_file="${stage_dir}/submission.json"

    log "Submitting the DMG using notarytool profile ${NOTARY_PROFILE}"
    if ! xcrun notarytool submit "${DMG_FILE}" \
            --keychain-profile "${NOTARY_PROFILE}" \
            --wait \
            --timeout "${NOTARY_TIMEOUT}" \
            --output-format json >"${submission_file}"; then
        [[ ! -s "${submission_file}" ]] || cat "${submission_file}" >&2
        die "notarytool submission failed"
    fi
    cat "${submission_file}"

    submission_id="$(/usr/bin/plutil -extract id raw -o - "${submission_file}" 2>/dev/null || true)"
    submission_status="$(/usr/bin/plutil -extract status raw -o - "${submission_file}" 2>/dev/null || true)"
    [[ -n "${submission_id}" ]] || die "notarytool returned no submission id"
    if [[ "${submission_status}" != "Accepted" ]]; then
        xcrun notarytool log "${submission_id}" \
            --keychain-profile "${NOTARY_PROFILE}" || true
        die "notarytool did not accept the DMG (status: ${submission_status:-unknown})"
    fi

    log "Retrieving the accepted notarization log"
    xcrun notarytool log "${submission_id}" \
        --keychain-profile "${NOTARY_PROFILE}"

    log "Stapling and validating the notarization ticket"
    xcrun stapler staple -v "${DMG_FILE}"
    xcrun stapler validate -v "${DMG_FILE}"

    log "Assessing the notarized DMG with Gatekeeper"
    spctl --assess --type open --context context:primary-signature \
        --verbose=4 "${DMG_FILE}"
    log "Notarized ${DMG_FILE}"
}

resolve_github_repository() {
    local remote_url
    local repository

    if [[ -n "${GITHUB_REPOSITORY}" ]]; then
        repository="${GITHUB_REPOSITORY}"
    else
        remote_url="$(git -C "${ROOT_DIR}" remote get-url "${GIT_REMOTE}")" || \
            die "could not read Git remote ${GIT_REMOTE}"
        case "${remote_url}" in
            git@github.com:*)
                repository="${remote_url#git@github.com:}"
                ;;
            https://github.com/*)
                repository="${remote_url#https://github.com/}"
                ;;
            ssh://git@github.com/*)
                repository="${remote_url#ssh://git@github.com/}"
                ;;
            *)
                die "${GIT_REMOTE} is not a supported github.com remote: ${remote_url}"
                ;;
        esac
        repository="${repository%.git}"
    fi

    [[ "${repository}" =~ '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' ]] || \
        die "GitHub repository must have OWNER/REPO form: ${repository}"
    GITHUB_REPOSITORY="${repository}"
}

prepare_publish_repository() {
    local worktree_status
    local branch
    local remote_commit
    local source_version

    require_command git
    require_command gh

    [[ "${GIT_REMOTE}" =~ '^[A-Za-z0-9][A-Za-z0-9._-]*$' ]] || \
        die "INKPOD_GIT_REMOTE is not a valid remote name: ${GIT_REMOTE}"
    git -C "${ROOT_DIR}" check-ref-format "refs/heads/${RELEASE_BRANCH}" \
        >/dev/null || die "INKPOD_RELEASE_BRANCH is not a valid branch: ${RELEASE_BRANCH}"
    [[ "${GITHUB_PRERELEASE}" == "0" || "${GITHUB_PRERELEASE}" == "1" ]] || \
        die "INKPOD_GITHUB_PRERELEASE must be 0 or 1"

    worktree_status="$(git -C "${ROOT_DIR}" status --porcelain)" || \
        die "could not inspect the Git working tree"
    [[ -z "${worktree_status}" ]] || \
        die "the working tree must be clean before publishing"

    branch="$(git -C "${ROOT_DIR}" branch --show-current)" || \
        die "could not determine the current Git branch"
    [[ "${branch}" == "${RELEASE_BRANCH}" ]] || \
        die "release branch ${RELEASE_BRANCH} is required, but ${branch:-detached HEAD} is checked out"

    source_version="$(project_version)"
    [[ "${source_version}" == "${VERSION}" ]] || \
        die "INKPOD_VERSION ${VERSION} does not match source version ${source_version}"

    resolve_github_repository
    gh auth status --hostname github.com >/dev/null || \
        die "GitHub CLI authentication is unavailable"

    git -C "${ROOT_DIR}" fetch --prune "${GIT_REMOTE}" "${RELEASE_BRANCH}" || \
        die "could not fetch ${GIT_REMOTE}/${RELEASE_BRANCH}"
    HEAD_COMMIT="$(git -C "${ROOT_DIR}" rev-parse HEAD)" || \
        die "could not resolve HEAD"
    remote_commit="$(git -C "${ROOT_DIR}" rev-parse "${GIT_REMOTE}/${RELEASE_BRANCH}")" || \
        die "could not resolve ${GIT_REMOTE}/${RELEASE_BRANCH}"
    [[ "${HEAD_COMMIT}" == "${remote_commit}" ]] || \
        die "HEAD ${HEAD_COMMIT} is not synchronized with ${GIT_REMOTE}/${RELEASE_BRANCH} ${remote_commit}"
}

verify_publish_candidate() {
    local expected_name="${APP_NAME}-${VERSION}-macOS-arm64.dmg"
    local checksum_line
    local mount_point
    local mounted_app
    local mounted_executable
    local mounted_info
    local mounted_entitlements
    local mounted_identifier
    local mounted_version
    local mounted_build
    local mounted_architecture
    local entitlement_count
    local entitlement_key

    require_command codesign
    require_command hdiutil
    require_command lipo
    require_command shasum
    require_command spctl
    require_command stat
    require_command xcrun
    validate_notarization_configuration

    [[ "${DMG_FILE:t}" == "${expected_name}" ]] || \
        die "publish requires DMG filename ${expected_name}: ${DMG_FILE}"
    [[ -f "${DMG_FILE}" ]] || die "release DMG was not found: ${DMG_FILE}"

    log "Verifying signed and notarized release candidate"
    codesign --verify --verbose=4 "${DMG_FILE}"
    xcrun stapler validate -v "${DMG_FILE}"
    spctl --assess --type open --context context:primary-signature \
        --verbose=4 "${DMG_FILE}"
    hdiutil verify "${DMG_FILE}"

    mount_point="$(mktemp -d "${PUBLISH_STATE_DIR}/mount.XXXXXX")"
    hdiutil attach -readonly -nobrowse -mountpoint "${mount_point}" \
        "${DMG_FILE}" >/dev/null || die "could not mount release DMG read-only"
    MOUNT_POINTS+=("${mount_point}")
    mounted_app="${mount_point}/${APP_NAME}.app"
    mounted_executable="${mounted_app}/Contents/MacOS/${APP_NAME}"
    mounted_info="${mounted_app}/Contents/Info.plist"
    mounted_entitlements="${PUBLISH_STATE_DIR}/mounted-entitlements.plist"

    [[ -d "${mounted_app}" && -x "${mounted_executable}" && -f "${mounted_info}" ]] || \
        die "release DMG does not contain a complete ${APP_NAME}.app"
    codesign --verify --deep --strict --verbose=4 "${mounted_app}"
    mounted_identifier="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
        "${mounted_info}")" || die "mounted app has no bundle identifier"
    mounted_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
        "${mounted_info}")" || die "mounted app has no marketing version"
    mounted_build="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' \
        "${mounted_info}")" || die "mounted app has no build number"
    [[ "${mounted_identifier}" == "com.inkpod.app" ]] || \
        die "mounted app has unexpected bundle identifier ${mounted_identifier}"
    [[ "${mounted_version}" == "${VERSION}" ]] || \
        die "mounted app version ${mounted_version} does not match ${VERSION}"
    [[ "${mounted_build}" == "${BUILD_NUMBER}" ]] || \
        die "mounted app build ${mounted_build} does not match ${BUILD_NUMBER}"
    mounted_architecture="$(lipo -archs "${mounted_executable}")" || \
        die "could not inspect mounted app architecture"
    [[ "${mounted_architecture}" == "arm64" ]] || \
        die "mounted app must contain exactly arm64, found ${mounted_architecture}"

    codesign -d --entitlements :- "${mounted_app}" \
        >"${mounted_entitlements}" 2>/dev/null || \
        die "could not read mounted app entitlements"
    /usr/bin/plutil -lint "${mounted_entitlements}" >/dev/null || \
        die "mounted app entitlements are not a valid property list"
    for entitlement_key in \
        com.apple.security.app-sandbox \
        com.apple.security.files.bookmarks.app-scope \
        com.apple.security.files.user-selected.read-write; do
        [[ "$(/usr/libexec/PlistBuddy -c "Print :${entitlement_key}" \
            "${mounted_entitlements}")" == "true" ]] || \
            die "mounted app is missing required entitlement ${entitlement_key}"
    done
    entitlement_count="$(
        grep -oF '<key>' "${mounted_entitlements}" |
            wc -l |
            tr -d '[:space:]'
    )"
    [[ "${entitlement_count}" == "3" ]] || \
        die "mounted app must contain exactly the three approved entitlements"

    hdiutil detach "${mount_point}" >/dev/null || \
        die "could not detach verified release DMG"
    MOUNT_POINTS[-1]=()

    LOCAL_DMG_SIZE="$(stat -f '%z' "${DMG_FILE}")" || \
        die "could not measure ${DMG_FILE}"
    checksum_line="$(shasum -a 256 "${DMG_FILE}")" || \
        die "could not hash ${DMG_FILE}"
    LOCAL_DMG_SHA256="${checksum_line%%[[:space:]]*}"
    [[ "${LOCAL_DMG_SHA256}" =~ '^[0-9a-f]{64}$' ]] || \
        die "shasum returned an invalid SHA-256 for ${DMG_FILE}"
}

read_remote_tag() {
    local lines
    local object_id
    local ref_name
    local direct_commit=""
    local peeled_commit=""

    lines="$(git -C "${ROOT_DIR}" ls-remote --tags "${GIT_REMOTE}" \
        "refs/tags/${RELEASE_TAG}" "refs/tags/${RELEASE_TAG}^{}")" || \
        die "could not query remote tag ${RELEASE_TAG}"
    while IFS=$'\t' read -r object_id ref_name; do
        if [[ "${ref_name}" == "refs/tags/${RELEASE_TAG}" ]]; then
            direct_commit="${object_id}"
        elif [[ "${ref_name}" == "refs/tags/${RELEASE_TAG}^{}" ]]; then
            peeled_commit="${object_id}"
        fi
    done <<< "${lines}"
    REMOTE_TAG_COMMIT="${peeled_commit:-${direct_commit}}"
}

ensure_remote_tag() {
    local local_tag_commit=""
    local local_tag_exists=0
    local created_local_tag=0

    if local_tag_commit="$(git -C "${ROOT_DIR}" rev-parse \
            "refs/tags/${RELEASE_TAG}^{commit}" 2>/dev/null)"; then
        local_tag_exists=1
    fi

    read_remote_tag
    if [[ -n "${REMOTE_TAG_COMMIT}" ]]; then
        [[ "${REMOTE_TAG_COMMIT}" == "${HEAD_COMMIT}" ]] || \
            die "remote tag ${RELEASE_TAG} points to ${REMOTE_TAG_COMMIT}, not HEAD ${HEAD_COMMIT}"
        if (( local_tag_exists )); then
            [[ "${local_tag_commit}" == "${HEAD_COMMIT}" ]] || \
                die "local tag ${RELEASE_TAG} points to ${local_tag_commit}, not HEAD ${HEAD_COMMIT}"
        fi
        return
    fi

    if (( local_tag_exists )); then
        [[ "${local_tag_commit}" == "${HEAD_COMMIT}" ]] || \
            die "local tag ${RELEASE_TAG} points to ${local_tag_commit}, not HEAD ${HEAD_COMMIT}"
    else
        git -C "${ROOT_DIR}" tag -a "${RELEASE_TAG}" "${HEAD_COMMIT}" \
            -m "inkpod ${RELEASE_TAG}" || \
            die "could not create local tag ${RELEASE_TAG}"
        created_local_tag=1
    fi

    if git -C "${ROOT_DIR}" push "${GIT_REMOTE}" \
            "refs/tags/${RELEASE_TAG}:refs/tags/${RELEASE_TAG}"; then
        read_remote_tag
        [[ "${REMOTE_TAG_COMMIT}" == "${HEAD_COMMIT}" ]] || \
            die "pushed tag ${RELEASE_TAG} did not resolve to HEAD ${HEAD_COMMIT}"
        return
    fi

    log "Tag push returned a conflict; checking for a concurrent publisher"
    read_remote_tag
    if [[ "${REMOTE_TAG_COMMIT}" == "${HEAD_COMMIT}" ]]; then
        log "Concurrent publisher fixed ${RELEASE_TAG} to the same commit"
        return
    fi
    if (( created_local_tag )); then
        git -C "${ROOT_DIR}" tag -d "${RELEASE_TAG}" >/dev/null || true
    fi
    if [[ -n "${REMOTE_TAG_COMMIT}" ]]; then
        die "remote tag ${RELEASE_TAG} points to ${REMOTE_TAG_COMMIT}, not HEAD ${HEAD_COMMIT}"
    fi
    die "tag push failed and remote tag ${RELEASE_TAG} was not created"
}

read_release_state() {
    local release_file="${PUBLISH_STATE_DIR}/release.json"
    local error_file="${PUBLISH_STATE_DIR}/release-error.txt"
    local actual_tag
    local asset_total
    local asset_index
    local asset_name

    RELEASE_EXISTS=0
    RELEASE_IS_DRAFT=""
    RELEASE_URL=""
    RELEASE_ASSET_COUNT=0
    RELEASE_ASSET_SIZE=""

    if ! gh release view "${RELEASE_TAG}" --repo "${GITHUB_REPOSITORY}" \
            --json tagName,isDraft,isPrerelease,url,assets \
            >"${release_file}" 2>"${error_file}"; then
        gh api "repos/${GITHUB_REPOSITORY}" --silent >/dev/null || {
            [[ ! -s "${error_file}" ]] || cat "${error_file}" >&2
            die "could not access GitHub repository ${GITHUB_REPOSITORY}"
        }
        return
    fi

    actual_tag="$(/usr/bin/plutil -extract tagName raw -o - "${release_file}")" || \
        die "GitHub release response has no tagName"
    [[ "${actual_tag}" == "${RELEASE_TAG}" ]] || \
        die "GitHub release returned tag ${actual_tag}, expected ${RELEASE_TAG}"
    RELEASE_IS_DRAFT="$(/usr/bin/plutil -extract isDraft raw -o - "${release_file}")" || \
        die "GitHub release response has no draft state"
    RELEASE_URL="$(/usr/bin/plutil -extract url raw -o - "${release_file}")" || \
        die "GitHub release response has no URL"
    [[ "${RELEASE_IS_DRAFT}" == "false" ]] || \
        die "GitHub release ${RELEASE_TAG} is a draft and is not a public release"

    asset_total="$(/usr/bin/plutil -extract assets raw -o - "${release_file}")" || \
        die "GitHub release response has no asset list"
    for (( asset_index = 0; asset_index < asset_total; ++asset_index )); do
        asset_name="$(/usr/bin/plutil -extract \
            "assets.${asset_index}.name" raw -o - "${release_file}")" || \
            die "GitHub release response contains an asset without a name"
        if [[ "${asset_name}" == "${DMG_FILE:t}" ]]; then
            (( ++RELEASE_ASSET_COUNT ))
            RELEASE_ASSET_SIZE="$(/usr/bin/plutil -extract \
                "assets.${asset_index}.size" raw -o - "${release_file}")" || \
                die "GitHub release asset ${asset_name} has no size"
        fi
    done
    (( RELEASE_ASSET_COUNT <= 1 )) || \
        die "GitHub release ${RELEASE_TAG} has duplicate ${DMG_FILE:t} assets"
    RELEASE_EXISTS=1
}

verify_remote_asset_bytes() {
    local download_dir
    local downloaded_asset
    local checksum_line
    local remote_sha256

    [[ "${RELEASE_ASSET_COUNT}" == "1" ]] || \
        die "GitHub release ${RELEASE_TAG} has no ${DMG_FILE:t} asset"
    [[ "${RELEASE_ASSET_SIZE}" == "${LOCAL_DMG_SIZE}" ]] || \
        die "GitHub release ${RELEASE_TAG} already contains ${DMG_FILE:t} with different bytes"

    download_dir="$(mktemp -d "${PUBLISH_STATE_DIR}/download.XXXXXX")"
    downloaded_asset="${download_dir}/${DMG_FILE:t}"
    gh release download "${RELEASE_TAG}" --repo "${GITHUB_REPOSITORY}" \
        --pattern "${DMG_FILE:t}" --dir "${download_dir}" || \
        die "could not download existing GitHub release asset ${DMG_FILE:t}"
    [[ -f "${downloaded_asset}" ]] || \
        die "GitHub release download did not produce ${DMG_FILE:t}"
    checksum_line="$(shasum -a 256 "${downloaded_asset}")" || \
        die "could not hash downloaded GitHub release asset"
    remote_sha256="${checksum_line%%[[:space:]]*}"
    [[ "${remote_sha256}" == "${LOCAL_DMG_SHA256}" ]] || \
        die "GitHub release ${RELEASE_TAG} already contains ${DMG_FILE:t} with different bytes"
}

publish_release_dmg() {
    typeset -a create_arguments

    prepare_publish_repository
    mkdir -p "${OUTPUT_DIR}"
    PUBLISH_STATE_DIR="$(mktemp -d "${OUTPUT_DIR}/.inkpod-publish.XXXXXX")"
    TEMP_DIRS+=("${PUBLISH_STATE_DIR}")
    verify_publish_candidate

    ensure_remote_tag
    read_release_state
    if (( ! RELEASE_EXISTS )); then
        create_arguments=(
            release create "${RELEASE_TAG}"
            "${DMG_FILE}"
            --repo "${GITHUB_REPOSITORY}"
            --target "${HEAD_COMMIT}"
            --verify-tag
            --generate-notes
            --title "inkpod ${VERSION}"
        )
        if [[ "${GITHUB_PRERELEASE}" == "1" ]]; then
            create_arguments+=(--prerelease)
        fi
        if ! gh "${create_arguments[@]}"; then
            log "Release creation returned a conflict; checking for a concurrent publisher"
            read_release_state
            (( RELEASE_EXISTS )) || \
                die "GitHub release creation failed and ${RELEASE_TAG} does not exist"
            log "Concurrent publisher created ${RELEASE_TAG} for the same tag"
        else
            read_release_state
            (( RELEASE_EXISTS )) || \
                die "GitHub release creation succeeded but ${RELEASE_TAG} cannot be read"
        fi
    fi

    read_remote_tag
    [[ "${REMOTE_TAG_COMMIT}" == "${HEAD_COMMIT}" ]] || \
        die "remote tag ${RELEASE_TAG} changed to ${REMOTE_TAG_COMMIT:-missing} before asset publication"

    if (( RELEASE_ASSET_COUNT == 1 )); then
        verify_remote_asset_bytes
        read_remote_tag
        [[ "${REMOTE_TAG_COMMIT}" == "${HEAD_COMMIT}" ]] || \
            die "remote tag ${RELEASE_TAG} changed to ${REMOTE_TAG_COMMIT:-missing} during asset verification"
        log "GitHub release ${RELEASE_TAG} already contains the identical ${DMG_FILE:t}; no upload needed"
        log "Published release: ${RELEASE_URL}"
        return
    fi

    log "Uploading ${DMG_FILE:t} to ${GITHUB_REPOSITORY} ${RELEASE_TAG}"
    if ! gh release upload "${RELEASE_TAG}" "${DMG_FILE}" \
            --repo "${GITHUB_REPOSITORY}"; then
        log "Asset upload returned a conflict; checking for a concurrent publisher"
        read_release_state
        if (( RELEASE_EXISTS && RELEASE_ASSET_COUNT == 1 )); then
            verify_remote_asset_bytes
            log "Concurrent publisher uploaded the identical ${DMG_FILE:t}"
        else
            die "GitHub asset upload failed and no identical remote asset exists"
        fi
    else
        read_release_state
        verify_remote_asset_bytes
    fi

    read_remote_tag
    [[ "${REMOTE_TAG_COMMIT}" == "${HEAD_COMMIT}" ]] || \
        die "remote tag ${RELEASE_TAG} changed to ${REMOTE_TAG_COMMIT:-missing} during asset publication"

    log "Published release: ${RELEASE_URL}"
    log "Published SHA-256: ${LOCAL_DMG_SHA256}"
}

run_through_dmg() {
    build_app
    package_app
    create_dmg
}

main() {
    local command="${1:-}"

    if [[ "${command}" == "help" || "${command}" == "-h" || "${command}" == "--help" ]]; then
        usage
        return
    fi
    [[ -n "${command}" ]] || {
        usage >&2
        return 2
    }
    [[ $# -eq 1 ]] || die "only one subcommand is accepted"

    validate_configuration

    case "${command}" in
        verify)
            verify_release_candidate
            ;;
        build)
            build_app
            ;;
        package)
            build_app
            package_app
            ;;
        dmg)
            run_through_dmg
            ;;
        notarize)
            validate_notarization_configuration
            run_through_dmg
            notarize_dmg
            ;;
        release)
            [[ "${SKIP_NOTARIZE}" != "1" ]] || \
                die "release cannot skip notarization; use the dmg command for local packaging"
            validate_notarization_configuration
            verify_release_candidate
            package_app
            create_dmg
            notarize_dmg
            if command -v shasum >/dev/null 2>&1; then
                shasum -a 256 "${DMG_FILE}"
            fi
            log "Release artifact: ${DMG_FILE}"
            ;;
        publish)
            publish_release_dmg
            ;;
        *)
            usage >&2
            die "unknown subcommand: ${command}"
            ;;
    esac
}

main "$@"
