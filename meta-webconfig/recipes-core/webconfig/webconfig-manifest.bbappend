# webconfig-manifest.bbappend — per-machine YAML board selection (spec §3).
#
# The six platforms differ ONLY in their board manifest. This bbappend maps each
# project's MACHINE name to the right manifest file. Replace the placeholder
# machine names below with the real MACHINE names from each project's BSP layer
# (e.g. Acadia's `raspberrypi-cm4-64`, Arches' `jetson-orin-nano`, etc.).
#
# In a multi-platform master build, this layer carries all six manifest files in
# files/ and selects one per MACHINE. In a standalone single-platform clone, only
# the relevant file is present and the default WEBCONFIG_MANIFEST_FILE picks it.

# Default (dev/CI when no machine override matches).
# WEBCONFIG_BOARD_FILE = "generic.yaml"

# Per-machine overrides — uncomment and set to your BSP's MACHINE names:
# WEBCONFIG_BOARD_FILE:raspberrypi-cm4-64   = "acadia.yaml"
# WEBCONFIG_BOARD_FILE:jetson-orin-nano      = "arches.yaml"
# WEBCONFIG_BOARD_FILE:am62xx-evm            = "joshua.yaml"
# WEBCONFIG_BOARD_FILE:imx8mp-var-dart       = "pinnacle.yaml"
# WEBCONFIG_BOARD_FILE:genericx86-64         = "sequoia.yaml"
# WEBCONFIG_BOARD_FILE:zynqmp-generic        = "zion.yaml"

# Mirror the platform id into the daemon environment so the embedded fallback
# manifest matches the installed one (spec §3).
# This would live in a webconfigd.bbappend per machine:
#   WEBCONFIG_PLATFORM:jetson-orin-nano = "arches"