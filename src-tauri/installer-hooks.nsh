!macro NSIS_HOOK_POSTINSTALL
  ; 将资源目录中的 WebView2Loader.dll 复制到安装根目录
  ; 这样 jtools-desktop.exe 启动时总能在同目录找到该 DLL。
  IfFileExists "$INSTDIR\resources\WebView2Loader.dll" 0 +2
    CopyFiles /SILENT "$INSTDIR\resources\WebView2Loader.dll" "$INSTDIR\WebView2Loader.dll"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; 卸载时顺带清理根目录中的 WebView2Loader.dll
  IfFileExists "$INSTDIR\WebView2Loader.dll" 0 +2
    Delete "$INSTDIR\WebView2Loader.dll"
!macroend
