//! Running external tools: one place for timeouts, environment and output limits.

use std::ffi::OsString;
use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::OnceCell;

/// Only the end of a tool's output is kept; it is what explains a failure.
pub const OUTPUT_LIMIT: usize = 64 * 1024;

/// What to run. Arguments are passed as a list and never through a shell.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    /// Program name or path.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
    /// Upper bound for the run time.
    pub timeout: Duration,
}

impl ProcessSpec {
    /// A spec without working directory or extra environment.
    #[must_use]
    pub fn new(program: impl Into<String>, args: &[&str], timeout: Duration) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(ToString::to_string).collect(),
            cwd: None,
            env: Vec::new(),
            timeout,
        }
    }
}

/// Result of a finished process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// Exit code; `None` if the process was killed by a signal.
    pub code: Option<i32>,
    /// Standard output, at most [`OUTPUT_LIMIT`] bytes from the end.
    pub stdout: String,
    /// Standard error, at most [`OUTPUT_LIMIT`] bytes from the end.
    pub stderr: String,
}

/// Why a process could not deliver output.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// The program does not exist or is not executable.
    #[error("{program} not found")]
    NotFound {
        /// Program name.
        program: String,
    },
    /// The process did not finish in time and was killed.
    #[error("{program} timed out after {seconds} s")]
    TimedOut {
        /// Program name.
        program: String,
        /// Timeout in seconds.
        seconds: u64,
    },
    /// Spawning or waiting failed.
    #[error("{program} could not be run")]
    Io {
        /// Program name.
        program: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// Runs external processes. A trait so adapters can be tested without real tools.
pub trait ProcessRunner: Send + Sync {
    /// Runs the process to completion.
    fn run(
        &self,
        spec: ProcessSpec,
    ) -> impl Future<Output = Result<ProcessOutput, ProcessError>> + Send;
}

/// Runs processes with Tokio, using the PATH of the login shell.
///
/// The PATH is resolved at most once and lazily: the first process waits for
/// it, later ones reuse it. Clones share the same resolution.
#[derive(Debug, Clone)]
pub struct TokioProcessRunner {
    path: Arc<OnceCell<OsString>>,
    login_shell: Option<Arc<LoginShell>>,
}

#[derive(Debug)]
struct LoginShell {
    shell: String,
    fallback: OsString,
}

impl TokioProcessRunner {
    /// A runner whose children see exactly `path` as `PATH`.
    #[must_use]
    pub fn new(path: OsString) -> Self {
        Self {
            path: Arc::new(OnceCell::new_with(Some(path))),
            login_shell: None,
        }
    }

    /// A runner that resolves `PATH` from `shell` as an interactive login
    /// shell on first use, falling back to `fallback`.
    #[must_use]
    pub fn with_login_path(shell: impl Into<String>, fallback: OsString) -> Self {
        Self {
            path: Arc::new(OnceCell::new()),
            login_shell: Some(Arc::new(LoginShell {
                shell: shell.into(),
                fallback,
            })),
        }
    }

    /// The `PATH` children see; resolves it on the first call.
    pub async fn path(&self) -> OsString {
        self.path
            .get_or_init(|| async {
                match &self.login_shell {
                    Some(login) => {
                        let probe = FixedPathRunner(login.fallback.clone());
                        let path = login_path(&probe, &login.shell, login.fallback.clone()).await;
                        tracing::info!(
                            entries = std::env::split_paths(&path).count(),
                            "resolved login PATH"
                        );
                        path
                    }
                    None => OsString::new(),
                }
            })
            .await
            .clone()
    }
}

impl ProcessRunner for TokioProcessRunner {
    async fn run(&self, spec: ProcessSpec) -> Result<ProcessOutput, ProcessError> {
        run_with_path(spec, &self.path().await).await
    }
}

/// Runner with a fixed `PATH`; resolves the login shell without recursion.
struct FixedPathRunner(OsString);

impl ProcessRunner for FixedPathRunner {
    async fn run(&self, spec: ProcessSpec) -> Result<ProcessOutput, ProcessError> {
        run_with_path(spec, &self.0).await
    }
}

async fn run_with_path(spec: ProcessSpec, path: &OsString) -> Result<ProcessOutput, ProcessError> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .env("PATH", path)
        .envs(spec.env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A timed-out child is killed when its handle is dropped.
        .kill_on_drop(true);
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    let child = command
        .spawn()
        .map_err(|source| spawn_error(&spec.program, source))?;
    let output = tokio::time::timeout(spec.timeout, child.wait_with_output())
        .await
        .map_err(|_| ProcessError::TimedOut {
            program: spec.program.clone(),
            seconds: spec.timeout.as_secs(),
        })?
        .map_err(|source| ProcessError::Io {
            program: spec.program.clone(),
            source,
        })?;
    Ok(ProcessOutput {
        code: output.status.code(),
        stdout: tail(&output.stdout),
        stderr: tail(&output.stderr),
    })
}

fn spawn_error(program: &str, source: std::io::Error) -> ProcessError {
    if source.kind() == std::io::ErrorKind::NotFound {
        ProcessError::NotFound {
            program: program.to_owned(),
        }
    } else {
        ProcessError::Io {
            program: program.to_owned(),
            source,
        }
    }
}

/// Last [`OUTPUT_LIMIT`] bytes as text, cut at a character boundary.
fn tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(OUTPUT_LIMIT);
    let text = String::from_utf8_lossy(&bytes[start..]);
    // A cut in the middle of a multi-byte character becomes U+FFFD; drop it.
    text.trim_start_matches('\u{fffd}').to_owned()
}

