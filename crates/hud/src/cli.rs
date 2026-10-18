//! Command-line flags. hud is a TUI; flags only answer questions and exit.

const HELP: &str = "\
hud - a heads-up display for your developer machine

Usage: hud [OPTIONS]

Options:
  -h, --help          Print help and exit
  -V, --version       Print version and exit
      --reset-config  Delete the configuration; the next start runs the setup
      --reset-data    Delete the database: scan cache, action log, install history,
                      AI usage (read again from the logs) and received telemetry
  -y, --yes           Delete without asking (with --reset-config or --reset-data)

Without options hud opens the terminal UI. Configuration: ~/.config/hud/config.toml";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the terminal UI.
    Run,
    /// Print this text and exit successfully.
    Print(String),
    /// Delete hud's files and exit.
    Reset(Reset),
    /// Print this text to stderr and exit with an error.
    Fail(String),
}

/// Which files to delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reset {
    /// The configuration file.
    pub config: bool,
    /// The database.
    pub data: bool,
    /// Skip the confirmation.
    pub yes: bool,
}

/// Interprets the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = String>) -> Action {
    let args: Vec<String> = args.into_iter().collect();
    match args.as_slice() {
        [] => Action::Run,
        [flag] if flag == "-V" || flag == "--version" => {
            Action::Print(format!("hud {}", env!("CARGO_PKG_VERSION")))
        }
        [flag] if flag == "-h" || flag == "--help" => Action::Print(HELP.to_owned()),
        _ => parse_reset(&args).map_or_else(
            || Action::Fail(format!("unknown arguments: {}\n\n{HELP}", args.join(" "))),
            Action::Reset,
        ),
    }
}

/// Reset flags in any order; `--yes` alone resets nothing and is rejected.
fn parse_reset(args: &[String]) -> Option<Reset> {
    let mut reset = Reset::default();
    for arg in args {
        match arg.as_str() {
            "--reset-config" => reset.config = true,
            "--reset-data" => reset.data = true,
            "-y" | "--yes" => reset.yes = true,
            _ => return None,
        }
    }
    (reset.config || reset.data).then_some(reset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn no_arguments_open_the_ui() {
        assert_eq!(parse(args(&[])), Action::Run);
    }

    #[test]
    fn version_flags_print_the_crate_version() {
        let expected = Action::Print(format!("hud {}", env!("CARGO_PKG_VERSION")));
        assert_eq!(parse(args(&["--version"])), expected);
        assert_eq!(parse(args(&["-V"])), expected);
    }

    #[test]
    fn help_mentions_the_config_file() {
        let Action::Print(text) = parse(args(&["--help"])) else {
            panic!("expected help")
        };
        assert!(text.contains("config.toml"));
    }

    #[test]
    fn reset_flags_combine_in_any_order() {
        let expected = Action::Reset(Reset {
            config: true,
            data: true,
            yes: true,
        });
        assert_eq!(
            parse(args(&["--yes", "--reset-data", "--reset-config"])),
            expected
        );
        assert_eq!(
            parse(args(&["--reset-config", "--reset-data", "-y"])),
            expected
        );
    }

    #[test]
    fn reset_asks_unless_yes_is_given() {
        assert_eq!(
            parse(args(&["--reset-data"])),
            Action::Reset(Reset {
                data: true,
                ..Reset::default()
            })
        );
    }

    #[test]
    fn yes_without_a_reset_flag_fails() {
        assert!(matches!(parse(args(&["--yes"])), Action::Fail(_)));
    }

    #[test]
    fn reset_mixed_with_other_flags_fails() {
        assert!(matches!(
            parse(args(&["--reset-data", "--version"])),
            Action::Fail(_)
        ));
    }

    #[test]
    fn unknown_arguments_fail_with_help() {
        assert!(matches!(parse(args(&["--frobnicate"])), Action::Fail(_)));
    }
}
