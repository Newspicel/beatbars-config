#!/usr/bin/env bash
# Builds "beat bars config.app" (Apple Silicon + Intel), signs it with a Developer ID,
# notarizes and staples it, and wraps it in a DMG. Output goes to dist/.
#
#   packaging/build-app.sh                 # sign + notarize
#   packaging/build-app.sh --no-notarize   # sign only (local testing)
#
# Environment:
#   SIGN_IDENTITY   codesigning identity (name or SHA-1). Default: the newest
#                   "Developer ID Application" certificate in the keychain.
#   NOTARY_PROFILE  notarytool keychain profile. Default: beatbars
#                   (create it once with `xcrun notarytool store-credentials beatbars
#                   --apple-id <id> --team-id <team>`).
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="beat bars config"
BUNDLE_ID="dev.newspicel.beatbars-config"
EXE="beatbars-config"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
NOTARY_PROFILE="${NOTARY_PROFILE:-beatbars}"
NOTARIZE=1
[[ "${1:-}" == "--no-notarize" ]] && NOTARIZE=0

BUILD="target/bundle"
APP="$BUILD/$APP_NAME.app"
DIST="dist"
DMG="$DIST/beat-bars-config-$VERSION.dmg"

step() { printf '\n==> %s\n' "$*"; }

newest_identity() {
    security find-identity -v -p codesigning | awk '/Developer ID Application/ {print $2}' | sort -u |
        while read -r hash; do
            local end
            end="$(security find-certificate -a -Z -p |
                awk -v h="$hash" 'BEGIN { RS = "SHA-1 hash: " } index($0, h) == 1' |
                sed -n '/BEGIN CERT/,/END CERT/p' | openssl x509 -noout -enddate | cut -d= -f2)"
            echo "$(date -j -f '%b %d %T %Y %Z' "$end" +%s) $hash"
        done | sort -rn | head -1 | cut -d' ' -f2
}

SIGN_IDENTITY="${SIGN_IDENTITY:-$(newest_identity)}"
if [[ -z "$SIGN_IDENTITY" ]]; then
    echo "No Developer ID Application certificate found; set SIGN_IDENTITY." >&2
    exit 1
fi

step "Building $APP_NAME $VERSION (arm64 + x86_64)"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

step "Assembling bundle"
rm -rf "$BUILD" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$BUILD/icon.iconset"
lipo -create -output "$APP/Contents/MacOS/$EXE" \
    "target/aarch64-apple-darwin/release/$EXE" "target/x86_64-apple-darwin/release/$EXE"

swift packaging/icon.swift "$BUILD/icon_1024.png" >/dev/null
for size in 16 32 128 256 512; do
    sips -z $size $size "$BUILD/icon_1024.png" --out "$BUILD/icon.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$BUILD/icon_1024.png" --out "$BUILD/icon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$BUILD/icon.iconset" -o "$APP/Contents/Resources/AppIcon.icns"

sed -e "s/@VERSION@/$VERSION/g" -e "s/@BUNDLE_ID@/$BUNDLE_ID/g" -e "s/@EXE@/$EXE/g" \
    -e "s/@APP_NAME@/$APP_NAME/g" packaging/Info.plist >"$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

step "Signing with $SIGN_IDENTITY"
codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
codesign --verify --strict --verbose=2 "$APP"

notarize() {
    xcrun notarytool submit "$1" --keychain-profile "$NOTARY_PROFILE" --wait
}

if ((NOTARIZE)); then
    step "Notarizing the app"
    ditto -c -k --keepParent "$APP" "$BUILD/app.zip"
    notarize "$BUILD/app.zip"
    xcrun stapler staple "$APP"
fi

step "Creating $DMG"
mkdir -p "$DIST" "$BUILD/dmg"
cp -R "$APP" "$BUILD/dmg/"
ln -s /Applications "$BUILD/dmg/Applications"
rm -f "$DMG"
hdiutil create -volname "$APP_NAME" -srcfolder "$BUILD/dmg" -ov -format UDZO "$DMG" >/dev/null
codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"

if ((NOTARIZE)); then
    step "Notarizing the DMG"
    notarize "$DMG"
    xcrun stapler staple "$DMG"
    step "Gatekeeper check"
    spctl --assess --type execute --verbose=2 "$APP"
    spctl --assess --type open --context context:primary-signature --verbose=2 "$DMG"
fi

rm -rf "$DIST/$APP_NAME.app"
cp -R "$APP" "$DIST/"
step "Done: $DMG and $DIST/$APP_NAME.app"
