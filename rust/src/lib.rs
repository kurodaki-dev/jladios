//! Pont JNI : fonctions appelées par la (fine) couche Java Android.
//! Toute la logique de l'app vit côté Rust.

mod ansi;
mod app;
mod brain;
mod character;
mod config;
mod llm;
mod models;
mod stt;
mod terminal;
mod tts;

use jni::objects::{JClass, JFloatArray, JIntArray, JString};
use jni::sys::{jboolean, jint, jstring, JNI_TRUE};
use jni::JNIEnv;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;

/// Empêche une panique Rust de traverser la frontière JNI (crash de l'app).
fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(default)
}

fn get_string(env: &mut JNIEnv, s: &JString) -> String {
    if s.is_null() {
        return String::new();
    }
    env.get_string(s).map(|s| s.into()).unwrap_or_default()
}

fn new_string(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_init<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    files: JString<'l>,
    cache: JString<'l>,
) {
    let files = get_string(&mut env, &files);
    let cache = get_string(&mut env, &cache);
    guard((), || app::init(files, cache));
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_onInput<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    line: JString<'l>,
) {
    let line = get_string(&mut env, &line);
    guard((), || {
        if app::try_app().is_some() {
            terminal::handle_input(&line)
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_pollEvent<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jstring {
    let ev = guard(None, || app::try_app().and_then(|a| a.pop_event()));
    match ev {
        Some(s) => new_string(&mut env, &s),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_pushAudio<'l>(
    env: JNIEnv<'l>,
    _c: JClass<'l>,
    arr: JFloatArray<'l>,
    n: jint,
) {
    let Some(a) = app::try_app() else { return };
    if n <= 0 || !a.listening.load(Ordering::Relaxed) {
        return;
    }
    let mut buf = vec![0f32; n as usize];
    if env.get_float_array_region(&arr, 0, &mut buf).is_ok() {
        guard((), || a.push_audio(&buf));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_pullAudio<'l>(
    env: JNIEnv<'l>,
    _c: JClass<'l>,
    arr: JFloatArray<'l>,
) -> jint {
    let Some(a) = app::try_app() else { return 0 };
    let len = env.get_array_length(&arr).unwrap_or(0).max(0) as usize;
    let mut buf = vec![0f32; len];
    let n = guard(0, || a.pull_audio(&mut buf));
    if n > 0 && env.set_float_array_region(&arr, 0, &buf[..n]).is_err() {
        return 0;
    }
    n as jint
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_renderCharacter<'l>(
    env: JNIEnv<'l>,
    _c: JClass<'l>,
    arr: JIntArray<'l>,
    w: jint,
    h: jint,
    mouth_open: jboolean,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let (w, h) = (w as usize, h as usize);
    let mut buf = vec![0i32; w * h];
    guard((), || character::render(&mut buf, w, h, mouth_open == JNI_TRUE));
    let _ = env.set_int_array_region(&arr, 0, &buf);
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_overlayState<'l>(
    _env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jint {
    guard(0, || app::try_app().map(|a| a.overlay_state()).unwrap_or(0))
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_subtitle<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jstring {
    let s = guard(String::new(), || app::try_app().map(|a| a.subtitle()).unwrap_or_default());
    new_string(&mut env, &s)
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_assistantName<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jstring {
    let s = guard(String::new(), || {
        app::try_app().map(|a| a.cfg().assistant_name).unwrap_or_default()
    });
    new_string(&mut env, &s)
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_onTap<'l>(_env: JNIEnv<'l>, _c: JClass<'l>) {
    guard((), || {
        if let Some(a) = app::try_app() {
            a.tap()
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_setOverlayVisible<'l>(
    _env: JNIEnv<'l>,
    _c: JClass<'l>,
    visible: jboolean,
) {
    guard((), || {
        if let Some(a) = app::try_app() {
            let v = visible == JNI_TRUE;
            a.overlay_visible.store(v, Ordering::SeqCst);
            if !v {
                a.stop_listening();
            }
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_micUnavailable<'l>(
    mut env: JNIEnv<'l>,
    _c: JClass<'l>,
    reason: JString<'l>,
) {
    let reason = get_string(&mut env, &reason);
    guard((), || {
        if let Some(a) = app::try_app() {
            a.mic_unavailable(&reason)
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_playbackRate<'l>(
    _env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jint {
    app::PLAYBACK_RATE
}

#[no_mangle]
pub extern "system" fn Java_com_jladios_assistant_Native_micRate<'l>(
    _env: JNIEnv<'l>,
    _c: JClass<'l>,
) -> jint {
    app::MIC_RATE
}
