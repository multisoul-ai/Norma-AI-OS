fn main() {
    let invocation = match norma::cli::parse_process_invocation_from(std::env::args_os()) {
        Ok(invocation) => invocation,
        Err(output) => {
            print_output_and_exit(output);
        }
    };
    let output = norma::cli_runtime::run_invocation(invocation);
    print_output_and_exit(output);
}

fn print_output_and_exit(output: norma::cli::CliOutput) -> ! {
    if !output.stdout.is_empty() {
        println!("{}", output.stdout);
    }
    if !output.stderr.is_empty() {
        eprintln!("{}", output.stderr);
    }
    std::process::exit(output.exit_code);
}
