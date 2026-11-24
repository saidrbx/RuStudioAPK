use std::sync::Mutex;

use egui::{Context, Event, Id, ImeEvent, Key, Modifiers, OutputCommand, RawInput};
use jni::objects::{JObject, JString};
use jni::sys::jint;
use jni::EnvUnowned;

#[derive(Default)]
struct Keyboard {
    ctx: Option<Context>,
    field: Option<(Id, bool)>,
    generation: i32,
    events: Vec<Event>,
}

static KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard {
    ctx: None,
    field: None,
    generation: 0,
    events: Vec::new(),
});

pub(super) fn reset() {
    *KEYBOARD.lock().unwrap() = Keyboard::default();
}

pub(crate) fn install(ctx: &Context) {
    KEYBOARD.lock().unwrap().ctx = Some(ctx.clone());
    ctx.on_end_pass(
        "Android editor keyboard",
        std::sync::Arc::new(|ui| {
            let ctx = ui.ctx();
            // NativeActivity has no editable input connection. Consume the IME
            // request so egui-winit does not also toggle the NativeActivity view.
            let (ime, copies) = ctx.output_mut(|out| {
                let copies = out
                    .commands
                    .iter()
                    .filter_map(|cmd| match cmd {
                        OutputCommand::CopyText(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                (out.ime.take(), copies)
            });
            for text in copies {
                let _ = super::with_activity(|env, activity| {
                    let text = env.new_string(text)?;
                    env.call_method(
                        activity,
                        jni::jni_str!("copyEditorText"),
                        jni::jni_sig!((java.lang.String) -> void),
                        &[(&text).into()],
                    )?;
                    Ok(())
                });
            }
            let field = ime.as_ref().and_then(|ime| {
                ctx.memory(|m| m.focused())
                    .map(|id| (id, ime.purpose == egui::viewport::IMEPurpose::Password))
            });
            let tapped = ime.is_some_and(|ime| {
                ctx.input(|input| {
                    input.pointer.primary_clicked()
                        && input
                            .pointer
                            .interact_pos()
                            .is_some_and(|pos| ime.rect.contains(pos))
                })
            });
            let generation = {
                let mut keyboard = KEYBOARD.lock().unwrap();
                if field != keyboard.field {
                    keyboard.field = field;
                    keyboard.generation = keyboard.generation.wrapping_add(1);
                    keyboard.events.clear();
                } else if !tapped || field.is_none() {
                    return;
                }
                keyboard.generation
            };
            let mode = match field {
                None => 0,
                Some((_, false)) => 1,
                Some((_, true)) => 2,
            };
            let _ = super::with_activity(|env, activity| {
                env.call_method(
                    activity,
                    jni::jni_str!("setEditorKeyboard"),
                    jni::jni_sig!((int, int) -> void),
                    &[mode.into(), generation.into()],
                )?;
                Ok(())
            });
        }),
    );
}

pub(crate) fn drain(raw: &mut RawInput) {
    raw.events.append(&mut KEYBOARD.lock().unwrap().events);
}

#[no_mangle]
extern "system" fn Java_com_rustudio_app_MainActivity_editorKeyboardEvent(
    mut env: EnvUnowned<'_>,
    _activity: JObject<'_>,
    generation: jint,
    kind: jint,
    text: JString<'_>,
    before: jint,
    after: jint,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let event = match kind {
            0 => Event::Ime(ImeEvent::Preedit {
                text: text.try_to_string(env)?,
                active_range_chars: None,
            }),
            1 => Event::Ime(ImeEvent::Commit(text.try_to_string(env)?)),
            2 => Event::Ime(ImeEvent::DeleteSurrounding {
                before_chars: before.max(0) as usize,
                after_chars: after.max(0) as usize,
            }),
            3 => {
                let key = match before {
                    19 => Key::ArrowUp,
                    20 => Key::ArrowDown,
                    21 => Key::ArrowLeft,
                    22 => Key::ArrowRight,
                    61 => Key::Tab,
                    66 => Key::Enter,
                    67 => Key::Backspace,
                    112 => Key::Delete,
                    _ => return Ok(()),
                };
                Event::Key {
                    key,
                    physical_key: None,
                    pressed: after != 0,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }
            }
            _ => return Ok(()),
        };
        let ctx = {
            let mut keyboard = KEYBOARD.lock().unwrap();
            if keyboard.field.is_none() || keyboard.generation != generation {
                return Ok(());
            }
            keyboard.events.push(event);
            keyboard.ctx.clone()
        };
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}
