//! MIDI devices for Web MIDI (js/webmidi.js in the kite crate), which QtWebEngine refuses
//! without asking: it has no permission for it.
//!
//! This talks to ALSA's sequencer, which is where PipeWire and JACK bridges show up too.
//! ALSA is loaded when first needed, not linked: without it there just are no MIDI devices,
//! and building Kite takes no ALSA headers.
//!
//! A `Session` belongs to one page. It's the only way to devices, and it's what keeps a page
//! without the permission for SysEx from it, which can rewrite a device's settings and firmware.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_int, c_long, c_uint, c_void, CStr};
use std::hash::{Hash, Hasher};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;

/// Longer than this, a SysEx message that doesn't end is a broken device or sender.
const MAX_SYSEX: usize = 1 << 20;

const OPEN_DUPLEX: c_int = 3;
const NONBLOCK: c_int = 1;
const CAP_READ: c_uint = 1;
const CAP_WRITE: c_uint = 1 << 1;
const CAP_SUBS_READ: c_uint = 1 << 5;
const CAP_SUBS_WRITE: c_uint = 1 << 6;
const CAP_NO_EXPORT: c_uint = 1 << 7;
const TYPE_MIDI_GENERIC: c_uint = 1 << 1;
const TYPE_APPLICATION: c_uint = 1 << 20;
const CLIENT_SYSTEM: c_int = 0;
const PORT_SYSTEM_ANNOUNCE: c_int = 1;
const QUEUE_DIRECT: u8 = 253;
const EVENT_NONE: u8 = 0;
const EVENT_CLIENT_START: u8 = 60;
const EVENT_PORT_CHANGE: u8 = 65;
const EAGAIN: c_int = 11;
const ENOSPC: c_int = 28;
/// How long a send keeps trying while the sequencer is full.
const SEND_TRIES: u32 = 500;

/// `snd_seq_event_t`, which is part of ALSA's stable interface.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SeqEvent {
    kind: u8,
    flags: u8,
    tag: u8,
    queue: u8,
    time: [u32; 2],
    source: [u8; 2],
    dest: [u8; 2],
    data: [u8; 12],
}

macro_rules! alsa_api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        #[allow(non_snake_case)]
        struct Alsa { $($name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)* }
        impl Alsa {
            unsafe fn load() -> Option<Alsa> {
                let lib = libc::dlopen(c"libasound.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                if lib.is_null() {
                    return None;
                }
                Some(Alsa { $($name: {
                    let symbol = libc::dlsym(lib, concat!(stringify!($name), "\0").as_ptr().cast());
                    if symbol.is_null() {
                        return None;
                    }
                    std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(symbol)
                },)* })
            }
        }
    };
}

alsa_api! {
    snd_seq_open: fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
    snd_seq_close: fn(*mut c_void) -> c_int;
    snd_seq_set_client_name: fn(*mut c_void, *const c_char) -> c_int;
    snd_seq_client_id: fn(*mut c_void) -> c_int;
    snd_seq_create_simple_port: fn(*mut c_void, *const c_char, c_uint, c_uint) -> c_int;
    snd_seq_connect_from: fn(*mut c_void, c_int, c_int, c_int) -> c_int;
    snd_seq_disconnect_from: fn(*mut c_void, c_int, c_int, c_int) -> c_int;
    snd_seq_poll_descriptors_count: fn(*mut c_void, i16) -> c_int;
    snd_seq_poll_descriptors: fn(*mut c_void, *mut libc::pollfd, c_uint, i16) -> c_int;
    snd_seq_event_input: fn(*mut c_void, *mut *mut SeqEvent) -> c_int;
    snd_seq_event_output_direct: fn(*mut c_void, *mut SeqEvent) -> c_int;
    snd_seq_client_info_malloc: fn(*mut *mut c_void) -> c_int;
    snd_seq_client_info_free: fn(*mut c_void) -> ();
    snd_seq_client_info_set_client: fn(*mut c_void, c_int) -> ();
    snd_seq_client_info_get_client: fn(*mut c_void) -> c_int;
    snd_seq_query_next_client: fn(*mut c_void, *mut c_void) -> c_int;
    snd_seq_port_info_malloc: fn(*mut *mut c_void) -> c_int;
    snd_seq_port_info_free: fn(*mut c_void) -> ();
    snd_seq_port_info_set_client: fn(*mut c_void, c_int) -> ();
    snd_seq_port_info_set_port: fn(*mut c_void, c_int) -> ();
    snd_seq_port_info_get_port: fn(*mut c_void) -> c_int;
    snd_seq_port_info_get_name: fn(*mut c_void) -> *const c_char;
    snd_seq_port_info_get_capability: fn(*mut c_void) -> c_uint;
    snd_seq_query_next_port: fn(*mut c_void, *mut c_void) -> c_int;
    snd_midi_event_new: fn(usize, *mut *mut c_void) -> c_int;
    snd_midi_event_free: fn(*mut c_void) -> ();
    snd_midi_event_no_status: fn(*mut c_void, c_int) -> ();
    snd_midi_event_reset_encode: fn(*mut c_void) -> ();
    snd_midi_event_reset_decode: fn(*mut c_void) -> ();
    snd_midi_event_encode: fn(*mut c_void, *const u8, c_long, *mut SeqEvent) -> c_long;
    snd_midi_event_decode: fn(*mut c_void, *mut u8, c_long, *const SeqEvent) -> c_long;
}

