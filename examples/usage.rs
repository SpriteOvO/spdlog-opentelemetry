// Run this example:
// cargo run --example usage -- http://<server>:4317

use std::{env, error::Error as StdError, process, sync::Arc};

use opentelemetry_otlp::WithExportConfig as _;
use spdlog::prelude::*;
use spdlog_opentelemetry::OpenTelemetrySink;

fn otel_logger_provider(
    service_name: &'static str,
) -> Result<opentelemetry_sdk::logs::SdkLoggerProvider, Box<dyn StdError>> {
    let Some(endpoint) = env::args().nth(1) else {
        error!("invalid cli argument. usage: `usage <otlp-endpoint>`");
        process::exit(1);
    };

    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;
    let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_service_name(service_name)
                .build(),
        )
        // Use batch exporter for better throughput.
        //
        // If you choose batch exporter, don't conbine your sink with spdlog-rs async sink, because
        // it's already async inside OpenTelemetry SDK's `BatchLogProcessor`.
        //
        // .with_simple_exporter(exporter)
        .with_batch_exporter(exporter)
        .build();

    Ok(logger_provider)
}

fn setup_logger(
    provider: &opentelemetry_sdk::logs::SdkLoggerProvider,
) -> Result<(), Box<dyn StdError>> {
    let sink = Arc::new(OpenTelemetrySink::builder().provider(provider).build()?);

    let logger = spdlog::default_logger().fork_with(|logger| {
        logger.set_level_filter(LevelFilter::All);
        logger.sinks_mut().push(sink);
        // Now the new logger has 3 sinks: stdout + stderr + OpenTelemetry
        //                                 ^^^^^^^^^^^^^^^
        //                                 forked from the default logger
        Ok(())
    })?;
    spdlog::set_default_logger(logger);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    let provider = otel_logger_provider("spdlog-opentelemetry-example")?;

    setup_logger(&provider)?;

    trace!("this is a trace message");
    info!("this is an info message", kv: { id = 1 });
    warn!("this is a warn message");
    error!("this is an error message");

    // Ensure logs are finally sent before exiting.
    _ = provider.shutdown();

    Ok(())
}
