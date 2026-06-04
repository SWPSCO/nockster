# Portable, reproducible Nockster firmware build environment.
#
# Bundles the Espressif Rust (Xtensa `esp`) toolchain + espflash so the firmware
# builds identically on any machine and in CI. The signing key is NEVER baked in —
# it's mounted read-only at run time.
#
# Build the image:
#   docker build -t nockster-fw -f docker/firmware/Dockerfile .
#
# Build + sign a release (release/--release profile; key stays on the host):
#   mkdir -p out && chmod 777 out
#   docker run --rm \
#     -e RELEASE_VERSION=7 -e FW_PROFILE=production \
#     -e NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<pubkey-sha256> \
#     -v "$PWD/release-signing.key:/keys/signing.key:ro" \
#     -v "$PWD/out:/out" \
#     nockster-fw
#
# Derive the trust-anchor pubkey hash (no firmware build):
#   docker run --rm -v "$PWD/release-signing.key:/keys/signing.key:ro" \
#     --entrypoint bash nockster-fw -lc \
#     'source "$HOME/export-esp.sh"; cargo run -q -p nockster-cli --bin nockster-cli -- \
#        update pubkey --signing-key-file /keys/signing.key'
#
# Pin the base tag (e.g. esp32s3_1.88.0.0) once a build is green for reproducibility.
# Note: if a build ever fails on esp-rom-sys's `to_ascii_lowercase()` (a c_char/i8 vs u8
# strictness quirk in some rustc builds), pin this tag to an esp toolchain that compiles it
# (i.e. matching a known-good local setup) — do NOT vendor/patch esp-rom-sys into the repo.
FROM espressif/idf-rust:esp32s3_latest

# The repo Makefile intentionally runs recipes under /bin/zsh; keep the
# container entrypoint on bash, but provide zsh for make.
USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends zsh \
    && rm -rf /var/lib/apt/lists/*
USER esp

# espflash turns the ELF into a flashable/OTA image (make update-firmware-image).
# The idf-rust base usually ships espflash already, so only install if missing.
RUN command -v espflash >/dev/null 2>&1 || cargo install espflash --locked

WORKDIR /work
COPY --chown=esp:esp . /work

ENTRYPOINT ["/work/docker/firmware/build-entry.sh"]
