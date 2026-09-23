!macro NSIS_HOOK_POSTINSTALL
  ; Notify the Shell to refresh the icon cache so newly installed
  ; icons appear immediately without a logoff/reboot.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Refresh the icon cache after uninstall so stale icons are removed.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend
