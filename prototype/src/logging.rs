use std::{fs::OpenOptions, sync::Mutex};

use tracing_subscriber::fmt::writer::{BoxMakeWriter, MakeWriterExt};

pub(crate) fn init() {
    let file = std::env::var_os("LOCALAPPDATA").and_then(|root| {
        let directory = std::path::PathBuf::from(root)
            .join("NexaNew")
            .join("gpui-prototype")
            .join("logs");
        std::fs::create_dir_all(&directory).ok()?;

        OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("prototype.log"))
            .ok()
    });
    let writer = match file {
        Some(file) => BoxMakeWriter::new(Mutex::new(file).and(std::io::stderr)),
        None => BoxMakeWriter::new(std::io::stderr),
    };

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(writer)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,nexa_gpui_prototype=info".into()),
        )
        .init();

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(operation = "panic", panic = %info, "Prototype panicked");
        previous(info);
    }));
}
