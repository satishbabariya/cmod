use std::fmt;
use std::io::Write;
use std::sync::Mutex;

use anstream::{AutoStream, ColorChoice};
use anstyle::{AnsiColor, Effects, Style};

// -- Style constants (Cargo-compatible) --

/// Green bold — used for most status labels (Compiling, Linking, Finished, etc.)
pub const HEADER: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);

/// Red bold — used for error messages.
pub const ERROR: Style = AnsiColor::Red.on_default().effects(Effects::BOLD);

/// Yellow bold — used for warnings.
pub const WARN: Style = AnsiColor::Yellow.on_default().effects(Effects::BOLD);

/// Cyan bold — used for notes and informational messages.
pub const NOTE: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);

// -- Verbosity --

/// Three-level verbosity: Quiet suppresses status, Normal is default, Verbose adds detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Suppress all status messages (errors still shown).
    Quiet,
    /// Default level — show key status messages.
    Normal,
    /// Show extra detail (per-file progress, cache hits, timings).
    Verbose,
}

// -- Shell --

/// Centralized output abstraction for colored, aligned status messages.
///
/// Mirrors Cargo's `Shell`: all status output goes to stderr with 12-character
/// right-aligned labels. Color is applied only to the label, not the message.
pub struct Shell {
    output: Mutex<ShellOut>,
    verbosity: Verbosity,
}

enum ShellOut {
    Stream(AutoStream<std::io::Stderr>),
    Write(Box<dyn Write + Send>),
}

impl Shell {
    /// Create a new Shell that writes colored output to stderr.
    pub fn new(verbosity: Verbosity) -> Self {
        Shell {
            output: Mutex::new(ShellOut::Stream(AutoStream::new(
                std::io::stderr(),
                ColorChoice::Auto,
            ))),
            verbosity,
        }
    }

    /// Create a Shell that captures output to a buffer (for tests).
    pub fn from_write(writer: Box<dyn Write + Send>, verbosity: Verbosity) -> Self {
        Shell {
            output: Mutex::new(ShellOut::Write(writer)),
            verbosity,
        }
    }

    /// Current verbosity level.
    pub fn verbosity(&self) -> Verbosity {
        self.verbosity
    }

    /// Returns true if the shell is in verbose mode.
    pub fn is_verbose(&self) -> bool {
        self.verbosity == Verbosity::Verbose
    }

    /// Print a right-aligned green status label with a message.
    ///
    /// Produces output like:
    /// ```text
    ///    Compiling my_math (debug)
    /// ```
    ///
    /// Suppressed in Quiet mode.
    pub fn status(&self, label: &str, message: impl fmt::Display) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        self.write_status(label, &message, &HEADER);
    }

    /// Print a right-aligned status label with a custom color.
    ///
    /// Suppressed in Quiet mode.
    pub fn status_with_color(&self, label: &str, message: impl fmt::Display, style: &Style) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        self.write_status(label, &message, style);
    }

    /// Print a status message only in Verbose mode.
    pub fn verbose(&self, label: &str, message: impl fmt::Display) {
        if self.verbosity != Verbosity::Verbose {
            return;
        }
        self.write_status(label, &message, &HEADER);
    }

    /// Print an error message. Always shown (even in Quiet mode).
    pub fn error(&self, message: impl fmt::Display) {
        self.write_prefixed("error", &message, &ERROR);
    }

    /// Print a warning message. Suppressed in Quiet mode.
    pub fn warn(&self, message: impl fmt::Display) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        self.write_prefixed("warning", &message, &WARN);
    }

    /// Print a note/hint message. Suppressed in Quiet mode.
    pub fn note(&self, message: impl fmt::Display) {
        if self.verbosity == Verbosity::Quiet {
            return;
        }
        self.write_prefixed("note", &message, &NOTE);
    }

    // -- Internal helpers --

    /// Write a 12-char right-aligned, colored status label followed by a message.
    fn write_status(&self, label: &str, message: &dyn fmt::Display, style: &Style) {
        let mut buf = Vec::new();
        let _ = writeln!(buf, "{style}{label:>12}{style:#} {message}");
        self.write_all(&buf);
    }

    /// Write a left-aligned "prefix: message" line (for error/warning/note).
    fn write_prefixed(&self, prefix: &str, message: &dyn fmt::Display, style: &Style) {
        let mut buf = Vec::new();
        let _ = writeln!(buf, "{style}{prefix}{style:#}: {message}");
        self.write_all(&buf);
    }

    fn write_all(&self, buf: &[u8]) {
        if let Ok(mut out) = self.output.lock() {
            let _ = match *out {
                ShellOut::Stream(ref mut s) => s.write_all(buf),
                ShellOut::Write(ref mut w) => w.write_all(buf),
            };
        }
    }
}

