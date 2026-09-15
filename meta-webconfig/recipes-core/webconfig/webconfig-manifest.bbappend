# webconfig-manifest.bbappend — Arches board manifest selection (spec §3).
#
# This repo is Arches-only; board-specific data lives solely in the board
# manifest. `webconfig-manifest.bb` already defaults WEBCONFIG_BOARD_FILE to
# the real "arches.json", so no override is required to get a correct image
# on any of Arches-OS's current Jetson boards (jetson-nano-devkit,
# jetson-tx1-devkit, jetson-tx2-devkit and variants,
# jetson-xavier-nx-devkit-tx2-nx — see kas/boards/*.yaml). This bbappend
# exists for the one case that still needs a MACHINE-specific override: a
# future board revision that ships a distinct manifest file.
WEBCONFIG_BOARD_FILE:jetson-nano-devkit = "arches.json"
