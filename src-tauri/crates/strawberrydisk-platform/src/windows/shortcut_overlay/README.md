# Shortcut overlay resource

The machine-wide `Shell Icons\29` override points to a durable, versioned ICO in
Windows' directory. Keep `IsShortcut` and third-party overrides untouched.

`transparent-v2.ico` is an original 256px PNG-based ICO. Its black pixels have
alpha 1/255 everywhere: Explorer can render all-zero-alpha overlays as opaque
black squares after rebuilding its image lists. A constant nonzero alpha also
survives resizing; the overlay darkens an 8-bit background channel by at most one
level. Regenerate it from the repository root with:

```sh
node scripts/generate-shortcut-overlay.mjs
```

The v1 registry value remains recognized as enabled and restorable, even if its
file is missing. Existing users disable and re-enable the setting to install v2
through the normal elevation and recovery workflow. Merely scanning settings must
not mutate the registry. Keep old files for exact recovery snapshots; never
replace a foreign or damaged file under elevation.

Run `cargo test -p strawberrydisk-platform shortcut` on macOS and Windows. Windows tests
also load and blend the resource at 16–256px using native APIs. These do not replace
Explorer interaction checks: test default arrows, v1, and v2 under the same DPI,
then verify icon sizes, Explorer/cache reloads, and system restart.
