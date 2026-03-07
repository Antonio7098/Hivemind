use super::*;

impl Registry {
    pub(crate) fn run_check_command(
        workdir: &Path,
        cargo_target_dir: &Path,
        command: &str,
        timeout_ms: Option<u64>,
    ) -> std::io::Result<(i32, String, bool)> {
        let started = Instant::now();

        let mut cmd = std::process::Command::new("sh");
        cmd.current_dir(workdir)
            .env("CARGO_TARGET_DIR", cargo_target_dir)
            .args(["-lc", command]);

        if let Some(timeout_ms) = timeout_ms {
            let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

            let mut out_buf = Vec::new();
            let mut err_buf = Vec::new();

            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let out_handle = std::thread::spawn(move || {
                if let Some(mut stdout) = stdout {
                    let _ = stdout.read_to_end(&mut out_buf);
                }
                out_buf
            });
            let err_handle = std::thread::spawn(move || {
                if let Some(mut stderr) = stderr {
                    let _ = stderr.read_to_end(&mut err_buf);
                }
                err_buf
            });

            let timeout = Duration::from_millis(timeout_ms);
            let mut timed_out = false;
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if started.elapsed() >= timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait()?;
                }
                std::thread::sleep(Duration::from_millis(10));
            };

            let stdout_buf = out_handle.join().unwrap_or_default();
            let stderr_buf = err_handle.join().unwrap_or_default();

            let mut combined = String::new();
            if timed_out {
                let _ = writeln!(combined, "timed out after {timeout_ms}ms");
            }
            combined.push_str(&String::from_utf8_lossy(&stdout_buf));
            if !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&String::from_utf8_lossy(&stderr_buf));

            let exit_code = if timed_out {
                124
            } else {
                status.code().unwrap_or(-1)
            };

            return Ok((exit_code, combined, timed_out));
        }

        let out = cmd.output()?;
        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&out.stdout));
        if !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
        Ok((out.status.code().unwrap_or(-1), combined, false))
    }
}
