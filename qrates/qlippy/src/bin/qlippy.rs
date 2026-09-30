fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let json = qlippy_engine::report::json_requested(&args);
    qlippy_engine::report::emit(qlippy_engine::qlippy::run(&args), json)
}
