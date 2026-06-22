fn main() -> anyhow::Result<()> {
    let command = norma::cli::parse_command_from(std::env::args_os())?;
    match norma::client::send_command(command) {
        Ok(result) => {
            let output = norma::cli::format_result(&result);
            if !output.is_empty() {
                println!("{output}");
            }
            Ok(())
        }
        Err(norma::client::ClientError::Command(error)) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
        Err(error) => Err(error.into()),
    }
}
