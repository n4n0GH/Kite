//! The native messaging hosts running for extensions' ports. Their threads hand
//! what happens to the runtime, and the effects of that to the Qt thread.

use crate::extensions::apply_effects_async;
use crate::state::app;
use kite_core::webext::native::{self, NativeOp, Session};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

static SESSIONS: Mutex<Option<HashMap<String, Session>>> = Mutex::new(None);

fn sessions() -> MutexGuard<'static, Option<HashMap<String, Session>>> {
    SESSIONS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The host, or what to tell the extension. (Why it's refused goes in the log only.)
fn find(name: &str, ext: &str) -> Result<native::Host, String> {
    native::find_host(name, ext, &native::default_search_dirs()).map_err(|why| {
        log::warn!("native messaging: {ext} can't use {name}: {why}");
        native::denied(name)
    })
}

pub fn run(op: NativeOp) {
    match op {
        NativeOp::Connect { port, ext, host } => {
            let started = find(&host, &ext).and_then(|h| {
                let (on_message, on_close) = (port.clone(), port.clone());
                Session::start(
                    &h,
                    &ext,
                    move |message| apply_effects_async(app().runtime().native_message(&on_message, message)),
                    move |error| {
                        sessions().get_or_insert_default().remove(&on_close);
                        apply_effects_async(app().runtime().native_closed(&on_close, error));
                    },
                )
                .map_err(|why| {
                    log::warn!("native messaging: {why}");
                    native::denied(&host)
                })
            });
            match started {
                Ok(session) => {
                    sessions().get_or_insert_default().insert(port, session);
                }
                Err(error) => apply_effects_async(app().runtime().native_closed(&port, Some(error))),
            }
        }
        NativeOp::Post { port, message } => {
            if let Some(session) = sessions().get_or_insert_default().get(&port) {
                session.send(&message);
            }
        }
        NativeOp::Close { port } => {
            if let Some(session) = sessions().get_or_insert_default().remove(&port) {
                session.close();
            }
        }
    }
}

/// `runtime.sendNativeMessage`, on a thread of its own.
pub fn send_once(ext: &str, args: &Value) -> Result<Value, String> {
    let name = args["host"].as_str().unwrap_or("");
    let host = find(name, ext)?;
    native::send_once(&host, ext, &args["message"])
}
