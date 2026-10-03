fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let json = qlippy_engine::support::report::json_requested(&args);
    qlippy_engine::support::report::emit(qlidoc_engine::run(&args), json)
}
