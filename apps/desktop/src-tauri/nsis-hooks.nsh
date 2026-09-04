!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\pga" "" "URL:Personal Growth Assistant Protocol"
  WriteRegStr HKCU "Software\Classes\pga" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\pga\DefaultIcon" "" '"$INSTDIR\personal-growth-assistant.exe",0'
  WriteRegStr HKCU "Software\Classes\pga\shell\open\command" "" '"$INSTDIR\personal-growth-assistant.exe" "%1"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "PersonalGrowthAssistant"
  DeleteRegKey HKCU "Software\Classes\pga"
!macroend
