#!/bin/zsh

set -euo pipefail

SCRIPT_DIR="${0:A:h}"
ROOT_DIR="${SCRIPT_DIR:h}"

APP_NAME="Inkpod"
BUILD_PRESET="${INKPOD_BUILD_PRESET:-macos-universal-release}"
BUILD_TARGET="${INKPOD_BUILD_TARGET:-inkpod_macos_archive}"
BUILD_DIR="${INKPOD_BUILD_DIR:-${ROOT_DIR}/build/macos-universal-release}"
SOURCE_APP="${INKPOD_SOURCE_APP:-${BUILD_DIR}/xcode-universal-derived/Build/Products/Release/${APP_NAME}.app}"
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
DMG_FILE="${INKPOD_DMG_PATH:-${OUTPUT_DIR}/${APP_NAME}-${VERSION}-macOS-universal.dmg}"
VOLUME_NAME="${INKPOD_VOLUME_NAME:-${APP_NAME}}"
CODESIGN_IDENTITY="${INKPOD_CODESIGN_IDENTITY:-${CODESIGN_IDENTITY:-Developer ID Application: Shuichi Kurabayashi (ETD7LJJGQZ)}}"
NOTARY_PROFILE="${INKPOD_NOTARY_PROFILE:-${NOTARY_PROFILE:-developer-id-notary}}"
NOTARY_TIMEOUT="${INKPOD_NOTARY_TIMEOUT:-${NOTARY_TIMEOUT:-30m}}"
SKIP_NOTARIZE="${INKPOD_SKIP_NOTARIZE:-${SKIP_NOTARIZE:-0}}"
export CODESIGN_IDENTITY NOTARY_PROFILE

typeset -a TEMP_DIRS
TEMP_DIRS=()

cleanup() {
    local temp_dir

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
  build       Build the Universal 2 Release Inkpod.app through CMake.
  package     Run build, stage the app bundle, set its version, and sign it.
  dmg         Run package and create a signed release DMG.
  notarize    Run dmg, submit it to Apple, staple the ticket, and assess it.
  release     Run the complete release pipeline (or stop at dmg when skipped).

Configuration environment variables:
  INKPOD_VERSION              Marketing version (default: CMake project version)
  INKPOD_BUILD_NUMBER         Decimal build number, 0-65535 (default: Git count)
  INKPOD_CODESIGN_IDENTITY    Developer ID Application identity
                              (default: Shuichi Kurabayashi / ETD7LJJGQZ)
  INKPOD_NOTARY_PROFILE       notarytool keychain profile
                              (default: developer-id-notary)
  INKPOD_NOTARY_TIMEOUT       notarytool timeout (default: 30m)
  INKPOD_SKIP_NOTARIZE=1      Make release stop after creating the DMG
  INKPOD_OUTPUT_DIR           Packaged app and DMG output directory
  INKPOD_DMG_PATH             Override the final DMG path

Set INKPOD_CODESIGN_IDENTITY=- to create an ad-hoc signed app for local
package/dmg checks. Ad-hoc artifacts cannot be notarized.
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
            if [[ "${SKIP_NOTARIZE}" != "1" ]]; then
                validate_notarization_configuration
            fi
            run_through_dmg
            if [[ "${SKIP_NOTARIZE}" == "1" ]]; then
                log "Skipping notarization because INKPOD_SKIP_NOTARIZE=1"
            else
                notarize_dmg
            fi
            if command -v shasum >/dev/null 2>&1; then
                shasum -a 256 "${DMG_FILE}"
            fi
            log "Release artifact: ${DMG_FILE}"
            ;;
        *)
            usage >&2
            die "unknown subcommand: ${command}"
            ;;
    esac
}

main "$@"
