//! Command-line flags. hud is a TUI; flags only answer questions and exit.

const HELP: &str = "\
hud - a heads-up display for your developer machine

Usage: hud [OPTIONS]

Options:
  -h, --help     Print help and exit
  -V, --version  Print version and exit

Without options hud opens the terminal UI. Configuration: ~/.config/hud/config.toml";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the terminal UI.
    Run,
    /// Print this text and exit successfully.
    Print(String),
    /// Print this text to stderr and exit with an error.
    Fail(String),
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
        _ => Action::Fail(format!("unknown arguments: {}\n\n{HELP}", args.join(" "))),
    }
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
    fn unknown_arguments_fail_with_help() {
        assert!(matches!(parse(args(&["--frobnicate"])), Action::Fail(_)));
    }
}
