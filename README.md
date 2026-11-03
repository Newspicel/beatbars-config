# beatbars-config

A small replacement for the original **beat bars config** app (`com.beatbars.adapters`, v1.4.2,
JavaFX), which crashes on current macOS. It configures beat bars pedals, foot switches and
adapters over USB MIDI (SysEx). The UI is built with [zgui](https://github.com/zortax/zgui).

```console
cargo run --release            # open the window
cargo run --release -- dump    # print the device's configuration
cargo run --release -- monitor # print what the buttons send while you press them
cargo run --release -- help    # diagnostics: ports, registers, raw SysEx
```

`BEATBARS_DEBUG=1` traces every SysEx frame on stderr.

## macOS app

```console
packaging/build-app.sh                # universal .app + DMG in dist/, signed and notarized
packaging/build-app.sh --no-notarize  # signed only
```

The script signs with the newest *Developer ID Application* certificate in your keychain
(override with `SIGN_IDENTITY`) and notarizes with the `beatbars-notary` keychain profile
(override with `NOTARY_PROFILE`), which you create once:

```console
xcrun notarytool store-credentials beatbars-notary --apple-id <apple id> --team-id <team id>
```

The notarized DMG opens on any Mac running macOS 11 or later, Apple Silicon or Intel, without
Gatekeeper warnings. The icon is drawn by [`packaging/icon.swift`](packaging/icon.swift).

The device is picked up automatically when it is plugged in. Edit the settings, then press
**Write to device**. The app reads the configuration back and confirms it matches.

## Protocol

Reverse engineered from the original app (decompiled jars 1.1.0 to 1.4.2) and checked against real
hardware ("beat bars controller", USB `04d8:ee40`). The full description is in
[`src/protocol.rs`](src/protocol.rs). In short:

```text
request   F0 00 21 3A <register> <count> <access> <bytes…> F7   access: 1 read, 2 write, 3 execute
response  F0 00 21 3A <register> <bytes…> F7                     (reads only; writes are silent)
```

The device exposes flat, memory-mapped registers. A read of `count` bytes at a register returns
the bytes from there to the end of that block. MIDI status bytes are stored minus 127 so they fit
in 7 bits.

| reg | contents |
|----:|----------|
| 1 | info: device type, firmware major, minor (some units only store a single `0`) |
| 8–16 | pedal: mode, min, max, message (CIN, status, param, range factor, length), reversed |
| 20 | pedal calibration (execute; the device answers when done) |
| 28–39 | button 1: mode, on/off values, message 1, step, button type, button mode |
| 41–45 | button 1: message 2 |
| 48–65 | button 2, same layout as button 1 (+20) |
| 70 | buttons linked |
| 127 | factory reset (execute) |

Differences from the original app:

- A device that does not report its type (the original then disabled everything) shows all of its
  controls. The pedal tab only appears if the pedal registers hold a real configuration.
- An unconfigured pedal is never written unless you edit it.
- Reads wait for each answer instead of firing on a fixed 160 ms timer. Writes keep the original
  pacing.
