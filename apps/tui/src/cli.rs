//! Command-line surface: `readio [文件] [-c | -l | -m] [--home <目录>]`.
//!
//! Kept in the library rather than in `main.rs` so the parsing rules — mode
//! flags in any position, conflicting flags, a mode with no file — are testable.
//!
//! `--home` is the only knob that cannot live in `config.yaml`, because it says
//! where that file is.

use crate::i18n::{t, tf};
use crate::library::Mode;
use crate::paths;

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// File to import and open. `None` starts on the library listing.
    pub path: Option<String>,
    pub mode: Mode,
    /// Host directory, when asked for explicitly.
    pub home: Option<String>,
}

/// Outcome of parsing: run the app, or print something and stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Run(Args),
    Help,
    Version,
}

/// Parse arguments (already stripped of the program name).
pub fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut path: Option<String> = None;
    let mut mode: Option<Mode> = None;
    let mut home: Option<String> = None;
    let mut expecting_home = false;

    for arg in args {
        if expecting_home {
            if arg.is_empty() {
                return Err(t("cli.home_needs_dir").to_string());
            }
            home = Some(arg.clone());
            expecting_home = false;
            continue;
        }
        if let Some(dir) = arg.strip_prefix("--home=") {
            if dir.is_empty() {
                return Err(t("cli.home_needs_dir").to_string());
            }
            home = Some(dir.to_string());
            continue;
        }
        if arg == "--home" {
            expecting_home = true;
            continue;
        }
        if let Some(found) = Mode::parse(arg) {
            if mode.is_some_and(|existing| existing != found) {
                return Err(t("cli.mode_conflict").to_string());
            }
            mode = Some(found);
            continue;
        }
        match arg.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "-V" | "--version" => return Ok(Parsed::Version),
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(tf("cli.unknown_flag", &[&other]));
            }
            "" => continue,
            other => {
                if path.is_some() {
                    return Err(t("cli.one_file").to_string());
                }
                path = Some(other.to_string());
            }
        }
    }

    if expecting_home {
        return Err(t("cli.home_needs_dir").to_string());
    }
    if path.is_none() && mode.is_some() {
        return Err(t("cli.mode_needs_file").to_string());
    }
    Ok(Parsed::Run(Args {
        path,
        mode: mode.unwrap_or_default(),
        home,
    }))
}

pub fn usage() -> String {
    format!(
        "readio {version}\n\n{body}",
        version = env!("CARGO_PKG_VERSION"),
        body = tf(
            "cli.usage",
            &[
                &paths::display(&paths::books_dir()),
                &paths::display(&paths::state_file()),
                &paths::display(&paths::config_file()),
            ]
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Parsed, String> {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse(&owned)
    }

    #[test]
    fn bare_launch_opens_the_library() {
        assert_eq!(
            run(&[]),
            Ok(Parsed::Run(Args {
                path: None,
                mode: Mode::Copy,
                home: None,
            }))
        );
    }

    #[test]
    fn copy_is_the_default_mode() {
        assert_eq!(
            run(&["book.epub"]),
            Ok(Parsed::Run(Args {
                path: Some("book.epub".to_string()),
                mode: Mode::Copy,
                home: None,
            }))
        );
    }

    #[test]
    fn accepts_a_mode_on_either_side_of_the_path() {
        for args in [["book.epub", "-m"], ["-m", "book.epub"]] {
            assert_eq!(
                run(&args),
                Ok(Parsed::Run(Args {
                    path: Some("book.epub".to_string()),
                    mode: Mode::Move,
                    home: None,
                })),
                "failed for {args:?}"
            );
        }
        assert_eq!(
            run(&["book.epub", "--link"]),
            Ok(Parsed::Run(Args {
                path: Some("book.epub".to_string()),
                mode: Mode::Link,
                home: None,
            }))
        );
    }

    #[test]
    fn rejects_contradictions_and_nonsense() {
        assert!(
            run(&["book.epub", "-c", "-m"]).is_err(),
            "conflicting modes"
        );
        assert!(run(&["a.epub", "b.epub"]).is_err(), "two files");
        assert!(run(&["-z"]).is_err(), "unknown flag");
        assert!(run(&["-l"]).is_err(), "mode without a file");
        // Repeating the same mode is harmless.
        assert!(run(&["book.epub", "-c", "-c"]).is_ok());
    }

    #[test]
    fn a_host_directory_can_be_given_on_the_command_line() {
        assert_eq!(
            run(&["--home", "/tmp/readio-demo", "book.epub"]),
            Ok(Parsed::Run(Args {
                path: Some("book.epub".to_string()),
                mode: Mode::Copy,
                home: Some("/tmp/readio-demo".to_string()),
            })),
            "--home takes the next argument"
        );
        assert_eq!(
            run(&["--home=/tmp/readio-demo"]),
            Ok(Parsed::Run(Args {
                path: None,
                mode: Mode::Copy,
                home: Some("/tmp/readio-demo".to_string()),
            })),
            "--home=<dir> works too"
        );
        assert!(run(&["--home"]).is_err(), "--home needs a directory");
        assert!(run(&["--home="]).is_err(), "an empty directory is an error");
        assert!(
            run(&["--home", "/tmp/x", "-m", "book.epub"]).is_ok(),
            "--home composes with an import mode"
        );
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert_eq!(run(&["--help"]), Ok(Parsed::Help));
        assert_eq!(run(&["book.epub", "-V"]), Ok(Parsed::Version));
        // Language-agnostic on purpose: another test in this binary may have
        // switched the interface language, and the usage text follows it.
        let text = usage();
        for fragment in ["readio", "-c", "-l", "-m", "--home"] {
            assert!(
                text.contains(fragment),
                "usage is missing {fragment}:\n{text}"
            );
        }
    }
}
