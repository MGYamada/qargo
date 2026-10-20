fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let json = qlinty_engine::report::json_requested(&args);
    qlinty_engine::report::emit(qlinty_engine::qlinty::run(&args), json)
}
