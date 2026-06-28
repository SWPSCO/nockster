# Firmware updates

Nockster firmware updates are signed. The host downloads and streams update data, but the device verifies the signed manifest and firmware image before activating the update.

## Install the latest firmware

1. Plug in Nockster.
2. Open [my.nockster.com](https://my.nockster.com).
3. Click **firmware updates** before connecting, or connect and use **Firmware update** in the device console.
4. Click **connect & install latest** or **update firmware**.
5. Confirm the browser device prompt if needed.
6. Wait while the app fetches and installs the update.
7. Reboot when prompted.
8. Reconnect after the device appears again.

The app writes the update to an inactive OTA slot, verifies it, then reboots into the installed firmware.

## Status fields

The update panel can show:

- installed firmware and release number
- latest available release
- build profile and protocol version
- OTA boot status
- trust anchor

If the app says the device is up to date, no update is installed.

## Advanced update tools

The **advanced** section lets operators load a bundle JSON and firmware `.bin`, fetch a release from URLs, verify the manifest, verify the image, or install a chosen build. Most buyers should use the latest signed update button instead.
