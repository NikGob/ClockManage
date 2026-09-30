; ClockManage NSIS hooks (bundle.windows.nsis.installerHooks).

; Install / update over an existing version: close the running app (it lives in the tray and
; refuses to quit during study time) and its launch guard, so files can be replaced without
; the "app is running" prompt. The new version is started by NSIS_HOOK_POSTINSTALL.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /T /IM clockmanage.exe'
  nsExec::Exec 'taskkill /F /IM cm-guard.exe'
  Sleep 800
!macroend

; Start the new version at once (hidden, with the installer's admin rights), so blocking comes
; back right away: the finish page checkbox may be unticked or its UAC prompt declined, and the
; 5-minute watchdog task may be gone (an uninstall run by the update deletes it). The app
; re-applies the lock, the launch guard and its scheduled tasks on start; opening it from the
; finish page later just shows the window of this running instance.
!macro NSIS_HOOK_POSTINSTALL
  Exec '"$INSTDIR\clockmanage.exe" --background'
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
