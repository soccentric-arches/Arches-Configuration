# webconfig-manifest — YAML board definition (spec §3 platform layer).
#
# Installs /usr/share/webconfig/board.yaml. The six platforms differ ONLY in
# this layer (spec §3). Per-machine customization is done via a bbappend that
# overrides WEBCONFIG_BOARD_FILE:
#
#   # meta-soccentric-webconfig/recipes-core/webconfig/webconfig-manifest.bbappend
#   WEBCONFIG_BOARD_FILE = "arches.yaml"
#   FILESEXTRAPATHS:prepend := "${THISDIR}/files:"
#
# The daemon reads this file at boot to drive I/O rendering, the service
# allowlist, dashboard tiles, and trust flags. Missing it falls back to the
# embedded example for WEBCONFIG_PLATFORM (dev/CI only — spec §3).

SUMMARY = "SoCcentric WebConfig board manifest"
LICENSE = "CLOSED"

# Default manifest; override per-machine via bbappend.
WEBCONFIG_BOARD_FILE ?= "generic.yaml"

# The board-manifests/ directory in this layer ships one file per platform.
FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI = "file://${WEBCONFIG_BOARD_FILE}"

S = "${WORKDIR}"

do_install() {
    install -d ${D}/usr/share/webconfig
    install -m 0644 ${S}/${WEBCONFIG_BOARD_FILE} \
        ${D}/usr/share/webconfig/board.yaml
}

FILES:${PN} = "/usr/share/webconfig/board.yaml"