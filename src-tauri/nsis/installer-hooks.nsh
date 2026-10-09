; Tauri's default NSIS template enables custom header images but leaves them
; aligned to the left. StrawberryDisk's compact brand mark is intentionally placed
; in the bitmap's right-side safe area, so force right alignment on installer
; and uninstaller inner pages. Keeping this as a small hook avoids maintaining
; a fork of Tauri's complete installer template.
!define MUI_HEADERIMAGE_RIGHT

