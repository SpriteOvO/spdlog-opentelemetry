//! Sends logs to [OpenTelemetry], based on [spdlog-rs].
//!
//! [spdlog-rs] is a fast, highly configurable Rust logging crate. This crate
//! provides a sink that emits the collected logs to [OpenTelemetry]-compatible
//! distributed logging systems for processing and visualization.
//!
//! ## Examples
//!
//! See directory [./examples].
//!
//! [OpenTelemetry]: https://opentelemetry.io/
//! [spdlog-rs]: https://crates.io/crates/spdlog-rs
//! [./examples]: https://github.com/SpriteOvO/spdlog-opentelemetry/tree/main/examples

#![warn(missing_docs)]

mod error;

use std::{convert::Infallible, result::Result as StdResult, sync::Arc};

pub use error::{Error, Result};
use opentelemetry::{
    Key as OtelKey,
    logs::{
        AnyValue as OtelAnyValue, LogRecord as _, Logger as OtelLogger,
        LoggerProvider as OtelLoggerProvider, Severity as OtelSeverity,
    },
};
use spdlog::{
    ErrorHandler, Record, StringBuf,
    formatter::{Formatter, FormatterContext, FullFormatter},
    prelude::*,
    sink::{GetSinkProp, Sink, SinkProp},
};
use value_bag::{ValueBag, visit::Visit as ValueBagVisit};

#[doc(hidden)]
pub mod __private {
    pub struct NotSet;
}
use __private::NotSet;

/// #
///
/// # Note
///
/// The generics here are designed to check for required fields at compile time,
/// users should not specify them manually and/or depend on them. If the generic
/// concrete types or the number of generic types are changed in the future, it
/// may not be considered as a breaking change.
pub struct OpenTelemetryBuilder<ArgL> {
    prop: SinkProp,
    logger: ArgL,
}

impl<ArgL> OpenTelemetryBuilder<ArgL> {
    /// Specifies a provider.
    ///
    /// The provided provider will be used to create a
    /// [`opentelemetry::logs::Logger`] for subsequent use.
    ///
    /// This parameter is **required**.
    pub fn provider<P>(self, provider: &P) -> OpenTelemetryBuilder<P::Logger>
    where
        P: OtelLoggerProvider,
    {
        // Using empty scope name for now.
        // https://github.com/open-telemetry/semantic-conventions/issues/1550
        let logger = provider.logger("");
        OpenTelemetryBuilder {
            prop: self.prop,
            logger,
        }
    }

    // Prop
    //

    /// Specifies a log level filter.
    ///
    /// This parameter is **optional**, and defaults to [`LevelFilter::All`].
    #[must_use]
    pub fn level_filter(self, level_filter: LevelFilter) -> Self {
        self.prop.set_level_filter(level_filter);
        self
    }

    /// Specifies a formatter.
    ///
    /// This parameter is **optional**, and defaults to [`FullFormatter`]
    /// `(!time !level !source_location !eol)`.
    #[must_use]
    pub fn formatter<F>(self, formatter: F) -> Self
    where
        F: Formatter + 'static,
    {
        self.prop.set_formatter(formatter);
        self
    }

    /// Specifies an error handler.
    ///
    /// This parameter is **optional**, and defaults to
    /// [`ErrorHandler::default()`].
    #[must_use]
    pub fn error_handler<F>(self, handler: F) -> Self
    where
        F: Into<ErrorHandler>,
    {
        self.prop.set_error_handler(handler);
        self
    }
}

impl OpenTelemetryBuilder<NotSet> {
    #[doc(hidden)]
    #[deprecated(note = "\n\n\
        builder compile-time error:\n\
        - missing required parameter `provider`\n\n\
    ")]
    pub fn build(self, _: Infallible) {}
}

impl<ArgL> OpenTelemetryBuilder<ArgL>
where
    ArgL: OtelLogger,
{
    /// Builds a `OpenTelemetrySink`.
    pub fn build(self) -> Result<OpenTelemetrySink<ArgL>> {
        Ok(OpenTelemetrySink {
            prop: self.prop,
            logger: self.logger,
        })
    }

    /// Builds a `Arc<OpenTelemetrySink>`.
    pub fn build_arc(self) -> Result<Arc<OpenTelemetrySink<ArgL>>> {
        Self::build(self).map(Arc::new)
    }
}

//

/// A sink with a OpenTelemetry provider as the target.
///
/// It takes a [`SdkLoggerProvider`] from the upstream [`opentelemetry_sdk`]
/// crate, the relevant settings should be configured there.
///
/// Note that when configuring [`SdkLoggerProvider`], you can use [the batch
/// exporter] to gain better throughput via its built-in asynchronous
/// implementation. If you choose to do so, combining `OpenTelemetrySink` with
/// [`AsyncPoolSink`] is no longer necessary.
///
/// [`SdkLoggerProvider`]: https://docs.rs/opentelemetry_sdk/0.31.0/opentelemetry_sdk/logs/struct.SdkLoggerProvider.html
/// [`opentelemetry_sdk`]: https://docs.rs/opentelemetry_sdk/latest/opentelemetry_sdk/
/// [the batch exporter]: https://docs.rs/opentelemetry_sdk/0.31.0/opentelemetry_sdk/logs/struct.LoggerProviderBuilder.html#method.with_batch_exporter
/// [`AsyncPoolSink`]: https://docs.rs/spdlog-rs/0.5.2/spdlog/sink/struct.AsyncPoolSink.html
pub struct OpenTelemetrySink<L> {
    prop: SinkProp,
    logger: L,
}

