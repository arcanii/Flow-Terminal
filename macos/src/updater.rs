//! Auto-update through Sparkle (see macos/RELEASING.md): an EdDSA-signed
//! appcast and the DMGs are published in the public arcanii/Flow-releases
//! repo.
//!
//! Sparkle.framework is loaded from Flow.app/Contents/Frameworks at run time,
//! so builds without it (`cargo run`, a bare bundle) simply have no updater,
//! and only a build made as one of Flow's releases starts it
//! (`version::UPDATES`). The feed and the public key come from the bundle's
//! Info.plist.

use std::cell::OnceCell;
use std::path::Path;

use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{ns_string, NSBundle, NSString};

thread_local! {
    /// The SPUStandardUpdaterController; lives as long as the app.
    static CONTROLLER: OnceCell<Retained<AnyObject>> = const { OnceCell::new() };
}

/// Loads Sparkle and starts its scheduled checks. False if unavailable.
/// Sparkle quits Flow itself (through NSApp) when it installs an update.
pub fn init() -> bool {
    if available() {
        return true;
    }
    let bundle = NSBundle::mainBundle();
    let Some(frameworks) = bundle.privateFrameworksPath() else { return false };
    let path = format!("{frameworks}/Sparkle.framework");
    if !Path::new(&path).is_dir() {
        log::info!("updater off: {path} not found");
        return false;
    }
    // Sparkle takes the key from Info.plist; refuse one that isn't Flow's,
    // so a packaging slip can't make updates trust the wrong signer.
    let key = bundle
        .objectForInfoDictionaryKey(ns_string!("SUPublicEDKey"))
        .and_then(|k| k.downcast::<NSString>().ok())
        .map(|k| k.to_string());
    if key.as_deref() != Some(flow_core::UPDATE_PUBLIC_KEY) {
        log::warn!("updater off: Info.plist SUPublicEDKey is {key:?}, not Flow's key");
        return false;
    }
    let Some(framework) = NSBundle::bundleWithPath(&NSString::from_str(&path)) else {
        log::warn!("updater off: could not open {path}");
        return false;
    };
    // SAFETY: loading a framework bundle; Sparkle has no load-time side effects.
    if !unsafe { framework.load() } {
        log::warn!("updater off: could not load {path}");
        return false;
    }
    let Some(class) = AnyClass::get(c"SPUStandardUpdaterController") else {
        log::warn!("updater off: SPUStandardUpdaterController missing from Sparkle");
        return false;
    };
    // SAFETY: Sparkle's documented initializer; nil delegates are allowed.
    let controller: Option<Retained<AnyObject>> = unsafe {
        let obj: Allocated<AnyObject> = msg_send![class, alloc];
        msg_send![
            obj,
            initWithStartingUpdater: true,
            updaterDelegate: None::<&AnyObject>,
            userDriverDelegate: None::<&AnyObject>
        ]
    };
    let Some(controller) = controller else {
        log::warn!("updater off: Sparkle did not start");
        return false;
    };
    CONTROLLER.with(|c| {
        let _ = c.set(controller);
    });
    log::info!("updater on ({})", flow_core::version::display());
    true
}

pub fn available() -> bool {
    CONTROLLER.with(|c| c.get().is_some())
}

/// "Check for Updates…": Sparkle shows its own progress and result.
pub fn check_with_ui() {
    CONTROLLER.with(|c| {
        if let Some(controller) = c.get() {
            // SAFETY: an IBAction on SPUStandardUpdaterController; the sender may be nil.
            unsafe {
                let _: () = msg_send![&**controller, checkForUpdates: None::<&AnyObject>];
            }
        }
    });
}
