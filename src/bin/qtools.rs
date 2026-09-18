//! The `qtools` command: applies the recommended Arch Linux settings, with a preview, a backup and an undo.

fn main() -> std::io::Result<std::process::ExitCode> {
    quvyta_tools::run()
}