fn alsa() -> Option<&'static Alsa> {
    static ALSA: OnceLock<Option<Alsa>> = OnceLock::new();
    ALSA.get_or_init(|| unsafe { Alsa::load() }).as_ref()
}

/// A device's way in or out, as a page sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Port {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum Event {
    /// What's there has changed.
    Ports { ports: Vec<Port> },
    /// One whole message from an input that's open.
    Message { port: String, data: Vec<u8> },
}

enum Command {
    Open(String),
    Close(String),
    Send(String, Vec<u8>),
    AllowSysex,
    Stop,
}

/// A page's connection to the MIDI devices. It ends when this is dropped.
pub struct Session {
    commands: Sender<Command>,
    wake: c_int,
}

impl Session {
    /// With the ports there are now. `site` keeps a device's id from being the same
    /// everywhere, which could tell sites they have the same visitor.
    pub fn open(site: &str, allow_sysex: bool, on_event: impl Fn(Event) + Send + 'static) -> anyhow::Result<(Session, Vec<Port>)> {
        let alsa = alsa().ok_or_else(|| anyhow::anyhow!("ALSA isn't available"))?;
        let client = unsafe { Client::open(alsa)? };
        let table = unsafe { client.ports(site) };
        let ports = table.iter().map(|p| p.port.clone()).collect();
        let (commands, received) = channel();
        let mut pipe = [0; 2];
        if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            anyhow::bail!("no pipe: {}", std::io::Error::last_os_error());
        }
        let site = site.to_owned();
        let worker = Worker { client, site, allow_sysex, table, opened: HashSet::new(), partial: HashMap::new(), received, on_event: Box::new(on_event) };
        let woken = pipe[0];
        std::thread::Builder::new().name("kite-midi".into()).spawn(move || {
            worker.run(woken);
            unsafe { libc::close(woken) };
        })?;
        Ok((Session { commands, wake: pipe[1] }, ports))
    }

    fn send_command(&self, command: Command) {
        let _ = self.commands.send(command);
        unsafe { libc::write(self.wake, b"x".as_ptr().cast(), 1) };
    }

    pub fn open_input(&self, id: &str) {
        self.send_command(Command::Open(id.to_owned()));
    }

    pub fn close_input(&self, id: &str) {
        self.send_command(Command::Close(id.to_owned()));
    }

    /// Whole messages for an output. Anything else, or SysEx without being allowed it, is dropped.
    pub fn send(&self, id: &str, data: Vec<u8>) {
        self.send_command(Command::Send(id.to_owned(), data));
    }

    pub fn allow_sysex(&self) {
        self.send_command(Command::AllowSysex);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.send_command(Command::Stop);
        unsafe { libc::close(self.wake) };
    }
}

