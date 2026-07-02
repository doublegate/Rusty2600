//! [`ScriptLog`] — the captured `print()`/error output a host's debugger
//! console panel shows a script author.
//!
//! Replaces Lua's default `print` (which would otherwise write to the
//! process's real stdout, invisible in a GUI app). Unlike
//! [`crate::Overlay`] (drained and cleared every frame), a console
//! log is a persistent HISTORY a user scrolls back through — so this caps
//! total line count instead (dropping the oldest line once full), rather
//! than clearing on read.

use std::collections::VecDeque;

/// How many log lines [`ScriptLog`] retains before dropping the oldest.
///
/// Sized the same as this project's other capped-history buffers (e.g.
/// `EmuCore::snapshots`'s ~600-frame rewind ring) scaled down for a
/// line-oriented text log rather than a per-frame binary snapshot.
pub const MAX_LINES: usize = 500;

/// One captured log line: either a script's own `print()` output, or a
/// runtime error the host caught while ticking the script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    /// A `print(...)` call's formatted output (arguments tab-joined, no
    /// trailing newline — matching Lua's own `print` formatting).
    Print(String),
    /// A script runtime error (e.g. `onFrame` raising), distinctly marked
    /// so a console UI can render it differently from plain print output.
    Error(String),
}

impl LogLine {
    /// The line's text, without the `Print`/`Error` distinction — for a
    /// plain-text render (or a test asserting on content only).
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Print(s) | Self::Error(s) => s,
        }
    }

    /// Whether this line is an error (vs. plain `print` output).
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// A capped ring buffer of [`LogLine`]s, oldest-first.
#[derive(Debug, Clone, Default)]
pub struct ScriptLog {
    lines: VecDeque<LogLine>,
}

impl ScriptLog {
    /// Appends `line`, dropping the oldest entry first if already at
    /// [`MAX_LINES`] capacity.
    pub fn push(&mut self, line: LogLine) {
        if self.lines.len() >= MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Every captured line so far, oldest-first.
    #[must_use]
    pub const fn lines(&self) -> &VecDeque<LogLine> {
        &self.lines
    }

    /// Discards all captured lines (a console UI's "Clear" button).
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_then_clear_round_trips() {
        let mut log = ScriptLog::default();
        log.push(LogLine::Print("hello".into()));
        log.push(LogLine::Error("boom".into()));
        assert_eq!(log.lines().len(), 2);
        assert!(!log.lines()[0].is_error());
        assert!(log.lines()[1].is_error());
        assert_eq!(log.lines()[0].text(), "hello");
        log.clear();
        assert!(log.lines().is_empty());
    }

    #[test]
    fn caps_at_max_lines_dropping_oldest() {
        let mut log = ScriptLog::default();
        for i in 0..MAX_LINES + 10 {
            log.push(LogLine::Print(i.to_string()));
        }
        assert_eq!(log.lines().len(), MAX_LINES);
        // The oldest 10 (0..10) were dropped; the buffer starts at "10".
        assert_eq!(log.lines()[0].text(), "10");
        assert_eq!(
            log.lines().back().unwrap().text(),
            (MAX_LINES + 9).to_string()
        );
    }
}
