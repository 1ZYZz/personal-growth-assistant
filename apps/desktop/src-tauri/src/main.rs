#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(output) = std::env::args().find_map(|argument| {
        argument
            .strip_prefix("--diagnostics-output=")
            .map(std::path::PathBuf::from)
    }) {
        if personal_growth_assistant_lib::write_diagnostic_report(&output).is_err() {
            std::process::exit(2);
        }
        return;
    }
    personal_growth_assistant_lib::run();
}