const PATH_START: &str = "__HUD_PATH_START__";
const PATH_END: &str = "__HUD_PATH_END__";
/// Interactive login shells can be slow to start (version managers, prompts).
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(3);

/// PATH as an interactive login shell sees it, so tools from version managers
/// such as nvm are found. Falls back to `fallback` if the shell fails.
///
/// The value is framed by markers because interactive shells may print
/// greetings or prompt fragments to stdout.
pub async fn login_path(runner: &impl ProcessRunner, shell: &str, fallback: OsString) -> OsString {
    let script = format!(r#"printf '%s%s%s' '{PATH_START}' "$PATH" '{PATH_END}'"#);
    let spec = ProcessSpec::new(shell, &login_shell_args(&script), LOGIN_SHELL_TIMEOUT);
    let output = match runner.run(spec).await {
        Ok(output) => output,
        Err(error) => {
            tracing::warn!(%error, "login shell failed, using inherited PATH");
            return fallback;
        }
    };
    let Some(path) = extract_path(&output.stdout) else {
        tracing::warn!("login shell printed no PATH, using inherited PATH");
        return fallback;
    };
    OsString::from(path)
}

/// `-i` loads interactive start-up files such as `.zshrc`, where version
/// managers extend PATH. `+m` turns job control off: otherwise the interactive
/// shell makes itself the terminal's foreground process group and the running
/// TUI is stopped when it next reads input.
fn login_shell_args(script: &str) -> [&str; 5] {
    ["-l", "-i", "+m", "-c", script]
}

fn extract_path(stdout: &str) -> Option<&str> {
    let start = stdout.find(PATH_START)? + PATH_START.len();
    let end = start + stdout[start..].find(PATH_END)?;
    let path = &stdout[start..end];
    (!path.is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_path_ignores_shell_noise() {
        let stdout = "Welcome!\n__HUD_PATH_START__/opt/homebrew/bin:/usr/bin__HUD_PATH_END__";
        assert_eq!(extract_path(stdout), Some("/opt/homebrew/bin:/usr/bin"));
    }

    #[test]
    fn login_shell_runs_without_job_control() {
        assert!(login_shell_args("true").contains(&"+m"));
    }

    #[test]
    fn extract_path_requires_both_markers() {
        assert_eq!(extract_path("__HUD_PATH_START__/usr/bin"), None);
    }

    #[test]
    fn tail_keeps_the_end_of_long_output() {
        let long = "a".repeat(OUTPUT_LIMIT) + "end";
        let kept = tail(long.as_bytes());
        assert_eq!(kept.len(), OUTPUT_LIMIT);
        assert!(kept.ends_with("end"));
    }

    #[test]
    fn tail_drops_a_cut_multibyte_character() {
        let long = "ä".repeat(OUTPUT_LIMIT);
        assert!(!tail(long.as_bytes()).starts_with('\u{fffd}'));
    }

    fn runner() -> TokioProcessRunner {
        TokioProcessRunner::new(std::env::var_os("PATH").unwrap_or_default())
    }

    #[tokio::test]
    async fn run_captures_output_and_exit_code() {
        let spec = ProcessSpec::new(
            "sh",
            &["-c", "echo out; echo err >&2; exit 3"],
            Duration::from_secs(5),
        );
        let output = runner().run(spec).await.expect("BUG: sh must run");
        assert_eq!(output.code, Some(3));
        assert_eq!(output.stdout, "out\n");
        assert_eq!(output.stderr, "err\n");
    }

    #[tokio::test]
    async fn run_reports_missing_program() {
        let spec = ProcessSpec::new("hud-no-such-program", &[], Duration::from_secs(5));
        let error = runner().run(spec).await.unwrap_err();
        assert!(matches!(error, ProcessError::NotFound { .. }), "{error}");
    }

    #[tokio::test]
    async fn run_times_out() {
        let spec = ProcessSpec::new("sleep", &["5"], Duration::from_millis(100));
        let error = runner().run(spec).await.unwrap_err();
        assert!(matches!(error, ProcessError::TimedOut { .. }), "{error}");
    }

    #[tokio::test]
    async fn run_closes_stdin() {
        let spec = ProcessSpec::new("cat", &[], Duration::from_secs(5));
        let output = runner().run(spec).await.expect("BUG: cat must run");
        assert_eq!(output.stdout, "");
    }

    #[tokio::test]
    async fn login_path_is_resolved_once_for_concurrent_runs() {
        let dir = tempfile::tempdir().expect("BUG");
        let counter = dir.path().join("calls");
        let shell = dir.path().join("fake-shell");
        std::fs::write(
            &shell,
            format!(
                "#!/bin/sh\necho call >> '{}'\nprintf '__HUD_PATH_START__/usr/bin:/bin__HUD_PATH_END__'\n",
                counter.display()
            ),
        )
        .expect("BUG");
        std::os::unix::fs::PermissionsExt::set_mode(
            &mut std::fs::metadata(&shell).expect("BUG").permissions(),
            0o755,
        );
        std::fs::set_permissions(&shell, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("BUG");

        let runner = TokioProcessRunner::with_login_path(
            shell.display().to_string(),
            OsString::from("/usr/bin:/bin"),
        );
        let spec = || ProcessSpec::new("true", &[], Duration::from_secs(5));
        let clone = runner.clone();
        let (a, b, c) = tokio::join!(runner.run(spec()), clone.run(spec()), runner.run(spec()));
        assert!(a.is_ok() && b.is_ok() && c.is_ok());
        assert_eq!(runner.path().await, OsString::from("/usr/bin:/bin"));
        let calls = std::fs::read_to_string(&counter).expect("BUG");
        assert_eq!(calls.lines().count(), 1);
    }
}
