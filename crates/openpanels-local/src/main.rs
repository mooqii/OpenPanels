fn main() {
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    let exit_code = openpanels_local::run_cli(&argv);
    std::process::exit(exit_code);
}

