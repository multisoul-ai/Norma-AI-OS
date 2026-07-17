//! Chrome Native Messaging stdio proxy for Norma Browser authentication.

fn main() {
    if let Err(error) = app::run_browser_auth_native_host() {
        eprintln!("Norma Browser auth host failed: {error}");
        std::process::exit(1);
    }
}
