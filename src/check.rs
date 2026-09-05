//! Shared check/report primitives used by `doctor` and platform modules.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Pass,
    Warn,
    Fail,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Pass => "PASS",
            Level::Warn => "WARN",
            Level::Fail => "FAIL",
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Level::Pass => "\u{2714}", // ✔
            Level::Warn => "!",
            Level::Fail => "\u{2718}", // ✘
        }
    }

    fn ansi(self) -> &'static str {
        match self {
            Level::Pass => "\x1b[32m",
            Level::Warn => "\x1b[33m",
            Level::Fail => "\x1b[31m",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Check {
    pub title: String,
    pub level: Level,
    pub detail: String,
}

impl Check {
    pub fn pass(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            level: Level::Pass,
            detail: detail.into(),
        }
    }

    pub fn warn(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            level: Level::Warn,
            detail: detail.into(),
        }
    }

    pub fn fail(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            level: Level::Fail,
            detail: detail.into(),
        }
    }

    pub fn print(&self, color: bool) {
        let prefix = if color {
            format!("{}{}\x1b[0m", self.level.ansi(), self.level.symbol())
        } else {
            format!("[{}]", self.level.label())
        };
        let indent = if self.detail.is_empty() {
            String::new()
        } else {
            format!("  {}", self.detail)
        };
        println!("{prefix} {}", self.title);
        if !self.detail.is_empty() {
            println!("       {indent}");
        }
    }
}

pub fn summarize(checks: &[Check]) -> (usize, usize, usize) {
    let pass = checks.iter().filter(|c| c.level == Level::Pass).count();
    let warn = checks.iter().filter(|c| c.level == Level::Warn).count();
    let fail = checks.iter().filter(|c| c.level == Level::Fail).count();
    (pass, warn, fail)
}
