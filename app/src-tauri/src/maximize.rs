/// Maximize/restore for the main window.
///
/// With `decorations: false` the macOS window is borderless, and tao maximizes
/// borderless windows with an animated `setFrame:` onto the *main* screen.
/// WKWebView lays out asynchronously in another process, so during that
/// animation its content trails the window and the newly exposed area is
/// blank. On macOS we drive the animation ourselves so the page is always at
/// least as large as the window (the excess is clipped, never exposed), onto
/// the window's own screen. Other platforms use the native behavior, which
/// does not have this problem.
use tauri::WebviewWindow;

pub fn toggle(window: &WebviewWindow) {
    let maximized = window.is_maximized().unwrap_or(false);
    set_maximized(window, !maximized, true);
}

/// `animate` only applies on macOS; pass `false` while the window is hidden
/// (e.g. at startup), where the page cannot render and there is nothing to see.
#[cfg(not(target_os = "macos"))]
pub fn set_maximized(window: &WebviewWindow, maximized: bool, _animate: bool) {
    if maximized {
        let _ = window.maximize();
    } else {
        let _ = window.unmaximize();
    }
}

#[cfg(target_os = "macos")]
pub fn set_maximized(window: &WebviewWindow, maximized: bool, animate: bool) {
    macos::set_maximized(window, maximized, animate);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSWindow};
    use objc2_foundation::{NSError, NSPoint, NSRect, NSSize, NSString};
    use objc2_web_kit::{WKContentWorld, WKWebView};
    use tauri::WebviewWindow;

    /// Frame to go back to on restore, captured when maximizing.
    static RESTORE_FRAME: Mutex<Option<NSRect>> = Mutex::new(None);

    /// Set while a maximize/restore is in flight; toggles during it are ignored.
    static BUSY: AtomicBool = AtomicBool::new(false);

    /// Used on restore when there is no captured frame, e.g. after the app
    /// started maximized. Matches the configured default window size.
    const DEFAULT_SIZE: NSSize = NSSize {
        width: 1200.0,
        height: 800.0,
    };

    /// Resolves once the page reports the given viewport size and has rendered
    /// two frames at it, or after 200ms regardless so a stalled page can never
    /// block the toggle.
    const WAIT_FOR_LAYOUT: &str = "
        const frame = () => new Promise((r) => requestAnimationFrame(r));
        const ready = (async () => {
            while (Math.abs(innerWidth - width) > 1 || Math.abs(innerHeight - height) > 1) {
                await frame();
            }
            await frame();
            await frame();
        })();
        await Promise.race([ready, new Promise((r) => setTimeout(r, 200))]);
    ";

    pub fn set_maximized(window: &WebviewWindow, maximized: bool, animate: bool) {
        if BUSY.swap(true, Ordering::AcqRel) {
            return;
        }
        let result = window.with_webview(move |platform| {
            // SAFETY: tauri hands out this window's live NSWindow and WKWebView,
            // and `with_webview` runs on the main thread.
            let started = unsafe {
                let ns_window = Retained::retain(platform.ns_window().cast::<NSWindow>());
                let webview = Retained::retain(platform.inner().cast::<WKWebView>());
                match (ns_window, webview) {
                    (Some(ns_window), Some(webview)) => {
                        start(ns_window, webview, maximized, animate)
                    }
                    _ => false,
                }
            };
            if !started {
                BUSY.store(false, Ordering::Release);
            }
        });
        if result.is_err() {
            BUSY.store(false, Ordering::Release);
        }
    }

    /// Returns whether a frame change was started; it clears `BUSY` when done.
    unsafe fn start(
        ns_window: Retained<NSWindow>,
        webview: Retained<WKWebView>,
        maximized: bool,
        animate: bool,
    ) -> bool {
        let Some(screen) = ns_window.screen() else {
            return false;
        };
        let visible = screen.visibleFrame();
        let current = ns_window.frame();

        let target = {
            let mut restore = RESTORE_FRAME.lock().unwrap();
            if maximized {
                if same_size(current, visible) {
                    return false;
                }
                *restore = Some(current);
                visible
            } else {
                if !same_size(current, visible) {
                    return false;
                }
                restore.take().unwrap_or_else(|| centered(visible))
            }
        };

        if !animate {
            ns_window.setFrame_display_animate(target, true, false);
            BUSY.store(false, Ordering::Release);
            return true;
        }

        if maximized {
            // Lay the page out at its final size first, then grow the window
            // over content that is already rendered.
            pin_top_left(&webview, target.size);
            let body = NSString::from_str(&format!(
                "const width = {}, height = {};{}",
                target.size.width, target.size.height, WAIT_FOR_LAYOUT
            ));
            let world = WKContentWorld::pageWorld(MainThreadMarker::new_unchecked());
            // Called on the main thread, also when the script fails.
            let wv = webview.clone();
            let done = RcBlock::new(move |_: *mut AnyObject, _: *mut NSError| {
                animate_to(&ns_window, &wv, target);
            });
            webview.callAsyncJavaScript_arguments_inFrame_inContentWorld_completionHandler(
                &body,
                None,
                None,
                &world,
                Some(&done),
            );
        } else {
            // Keep the larger page while the window shrinks over it, and only
            // relayout at the final size.
            pin_top_left(&webview, webview.frame().size);
            animate_to(&ns_window, &webview, target);
        }
        true
    }

    /// Detaches the webview from the window's live size: fixes it at `size`,
    /// anchored to the top-left corner, so the window edge clips it instead.
    fn pin_top_left(webview: &WKWebView, size: NSSize) {
        // SAFETY: the webview is in the view hierarchy for the call's duration.
        let Some(parent) = (unsafe { webview.superview() }) else {
            return;
        };
        let flipped = parent.isFlipped();
        let y = if flipped {
            0.0
        } else {
            parent.bounds().size.height - size.height
        };
        webview.setAutoresizingMask(if flipped {
            NSAutoresizingMaskOptions::ViewMaxYMargin
        } else {
            NSAutoresizingMaskOptions::ViewMinYMargin
        });
        webview.setFrame(NSRect {
            origin: NSPoint { x: 0.0, y },
            size,
        });
    }

    fn animate_to(ns_window: &NSWindow, webview: &WKWebView, target: NSRect) {
        // Blocks until the animation has finished.
        ns_window.setFrame_display_animate(target, true, true);
        // Hand sizing back to AppKit, as wry set it up.
        webview.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // SAFETY: the webview is in the view hierarchy for the call's duration.
        if let Some(parent) = unsafe { webview.superview() } {
            webview.setFrame(parent.bounds());
        }
        BUSY.store(false, Ordering::Release);
    }

    /// Same tolerance tao uses to decide whether a borderless window is maximized.
    fn same_size(a: NSRect, b: NSRect) -> bool {
        (a.size.width - b.size.width).abs() < 1.0 && (a.size.height - b.size.height).abs() < 1.0
    }

    fn centered(visible: NSRect) -> NSRect {
        let size = NSSize {
            width: DEFAULT_SIZE.width.min(visible.size.width),
            height: DEFAULT_SIZE.height.min(visible.size.height),
        };
        NSRect {
            origin: NSPoint {
                x: visible.origin.x + (visible.size.width - size.width) / 2.0,
                y: visible.origin.y + (visible.size.height - size.height) / 2.0,
            },
            size,
        }
    }
}
