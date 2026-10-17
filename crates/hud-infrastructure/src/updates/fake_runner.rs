//! A [`ProcessRunner`] that returns canned output, for adapter tests.

use std::sync::Mutex;

use crate::process::{ProcessError, ProcessOutput, ProcessRunner, ProcessSpec};

/// Answers each call with the first rule whose argument prefix matches.
#[derive(Default)]
pub(crate) struct FakeRunner {
    rules: Vec<(Vec<String>, Result<ProcessOutput, ProcessErrorKind>)>,
    pub calls: Mutex<Vec<ProcessSpec>>,
}

#[derive(Clone, Copy)]
pub(crate) enum ProcessErrorKind {
    NotFound,
    TimedOut,
}

impl FakeRunner {
    pub fn answer(mut self, args: &[&str], code: i32, stdout: &str, stderr: &str) -> Self {
        let output = ProcessOutput {
            code: Some(code),
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        };
        self.rules
            .push((args.iter().map(ToString::to_string).collect(), Ok(output)));
        self
    }

    pub fn fail(mut self, args: &[&str], kind: ProcessErrorKind) -> Self {
        self.rules
            .push((args.iter().map(ToString::to_string).collect(), Err(kind)));
        self
    }

    pub fn args_of_calls(&self) -> Vec<Vec<String>> {
        self.calls
            .lock()
            .expect("BUG")
            .iter()
            .map(|spec| spec.args.clone())
            .collect()
    }
}

#[expect(
    clippy::unused_async_trait_impl,
    reason = "canned answers need no awaiting"
)]
impl ProcessRunner for FakeRunner {
    async fn run(&self, spec: ProcessSpec) -> Result<ProcessOutput, ProcessError> {
        let program = spec.program.clone();
        let rule = self
            .rules
            .iter()
            .find(|(prefix, _)| {
                spec.args.len() >= prefix.len() && spec.args[..prefix.len()] == prefix[..]
            })
            .map(|(_, answer)| answer.clone());
        self.calls.lock().expect("BUG").push(spec);
        match rule {
            Some(Ok(output)) => Ok(output),
            Some(Err(ProcessErrorKind::NotFound)) => Err(ProcessError::NotFound { program }),
            Some(Err(ProcessErrorKind::TimedOut)) => Err(ProcessError::TimedOut {
                program,
                seconds: 1,
            }),
            None => panic!(
                "BUG: unexpected call {program} {:?}",
                self.calls.lock().expect("BUG").last()
            ),
        }
    }
}
