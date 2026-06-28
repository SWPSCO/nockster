# Troubleshooting

## Browser says USB is unsupported

Use Chrome, Edge, or Opera. Browser USB access does not work in every browser, and it requires HTTPS or localhost.

## Nockster does not appear in the browser prompt

- Confirm the cable supports data.
- Unplug and replug the device.
- Close other browser tabs or terminal sessions using the device.
- Try a different USB port.
- On Linux, use a normal Chrome or Edge package rather than Snap or Flatpak if HID open fails.

## Connection fails after choosing the device

Only one connection can be open at a time. Disconnect from the desktop app, close CLI commands, and close other tabs using Nockster. Then unplug and reconnect.

## Unlock fails

Check the PIN and try again. The status panel shows remaining PIN attempts when the firmware reports them. If the device is locked out, stop guessing and use your recovery material on a reset or replacement device.

## The seed form will not submit

For seed phrases, use 12, 15, 18, 21, or 24 BIP39 words. For an initial device setup, you must set a PIN. A `zprv` extended key can be imported only after the device already has an initial seed and PIN.

## Signing fails

- Make sure the device is connected and unlocked.
- Select the correct wallet slot.
- Confirm the draft is a supported V1 transaction draft.
- Watch the device screen for approval or rejection prompts.

## Firmware update fails

- Reconnect and try again.
- Check that the update panel says secure update is available.
- Do not unplug during install.
- If the install finished but reboot failed, press reset or unplug and reconnect the device.

## Touch input is off

Use the device **Settings** menu and choose **Calibrate**. Touch each target once and wait for calibration to save.
