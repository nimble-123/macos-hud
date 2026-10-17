//! [`UpdateSource`](hud_application::ports::updates::UpdateSource) adapters.

pub mod brew;
pub mod npm;
pub mod skills;

#[cfg(test)]
pub(crate) mod fake_runner;

use crate::process::ProcessOutput;

/// Lines of a successful run kept for the action log.
const SUCCESS_LINES: usize = 3;

/// End of what a successful tool printed: the last non-empty lines of
/// stdout, or of stderr when stdout is empty. Credentials in URLs are masked,
/// because the text is stored (AGENTS.md: never store credentials).
pub(crate) fn success_output(output: &ProcessOutput) -> String {
    let text = if output.stdout.trim().is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines[lines.len().saturating_sub(SUCCESS_LINES)..]
        .iter()
        .map(|line| mask_credentials(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replaces the user info of URLs such as `https://user:token@host` with `***`.
fn mask_credentials(line: &str) -> String {
    let mut masked = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(scheme) = rest.find("://") {
        let (before, after) = rest.split_at(scheme + 3);
        masked.push_str(before);
        let authority_end = after.find(['/', ' ']).unwrap_or(after.len());
        match after[..authority_end].rfind('@') {
            Some(at) => {
                masked.push_str("***");
                rest = &after[at..];
            }
            None => rest = after,
        }
    }
    masked.push_str(rest);
    masked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stdout: &str, stderr: &str) -> ProcessOutput {
        ProcessOutput {
            code: Some(0),
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn success_output_keeps_the_last_three_non_empty_lines() {
        let text = "==> Fetching wget\n\n==> Pouring\n==> Summary\n🍺 wget 1.24.5: 92 files\n\n";
        assert_eq!(
            success_output(&output(text, "")),
            "==> Pouring\n==> Summary\n🍺 wget 1.24.5: 92 files"
        );
    }

    #[test]
    fn success_output_falls_back_to_stderr() {
        assert_eq!(
            success_output(&output("", "changed 1 package in 2s\n")),
            "changed 1 package in 2s"
        );
    }

    #[test]
    fn credentials_in_urls_are_masked() {
        assert_eq!(
            mask_credentials(
                "fetched https://user:s3cret@registry.example.com/pkg and http://plain.example"
            ),
            "fetched https://***@registry.example.com/pkg and http://plain.example"
        );
    }
}
