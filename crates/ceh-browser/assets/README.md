# TLS intermediate

`rapidssl-tls-rsa-ca-g1.der` is the public RapidSSL TLS RSA CA G1 intermediate from https://cacerts.digicert.com/RapidSSLTLSRSACAG1.crt (HTTPS-verified download).

SHA-256: `4422e963ee53cd58cc9f85cd40bf5ffec0095fdf1a154535661c1c06bcadc69b`. Expires 2027-11-02. Issuer: DigiCert Global Root G2.

The site sends its certificate without this intermediate. Firefox does not fetch a missing intermediate, so a new profile would open the site on "Warning: Security Risk"; Chrome, Edge and Brave fetch it themselves and need nothing. It is added on macOS, Windows and Linux with the installed Firefox's own NSS: `libnss3.dylib` (macOS), `nss3.dll` (Windows), and on Linux Mozilla's combined `libnss3.so` next to Firefox or, for a distribution's split NSS, `libsmime3.so` next to Firefox (Debian) or from the system (Arch, Fedora).

Imported only into CEH-owned Firefox profiles, with `includeTrust = false`. No root trust, certificate exception, system store or personal browser profile is changed. Firefox itself validates the server hostname, validity and chain to an existing trusted root.

Update this public intermediate when the site's issuer changes. Failure must not disable TLS verification. Dynamic issuer discovery from the Python implementation remains a later parity task.
