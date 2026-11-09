//! The log file and crash handling (kite_core::crashlog has the files).
//!
//! * Log lines go to stderr as before and to the profile's `kite.log`; lines
//!   logged before the profile is known are kept until it is.
//! * A Rust panic writes `crash.txt` with a backtrace.
//! * A native crash (SIGSEGV, SIGABRT, … in Kite's own process, so Qt or
//!   QtWebEngine's browser side too) writes what it can from the signal
//!   handler: only async-signal-safe calls there. The signal then goes on to
//!   whoever handled it before (in the end the system's core dump).
//! * At the next start that becomes `last-crash.txt` (with the system's view
//!   of a native crash from coredumpctl, where there is one), shown in a dialog.

use kite_core::crashlog::{self, LogFile};
use std::ffi::CString;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Page and extension console lines per second written to the file; more are counted.
const CONSOLE_LINES_PER_SECOND: u32 = 50;
/// Lines kept from before the profile is known.
const EARLY_LINES: usize = 500;

static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
/// `crash.txt`, ready for the signal handler (no allocating there).
static CRASH_PATH: OnceLock<CString> = OnceLock::new();
/// The last run left a report not yet shown.
static NEW_REPORT: AtomicBool = AtomicBool::new(false);
/// A `--private` Kite: every address in the log is scrambled.
static PRIVATE: AtomicBool = AtomicBool::new(false);

struct Sink {
    file: Option<LogFile>,
    early: Vec<String>,
    window: Instant,
    console_lines: u32,
    dropped: u32,
}

struct Logger {
    inner: env_logger::Logger,
    sink: Mutex<Sink>,
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        if !self.inner.matches(record) {
            return;
        }
        let message = record.args().to_string();
        let message = if PRIVATE.load(Relaxed) { crashlog::scramble_urls(&message) } else { message };
        self.inner.log(&log::Record::builder().args(format_args!("{message}")).level(record.level()).target(record.target()).module_path(record.module_path()).file(record.file()).line(record.line()).build());
        let line = format!("{} {:<5} {}: {message}", kite_core::util::local_time_fmt(std::time::SystemTime::now(), false, true), record.level(), record.target());
        let mut sink = self.sink.lock().unwrap_or_else(|e| e.into_inner());
        if record.target().starts_with("console::") {
            if sink.window.elapsed().as_secs() >= 1 {
                if sink.dropped > 0 {
                    let note = format!("({} page console lines left out)", sink.dropped);
                    sink.write(&note);
                }
                sink.window = Instant::now();
                sink.console_lines = 0;
                sink.dropped = 0;
            }
            sink.console_lines += 1;
            if sink.console_lines > CONSOLE_LINES_PER_SECOND {
                sink.dropped += 1;
                return;
            }
        }
        sink.write(&line);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

impl Sink {
    fn write(&mut self, line: &str) {
        match &mut self.file {
            Some(f) => f.write_line(line),
            None if self.early.len() < EARLY_LINES => self.early.push(line.to_owned()),
            None => {}
        }
    }
}

static LOGGER: OnceLock<&'static Logger> = OnceLock::new();

/// Logging to stderr from the start (RUST_LOG, as before); the file follows
/// once `start` knows the profile.
pub fn init_logging() {
    let inner = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,console::page=error")).build();
    let filter = inner.filter();
    let logger: &'static Logger = Box::leak(Box::new(Logger { inner, sink: Mutex::new(Sink { file: None, early: Vec::new(), window: Instant::now(), console_lines: 0, dropped: 0 }) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(filter);
        let _ = LOGGER.set(logger);
    }
}

