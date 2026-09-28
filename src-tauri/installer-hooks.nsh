; ClockManage NSIS hooks (bundle.windows.nsis.installerHooks).

; Install / update over an existing version: close the running app (it lives in the tray and
; refuses to quit during study time) and its launch guard, so files can be replaced without
; the "app is running" prompt. The new version starts from the finish page / at next logon.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /T /IM clockmanage.exe'
  nsExec::Exec 'taskkill /F /IM cm-guard.exe'
  Sleep 800
!macroend

!macro NSIS_HOOK_POSTINSTALL
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
