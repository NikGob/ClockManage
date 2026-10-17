; ClockManage NSIS hooks (bundle.windows.nsis.installerHooks).

; Install / update over an existing version: close the running app (it lives in the tray and
; refuses to quit during study time) and its launch guard, so files can be replaced without
; the "app is running" prompt. The new version starts from the finish page / at next logon.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /T /IM clockmanage.exe'
  nsExec::Exec 'taskkill /F /IM cm-guard.exe'
  Sleep 800
!macroend

; An update from the app itself (/UPDATE): start it again, in the tray. Started from here it
; keeps the installer's admin rights, so no UAC prompt pops up while nobody is at the PC.
!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode = 1
    Exec '"$INSTDIR\clockmanage.exe" --background'
  ${EndIf}
!macroend

; Uninstall: stop the app, then undo everything it put into the system (hosts section,
; browser policies, launch-guard redirects, scheduled tasks) so nothing stays blocked.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /T /IM clockmanage.exe'
  nsExec::Exec 'taskkill /F /IM cm-guard.exe'
  Sleep 800
  nsExec::Exec '"$INSTDIR\clockmanage.exe" --cleanup'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