/// The profile is known: open its log (turning a crash of the last run into its
/// report) and catch crashes from here on.
pub fn start(profile: &str, private: bool) {
    PRIVATE.store(private, Relaxed);
    let Ok(dir) = crashlog::dir(profile, private) else { return };
    let opened = match LogFile::open(&dir) {
        Ok(o) => o,
        Err(e) => {
            log::warn!("no log file in {}: {e}", dir.display());
            return;
        }
    };
    if let Some(logger) = LOGGER.get() {
        let mut sink = logger.sink.lock().unwrap_or_else(|e| e.into_inner());
        let mut file = opened.log;
        file.write_line(&format!("Kite {} with {}, started {}", env!("CARGO_PKG_VERSION"), crate::ffi::engine_version(), kite_core::util::local_time(std::time::SystemTime::now())));
        for line in sink.early.drain(..) {
            file.write_line(&line);
        }
        sink.file = Some(file);
    }
    let _ = CRASH_PATH.set(CString::new(dir.join(crashlog::CRASH).into_os_string().into_encoded_bytes()).unwrap_or_default());
    NEW_REPORT.store(opened.new_report, Relaxed);
    if opened.new_report {
        add_system_view(&dir);
    }
    let _ = DIR.set(dir);
    install_panic_hook();
    install_signal_handlers();
}

/// For a native crash, what systemd-coredump made of it (a backtrace with
/// names, where the core was kept), in the background: it can take a moment.
fn add_system_view(dir: &std::path::Path) {
    let Some(report) = crashlog::report(dir) else { return };
    let Some(pid) = crashlog::crashed_pid(&report).filter(|_| crashlog::is_native(&report)) else { return };
    let dir = dir.to_owned();
    std::thread::spawn(move || {
        let out = std::process::Command::new("coredumpctl").args(["info", "--no-pager", &pid.to_string()]).env("SYSTEMD_COLORS", "0").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output();
        let text = match out {
            Ok(o) if o.status.success() && !o.stdout.is_empty() => {
                let t = String::from_utf8_lossy(&o.stdout);
                t.lines().take(400).collect::<Vec<_>>().join("\n")
            }
            Ok(_) => "No core dump of it was found.".to_owned(),
            Err(_) => "coredumpctl isn't available on this system.".to_owned(),
        };
        let _ = crashlog::append_to_report(&dir, &format!("coredumpctl info {pid}"), &text);
    });
}

/// Whether the last run's crash report is new (to show it once).
pub fn take_new_report() -> bool {
    NEW_REPORT.swap(false, Relaxed)
}

pub fn report() -> Option<String> {
    DIR.get().and_then(|d| crashlog::report(d))
}

pub fn dir() -> Option<&'static std::path::Path> {
    DIR.get().map(|d| d.as_path())
}

/// Kite closed normally: a panic it survived (one of its threads) stays reported, saying so.
pub fn clean_exit() {
    let Some(path) = DIR.get().map(|d| d.join(crashlog::CRASH)) else { return };
    if path.exists() {
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&path) {
            let _ = f.write_all(b"\n(Kite went on running after this and was closed normally.)\n");
        }
    }
}

// ---- Rust panics ---------------------------------------------------------------

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = DIR.get() {
            let message = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
            let thread = std::thread::current();
            let text = format!(
                "Kite {} panicked\nthread: {}\nat: {}\npid: {}\nmessage: {message}\n\nBacktrace:\n{}\n",
                env!("CARGO_PKG_VERSION"),
                thread.name().unwrap_or("unnamed"),
                info.location().map(|l| l.to_string()).unwrap_or_default(),
                std::process::id(),
                std::backtrace::Backtrace::force_capture()
            );
            // (Appended: a panic across the C++ boundary aborts, and the signal adds to it.)
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(crashlog::CRASH)) {
                let _ = f.write_all(text.as_bytes());
            }
        }
        previous(info);
    }));
}

// ---- native crashes --------------------------------------------------------------

const SIGNALS: [(libc::c_int, &str); 5] = [(libc::SIGSEGV, "SIGSEGV"), (libc::SIGBUS, "SIGBUS"), (libc::SIGILL, "SIGILL"), (libc::SIGFPE, "SIGFPE"), (libc::SIGABRT, "SIGABRT")];
/// What handled each signal before Kite (std's stack overflow check, the default…).
static mut PREVIOUS: [Option<libc::sigaction>; 32] = [None; 32];
static IN_HANDLER: AtomicBool = AtomicBool::new(false);

