fn main() {
    let invocation =
        norma::cli::parse_invocation_from(std::env::args_os()).unwrap_or_else(|error| error.exit());
    let output = norma::cli_runtime::run_invocation(invocation);
    if !output.stdout.is_empty() {
        println!("{}", output.stdout);
    }
    if !output.stderr.is_empty() {
        eprintln!("{}", output.stderr);
    }
    std::process::exit(output.exit_code);
}
