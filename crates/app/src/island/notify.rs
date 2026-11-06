//! System-notification fallback while the island is hidden (UserNotifications).
//! A click reports the request identifier so the controller can locate the original card.
use async_channel::Sender;
use block2::RcBlock;
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
    DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NormaIslandNotificationDelegate"]
    #[ivars = Sender<String>]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            // The hidden island has no other surface, so show the banner even while Norma is active.
            handler.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let id = response.notification().request().identifier().to_string();
            let _ = self.ivars().try_send(id);
            handler.call(());
        }
    }
);

pub(super) struct Notifier {
    center: Retained<UNUserNotificationCenter>,
    _delegate: Retained<Delegate>,
}

impl Notifier {
    /// `None` outside an app bundle (e.g. `cargo run`), where UserNotifications would abort.
    pub fn new(clicks: Sender<String>) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        if NSBundle::mainBundle().bundleIdentifier().is_none() {
            return None;
        }
        let allocated = Delegate::alloc(mtm).set_ivars(clicks);
        // SAFETY: initializes an allocated NSObject subclass on the main thread.
        let delegate: Retained<Delegate> = unsafe { msg_send![super(allocated), init] };
        let center = UNUserNotificationCenter::currentNotificationCenter();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Some(Self {
            center,
            _delegate: delegate,
        })
    }

    pub fn post(&self, id: &str, title: &str, body: &str) {
        let authorized = RcBlock::new(|_granted: objc2::runtime::Bool, _error: *mut NSError| {});
        self.center
            .requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
                &authorized,
            );
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(id),
            &content,
            None,
        );
        let added = RcBlock::new(|error: *mut NSError| {
            if !error.is_null() {
                tracing::warn!("island system notification could not be delivered");
            }
        });
        self.center
            .addNotificationRequest_withCompletionHandler(&request, Some(&added));
    }
}
