fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let json = qargo_tools::report::json_requested(&args);
    qargo_tools::report::emit(qargo_tools::qlifmt_engine::run(&args), json)
}
