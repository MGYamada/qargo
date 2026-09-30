use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    qargo_tools::report::emit(
        qargo_tools::qargo::run(&args),
        qargo_tools::report::json_requested(&args),
    )
}
