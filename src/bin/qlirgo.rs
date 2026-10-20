use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    qlirgo_tools::report::emit(
        qlirgo_tools::qlirgo::run(&args),
        qlirgo_tools::report::json_requested(&args),
    )
}
