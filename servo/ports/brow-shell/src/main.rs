/* brow-shell binary: a thin wrapper over the brow_shell library. */

#[cfg(feature = "engine")]
use brow_shell::app;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "engine")]
    {
        // Phase 2 engine hardening: rustls with the aws-lc provider, as
        // servoshell does.
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("Failed to install rustls crypto provider");

        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

        return app::run();
    }

    #[cfg(not(feature = "engine"))]
    {
        eprintln!(
            "brow-shell was built without the `engine` feature (chrome UI stack only). \
             Build with default features to embed the Servo engine."
        );
        std::process::exit(2);
    }
}