/// Is this whole MIDI messages, one after the other? (`0xF0 … 0xF7` being SysEx.)
pub fn valid_messages(data: &[u8], allow_sysex: bool) -> bool {
    let mut at = 0;
    while at < data.len() {
        let status = data[at];
        if status < 0x80 {
            return false;
        }
        at += 1;
        if status == 0xF0 {
            if !allow_sysex {
                return false;
            }
            loop {
                match data.get(at) {
                    None => return false,
                    Some(0xF7) => break,
                    // (Realtime messages may come in the middle of it.)
                    Some(0xF8..=0xFF) | Some(0..=0x7F) => at += 1,
                    Some(_) => return false,
                }
            }
            at += 1;
            continue;
        }
        let needed = match status {
            0x80..=0xBF | 0xE0..=0xEF | 0xF2 => 2,
            0xC0..=0xDF | 0xF1 | 0xF3 => 1,
            0xF6 | 0xF8..=0xFF => 0,
            // 0xF4, 0xF5 and a 0xF7 on its own aren't messages.
            _ => return false,
        };
        for _ in 0..needed {
            if data.get(at).is_none_or(|b| *b >= 0x80) {
                return false;
            }
            at += 1;
        }
    }
    true
}

/// Stays the same for a device on a site, across plugging it in again.
fn port_id(site: &str, kind: &str, name: &str, nth: usize) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (site, kind, name, nth).hash(&mut hash);
    format!("{:016x}", hash.finish())
}

struct Known {
    port: Port,
    address: (c_int, c_int),
}

/// A connection to the sequencer, and the two ports of ours: to read and to send from.
struct Client {
    alsa: &'static Alsa,
    seq: *mut c_void,
    id: c_int,
    input: c_int,
    output: c_int,
    encoder: *mut c_void,
    decoder: *mut c_void,
}

// (Only used by one thread at a time: made on one, and then given to the worker's.)
unsafe impl Send for Client {}

impl Client {
    unsafe fn open(alsa: &'static Alsa) -> anyhow::Result<Client> {
        let mut seq = std::ptr::null_mut();
        let opened = (alsa.snd_seq_open)(&mut seq, c"default".as_ptr(), OPEN_DUPLEX, NONBLOCK);
        anyhow::ensure!(opened >= 0, "can't open the ALSA sequencer ({opened})");
        // (Before anything else could fail, for what's below to close it.)
        let mut client = Client { alsa, seq, id: 0, input: 0, output: 0, encoder: std::ptr::null_mut(), decoder: std::ptr::null_mut() };
        (alsa.snd_seq_set_client_name)(seq, c"Kite".as_ptr());
        client.id = (alsa.snd_seq_client_id)(seq);
        // Not for others to connect to: a site's messages are only for where it sends them.
        client.input = (alsa.snd_seq_create_simple_port)(seq, c"Kite in".as_ptr(), CAP_WRITE | CAP_SUBS_WRITE | CAP_NO_EXPORT, TYPE_APPLICATION | TYPE_MIDI_GENERIC);
        client.output = (alsa.snd_seq_create_simple_port)(seq, c"Kite out".as_ptr(), CAP_READ | CAP_SUBS_READ | CAP_NO_EXPORT, TYPE_APPLICATION | TYPE_MIDI_GENERIC);
        anyhow::ensure!(client.input >= 0 && client.output >= 0, "can't make ALSA sequencer ports");
        anyhow::ensure!((alsa.snd_midi_event_new)(256, &mut client.encoder) >= 0, "no MIDI encoder");
        anyhow::ensure!((alsa.snd_midi_event_new)(256, &mut client.decoder) >= 0, "no MIDI decoder");
        (alsa.snd_midi_event_no_status)(client.decoder, 1);
        (alsa.snd_seq_connect_from)(seq, client.input, CLIENT_SYSTEM, PORT_SYSTEM_ANNOUNCE);
        Ok(client)
    }