extern "C" {
    // glibc's <execinfo.h>.
    fn backtrace(buffer: *mut *mut libc::c_void, size: libc::c_int) -> libc::c_int;
    fn backtrace_symbols_fd(buffer: *const *mut libc::c_void, size: libc::c_int, fd: libc::c_int);
}

/// Install the handlers, or again if something replaced them since (QtWebEngine
/// starting Chromium): they then pass the signal on to that.
pub fn install_signal_handlers() {
    if CRASH_PATH.get().is_none() {
        return;
    }
    static ALT_STACK: std::sync::Once = std::sync::Once::new();
    ALT_STACK.call_once(|| unsafe {
        // A stack of its own, for a crash from running out of stack.
        let size = 64 * 1024;
        let stack = libc::stack_t { ss_sp: Box::leak(vec![0u8; size].into_boxed_slice()).as_mut_ptr().cast(), ss_flags: 0, ss_size: size };
        libc::sigaltstack(&stack, std::ptr::null_mut());
        // backtrace() loads libgcc on first use, which allocates: not in a crash.
        let mut frames = [std::ptr::null_mut(); 4];
        backtrace(frames.as_mut_ptr(), 4);
    });
    for (sig, name) in SIGNALS {
        unsafe {
            let mut current: libc::sigaction = std::mem::zeroed();
            libc::sigaction(sig, std::ptr::null(), &mut current);
            if current.sa_sigaction == on_signal as *const () as usize {
                continue;
            }
            if (*std::ptr::addr_of!(PREVIOUS))[sig as usize].is_some() {
                log::info!("{name} handler replaced since Kite's; Kite's goes first again");
            }
            (*std::ptr::addr_of_mut!(PREVIOUS))[sig as usize] = Some(current);
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as *const () as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
}

fn write_fd(fd: libc::c_int, bytes: &[u8]) {
    unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
}

fn write_number(fd: libc::c_int, mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    write_fd(fd, &buf[i..]);
}

extern "C" fn on_signal(sig: libc::c_int, info: *mut libc::siginfo_t, context: *mut libc::c_void) {
    // Only async-signal-safe calls from here: open, write, close, sigaction, raise
    // (and backtrace, loaded beforehand).
    if !IN_HANDLER.swap(true, Relaxed) {
        if let Some(path) = CRASH_PATH.get() {
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND | libc::O_CLOEXEC, 0o600) };
            if fd >= 0 {
                let name = SIGNALS.iter().find(|(s, _)| *s == sig).map(|(_, n)| *n).unwrap_or("?");
                write_fd(fd, concat!("Kite ", env!("CARGO_PKG_VERSION"), " crashed\nsignal: ").as_bytes());
                write_number(fd, sig as u64);
                write_fd(fd, b" (");
                write_fd(fd, name.as_bytes());
                write_fd(fd, b")\npid: ");
                write_number(fd, unsafe { libc::getpid() } as u64);
                write_fd(fd, b"\nthread: ");
                write_number(fd, unsafe { libc::gettid() } as u64);
                write_fd(fd, b"\n\nBacktrace (addresses; coredumpctl's below names them, if the system kept the core):\n");
                let mut frames = [std::ptr::null_mut(); 64];
                let n = unsafe { backtrace(frames.as_mut_ptr(), frames.len() as libc::c_int) };
                unsafe { backtrace_symbols_fd(frames.as_ptr(), n, fd) };
                unsafe { libc::close(fd) };
            }
        }
    }
    // On to the handler from before (std's stack overflow check, Chromium's…), then
    // the system's: the process ends with a core dump, whatever the one before did.
    // (Raised now, it's delivered once this returns.)
    unsafe {
        if let Some(previous) = (*std::ptr::addr_of!(PREVIOUS))[sig as usize] {
            let h = previous.sa_sigaction;
            if h != libc::SIG_DFL && h != libc::SIG_IGN && h != on_signal as *const () as usize {
                if previous.sa_flags & libc::SA_SIGINFO != 0 {
                    let f: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) = std::mem::transmute(h);
                    f(sig, info, context);
                } else {
                    let f: extern "C" fn(libc::c_int) = std::mem::transmute(h);
                    f(sig);
                }
            }
        }
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}
