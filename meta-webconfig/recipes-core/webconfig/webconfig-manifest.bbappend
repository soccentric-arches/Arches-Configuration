# webconfig-manifest.bbappend — Arches board manifest selection (spec §3).
#
# This repo is Arches-only; board-specific data lives solely in the board
# manifest. This bbappend maps the Arches BSP's MACHINE name to the manifest
# file. Replace the placeholder machine name below with the real MACHINE name
# from the Arches BSP layer (e.g. `jetson-orin-nano`).

# Default (dev/CI when no machine override matches).
# WEBCONFIG_BOARD_FILE = "generic.yaml"

# Machine override — uncomment and set to your BSP's MACHINE name:
# WEBCONFIG_BOARD_FILE:jetson-orin-nano = "arches.yaml"

# Mirror the platform id into the daemon environment so the embedded fallback
# manifest matches the installed one (spec §3).
# This would live in a webconfigd.bbappend per machine:
#   WEBCONFIG_PLATFORM:jetson-orin-nano = "arches"