    unsafe fn ports(&self, site: &str) -> Vec<Known> {
        let a = self.alsa;
        let (mut client, mut port) = (std::ptr::null_mut(), std::ptr::null_mut());
        (a.snd_seq_client_info_malloc)(&mut client);
        (a.snd_seq_port_info_malloc)(&mut port);
        let mut found = Vec::new();
        let mut seen: HashMap<(&'static str, String), usize> = HashMap::new();
        (a.snd_seq_client_info_set_client)(client, -1);
        while (a.snd_seq_query_next_client)(self.seq, client) >= 0 {
            let id = (a.snd_seq_client_info_get_client)(client);
            if id == CLIENT_SYSTEM || id == self.id {
                continue;
            }
            (a.snd_seq_port_info_set_client)(port, id);
            (a.snd_seq_port_info_set_port)(port, -1);
            while (a.snd_seq_query_next_port)(self.seq, port) >= 0 {
                let caps = (a.snd_seq_port_info_get_capability)(port);
                if caps & CAP_NO_EXPORT != 0 {
                    continue;
                }
                let name = CStr::from_ptr((a.snd_seq_port_info_get_name)(port)).to_string_lossy().into_owned();
                let number = (a.snd_seq_port_info_get_port)(port);
                for (kind, needs) in [("input", CAP_READ | CAP_SUBS_READ), ("output", CAP_WRITE | CAP_SUBS_WRITE)] {
                    if caps & needs == needs {
                        let nth = seen.entry((kind, name.clone())).or_default();
                        found.push(Known { port: Port { id: port_id(site, kind, &name, *nth), name: name.clone(), kind }, address: (id, number) });
                        *nth += 1;
                    }
                }
            }
        }
        (a.snd_seq_port_info_free)(port);
        (a.snd_seq_client_info_free)(client);
        found
    }

    unsafe fn send(&self, to: (c_int, c_int), data: &[u8]) {
        let a = self.alsa;
        (a.snd_midi_event_reset_encode)(self.encoder);
        let mut rest = data;
        while !rest.is_empty() {
            let mut event = SeqEvent::default();
            let used = (a.snd_midi_event_encode)(self.encoder, rest.as_ptr(), rest.len() as c_long, &mut event);
            if used <= 0 {
                return;
            }
            rest = &rest[used as usize..];
            if event.kind == EVENT_NONE {
                continue;
            }
            event.queue = QUEUE_DIRECT;
            event.source = [self.id as u8, self.output as u8];
            event.dest = [to.0 as u8, to.1 as u8];
            for _ in 0..SEND_TRIES {
                if (a.snd_seq_event_output_direct)(self.seq, &mut event) != -EAGAIN {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            if !self.encoder.is_null() {
                (self.alsa.snd_midi_event_free)(self.encoder);
            }
            if !self.decoder.is_null() {
                (self.alsa.snd_midi_event_free)(self.decoder);
            }
            (self.alsa.snd_seq_close)(self.seq);
        }
    }
}

struct Worker {
    client: Client,
    site: String,
    allow_sysex: bool,
    table: Vec<Known>,
    opened: HashSet<String>,
    /// SysEx messages under way, from where they come: they arrive in pieces.
    partial: HashMap<[u8; 2], Vec<u8>>,
    received: Receiver<Command>,
    on_event: Box<dyn Fn(Event) + Send>,
}

impl Worker {
    fn run(mut self, woken: c_int) {
        let a = self.client.alsa;
        let count = unsafe { (a.snd_seq_poll_descriptors_count)(self.client.seq, libc::POLLIN) }.max(0) as usize;
        let mut fds = vec![libc::pollfd { fd: woken, events: libc::POLLIN, revents: 0 }; count + 1];
        unsafe { (a.snd_seq_poll_descriptors)(self.client.seq, fds[1..].as_mut_ptr(), count as c_uint, libc::POLLIN) };
        loop {
            if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) } < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            let mut drain = [0u8; 64];
            while unsafe { libc::read(woken, drain.as_mut_ptr().cast(), drain.len()) } > 0 {}
            if !self.commands() {
                return;
            }
            self.events();
        }
    }

    /// Whether to go on.
    fn commands(&mut self) -> bool {
        while let Ok(command) = self.received.try_recv() {
            match command {
                Command::Stop => return false,
                Command::AllowSysex => self.allow_sysex = true,
                Command::Open(id) => self.connect(&id),
                Command::Close(id) => {
                    if self.opened.remove(&id) {
                        if let Some(address) = self.address(&id, "input") {
                            unsafe { (self.client.alsa.snd_seq_disconnect_from)(self.client.seq, self.client.input, address.0, address.1) };
                        }
                    }
                }
                Command::Send(id, data) => {
                    if valid_messages(&data, self.allow_sysex) {
                        if let Some(address) = self.address(&id, "output") {
                            unsafe { self.client.send(address, &data) };
                        }
                    }
                }
            }
        }
        true
    }

    fn address(&self, id: &str, kind: &str) -> Option<(c_int, c_int)> {
        self.table.iter().find(|k| k.port.id == id && k.port.kind == kind).map(|k| k.address)
    }

    fn connect(&mut self, id: &str) {
        self.opened.insert(id.to_owned());
        let Some(address) = self.address(id, "input") else { return };
        let connected = unsafe { (self.client.alsa.snd_seq_connect_from)(self.client.seq, self.client.input, address.0, address.1) };
        if connected < 0 {
            log::debug!("can't connect to MIDI input {address:?}: {connected}");
        }
    }

    fn events(&mut self) {
        let a = self.client.alsa;
        let mut changed = false;
        loop {
            let mut event = std::ptr::null_mut();
            let got = unsafe { (a.snd_seq_event_input)(self.client.seq, &mut event) };
            if got == -ENOSPC {
                // Too much came too fast, and some was lost: pieces of SysEx wouldn't add up.
                self.partial.clear();
                continue;
            }
            if got < 0 || event.is_null() {
                break;
            }
            let event = unsafe { &*event };
            if (EVENT_CLIENT_START..=EVENT_PORT_CHANGE).contains(&event.kind) {
                changed = true;
            } else {
                self.message(event);
            }
        }
        if changed {
            self.refresh();
        }
    }

    /// Devices come and go, and may have gone with an input that was open.
    fn refresh(&mut self) {
        let table = unsafe { self.client.ports(&self.site) };
        let same = table.len() == self.table.len() && table.iter().zip(&self.table).all(|(a, b)| a.port == b.port && a.address == b.address);
        if same {
            return;
        }
        let before = std::mem::replace(&mut self.table, table);
        // An open input that's where it was stays connected. One that's back, or moved,
        // needs connecting; one that's gone stays wanted, for when it comes back.
        for id in self.opened.clone() {
            let was = before.iter().find(|k| k.port.id == id && k.port.kind == "input").map(|k| k.address);
            if self.address(&id, "input").is_some_and(|now| Some(now) != was) {
                self.connect(&id);
            }
        }
        (self.on_event)(Event::Ports { ports: self.table.iter().map(|k| k.port.clone()).collect() });
    }

    fn message(&mut self, event: &SeqEvent) {
        let a = self.client.alsa;
        let source = (event.source[0] as c_int, event.source[1] as c_int);
        let Some(known) = self.table.iter().find(|k| k.address == source && k.port.kind == "input") else { return };
        if !self.opened.contains(&known.port.id) {
            return;
        }
        let port = known.port.id.clone();
        let mut bytes = vec![0u8; 512];
        let mut size;
        loop {
            unsafe { (a.snd_midi_event_reset_decode)(self.client.decoder) };
            size = unsafe { (a.snd_midi_event_decode)(self.client.decoder, bytes.as_mut_ptr(), bytes.len() as c_long, event) };
            // (Longer than it had room for: SysEx.)
            if size == -(ENOMEM as c_long) && bytes.len() < MAX_SYSEX {
                bytes.resize(bytes.len() * 2, 0);
                continue;
            }
            break;
        }
        if size <= 0 {
            return;
        }
        bytes.truncate(size as usize);
        for message in join_sysex(&mut self.partial, event.source, bytes, self.allow_sysex) {
            (self.on_event)(Event::Message { port: port.clone(), data: message });
        }
    }
}

const ENOMEM: c_int = 12;

/// Puts SysEx that comes in pieces back together. Gives what's whole now.
fn join_sysex(partial: &mut HashMap<[u8; 2], Vec<u8>>, source: [u8; 2], bytes: Vec<u8>, allowed: bool) -> Vec<Vec<u8>> {
    let under_way = partial.contains_key(&source);
    if !under_way && bytes.first() != Some(&0xF0) {
        return vec![bytes];
    }
    // Realtime messages can come in the middle of it, and stay messages of their own.
    if under_way && bytes.len() == 1 && bytes[0] >= 0xF8 {
        return vec![bytes];
    }
    let buffer = partial.entry(source).or_default();
    // Another message before the end: what's under way isn't going to end.
    if under_way && bytes.first().is_some_and(|b| *b >= 0x80 && *b != 0xF7 && *b != 0xF0) {
        partial.remove(&source);
        return vec![bytes];
    }
    if under_way && bytes.first() == Some(&0xF0) {
        buffer.clear();
    }
    buffer.extend_from_slice(&bytes);
    if buffer.len() > MAX_SYSEX {
        partial.remove(&source);
        return Vec::new();
    }
    if buffer.last() != Some(&0xF7) {
        return Vec::new();
    }
    let whole = partial.remove(&source).unwrap_or_default();
    if allowed {
        vec![whole]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::time::Duration;

    #[test]
    fn whole_messages_only() {
        for good in [&[0x90u8, 60, 127][..], &[0xC0, 5], &[0xF8], &[0xFE], &[0x90, 60, 127, 0x80, 60, 0], &[0xF2, 1, 2], &[0xF1, 3], &[0xF6], &[]] {
            assert!(valid_messages(good, false), "{good:02x?}");
        }
        for bad in [&[60u8][..], &[0x90, 60], &[0x90, 60, 200], &[0xF4], &[0xF5], &[0xF7], &[0xC0], &[0x90, 60, 127, 5]] {
            assert!(!valid_messages(bad, true), "{bad:02x?}");
        }
    }

    #[test]
    fn sysex_only_when_allowed() {
        let sysex = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];
        assert!(valid_messages(&sysex, true));
        assert!(!valid_messages(&sysex, false));
        assert!(!valid_messages(&[0x90, 60, 1, 0xF0, 1, 0xF7], false), "hidden behind another message");
        assert!(!valid_messages(&[0xF0, 1, 2], true), "without an end");
        assert!(!valid_messages(&[0xF0, 1, 0x90, 2, 0xF7], true), "another message inside");
        assert!(valid_messages(&[0xF0, 1, 0xF8, 2, 0xF7], true), "realtime inside");
    }

    #[test]
    fn ids_stay_the_same_for_a_site_only() {
        assert_eq!(port_id("a.org", "input", "Synth", 0), port_id("a.org", "input", "Synth", 0));
        assert_ne!(port_id("a.org", "input", "Synth", 0), port_id("b.org", "input", "Synth", 0));
        assert_ne!(port_id("a.org", "input", "Synth", 0), port_id("a.org", "output", "Synth", 0));
        assert_ne!(port_id("a.org", "input", "Synth", 0), port_id("a.org", "input", "Synth", 1));
    }

    #[test]
    fn sysex_is_put_back_together() {
        let mut partial = HashMap::new();
        let from = [20, 0];
        assert!(join_sysex(&mut partial, from, vec![0xF0, 1, 2], true).is_empty());
        assert_eq!(join_sysex(&mut partial, from, vec![0xF8], true), [vec![0xF8]], "realtime goes through");
        assert_eq!(join_sysex(&mut partial, from, vec![3, 0xF7], true), [vec![0xF0, 1, 2, 3, 0xF7]]);
        assert_eq!(join_sysex(&mut partial, from, vec![0x90, 60, 1], true), [vec![0x90, 60, 1]], "and on to the next");
        assert!(partial.is_empty());
    }

    #[test]
    fn sysex_is_dropped_without_permission_but_the_rest_goes_on() {
        let mut partial = HashMap::new();
        let from = [20, 0];
        assert!(join_sysex(&mut partial, from, vec![0xF0, 1, 0xF7], false).is_empty());
        assert_eq!(join_sysex(&mut partial, from, vec![0x90, 60, 1], false), [vec![0x90, 60, 1]]);
    }

    #[test]
    fn unfinished_sysex_is_given_up_on() {
        let mut partial = HashMap::new();
        let from = [20, 0];
        assert!(join_sysex(&mut partial, from, vec![0xF0, 1], true).is_empty());
        assert_eq!(join_sysex(&mut partial, from, vec![0x90, 60, 1], true), [vec![0x90, 60, 1]]);
        assert!(partial.is_empty());
        assert!(join_sysex(&mut partial, from, vec![0xF0], true).is_empty());
        assert!(join_sysex(&mut partial, from, vec![0xF0, 2], true).is_empty(), "started over");
        assert_eq!(join_sysex(&mut partial, from, vec![0xF7], true), [vec![0xF0, 2, 0xF7]]);
    }

    #[test]
    fn events_are_alsas_size() {
        assert_eq!(std::mem::size_of::<SeqEvent>(), 28);
    }

    /// A device, for the tests: another client of the sequencer, with a port each way.
    struct Device {
        client: Client,
        port: c_int,
    }

    impl Device {
        fn new(name: &str) -> Option<Device> {
            let alsa = alsa()?;
            let client = unsafe { Client::open(alsa).ok()? };
            let name = CString::new(name).unwrap();
            let caps = CAP_READ | CAP_SUBS_READ | CAP_WRITE | CAP_SUBS_WRITE;
            let port = unsafe { (alsa.snd_seq_create_simple_port)(client.seq, name.as_ptr(), caps, TYPE_MIDI_GENERIC) };
            (port >= 0).then_some(Device { client, port })
        }

        /// To whoever's connected to it, as a device would.
        fn play(&self, data: &[u8]) {
            let a = self.client.alsa;
            unsafe {
                (a.snd_midi_event_reset_encode)(self.client.encoder);
                let mut rest = data;
                while !rest.is_empty() {
                    let mut event = SeqEvent::default();
                    let used = (a.snd_midi_event_encode)(self.client.encoder, rest.as_ptr(), rest.len() as c_long, &mut event);
                    assert!(used > 0);
                    rest = &rest[used as usize..];
                    if event.kind == EVENT_NONE {
                        continue;
                    }
                    event.queue = QUEUE_DIRECT;
                    event.source = [self.client.id as u8, self.port as u8];
                    event.dest = [254, 253];
                    assert!((a.snd_seq_event_output_direct)(self.client.seq, &mut event) >= 0);
                }
            }
        }

        /// What arrived at it, until nothing has for a moment.
        fn heard(&self) -> Vec<u8> {
            let a = self.client.alsa;
            let mut all = Vec::new();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut quiet = 0;
            while std::time::Instant::now() < deadline && quiet < 20 {
                let mut event = std::ptr::null_mut();
                let got = unsafe { (a.snd_seq_event_input)(self.client.seq, &mut event) };
                if got < 0 || event.is_null() {
                    quiet += 1;
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                quiet = 0;
                let event = unsafe { &*event };
                if (EVENT_CLIENT_START..=67).contains(&event.kind) {
                    continue;
                }
                let mut bytes = vec![0u8; 4096];
                unsafe { (a.snd_midi_event_reset_decode)(self.client.decoder) };
                let size = unsafe { (a.snd_midi_event_decode)(self.client.decoder, bytes.as_mut_ptr(), bytes.len() as c_long, event) };
                if size > 0 {
                    all.extend_from_slice(&bytes[..size as usize]);
                }
            }
            all
        }
    }

    type Events = Receiver<Event>;

    /// A session and a device, or None without a sequencer on this machine.
    fn setup(name: &str, sysex: bool) -> Option<(Session, Vec<Port>, Device, Events)> {
        let device = Device::new(name)?;
        let (sent, events) = channel();
        let (session, ports) = Session::open("test.example", sysex, move |e| {
            let _ = sent.send(e);
        })
        .ok()?;
        Some((session, ports, device, events))
    }

    fn id_of(ports: &[Port], name: &str, kind: &str) -> String {
        ports.iter().find(|p| p.name == name && p.kind == kind).unwrap_or_else(|| panic!("{name} {kind} in {ports:?}")).id.clone()
    }

    // (Tests run side by side, and each other's devices come and go.)
    fn next_message_within(events: &Events, time: Duration) -> Option<Vec<u8>> {
        let deadline = std::time::Instant::now() + time;
        loop {
            match events.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).ok()? {
                Event::Message { data, .. } => return Some(data),
                Event::Ports { .. } => {}
            }
        }
    }

    fn next_message(events: &Events) -> Option<Vec<u8>> {
        next_message_within(events, Duration::from_secs(2))
    }

    #[test]
    fn real_messages_both_ways() {
        let Some((session, ports, device, events)) = setup("Kite Test One", false) else {
            eprintln!("no ALSA sequencer here: skipped");
            return;
        };
        session.send(&id_of(&ports, "Kite Test One", "output"), vec![0x90, 60, 100, 0x80, 60, 0]);
        assert_eq!(device.heard(), [0x90, 60, 100, 0x80, 60, 0]);

        let input = id_of(&ports, "Kite Test One", "input");
        device.play(&[0x90, 61, 1]);
        assert_eq!(next_message_within(&events, Duration::from_millis(300)), None, "not open yet");
        session.open_input(&input);
        std::thread::sleep(Duration::from_millis(100));
        device.play(&[0x90, 61, 1]);
        assert_eq!(next_message(&events), Some(vec![0x90, 61, 1]));
        session.close_input(&input);
        std::thread::sleep(Duration::from_millis(100));
        device.play(&[0x90, 62, 1]);
        assert_eq!(next_message_within(&events, Duration::from_millis(300)), None, "closed again");
    }

    #[test]
    fn real_sysex_needs_the_permission() {
        let Some((session, ports, device, events)) = setup("Kite Test Two", false) else { return };
        let (output, input) = (id_of(&ports, "Kite Test Two", "output"), id_of(&ports, "Kite Test Two", "input"));
        session.open_input(&input);
        std::thread::sleep(Duration::from_millis(100));

        let sysex = vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];
        session.send(&output, sysex.clone());
        assert!(device.heard().is_empty(), "not sent");
        device.play(&sysex);
        device.play(&[0x90, 63, 1]);
        assert_eq!(next_message(&events), Some(vec![0x90, 63, 1]), "not received, and the next one is");

        session.allow_sysex();
        session.send(&output, sysex.clone());
        assert_eq!(device.heard(), sysex);
        device.play(&sysex);
        assert_eq!(next_message(&events), Some(sysex));
    }

