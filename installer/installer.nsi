; Commission Showcase 2 (Rust) — per-user installer.
; Build: makensis /DEXE=<path to release exe> /DICON=<icon.ico> /DOUT=<setup.exe> installer.nsi
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!define APP "Commission Showcase"
!define VERSION "2.0.0"
!define KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\CommissionShowcase"
; Uninstall key written by the earlier Electron build (electron-builder GUID)
!define OLD_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\88b750e6-87a2-59a9-8e89-57486aa436fb"

Name "${APP}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\${APP}"
InstallDirRegKey HKCU "${KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\commission-showcase.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Buka ${APP} sekarang"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Indonesian"

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=${LANG_INDONESIAN} "ProductName" "${APP}"
VIAddVersionKey /LANG=${LANG_INDONESIAN} "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_INDONESIAN} "FileDescription" "${APP} Setup"

Section "Install"
  ; Replace the older Electron version if it is installed (library & presets in
  ; %APPDATA%\CommissionShowcase are kept — the new version reads the same files).
  StrCpy $1 "$LOCALAPPDATA\Programs\${APP}"
  ${If} ${FileExists} "$1\Uninstall ${APP}.exe"
    DetailPrint "Menghapus versi lama (Electron)..."
    ; _?= runs the old uninstaller in place and waits for it to finish
    ExecWait '"$1\Uninstall ${APP}.exe" /currentuser /S _?=$1'
    Delete "$1\Uninstall ${APP}.exe"
    RMDir /r "$1\resources"
    RMDir /r "$1\locales"
    Delete "$1\*.*"
    DeleteRegKey HKCU "${OLD_KEY}"
  ${EndIf}

  SetOutPath "$INSTDIR"
  File /oname=commission-showcase.exe "${EXE}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\commission-showcase.exe"
  CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\commission-showcase.exe"

  WriteRegStr HKCU "${KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${KEY}" "Publisher" "${APP}"
  WriteRegStr HKCU "${KEY}" "DisplayIcon" "$INSTDIR\commission-showcase.exe"
  WriteRegStr HKCU "${KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\commission-showcase.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$DESKTOP\${APP}.lnk"
  Delete "$SMPROGRAMS\${APP}.lnk"
  DeleteRegKey HKCU "${KEY}"
SectionEnd
