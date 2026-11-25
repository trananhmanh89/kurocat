; KuroCat installer (NSIS 3).
;
; Build after `cargo build --release`, from the repository root:
;   makensis /DVERSION=5.0.0 installer\kurocat.nsi
; Optional defines:
;   /DEXE_PATH=...    the exe to package (default: target\release\KuroCat.exe)
;   /DOUTPUT=...      the setup file to write (default: target\release\KuroCat-<VERSION>-x64-setup.exe)
;
; Installs for the current user only (no admin rights) into %LOCALAPPDATA%\KuroCat, using the same
; folder and uninstall registry key as the old Tauri installer, so it upgrades v4.x in place.
;
; Command-line flags:
;   /S        silent install (used by the in-app updater of older v5 releases)
;   /P        passive install: no questions, progress only (used by the v4.x updater)
;   /R        start KuroCat after installing (used by the v4.x updater)
;   /UPDATE   accepted for v4.x compatibility; no effect

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma

!ifndef VERSION
  !error "Pass the version: makensis /DVERSION=x.y.z installer\kurocat.nsi"
!endif
!ifndef EXE_PATH
  !define EXE_PATH "..\target\release\KuroCat.exe"
!endif
!ifndef OUTPUT
  !define OUTPUT "..\target\release\KuroCat-${VERSION}-x64-setup.exe"
!endif

!define PRODUCT_NAME "KuroCat"
!define PUBLISHER "KuroCat"
!define EXE_NAME "KuroCat.exe"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\${PRODUCT_NAME}"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel user
BrandingText "${PRODUCT_NAME} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "MIT License"

!define MUI_ICON "..\assets\icons\icon.ico"
!define MUI_UNICON "..\assets\icons\icon.ico"
!define MUI_WELCOMEFINISHPAGE_BITMAP "..\assets\icons\nsis-sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "..\assets\icons\nsis-sidebar.bmp"
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "..\assets\icons\nsis-header.bmp"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE_NAME}"
!define MUI_FINISHPAGE_RUN_TEXT "Start KuroCat"

; Pages are skipped in passive mode (see SkipInPassiveMode).
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipInPassiveMode
!insertmacro MUI_PAGE_WELCOME
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipInPassiveMode
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipInPassiveMode
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Var PassiveMode
Var RelaunchAfterInstall

; Reads the /P and /R flags.
Function .onInit
  StrCpy $PassiveMode 0
  StrCpy $RelaunchAfterInstall 0
  ${GetParameters} $0
  ClearErrors
  ${GetOptions} $0 "/P" $1
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}
  ClearErrors
  ${GetOptions} $0 "/R" $1
  ${IfNot} ${Errors}
    StrCpy $RelaunchAfterInstall 1
  ${EndIf}
FunctionEnd

; Skips a wizard page when the installer runs in passive mode.
Function SkipInPassiveMode
  ${If} $PassiveMode == 1
    Abort
  ${EndIf}
FunctionEnd

; Closes the running pet, but not the `KuroCat.exe --mcp` servers that AI clients started.
!macro ClosePet
  nsExec::Exec `powershell -NoProfile -NonInteractive -Command "Get-CimInstance Win32_Process -Filter 'Name=''${EXE_NAME}''' | Where-Object { $$_.CommandLine -notlike '*--mcp*' } | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force }"`
  Pop $0
  Sleep 400
!macroend

; Closes every running KuroCat, including MCP servers.
!macro CloseAllInstances
  nsExec::Exec 'taskkill /IM "${EXE_NAME}" /F'
  Pop $0
  Sleep 400
!macroend

Section "KuroCat" SectionMain
  SectionIn RO
  !insertmacro ClosePet

  SetOutPath "$INSTDIR"
  ; MCP servers may still run the old exe, which locks it. A running exe can be renamed, so move
  ; it aside; the leftover copy is removed by the next install. If even that fails, close them.
  Delete "$INSTDIR\KuroCat.old.exe"
  ${If} ${FileExists} "$INSTDIR\${EXE_NAME}"
    ClearErrors
    Rename "$INSTDIR\${EXE_NAME}" "$INSTDIR\KuroCat.old.exe"
    ${If} ${Errors}
      !insertmacro CloseAllInstances
    ${EndIf}
  ${EndIf}
  File "/oname=${EXE_NAME}" "${EXE_PATH}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXE_NAME}"

  ; Remove the autostart value used up to v4.2.
  DeleteRegValue HKCU "${RUN_KEY}" "KuroCatPet"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$\"$INSTDIR\${EXE_NAME}$\""
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$\"$INSTDIR$\""
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" "$0"

  ${If} $RelaunchAfterInstall == 1
    Exec '"$INSTDIR\${EXE_NAME}"'
  ${EndIf}
SectionEnd

; Removes the program, its shortcuts and registry entries. The user's settings and cats in
; %APPDATA%\kuro-cat-pet are kept.
Section "Uninstall"
  !insertmacro CloseAllInstances

  Delete "$INSTDIR\${EXE_NAME}"
  Delete "$INSTDIR\KuroCat.old.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${PRODUCT_NAME}.lnk"
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"

  DeleteRegValue HKCU "${RUN_KEY}" "KuroCat"
  DeleteRegValue HKCU "${RUN_KEY}" "KuroCatPet"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
