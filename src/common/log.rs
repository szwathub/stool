use std::io::Write;
use termcolor::{Color, ColorChoice as TermColorChoice, ColorSpec, StandardStream, WriteColor};

/// 快捷输出 Info 级别日志的宏，支持 `format!` 风格的参数。
///
/// # Examples
///
/// ```
/// use stool::infolog;
///
/// infolog!("服务启动成功");
/// infolog!("监听端口: {}", 8080);
/// ```
#[macro_export]
macro_rules! infolog {
    ($($arg:tt)*) => {
        $crate::common::log::log_rich(&[
            $crate::common::log::Span::new("[INFO] ")
                .color(termcolor::Color::Green)
                .bold(),
            $crate::common::log::Span::new(format!($($arg)*)),
        ]).expect("日志输出失败")
    };
}

#[allow(dead_code)]
pub enum LogLevel {
    Info,
    Debug,
    Trace,
}

/// A text segment with optional color and bold styling.
///
/// # Examples
///
/// ```
/// use stool::common::log::Span;
/// use termcolor::Color;
///
/// // Plain text (default color)
/// let plain = Span::new("hello");
///
/// // Colored text
/// let colored = Span::new("world").color(Color::Green);
///
/// // Colored and bold
/// let bold = Span::new("important").color(Color::Red).bold();
/// ```
#[allow(dead_code)]
pub struct Span {
    pub text: String,
    pub color: Option<Color>,
    pub bold: bool,
}

#[allow(dead_code)]
impl Span {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            bold: false,
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
}

/// Log a line composed of multiple colored spans.
///
/// # Examples
///
/// ```
/// use stool::common::log::{log_rich, Span};
/// use termcolor::Color;
///
/// // Multi-color output in a single line
/// log_rich(&[
///     Span::new("已安装 ").color(Color::Green),
///     Span::new("stool v0.2.3").color(Color::Cyan).bold(),
///     Span::new(" 到 "),
///     Span::new("/usr/local/bin").color(Color::Yellow),
/// ]).unwrap();
///
/// // Mix plain and colored text
/// log_rich(&[
///     Span::new("[OK] ").color(Color::Green).bold(),
///     Span::new("编译完成，耗时 "),
///     Span::new("1.23s").color(Color::Cyan),
/// ]).unwrap();
/// ```
#[allow(dead_code)]
pub fn log_rich(spans: &[Span]) -> std::io::Result<()> {
    let mut stdout = StandardStream::stdout(TermColorChoice::Auto);

    for span in spans {
        let mut spec = ColorSpec::new();
        if let Some(c) = span.color {
            spec.set_fg(Some(c));
        }
        spec.set_bold(span.bold);
        stdout.set_color(&spec)?;
        write!(&mut stdout, "{}", span.text)?;
    }

    stdout.reset()?;
    writeln!(&mut stdout)?;
    Ok(())
}
