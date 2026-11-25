//! The Mac's notification of a long command done (`Platform::notify`):
//! posted to Notification Center through UNUserNotificationCenter, shown
//! even while Flow is the app in front, and its click sent back to the app
//! as `UserEvent::NotificationClicked` with the tag it was posted with,
//! macOS bringing Flow forward meanwhile. Authorization is asked with each
//! one: macOS asks the user the first time only, and after that answers at
//! once with what they chose. A bare binary (`cargo run`) has no bundle,
//! without which UNUserNotificationCenter raises an exception, so it
//! bounces Flow's Dock icon instead.

use std::cell::OnceCell;
use std::sync::Mutex;

use block2::{DynBlock, RcBlock};
use flow_core::event::UserEvent;
use flow_core::session::Waker;
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, MainThreadMarker};
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::{NSError, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions,
    UNNotificationRequest, UNNotificationResponse, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};
use winit::window::{UserAttentionType, Window};

/// Where a click goes: macOS calls the delegate on a queue of its choosing,
/// so the waker is kept where any thread reaches it.
static SINK: Mutex<Option<Waker>> = Mutex::new(None);

thread_local! {
    /// The center keeps its delegate weakly, so it's kept here, made on the
    /// main thread the first time Flow notifies.
    static DELEGATE: OnceCell<Retained<Delegate>> = const { OnceCell::new() };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and Delegate has no
    // Drop impl.
    #[unsafe(super(NSObject))]
    #[name = "FlowNotificationDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        /// Shown as a banner, and kept in Notification Center, even while
        /// Flow is the app in front (its window behind another of its own
        /// or minimized), where macOS would otherwise drop it.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            handler.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List,));
        }

        /// Clicked: the tag, which is its identifier, goes to the app.
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(&self, _center: &UNUserNotificationCenter, response: &UNNotificationResponse, handler: &DynBlock<dyn Fn()>) {
            if let Ok(tag) = response.notification().request().identifier().to_string().parse::<u64>() {
                if let Some(waker) = SINK.lock().ok().and_then(|s| s.clone()) {
                    waker.send(UserEvent::NotificationClicked(tag));
                }
            }
            handler.call(());
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// Where clicks go, from the waker the menus are given.
pub fn keep_waker(waker: Waker) {
    if let Ok(mut sink) = SINK.lock() {
        *sink = Some(waker);
    }
}

/// Posts `title` and `body`, under `tag`; a second one with the tag (the
/// same tab's next command) takes the first one's place.
pub fn notify(window: &Window, title: &str, body: &str, tag: u64) {
    let bundled = NSRunningApplication::currentApplication().bundleIdentifier().is_some();
    if !bundled || MainThreadMarker::new().is_none() {
        window.request_user_attention(Some(UserAttentionType::Informational));
        return;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    DELEGATE.with(|d| {
        let delegate = d.get_or_init(Delegate::new);
        center.setDelegate(Some(ProtocolObject::from_ref(&**delegate)));
    });
    let (title, body) = (title.to_string(), body.to_string());
    let then = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
        if granted.as_bool() {
            post(&title, &body, tag);
        } else {
            log::info!("notifications aren't allowed for Flow (System Settings › Notifications)");
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &then);
}

/// Adds the notification, at once (no trigger), from whatever queue the
/// authorization's answer came on, as the center allows.
fn post(title: &str, body: &str, tag: u64) {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(&tag.to_string()), &content, None);
    let done = RcBlock::new(|error: *mut NSError| {
        // SAFETY: the center hands a valid error or none.
        if let Some(error) = unsafe { error.as_ref() } {
            log::warn!("could not post a notification: {}", error.localizedDescription());
        }
    });
    UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, Some(&done));
}
