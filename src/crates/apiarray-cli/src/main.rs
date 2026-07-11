use apiarray_core::cli::parse_and_dispatch;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut had_failure = false;
    let mut processed = 0_u64;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                eprintln!("error cli.stdin READ_FAILED {error}");
                return ExitCode::from(2);
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let response = parse_and_dispatch(&line);
        had_failure |= !response.ok;
        processed = processed.saturating_add(1);
        match serde_json::to_writer(&mut stdout, &response) {
            Ok(()) => {
                if writeln!(&mut stdout).is_err() {
                    eprintln!("error cli.stdout WRITE_FAILED");
                    return ExitCode::from(3);
                }
            }
            Err(error) => {
                eprintln!("error cli.stdout SERIALIZATION_FAILED {error}");
                return ExitCode::from(3);
            }
        }
    }

    if stdout.flush().is_err() {
        eprintln!("error cli.stdout FLUSH_FAILED");
        return ExitCode::from(3);
    }
    eprintln!(
        "info cli.complete processed={processed} failures={}",
        u8::from(had_failure)
    );
    if had_failure {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
