//! Copies lines from stdin to stdout and prepends them with a unix
//! time stamp in microsecond precision
//!
//! Uses no dependencies so that it can be compiled as quickly as
//! possible directly by `rustc`.

use std::{
    io::{stdin, stdout, BufRead, BufReader, BufWriter, Write},
    process::exit,
    time::{SystemTime, UNIX_EPOCH},
};

fn on_stdin<T>(res: Result<T, std::io::Error>) -> Result<T, String> {
    res.map_err(|e| format!("stdin: {e:#}"))
}

fn on_stdout<T>(res: Result<T, std::io::Error>) -> Result<T, String> {
    res.map_err(|e| format!("stdout: {e:#}"))
}

fn write_now(mut output: impl Write) -> Result<(), String> {
    let t = SystemTime::now();
    let unixtime = t
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("getting time: {e}"))?;

    let us = unixtime.as_micros();
    let s = us / 1_000_000;
    let rest = us % 1_000_000;

    on_stdout(write!(&mut output, "{s}.{rest:06}"))
}

fn log_timestamp() -> Result<(), String> {
    let mut input = BufReader::new(stdin().lock());
    let mut output = BufWriter::new(stdout().lock());
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = on_stdin(input.read_until(b'\n', &mut buf))?;
        if n == 0 {
            break Ok(());
        }
        write_now(&mut output)?;
        on_stdout((|| {
            output.write_all(b"\t")?;
            output.write_all(&buf)?;
            output.flush()?;
            Ok(())
        })())?;
    }
}

fn main() {
    if let Err(e) = log_timestamp() {
        eprintln!("Error: {e}");
        exit(1);
    }
}
