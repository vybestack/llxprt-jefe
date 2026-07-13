use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(error) = run() {
        let _ = writeln!(std::io::stderr(), "psmux smoke fixture failed: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> std::io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let _raw_mode = RawModeGuard;
    let mut output = std::io::stdout().lock();
    output.write_all(b"PSMUX_SMOKE_READY\r\n")?;
    output.flush()?;

    let mut byte = [0_u8; 1];
    loop {
        std::io::stdin().read_exact(&mut byte)?;
        writeln!(output, "PSMUX_BYTE_{:02X}", byte[0])?;
        output.flush()?;
        if byte[0] == 0x04 {
            return Ok(());
        }
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}