impl OpenTelemetrySink<()> {
    /// Gets a builder of `TelegramSink` with default parameters:
    ///
    /// | Parameter         | Default Value                                            |
    /// |-------------------|----------------------------------------------------------|
    /// | [level_filter]    | `All`                                                    |
    /// | [formatter]       | [`FullFormatter`] `(!time !level !source_location !eol)` |
    /// | [error_handler]   | [`ErrorHandler::default()`]                              |
    /// |                   |                                                          |
    /// | [provider]        | *must be specified*                                      |
    ///
    /// [level_filter]: OpenTelemetryBuilder::level_filter
    /// [formatter]: OpenTelemetryBuilder::formatter
    /// [error_handler]: OpenTelemetryBuilder::error_handler
    /// [provider]: OpenTelemetryBuilder::provider
    #[must_use]
    pub fn builder() -> OpenTelemetryBuilder<NotSet> {
        let prop = SinkProp::default();
        prop.set_formatter(
            FullFormatter::builder()
                .time(false)
                .level(false)
                .source_location(false)
                .eol(false)
                .build(),
        );
        OpenTelemetryBuilder {
            prop,
            logger: NotSet,
        }
    }
}

impl<L> OpenTelemetrySink<L> {
    const LEVELS_MAPPING: LevelsMapping = LevelsMapping::new();
}

impl<L> GetSinkProp for OpenTelemetrySink<L> {
    fn prop(&self) -> &SinkProp {
        &self.prop
    }
}

impl<L> Sink for OpenTelemetrySink<L>
where
    L: OtelLogger + Send + Sync,
{
    // Unstable "spec_unstable_logs_enabled"
    // https://github.com/open-telemetry/opentelemetry-rust/issues/3020
    //
    // fn should_log(&self, level: Level) -> bool {}

    fn log(&self, record: &Record) -> spdlog::Result<()> {
        let mut string_buf = StringBuf::new();
        let mut ctx = FormatterContext::new();
        self.prop
            .formatter()
            .format(record, &mut string_buf, &mut ctx)?;

        let mut otel_record = self.logger.create_log_record();
        if let Some(logger_name) = record.logger_name() {
            otel_record.set_target(logger_name.to_owned());
        }
        otel_record.set_timestamp(record.time());
        otel_record.set_severity_text(record.level().as_str());
        otel_record.set_severity_number(Self::LEVELS_MAPPING.level(record.level()));
        otel_record.set_body(OtelAnyValue::from(string_buf));
        if let Some(srcloc) = record.source_location() {
            // https://opentelemetry.io/docs/specs/semconv/registry/attributes/code/#code-attributes
            otel_record.add_attribute("code.line.number", srcloc.line());
            otel_record.add_attribute("code.column.number", srcloc.column());
            otel_record.add_attribute("code.file.path", srcloc.file());
            otel_record.add_attribute("code.function.name", srcloc.module_path());
        }
        otel_record.add_attributes(convert_attrs(record)?);

        self.logger.emit(otel_record);
        Ok(())
    }

    fn flush(&self) -> spdlog::Result<()> {
        Ok(())
    }
}

//

struct LevelsMapping([OtelSeverity; Level::count()]);

impl LevelsMapping {
    #[must_use]
    const fn new() -> Self {
        Self([
            OtelSeverity::Fatal, // spdlog::Critical
            OtelSeverity::Error, // spdlog::Error
            OtelSeverity::Warn,  // spdlog::Warn
            OtelSeverity::Info,  // spdlog::Info
            OtelSeverity::Debug, // spdlog::Debug
            OtelSeverity::Trace, // spdlog::Trace
        ])
    }

    #[must_use]
    fn level(&self, level: Level) -> OtelSeverity {
        self.0[level as usize]
    }
}

fn convert_attrs(record: &Record) -> spdlog::Result<Vec<(OtelKey, OtelAnyValue)>> {
    record
        .key_values()
        .iter()
        .map(|(k, v)| convert_value(v).map(|v| (OtelKey::from(k.as_str().to_owned()), v)))
        .collect::<spdlog::Result<_>>()
}

fn convert_value(value: ValueBag) -> spdlog::Result<OtelAnyValue> {
    struct Converter(Option<OtelAnyValue>);

    impl<'a> ValueBagVisit<'a> for Converter {
        fn visit_any(&mut self, value: ValueBag) -> StdResult<(), value_bag::Error> {
            self.0 = Some(OtelAnyValue::from(value.to_string()));
            Ok(())
        }

        fn visit_i64(&mut self, value: i64) -> StdResult<(), value_bag::Error> {
            self.0 = Some(OtelAnyValue::Int(value));
            Ok(())
        }

        fn visit_u64(&mut self, value: u64) -> StdResult<(), value_bag::Error> {
            if let Ok(value) = value.try_into() {
                self.visit_i64(value)
            } else {
                self.visit_any(value.into())
            }
        }

        fn visit_i128(&mut self, value: i128) -> StdResult<(), value_bag::Error> {
            if let Ok(value) = value.try_into() {
                self.visit_i64(value)
            } else {
                self.visit_any(value.into())
            }
        }

        fn visit_u128(&mut self, value: u128) -> StdResult<(), value_bag::Error> {
            if let Ok(value) = value.try_into() {
                self.visit_i64(value)
            } else {
                self.visit_any(value.into())
            }
        }

        fn visit_f64(&mut self, value: f64) -> StdResult<(), value_bag::Error> {
            self.0 = Some(OtelAnyValue::Double(value));
            Ok(())
        }

        fn visit_bool(&mut self, value: bool) -> StdResult<(), value_bag::Error> {
            self.0 = Some(OtelAnyValue::Boolean(value));
            Ok(())
        }
    }

    let mut cvt = Converter(None);
    value
        .visit(&mut cvt)
        .map_err(|err| spdlog::Error::Downstream(Box::new(Error::ValueBag(err))))?;
    Ok(cvt.0.unwrap())
}
