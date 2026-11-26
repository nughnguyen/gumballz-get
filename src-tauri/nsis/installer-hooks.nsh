!include "LogicLib.nsh"

!macro NSIS_HOOK_PREINSTALL
  ; Terminate running GumballZ Get processes so uninstaller can remove files without locks
  ; (giu ca ten cu OmniGet de don sach ban 1.0.0 neu may van con).
  DetailPrint "Stopping running instances of GumballZ Get..."
  nsExec::Exec 'taskkill /F /IM "gumballz-get.exe"'
  nsExec::Exec 'taskkill /F /IM "GumballZ Get.exe"'
  nsExec::Exec 'taskkill /F /IM "omniget.exe"'
  nsExec::Exec 'taskkill /F /IM "OmniGet.exe"'
  Sleep 1000

  ; 1. Check registry UninstallString (catches custom install locations)
  StrCpy $R0 ""
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\GumballZ Get" "UninstallString"
  ${If} $R0 == ""
    ReadRegStr $R0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\GumballZ Get" "UninstallString"
  ${EndIf}
  ${If} $R0 == ""
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.gumballz.get" "UninstallString"
  ${EndIf}
  ${If} $R0 == ""
    ReadRegStr $R0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.gumballz.get" "UninstallString"
  ${EndIf}
  ${If} $R0 == ""
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OmniGet" "UninstallString"
  ${EndIf}
  ${If} $R0 == ""
    ReadRegStr $R0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\OmniGet" "UninstallString"
  ${EndIf}
  ${If} $R0 == ""
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.nughnguyen.omniget" "UninstallString"
  ${EndIf}
  ${If} $R0 == ""
    ReadRegStr $R0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.nughnguyen.omniget" "UninstallString"
  ${EndIf}

  ${If} $R0 != ""
    DetailPrint "Executing GumballZ Get uninstaller from registry..."
    ExecWait '$R0 /S'
    Sleep 1000
  ${EndIf}

  ; 2. Check standard Tauri per-user install directory ($LOCALAPPDATA\GumballZ Get)
  ${If} ${FileExists} "$LOCALAPPDATA\GumballZ Get\uninstall.exe"
    DetailPrint "Uninstalling GumballZ Get ($LOCALAPPDATA\GumballZ Get)..."
    ExecWait '"$LOCALAPPDATA\GumballZ Get\uninstall.exe" /S _?=$LOCALAPPDATA\GumballZ Get'
    Sleep 1000
  ${EndIf}

  ; 3. Check Programs per-user install directory ($LOCALAPPDATA\Programs\GumballZ Get)
  ${If} ${FileExists} "$LOCALAPPDATA\Programs\GumballZ Get\uninstall.exe"
    DetailPrint "Uninstalling GumballZ Get ($LOCALAPPDATA\Programs\GumballZ Get)..."
    ExecWait '"$LOCALAPPDATA\Programs\GumballZ Get\uninstall.exe" /S _?=$LOCALAPPDATA\Programs\GumballZ Get'
    Sleep 1000
  ${EndIf}

  ; 4. Check machine-wide install directory ($PROGRAMFILES\GumballZ Get)
  ${If} ${FileExists} "$PROGRAMFILES\GumballZ Get\uninstall.exe"
    DetailPrint "Uninstalling GumballZ Get ($PROGRAMFILES\GumballZ Get)..."
    ExecWait '"$PROGRAMFILES\GumballZ Get\uninstall.exe" /S _?=$PROGRAMFILES\GumballZ Get'
    Sleep 1000
  ${EndIf}

  ; 5. Check 64-bit machine-wide install directory ($PROGRAMFILES64\GumballZ Get)
  ${If} ${FileExists} "$PROGRAMFILES64\GumballZ Get\uninstall.exe"
    DetailPrint "Uninstalling GumballZ Get ($PROGRAMFILES64\GumballZ Get)..."
    ExecWait '"$PROGRAMFILES64\GumballZ Get\uninstall.exe" /S _?=$PROGRAMFILES64\GumballZ Get'
    Sleep 1000
  ${EndIf}

  ; 6. Hard-clean leftover directories in case uninstaller left any lingering files
  DetailPrint "Cleaning any leftover GumballZ Get artifacts..."
  RMDir /r "$LOCALAPPDATA\GumballZ Get"
  RMDir /r "$LOCALAPPDATA\Programs\GumballZ Get"
  RMDir /r "$PROGRAMFILES\GumballZ Get"
  RMDir /r "$PROGRAMFILES64\GumballZ Get"
  RMDir /r "$LOCALAPPDATA\OmniGet"
  RMDir /r "$LOCALAPPDATA\Programs\OmniGet"
  RMDir /r "$PROGRAMFILES\OmniGet"
  RMDir /r "$PROGRAMFILES64\OmniGet"

  ; 7. Remove shortcuts (current user and all users)
  SetShellVarContext current
  Delete "$DESKTOP\GumballZ Get.lnk"
  Delete "$SMPROGRAMS\GumballZ Get.lnk"
  Delete "$SMPROGRAMS\GumballZ Get\*.lnk"
  RMDir /r "$SMPROGRAMS\GumballZ Get"
  Delete "$DESKTOP\OmniGet.lnk"
  Delete "$SMPROGRAMS\OmniGet.lnk"
  Delete "$SMPROGRAMS\OmniGet\*.lnk"
  RMDir /r "$SMPROGRAMS\OmniGet"

  SetShellVarContext all
  Delete "$DESKTOP\GumballZ Get.lnk"
  Delete "$SMPROGRAMS\GumballZ Get.lnk"
  Delete "$SMPROGRAMS\GumballZ Get\*.lnk"
  RMDir /r "$SMPROGRAMS\GumballZ Get"
  Delete "$DESKTOP\OmniGet.lnk"
  Delete "$SMPROGRAMS\OmniGet.lnk"
  Delete "$SMPROGRAMS\OmniGet\*.lnk"
  RMDir /r "$SMPROGRAMS\OmniGet"
  SetShellVarContext current

  ; 8. Clean obsolete registry uninstall keys
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\GumballZ Get"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\GumballZ Get"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.gumballz.get"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.gumballz.get"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OmniGet"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\OmniGet"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.nughnguyen.omniget"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\com.nughnguyen.omniget"
!macroend
