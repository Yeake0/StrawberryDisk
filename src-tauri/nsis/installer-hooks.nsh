; Tauri's default NSIS template enables custom header images but leaves them
; aligned to the left. StrawberryDisk's compact brand mark is intentionally placed
; in the bitmap's right-side safe area, so force right alignment on installer
; and uninstaller inner pages. Keeping this as a small hook avoids maintaining
; a fork of Tauri's complete installer template.
!define MUI_HEADERIMAGE_RIGHT

; Tauri stores the selected NSIS installation directory below a registry key
; derived from `bundle.publisher`. The original 1.0.0 and 1.0.1 used `harry0703`,
; while 1.0.2 and 1.0.3 used the GitHub profile URL. An automatic update built
; with the stable `StrawberryDisk` publisher cannot discover either custom install
; directory through the new publisher key, so it would fall back to
; `%LOCALAPPDATA%\StrawberryDisk` and leave the existing shortcut on the old binary.
;
; Limit the compatibility lookup to updater-driven installs. Interactive
; installers must continue to respect the directory explicitly chosen by the
; user. The URL publisher is checked first because it belongs to the newer
; releases; the original publisher is only a fallback for direct upgrades from
; 1.0.0 or 1.0.1. Requiring the expected executable prevents a stale registry
; value from redirecting installation into a missing or unrelated directory.
!macro NSIS_HOOK_PREINSTALL
  ${If} $UpdateMode = 1
    ReadRegStr $R8 SHCTX "Software\MangoDisk\MangoDisk" ""

    ${If} $R8 == ""
      ReadRegStr $R8 SHCTX "Software\https://github.com/harry0703\MangoDisk" ""
    ${EndIf}

    ${If} $R8 == ""
      ReadRegStr $R8 SHCTX "Software\harry0703\MangoDisk" ""
    ${EndIf}

    ${If} $R8 != ""
    ${AndIf} ${FileExists} "$R8\MangoDisk.exe"
      StrCpy $INSTDIR $R8

      ; Tauri selects the output directory before expanding this hook. Reset it
      ; after changing `$INSTDIR` so every bundled file is copied to the restored
      ; location rather than the publisher's new default directory.
      SetOutPath $INSTDIR
      DetailPrint "Restored the existing installation directory for StrawberryDisk migration"
    ${EndIf}
  ${EndIf}
!macroend

; The updater has stopped the original executable before this point. Once the
; new binary and shortcuts exist, remove obsolete launch paths so users do not
; accidentally start the old version after the rebrand.
!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode = 1
  ${AndIf} ${FileExists} "$INSTDIR\StrawberryDisk.exe"
  ${AndIf} ${FileExists} "$INSTDIR\MangoDisk.exe"
    Delete "$INSTDIR\MangoDisk.exe"
    Delete "$SMPROGRAMS\MangoDisk.lnk"
    Delete "$DESKTOP\MangoDisk.lnk"
    DeleteRegKey SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\MangoDisk"
    DetailPrint "Removed obsolete launch paths after StrawberryDisk upgrade"
  ${EndIf}
!macroend
