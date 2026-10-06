# Application icons

`public/strawberrydisk-source.png` preserves the supplied artwork. The cropped,
transparent `public/strawberrydisk.png` is the source for generated application
and tray icons. Regenerate them from the repository root:

```sh
node scripts/generate-resident-icons.mjs
```

The script produces Windows, macOS, and tray icon variants. The monochrome
tray template is a mask; the operating system chooses its foreground color.
The Windows ICO contains multiple native sizes, with the largest entry first
for the taskbar. Do not edit generated icon files by hand.

NSIS artwork is generated separately from the same image:

```powershell
powershell -File scripts/generate-installer-artwork.ps1
```
