/// Disables WebView2's browser accelerator keys (F5/Ctrl+R reload, Ctrl+P print,
/// Ctrl+F/F3 find, Ctrl+U view source, F12/Ctrl+Shift+I devtools, Alt+Left/Right
/// navigation, etc.). Text-editing keys (Ctrl+C/V/X/A/Z/Y) and caret movement
/// are unaffected, and all keys are still delivered to the page as keydown
/// events. The frontend whitelist in `src/lib/shortcuts.ts` covers the rest.
///
/// Debug builds keep the defaults so F5 reload and F12 devtools stay available.
pub fn disable_browser_accelerator_keys(window: &tauri::WebviewWindow) {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
        use windows_core::Interface;

        let _ = window.with_webview(|webview| unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
                let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
        });
    }

    #[cfg(not(all(windows, not(debug_assertions))))]
    let _ = window;
}