// -- Utility --

/// Format a byte count as a human-readable string (e.g., "1.5 MB").
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    /// Helper to capture shell output for assertions.
    fn capture_shell(verbosity: Verbosity) -> (Shell, Arc<StdMutex<Vec<u8>>>) {
        let buf = Arc::new(StdMutex::new(Vec::new()));
        let writer = {
            let buf = Arc::clone(&buf);
            Box::new(SharedWriter(buf)) as Box<dyn Write + Send>
        };
        (Shell::from_write(writer, verbosity), buf)
    }

    struct SharedWriter(Arc<StdMutex<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(data)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn output_string(buf: &Arc<StdMutex<Vec<u8>>>) -> String {
        // Strip ANSI escape sequences for easier assertions
        let raw = buf.lock().unwrap().clone();
        let s = String::from_utf8(raw).unwrap();
        strip_ansi(&s)
    }

    fn strip_ansi(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until 'm' (SGR terminator)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    #[test]
    fn test_status_alignment() {
        let (shell, buf) = capture_shell(Verbosity::Normal);
        shell.status("Compiling", "my_math (debug)");
        let out = output_string(&buf);
        assert!(out.contains("   Compiling my_math (debug)"));
    }

    #[test]
    fn test_quiet_suppresses_status() {
        let buf = Arc::new(StdMutex::new(Vec::new()));
        let writer = Box::new(SharedWriter(Arc::clone(&buf))) as Box<dyn Write + Send>;
        let shell = Shell::from_write(writer, Verbosity::Quiet);
        shell.status("Compiling", "foo");
        assert!(buf.lock().unwrap().is_empty());
    }

    #[test]
    fn test_quiet_shows_errors() {
        let buf = Arc::new(StdMutex::new(Vec::new()));
        let writer = Box::new(SharedWriter(Arc::clone(&buf))) as Box<dyn Write + Send>;
        let shell = Shell::from_write(writer, Verbosity::Quiet);
        shell.error("build failed");
        let out = output_string(&buf);
        assert!(out.contains("error: build failed"));
    }

    #[test]
    fn test_verbose_only_in_verbose_mode() {
        let buf = Arc::new(StdMutex::new(Vec::new()));
        let writer = Box::new(SharedWriter(Arc::clone(&buf))) as Box<dyn Write + Send>;
        let shell = Shell::from_write(writer, Verbosity::Normal);
        shell.verbose("Fresh", "foo.cppm");
        assert!(buf.lock().unwrap().is_empty());
    }

    #[test]
    fn test_verbose_shown_when_verbose() {
        let buf = Arc::new(StdMutex::new(Vec::new()));
        let writer = Box::new(SharedWriter(Arc::clone(&buf))) as Box<dyn Write + Send>;
        let shell = Shell::from_write(writer, Verbosity::Verbose);
        shell.verbose("Fresh", "foo.cppm");
        let out = output_string(&buf);
        assert!(out.contains("Fresh foo.cppm"));
    }

    #[test]
    fn test_error_format() {
        let (shell, buf) = capture_shell(Verbosity::Normal);
        shell.error("something went wrong");
        let out = output_string(&buf);
        assert!(out.contains("error: something went wrong"));
    }

    #[test]
    fn test_warn_format() {
        let (shell, buf) = capture_shell(Verbosity::Normal);
        shell.warn("deprecated feature");
        let out = output_string(&buf);
        assert!(out.contains("warning: deprecated feature"));
    }

    #[test]
    fn test_note_format() {
        let (shell, buf) = capture_shell(Verbosity::Normal);
        shell.note("run `cmod resolve`");
        let out = output_string(&buf);
        assert!(out.contains("note: run `cmod resolve`"));
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }
}
