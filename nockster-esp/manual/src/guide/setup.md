# Setup

## Before you start

Have these ready:

- Your Nockster.
- A USB data cable. Some charging cables do not carry data.
- A computer with Chrome, Edge, or Opera, or the Nockster desktop app.
- Paper or another offline place to write down recovery words.

If you already have a Nockchain wallet, decide whether you want to import its seed phrase or keep it separate and add Nockster as a new wallet.

## Connect in the browser

1. Plug Nockster into your computer.
2. Open [my.nockster.com](https://my.nockster.com).
3. Click **connect device**.
4. In the browser prompt, choose **Nockster**.
5. Wait for the device console to show the device status.

The browser path prefers WebHID and can fall back to Web Serial. Browser USB APIs require a secure site, which is why the hosted app uses HTTPS.

## Connect in the desktop app

1. Plug Nockster into your computer.
2. Open the Nockster desktop app.
3. Click **select port**.
4. Choose the Nockster serial port.
5. Click **connect device**.

Only one program can talk to the device at a time. Close other browser tabs, terminals, or wallet tools if the connection fails.

## If the device is new

A new or factory-reset device has no seed and no PIN. You can initialize it from the device screen or from the web app. The safest path is to generate the seed on the device and write it down offline.

Go to [First seed and PIN](./first-seed.md) next.