    #[test]
    fn long_sysex_arrives_whole() {
        let Some((session, ports, device, events)) = setup("Kite Test Three", true) else { return };
        let input = id_of(&ports, "Kite Test Three", "input");
        session.open_input(&input);
        std::thread::sleep(Duration::from_millis(100));
        let mut long = vec![0xF0];
        long.extend((0..3000).map(|i| (i % 128) as u8));
        long.push(0xF7);
        device.play(&long);
        assert_eq!(next_message(&events), Some(long));
    }

    #[test]
    fn devices_coming_and_going_are_noticed() {
        let Some((_session, ports, _device, events)) = setup("Kite Test Four", false) else { return };
        assert!(!ports.iter().any(|p| p.name == "Kite Test Later"));
        let later = Device::new("Kite Test Later").expect("a sequencer");
        let appeared = (0..20).find_map(|_| match events.recv_timeout(Duration::from_secs(1)).ok()? {
            Event::Ports { ports } if ports.iter().any(|p| p.name == "Kite Test Later") => Some(ports),
            _ => None,
        });
        let appeared = appeared.expect("it showed up");
        let id = id_of(&appeared, "Kite Test Later", "input");
        drop(later);
        let gone = (0..20).find_map(|_| match events.recv_timeout(Duration::from_secs(1)).ok()? {
            Event::Ports { ports } if !ports.iter().any(|p| p.id == id) => Some(()),
            _ => None,
        });
        assert!(gone.is_some(), "it went away");
    }

    #[test]
    fn another_device_arriving_leaves_open_inputs_connected() {
        let Some((session, ports, device, events)) = setup("Kite Test Six", false) else { return };
        session.open_input(&id_of(&ports, "Kite Test Six", "input"));
        std::thread::sleep(Duration::from_millis(100));
        let _another = Device::new("Kite Test Six Other").expect("a sequencer");
        std::thread::sleep(Duration::from_millis(200));
        device.play(&[0x90, 64, 1]);
        assert_eq!(next_message(&events), Some(vec![0x90, 64, 1]));
    }

    #[test]
    fn a_session_shows_neither_itself_nor_other_pages() {
        let Some((_one, _, _device, _events)) = setup("Kite Test Five", false) else { return };
        let (_two, ports) = Session::open("other.example", false, |_| {}).unwrap();
        assert!(!ports.iter().any(|p| p.name.starts_with("Kite in") || p.name.starts_with("Kite out")), "{ports:?}");
    }
}
