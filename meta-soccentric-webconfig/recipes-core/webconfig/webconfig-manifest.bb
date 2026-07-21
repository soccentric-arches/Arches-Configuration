# webconfig-manifest — per-platform board manifest (spec §3 platform layer).
#
# Installs /usr/share/webconfig/board-manifest.json. The six platforms differ
# ONLY in this layer (spec §3). Per-machine customization is done via a
# bbappend that overrides WEBCONFIG_MANIFEST_FILE:
#
#   # meta-soccentric-webconfig/recipes-core/webconfig/webconfig-manifest.bbappend
#   WEBCONFIG_MANIFEST_FILE = "arches.json"
#   FILESEXTRAPATHS:prepend := "${THISDIR}/files:"
#
# The daemon reads this file at boot to drive I/O rendering, the service
# allowlist, dashboard tiles, and trust flags. Missing it falls back to the
# embedded example for WEBCONFIG_PLATFORM (dev/CI only — spec §3).

SUMMARY = "SoCcentric WebConfig board manifest"
LICENSE = "CLOSED"

# Default manifest; override per-machine via bbappend.
WEBCONFIG_MANIFEST_FILE ?= "arches.json"

# The board-manifests/ directory in this layer ships one file per platform.
FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI = "file://${WEBCONFIG_MANIFEST_FILE}"

S = "${WORKDIR}"

do_install() {
    install -d ${D}/usr/share/webconfig
    install -m 0644 ${S}/${WEBCONFIG_MANIFEST_FILE} \
        ${D}/usr/share/webconfig/board-manifest.json
}

FILES:${PN} = "/usr/share/webconfig/board-manifest.json"