//! NativeActivity entry point and Android input integration.

mod environment;
pub(crate) mod keyboard;
mod logcat;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use winit::platform::android::activity::AndroidApp;

static APP: RwLock<Option<AndroidApp>> = RwLock::new(None);

/// The activity glue used to access the Android UI.
pub fn app() -> Option<AndroidApp> {
    APP.read().ok()?.clone()
}

static STARTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
fn android_main(app: AndroidApp) {
    // android_main can be entered again if the activity is recreated.
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    logcat::init();
    let data = environment::setup(&app);
    let shared = std::path::Path::new(environment::SHARED_STUDIO).join("Logs");
    let private = data.join("RuStudio").join("Logs");
    if let Err(error) = rustudio_core::diagnostics::init_android(&shared, &private) {
        eprintln!("rustudio: could not create a diagnostic log: {error}");
    }
    *APP.write().expect("Android activity state") = Some(app);
    eprintln!("rustudio: android_main (HOME={})", std::env::var("HOME").unwrap_or_default());

    if let Err(e) = crate::run() {
        eprintln!("rustudio: stopped with an error: {e}");
    }
    keyboard::reset();
    // Leaving android_main ends the native thread; the activity finishes.
    *APP.write().expect("Android activity state") = None;
    STARTED.store(false, Ordering::SeqCst);
}

/// Opens the Android share sheet for saved diagnostic logs.
pub(crate) fn share_logs() -> Result<(), String> {
    with_activity(|env, activity| {
        env.call_method(
            activity,
            jni::jni_str!("shareDiagnosticLogs"),
            jni::jni_sig!(() -> void),
            &[],
        )?;
        Ok(())
    })
}

fn with_activity(
    call: impl FnOnce(&mut jni::Env<'_>, &jni::objects::JObject<'_>) -> jni::errors::Result<()>,
) -> Result<(), String> {
    let app = app().ok_or("Android activity unavailable")?;
    // The app owns both JNI references throughout this call.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr().cast();
    vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let activity = unsafe { env.as_cast_raw::<jni::objects::JObject>(&activity)? };
        call(env, activity.as_ref())
    })
    .map_err(|error| error.to_string())
}
