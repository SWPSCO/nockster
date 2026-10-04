#!/usr/bin/env bash
set -euo pipefail
: "${ANDROID_KEYSTORE_B64:?Set ANDROID_KEYSTORE_B64}"
: "${ANDROID_KEYSTORE_PASSWORD:?Set ANDROID_KEYSTORE_PASSWORD}"
: "${ANDROID_KEY_ALIAS:?Set ANDROID_KEY_ALIAS}"
: "${ANDROID_KEY_PASSWORD:?Set ANDROID_KEY_PASSWORD}"
: "${NOCKSTER_VERSION:?Set NOCKSTER_VERSION}"
: "${NOCKSTER_BUILD_NUMBER:?Set NOCKSTER_BUILD_NUMBER}"
signing_dir=$(mktemp -d "${RUNNER_TEMP:-/tmp}/nockster-android-signing.XXXXXX")
trap 'rm -rf "$signing_dir"' EXIT
export ANDROID_KEYSTORE_PATH="$signing_dir/release.jks"
umask 077
printf '%s' "$ANDROID_KEYSTORE_B64" | base64 --decode > "$ANDROID_KEYSTORE_PATH"
unset ANDROID_KEYSTORE_B64
(
  cd apps/mobile/android
  ./gradlew --no-daemon :app:testReleaseUnitTest :app:lintRelease :app:assembleRelease :app:bundleRelease
)
python3 scripts/ci/android-verify-bundle.py \
  apps/mobile/android/app/build/outputs/bundle/release/app-release.aab \
  apps/mobile/android/app/build/outputs/mapping/release/mapping.txt \
  | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"
"$ANDROID_HOME/build-tools/36.0.0/apksigner" verify --verbose --print-certs apps/mobile/android/app/build/outputs/apk/release/app-release.apk
# jarsigner reports unsigned archives with exit status zero; also require a signer certificate.
jarsigner -verify apps/mobile/android/app/build/outputs/bundle/release/app-release.aab
keytool -printcert -jarfile apps/mobile/android/app/build/outputs/bundle/release/app-release.aab > "$signing_dir/bundle-certificate.txt"
grep -q 'SHA256:' "$signing_dir/bundle-certificate.txt"
mkdir -p release/android
cp apps/mobile/android/app/build/outputs/apk/release/app-release.apk "release/android/nockster-android-$NOCKSTER_VERSION-$NOCKSTER_BUILD_NUMBER.apk"
cp apps/mobile/android/app/build/outputs/bundle/release/app-release.aab "release/android/nockster-android-$NOCKSTER_VERSION-$NOCKSTER_BUILD_NUMBER.aab"
cp apps/mobile/android/app/build/outputs/mapping/release/mapping.txt "release/android/nockster-android-$NOCKSTER_VERSION-$NOCKSTER_BUILD_NUMBER-mapping.txt"
(cd release/android && sha256sum ./*.apk ./*.aab ./*-mapping.txt > android-SHA256SUMS)
