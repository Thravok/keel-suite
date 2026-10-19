; NSIS installer for Keel Deck
!include "MUI2.nsh"

!ifndef PRODUCT_VERSION
  !define PRODUCT_VERSION "0.1.0"
!endif
!ifndef SOURCE_DIR
  !define SOURCE_DIR "."
!endif

Name "Keel Deck"
OutFile "KeelDeck-${PRODUCT_VERSION}-windows-x64.exe"
InstallDir "$PROGRAMFILES64\Keel\Deck"
RequestExecutionLevel admin
Unicode true

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "${SOURCE_DIR}\keel.exe"
  CreateDirectory "$SMPROGRAMS\Keel"
  CreateShortCut "$SMPROGRAMS\Keel\Keel Deck.lnk" "$INSTDIR\keel.exe"
  CreateShortCut "$DESKTOP\Keel Deck.lnk" "$INSTDIR\keel.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelDeck" "DisplayName" "Keel Deck"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelDeck" "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelDeck" "DisplayVersion" "${PRODUCT_VERSION}"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\keel.exe"
  Delete "$INSTDIR\Uninstall.exe"
  Delete "$SMPROGRAMS\Keel\Keel Deck.lnk"
  Delete "$DESKTOP\Keel Deck.lnk"
  RMDir "$SMPROGRAMS\Keel"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\KeelDeck"
SectionEnd
