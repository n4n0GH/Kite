//! Starting programs outside Kite: native messaging hosts (webext/native.rs)
//! and an external download manager (Settings → Downloads, like Falkon's).

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A command for `program` with what Kite set for itself taken out of its
/// environment (the AppImage's library and Qt paths in particular would break
/// a program using its own Qt), and Kite's open files not handed down.
pub fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    for var in ["QTWEBENGINE_CHROMIUM_FLAGS", "QT_PLUGIN_PATH", "QML_IMPORT_PATH", "QML2_IMPORT_PATH", "QTWEBENGINEPROCESS_PATH", "QTWEBENGINE_RESOURCES_PATH", "QTWEBENGINE_LOCALES_PATH", "KITE_DATA_DIR"] {
        c.env_remove(var);
    }
    if std::env::var_os("APPIMAGE").is_some() {
        for var in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "APPIMAGE", "ARGV0", "OWD"] {
            c.env_remove(var);
        }
    }
    // Nor is what Kite has open theirs, such as the profile's files: Chromium doesn't
    // mark all of it to close when a program starts. (Marked rather than closed
    // here: that a program failed to start comes back through one of them.)
    unsafe {
        c.pre_exec(|| {
            mark_close_on_exec();
            Ok(())
        });
    }
    c
}

/// Runs between fork and exec, so only async-signal-safe calls.
fn mark_close_on_exec() {
    const CLOSE_RANGE_CLOEXEC: libc::c_uint = 1 << 2;
    unsafe {
        if libc::syscall(libc::SYS_close_range, 3 as libc::c_uint, libc::c_uint::MAX, CLOSE_RANGE_CLOEXEC) == 0 {
            return;
        }
        // (Kernels before 5.11.)
        let max = libc::sysconf(libc::_SC_OPEN_MAX);
        for fd in 3..(if max > 0 { max.min(65536) } else { 1024 }) as libc::c_int {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
}

/// A program by its path, or its name looked up on PATH.
pub fn find_program(program: &str) -> Option<PathBuf> {
    let program = program.trim();
    if program.is_empty() {
        return None;
    }
    let runnable = |p: &Path| {
        use std::os::unix::fs::PermissionsExt;
        p.is_file() && p.metadata().is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        let p = crate::util::local_path(program);
        let p = match p.strip_prefix("~") {
            Ok(rest) => directories::BaseDirs::new().map(|d| d.home_dir().join(rest)).unwrap_or(p),
            Err(_) => p,
        };
        return runnable(&p).then_some(p);
    }
    std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|d| d.join(program)).find(|p| runnable(p)))
}

/// Words of an argument line, split as a shell would (quotes and backslashes),
/// but never run through one: nothing in it is a command.
pub fn split_arguments(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => word.push(c),
                        None => return Err("a quote (') isn't closed".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(c @ ('"' | '\\' | '$' | '`')) => word.push(c),
                            Some(c) => {
                                word.push('\\');
                                word.push(c)
                            }
                            None => return Err("a quote (\") isn't closed".into()),
                        },
                        Some(c) => word.push(c),
                        None => return Err("a quote (\") isn't closed".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(c) = chars.next() {
                    word.push(c);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                word.push(c);
            }
        }
    }
    if in_word {
        words.push(word);
    }
    Ok(words)
}

/// The external download manager's arguments for a download: `%d` the
/// address, `%f` the file's suggested name, `%%` a percent sign. Without `%d`
/// anywhere, the address goes last.
pub fn download_arguments(line: &str, url: &str, file_name: &str) -> Result<Vec<String>, String> {
    let mut has_url = false;
    let mut out = Vec::new();
    for word in split_arguments(line)? {
        let mut arg = String::new();
        let mut chars = word.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                arg.push(c);
                continue;
            }
            match chars.peek() {
                Some('d') => {
                    chars.next();
                    arg.push_str(url);
                    has_url = true;
                }
                Some('f') => {
                    chars.next();
                    arg.push_str(file_name);
                }
                Some('%') => {
                    chars.next();
                    arg.push('%');
                }
                _ => arg.push('%'),
            }
        }
        out.push(arg);
    }
    if !has_url {
        out.push(url.to_owned());
    }
    Ok(out)
}

/// Hand a download to the external download manager. It runs on its own
/// (its own process group: it outlives Kite), with no input or output.
pub fn start_download_manager(program: &str, arguments: &str, url: &str, file_name: &str) -> Result<String, String> {
    let path = find_program(program).ok_or_else(|| if program.trim().is_empty() { "no program is set".to_owned() } else { format!("“{}” wasn't found or can't be run", program.trim()) })?;
    let args = download_arguments(arguments, url, file_name)?;
    let mut c = command(&path);
    c.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
    if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_owned()) {
        c.current_dir(home);
    }
    let mut child = c.spawn().map_err(|e| format!("{} couldn't be started: {e}", path.display()))?;
    // (Reaped once it ends, so it doesn't linger as a zombie.)
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_split_like_a_shell_without_one() {
        assert_eq!(split_arguments(r#"-c "a b" 'c d' e\ f  g"#).unwrap(), ["-c", "a b", "c d", "e f", "g"]);
        assert_eq!(split_arguments(r#""say \"hi\"" ''"#).unwrap(), ["say \"hi\"", ""]);
        assert_eq!(split_arguments("  ").unwrap(), Vec::<String>::new());
        assert!(split_arguments("'open").is_err());
        assert!(split_arguments("\"open").is_err());
        // Shell syntax is only text.
        assert_eq!(split_arguments("$(rm -rf ~); ls | cat").unwrap(), ["$(rm", "-rf", "~);", "ls", "|", "cat"]);
    }

    #[test]
    fn placeholders_are_filled_and_the_address_kept_whole() {
        let url = "https://example.org/a b;$(x).iso?q='1'";
        assert_eq!(download_arguments("%d", url, "a.iso").unwrap(), [url]);
        assert_eq!(download_arguments("-o %f --dir=~/Downloads %d", url, "a b.iso").unwrap(), ["-o", "a b.iso", "--dir=~/Downloads", url]);
        assert_eq!(download_arguments("--add", url, "a.iso").unwrap(), ["--add", url], "the address goes last without %d");
        assert_eq!(download_arguments("", url, "a.iso").unwrap(), [url]);
        assert_eq!(download_arguments("100%% %x %d", "u", "f").unwrap(), ["100%", "%x", "u"]);
    }

    #[test]
    fn programs_are_found_by_path_or_name() {
        assert!(find_program("sh").is_some_and(|p| p.is_absolute()));
        assert!(find_program("/bin/sh").is_some());
        assert!(find_program("").is_none());
        assert!(find_program("no-such-program-kite").is_none());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-runnable");
        std::fs::write(&file, "x").unwrap();
        assert!(find_program(file.to_str().unwrap()).is_none(), "not executable");
    }

    #[test]
    fn a_manager_gets_the_address_as_one_argument() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("args");
        let script = dir.path().join("manager");
        std::fs::write(&script, format!("#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\n", out.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let url = "https://example.org/a b;$(touch pwned).iso";
        let name = start_download_manager(script.to_str().unwrap(), "--name %f %d", url, "a b.iso").unwrap();
        assert_eq!(name, "manager");
        for _ in 0..100 {
            if out.exists() && std::fs::read_to_string(&out).unwrap().lines().count() == 3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_to_string(&out).unwrap(), format!("--name\na b.iso\n{url}\n"));
        assert!(start_download_manager("no-such-program-kite", "%d", url, "f").unwrap_err().contains("wasn't found"));
        assert!(start_download_manager("", "%d", url, "f").unwrap_err().contains("no program"));
    }
}
