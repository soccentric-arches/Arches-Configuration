# webconfigd — on-device WebConfig daemon recipe (spec §2/§5).
#
# Builds the Rust workspace with the exact release profile (opt-level=z, lto,
# codegen-units=1, panic=abort, stripped) for the three cross-targets, using
# the committed vendored crate tarball so builds are fully offline and
# reproducible (spec §2). No network access at build time.
#
# The daemon runs as a non-root user with a hardened systemd unit (see
# webconfigd.service) and talks to systemd over D-Bus via polkit (see
# 50-webconfigd.polkit.rules).
#
# Arches-OS's poky is kirkstone, which does not ship oe-core's built-in
# cargo.bbclass path the way scarthgap+ does, and its rust-native is well
# below this daemon's MSRV (1.85). Rather than building a newer Rust from
# source, this inherits cargo_bin — meta-rust-bin's prebuilt-toolchain class
# (already in Arches-OS's kas layer set) — instead of the plain cargo class
# every other project's webconfigd.bb uses. See kas/common.yaml's
# PREFERRED_VERSION:rust-bin-cross-aarch64 / cargo-bin-cross-aarch64 pins,
# which were already staged there for this recipe.

SUMMARY = "SoCcentric on-device web configuration daemon"
DESCRIPTION = "Browser-based device configuration UI for embedded Linux; one Rust daemon, TLS, no database."
HOMEPAGE = "https://soccentric.example/webconfig"
LICENSE = "CLOSED"
LIC_FILES_CHKSUM = "file://LICENSE;md5=3382002c026d2dcb5817068a8f184313"

SRC_URI = " \
    file://webconfigd-${PV}.tar.gz;unpack=1;name=src \
    file://vendor.tar.gz;unpack=1;subdir=webconfigd-${PV};name=vendor \
    file://webconfigd.service \
    file://webconfigd.env \
    file://50-webconfigd.polkit.rules \
"
# SHA-256 of the source + vendored tarballs, for the SPDX/SBOM and
# supply-chain audit (global §1: reproducible builds, pinned versions).
# These were a placeholder value copied verbatim across every project in
# the fleet (identical across repos with different content, which cannot
# be a real checksum for any of them) and the tarballs themselves never
# existed anywhere -- do_fetch failed outright the first time this recipe
# was ever actually built. Both tarballs are now committed in this
# directory's files/ (git archive of this commit + `cargo vendor
# --versioned-dirs`) with real checksums computed from them. Regenerate
# both (tools/release-artifacts.sh, or the equivalent by hand) at every
# release cut -- this checksum is only valid for the exact tarball
# contents it was computed from.
SRC_URI[src.sha256sum] = "7841421095a1b485e1e8703937c0ab072f2a84964eadfd0fdd6fde243cbd8544"
SRC_URI[vendor.sha256sum] = "4adb755f4fbb75a571849aa7eeea901c18ab3fe28ff2acfd2a8e77822d9a2c06"

S = "${WORKDIR}/webconfigd-${PV}"

inherit cargo_bin cargo-update-recipe-crates pkgconfig systemd useradd

# --- Build configuration (spec §2 release profile is in Cargo.toml) ----------
# The release profile (opt-level="z", lto=true, codegen-units=1, panic="abort",
# strip="symbols") is committed in Cargo.toml and honored by cargo.bbclass.
CARGO_BUILD_FLAGS = "--release"

# Offline build: use the committed vendor/ tarball, never the network.
CARGO_VENDORING_DIR = "${S}/vendor"
export CARGO_NET_OFFLINE = "1"

# Target the image's target triple (cross-compile handled by the toolchain).
CARGO_SRC_DIR = "${S}"

# --- Runtime user (non-root, spec §4.8) --------------------------------------
WEBCONFIG_USER = "webconfig"
WEBCONFIG_GROUP = "webconfig"
USERADD_PACKAGES = "${PN}"
USERADD_PARAM:${PN} = "-r -M -d /var/lib/webconfig -s /usr/sbin/nologin ${WEBCONFIG_USER}"
GROUPADD_PARAM:${PN} = "-r ${WEBCONFIG_GROUP}"

# --- Installed files ---------------------------------------------------------
# The stripped, size-optimized binary (spec §2: 3-6 MB).
do_install:append() {
    install -d ${D}${bindir}
    install -m 0755 ${B}/release/webconfigd ${D}${bindir}/webconfigd

    # Data partition owned by the runtime user (config.json, users.json, certs).
    install -d -o ${WEBCONFIG_USER} -g ${WEBCONFIG_GROUP} ${D}/var/lib/webconfig

    # Hardened systemd unit + environment.
    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${WORKDIR}/webconfigd.service ${D}${systemd_system_unitdir}/
    install -d ${D}${sysconfdir}/default
    install -m 0644 ${WORKDIR}/webconfigd.env ${D}${sysconfdir}/default/webconfigd

    # polkit rule granting the daemon the limited D-Bus actions it needs.
    install -d ${D}${datadir}/polkit-1/rules.d
    install -m 0644 ${WORKDIR}/50-webconfigd.polkit.rules \
        ${D}${datadir}/polkit-1/rules.d/50-webconfigd.rules
}

SYSTEMD_SERVICE:${PN} = "webconfigd.service"
SYSTEMD_AUTO_ENABLE:${PN} = "enable"

# Pull in CA certs for TLS verification against upstream (OTA) servers.
RDEPENDS:${PN} = "ca-certificates"
# systemd is the supervisor (D-Bus actions via polkit).
RRECOMMENDS:${PN} = "systemd polkit"

# SPDX/SBOM: emit the cargo dependency manifest as part of the recipe metadata
# (global §1 supply-chain concern; new deps require approval).
INHERIT += "create-spdx"
