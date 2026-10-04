# CLI reference

Most buyers should use the web app. The CLI is useful for scripting, diagnostics, and recovery workflows.

The CLI defaults to HID:

```sh
nockster-cli info
```

Use a serial port only when you need it:

```sh
nockster-cli info --port /dev/ttyACM0
```

## Common commands

```sh
nockster-cli list-ports
nockster-cli info
nockster-cli unlock --pin 1234
nockster-cli lock
nockster-cli reboot
```

Seed and wallet slot management:

```sh
nockster-cli seed --seedphrase "word word ..." --pin 1234
nockster-cli seed --keyfile keys.export --pin 1234
nockster-cli seed --list
nockster-cli seed --select 0
nockster-cli seed --delete 1
```

Signing:

```sh
nockster-cli sign-draft unsigned.draft --out signed.tx
nockster-cli show-address --slot 0 --path m
nockster-cli sign-message --slot 0 --message "hello"
```

Firmware and diagnostics:

```sh
nockster-cli health
nockster-cli smoke
nockster-cli touch --calibrate
nockster-cli update status
```

Advanced key tools:

```sh
nockster-cli export-master-pubkey --slot 0 --out master-pubkey.export
nockster-cli shamir split --zprv <zprv> --threshold 3 --shares 5
nockster-cli shamir combine --share <share1> --share <share2> --share <share3>
```

Factory reset clears seed and persistent PIN state:

```sh
nockster-cli reset
```

Do not run reset unless your recovery material is safely backed up.
