//! Bounded process fixtures drain both output pipes while requests are written.

use std::{
    io::{self, Read, Write},
    process::{Child, Command, Output, Stdio},
    time::{Duration, Instant},
};

pub(super) struct ProcessGuard(pub(super) Child);
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) fn run(
    executable: &str,
    arguments: &[&str],
    input: &[u8],
) -> Result<Output, Box<dyn std::error::Error>> {
    std::thread::scope(|scope| {
        // Drop/kill precedes scoped joins even on deadline or I/O failure.
        let mut process = ProcessGuard(
            Command::new(executable)
                .args(arguments)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let mut stdin = process.0.stdin.take().ok_or("missing input pipe")?;
        let stdout = process.0.stdout.take().ok_or("missing output pipe")?;
        let stderr = process.0.stderr.take().ok_or("missing error pipe")?;
        let writing = scope.spawn(move || stdin.write_all(input));
        let reading = scope.spawn(move || read_bounded(stdout));
        let errors = scope.spawn(move || read_bounded(stderr));
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = process.0.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                return Err("process deadline exceeded".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        writing.join().map_err(|_| "input thread failed")??;
        Ok(Output {
            status,
            stdout: reading.join().map_err(|_| "output thread failed")??,
            stderr: errors.join().map_err(|_| "error thread failed")??,
        })
    })
}

fn read_bounded(reader: impl Read) -> io::Result<Vec<u8>> {
    const LIMIT: u64 = 4 * 1024 * 1024;
    let mut bytes = Vec::new();
    reader.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(io::Error::other("fixture output limit"));
    }
    Ok(bytes)
}
