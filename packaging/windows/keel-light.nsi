; NSIS installer for Keel Light (engine + Qt UI)
!include "MUI2.nsh"

!ifndef PRODUCT_VERSION
  !define PRODUCT_VERSION "0.1.0"
!endif
!ifndef SOURCE_DIR
  !define SOURCE_DIR "."
!endif

Name "Keel Light"
OutFile "KeelLight-${PRODUCT_VERSION}-windows-x64.exe"
InstallDir "$PROGRAMFILES64\Keel\Light"
RequestExecutionLevel admin
Unicode true

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File /r "${SOURCE_DIR}\*.*"
  CreateDirectory "$SMPROGRAMS\Keel"
  CreateShortCut "$SMPROGRAMS\Keel\Keel Light.lnk" "$INSTDIR\keel-light.exe"
  CreateShortCut "$DESKTOP\Keel Light.lnk" "$INSTDIR\keel-light.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelLight" "DisplayName" "Keel Light"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelLight" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelLight" "DisplayVersion" "${PRODUCT_VERSION}"
SectionEnd

Section "Uninstall"
  RMDir /r "$INSTDIR"
  Delete "$SMPROGRAMS\Keel\Keel Light.lnk"
  Delete "$DESKTOP\Keel Light.lnk"
  RMDir "$SMPROGRAMS\Keel"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelLight"
SectionEnd
