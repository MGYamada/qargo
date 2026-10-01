//! Bound selected-tool execution and clean up its process group on failure.

use std::process::Command;

use crate::report::Diagnostic;

pub(crate) fn bounded_output(command: &mut Command) -> Result<(Vec<u8>, i32), Diagnostic> {
    #[cfg(unix)]
    {
        unix::capture(command, std::time::Duration::from_secs(30))
    }
    #[cfg(not(unix))]
    {
        let _ = command;
        Err(Diagnostic::error(
            "tool_execution",
            "tool",
            "Selected-tool process group control is unavailable on this platform.",
        ))
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{self, Read};
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    use rustix::process::{Pid, Signal, kill_process_group};

    use crate::tool_response::transport;

    struct ToolChild {
        child: Child,
        group: Pid,
        finished: bool,
    }

    impl Drop for ToolChild {
        fn drop(&mut self) {
            if !self.finished {
                let _ = kill_process_group(self.group, Signal::KILL);
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }

    struct Readers {
        cancelled: Arc<AtomicBool>,
        threads: Vec<JoinHandle<()>>,
    }

    impl Drop for Readers {
        fn drop(&mut self) {
            self.cancelled.store(true, Ordering::Relaxed);
            for reader in self.threads.drain(..) {
                let _ = reader.join();
            }
        }
    }

    fn execution_error(message: impl Into<String>) -> Diagnostic {
        Diagnostic::error("tool_execution", "tool", message)
    }

    fn deadline_error() -> Diagnostic {
        execution_error("The selected tool exceeded the 30-second execution limit.")
    }

    fn read_stream(
        stream: Box<dyn Read + Send>,
        is_stdout: bool,
        limit: usize,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        let mut stream = stream.take(limit as u64 + 1);
        let mut bytes = Vec::new();
        let mut buffer = [0; 16 * 1024];
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err("Selected-tool output capture was cancelled.".into());
            }
            match stream.read(&mut buffer) {
                Ok(0) => return Ok(bytes),
                Ok(count) => {
                    bytes.extend_from_slice(&buffer[..count]);
                    if !is_stdout || bytes.len() > limit {
                        return Ok(bytes);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(format!("Cannot read selected tool output: {error}")),
            }
        }
    }

    pub(super) fn capture(
        command: &mut Command,
        timeout: Duration,
    ) -> Result<(Vec<u8>, i32), Diagnostic> {
        command
            .process_group(0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        let child = command
            .spawn()
            .map_err(|error| execution_error(format!("Cannot execute selected tool: {error}")))?;
        let mut child = ToolChild {
            group: Pid::from_child(&child),
            child,
            finished: false,
        };
        let stdout = child.child.stdout.take().expect("piped child stdout");
        let stderr = child.child.stderr.take().expect("piped child stderr");
        // Cancellable nonblocking reads also bound cleanup if a descendant keeps a pipe open.
        for result in [
            fcntl_getfl(&stdout).and_then(|flags| fcntl_setfl(&stdout, flags | OFlags::NONBLOCK)),
            fcntl_getfl(&stderr).and_then(|flags| fcntl_setfl(&stderr, flags | OFlags::NONBLOCK)),
        ] {
            result.map_err(|error| {
                execution_error(format!("Cannot configure tool pipes: {error}"))
            })?;
        }
        let deadline = Instant::now() + timeout;
        let (sender, receiver) = mpsc::channel();
        let mut readers = Readers {
            cancelled: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        };
        for (is_stdout, stream, limit) in [
            (
                true,
                Box::new(stdout) as Box<dyn Read + Send>,
                4 * 1024 * 1024,
            ),
            (false, Box::new(stderr) as Box<dyn Read + Send>, 64 * 1024),
        ] {
            let sender = sender.clone();
            let cancelled = Arc::clone(&readers.cancelled);
            readers.threads.push(thread::spawn(move || {
                let result = read_stream(stream, is_stdout, limit, &cancelled);
                let _ = sender.send((is_stdout, limit, result));
            }));
        }
        drop(sender);
        let mut stdout = Vec::new();
        for _ in 0..2 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(remaining) {
                Ok((is_stdout, limit, Ok(bytes))) if bytes.len() <= limit => {
                    if !is_stdout && !bytes.is_empty() {
                        return Err(transport(
                            "The selected tool wrote unexpected stderr output.",
                        ));
                    }
                    if is_stdout {
                        stdout = bytes;
                    }
                }
                Ok((_, _, Ok(_))) => {
                    return Err(transport(
                        "The selected tool output exceeds the transport size budget.",
                    ));
                }
                Ok((_, _, Err(message))) => return Err(transport(message)),
                Err(_) => return Err(deadline_error()),
            }
        }
        // Closing both streams does not exempt a live tool from the execution deadline.
        let status = loop {
            match child.child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                Ok(None) => return Err(deadline_error()),
                Err(error) => {
                    return Err(execution_error(format!(
                        "Cannot wait for selected tool: {error}"
                    )));
                }
            }
        };
        let code = status.code().ok_or_else(|| {
            execution_error("The selected tool was terminated without an exit code.")
        })?;
        child.finished = true;
        Ok((stdout, code))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::fs;

        fn descendant_is_dead(pid: &str) -> bool {
            let output = Command::new("ps")
                .args(["-p", pid, "-o", "stat="])
                .output()
                .unwrap();
            let status = String::from_utf8(output.stdout).unwrap();
            // A terminated orphan may briefly await reaping by init on Linux.
            status.trim().is_empty() || status.trim().starts_with('Z')
        }

        fn rejects_and_terminates_descendant(
            action: &str,
            expected_id: &str,
            detached_pipes: bool,
        ) {
            let directory = tempfile::tempdir().unwrap();
            let pid_path = directory.path().join("descendant.pid");
            let script = directory.path().join("tool.sh");
            fs::write(
                &script,
                format!(
                    "sleep 60 {} &\ndescendant=$!\nprintf '%s' \"$descendant\" > \"$1\"\n{action}\n",
                    if detached_pipes { ">/dev/null 2>&1" } else { "" }
                ),
            )
            .unwrap();
            let mut command = Command::new("sh");
            command.arg(&script).arg(&pid_path);
            let started = Instant::now();
            let error = capture(&mut command, Duration::from_secs(2)).unwrap_err();
            assert_eq!(error.id, expected_id, "{}", error.message);
            assert!(started.elapsed() < Duration::from_secs(5));
            let pid = fs::read_to_string(pid_path).unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !descendant_is_dead(&pid) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(
                descendant_is_dead(&pid),
                "Descendant {pid} survived tool cleanup"
            );
        }

        #[test]
        fn timeout_terminates_descendants_holding_output_pipes() {
            rejects_and_terminates_descendant("wait", "tool_execution", false);
        }

        #[test]
        fn timeout_terminates_descendants_after_the_direct_child_exits() {
            rejects_and_terminates_descendant("exit 0", "tool_execution", false);
        }

        #[test]
        fn closing_output_pipes_does_not_exempt_a_live_process_group() {
            rejects_and_terminates_descendant("exec >/dev/null 2>&1\nwait", "tool_execution", true);
        }

        #[test]
        fn oversized_output_terminates_descendants() {
            rejects_and_terminates_descendant(
                "dd if=/dev/zero bs=1048576 count=5 2>/dev/null\nwait",
                "invalid_tool_response",
                false,
            );
        }

        #[test]
        fn unexpected_stderr_terminates_descendants_without_waiting_for_eof() {
            rejects_and_terminates_descendant(
                "printf unexpected >&2\nwait",
                "invalid_tool_response",
                false,
            );
        }

        #[test]
        fn normal_tool_stdout_and_exit_status_are_preserved() {
            let mut command = Command::new("sh");
            command.args(["-c", "printf response; exit 1"]);
            let (stdout, status) = capture(&mut command, Duration::from_secs(2)).unwrap();
            assert_eq!(stdout, b"response");
            assert_eq!(status, 1);
        }
    }
}
