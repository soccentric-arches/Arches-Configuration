# webconfig-static — frontend assets served by the daemon (spec §2/§3).
#
# Vanilla JS, no build step, no node at build time. Total <500 KB. The daemon
# serves these from /usr/share/webconfig/static via ServeDir. No server-side
# rendering (spec §2).

SUMMARY = "SoCcentric WebConfig frontend static assets"
LICENSE = "CLOSED"

# Source is the frontend/ tree from this repo. In a released build this would
# be a tarball; for in-tree builds we point SRC_URI at the checked-in files.
SRC_URI = "file://index.html \
           file://styles.css \
           file://schema-form.js \
           file://pages.js \
           file://app.js"

S = "${WORKDIR}"

do_install() {
    install -d ${D}/usr/share/webconfig/static
    install -m 0644 ${S}/index.html ${D}/usr/share/webconfig/static/
    install -m 0644 ${S}/styles.css ${D}/usr/share/webconfig/static/
    install -m 0644 ${S}/schema-form.js ${D}/usr/share/webconfig/static/
    install -m 0644 ${S}/pages.js ${D}/usr/share/webconfig/static/
    install -m 0644 ${S}/app.js ${D}/usr/share/webconfig/static/
}

FILES:${PN} = "/usr/share/webconfig/static"