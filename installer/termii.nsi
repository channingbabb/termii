; ============================================================
;  Termii – NSIS Installer Script
;
;  Behaviours:
;    • Fresh install  - proceeds normally
;    • New version    - confirms update, then installs over existing
;    • Same version   - asks user whether to reinstall
;    • Older version  - refuses (downgrade blocked)
; ============================================================

!include "MUI2.nsh"
!include "WordFunc.nsh"
!include "LogicLib.nsh"

; ---- Build-time defines (overridden via /DAPP_VERSION=x.y.z on CLI) ----
!ifndef APP_VERSION
  !define APP_VERSION "0.1.0"
!endif

!define APP_NAME          "Termii"
!define APP_PUBLISHER     "Channing Babb"
!define APP_EXE           "termii.exe"
!define APP_REG_KEY       "Software\Termii"
!define APP_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Termii"
!define INSTALL_DIR       "$PROGRAMFILES64\Termii"

; ---- Installer attributes ----
Name              "${APP_NAME} ${APP_VERSION}"
OutFile           "termii-setup-${APP_VERSION}.exe"
InstallDir        "${INSTALL_DIR}"
InstallDirRegKey  HKLM "${APP_REG_KEY}" "InstallDir"
RequestExecutionLevel admin
SetCompressor     /SOLID lzma

; ---- MUI pages ----
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN      "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ${APP_NAME}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; ============================================================
;  Version check on startup
;
;  $0 = version string currently in registry (empty = not installed)
;  $1 = VersionCompare result:
;         0  → installer version == installed version  (same)
;         1  → installer version >  installed version  (update)
;         2  → installer version <  installed version  (downgrade)
; ============================================================
Function .onInit
  ReadRegStr $0 HKLM "${APP_REG_KEY}" "Version"

  ${If} $0 == ""
    ; Not installed — proceed with fresh install
    Return
  ${EndIf}

  ${VersionCompare} "${APP_VERSION}" "$0" $1

  ${If} $1 == 2
    ; Installer is OLDER than what is already installed — block downgrade
    MessageBox MB_ICONINFORMATION|MB_OK \
      "${APP_NAME} $0 is already installed.$\n\
       This installer carries version ${APP_VERSION}, which is older.$\n\
       Download a newer installer to upgrade."
    Abort

  ${ElseIf} $1 == 0
    ; Same version already present — offer reinstall
    MessageBox MB_ICONQUESTION|MB_YESNO \
      "${APP_NAME} ${APP_VERSION} is already installed.$\n\
       Do you want to reinstall it?" \
      IDYES +2
    Abort

  ${Else}
    ; Installer is NEWER — confirm the update
    MessageBox MB_ICONQUESTION|MB_YESNO \
      "${APP_NAME} $0 is currently installed.$\n\
       Do you want to update it to version ${APP_VERSION}?" \
      IDYES +2
    Abort
  ${EndIf}
FunctionEnd

; ============================================================
;  Main Section – copy files, create shortcuts, write registry
; ============================================================
Section "Termii (required)" SecMain
  SectionIn RO

  SetOutPath "$INSTDIR"

  ; Terminate any running instance so the binary can be replaced
  ExecWait 'taskkill /F /IM "${APP_EXE}"' $0

  ; Install the application binary (staged under installer\dist\ by the workflow)
  File "dist\${APP_EXE}"

  ; ---- Registry: application metadata ----
  WriteRegStr HKLM "${APP_REG_KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${APP_REG_KEY}" "Version"    "${APP_VERSION}"

  ; ---- Registry: Add / Remove Programs entry ----
  WriteRegStr   HKLM "${APP_UNINSTALL_KEY}" "DisplayName"          "${APP_NAME}"
  WriteRegStr   HKLM "${APP_UNINSTALL_KEY}" "DisplayVersion"       "${APP_VERSION}"
  WriteRegStr   HKLM "${APP_UNINSTALL_KEY}" "Publisher"            "${APP_PUBLISHER}"
  WriteRegStr   HKLM "${APP_UNINSTALL_KEY}" "InstallLocation"      "$INSTDIR"
  WriteRegStr   HKLM "${APP_UNINSTALL_KEY}" "UninstallString"      '"$INSTDIR\uninstall.exe"'
  WriteRegStr   HKLM "${APP_UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKLM "${APP_UNINSTALL_KEY}" "NoModify"             1
  WriteRegDWORD HKLM "${APP_UNINSTALL_KEY}" "NoRepair"             1

  ; Write the uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; ---- Shortcuts ----
  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut  "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"           "$INSTDIR\${APP_EXE}"
  CreateShortcut  "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk" "$INSTDIR\uninstall.exe"
  CreateShortcut  "$DESKTOP\${APP_NAME}.lnk"                          "$INSTDIR\${APP_EXE}"
SectionEnd

; ============================================================
;  Uninstall Section
; ============================================================
Section "Uninstall"
  ; Terminate running instance
  ExecWait 'taskkill /F /IM "${APP_EXE}"' $0

  ; Remove application files
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir  "$INSTDIR"

  ; Remove shortcuts
  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\Uninstall ${APP_NAME}.lnk"
  RMDir  "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  ; Remove registry entries
  DeleteRegKey HKLM "${APP_REG_KEY}"
  DeleteRegKey HKLM "${APP_UNINSTALL_KEY}"
SectionEnd
