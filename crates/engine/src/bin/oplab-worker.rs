//! Framed worker executable. All user-facing operations remain in the engine library.

fn main() -> std::process::ExitCode {
    // Keep dependency panic payloads and build-machine paths out of routine stderr.
    std::panic::set_hook(Box::new(|_| eprintln!("engine_backend_panic")));
    let result = oplab_engine::worker::serve();
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // Errors expose only stable categories, never a request, source, or environment.
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
