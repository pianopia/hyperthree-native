use anyhow::{Context as _, Result};
use boa_engine::{Context, Source};
use std::path::Path;

/// JavaScript execution boundary.
///
/// This first vertical slice uses an embeddable ECMAScript runtime so the
/// host can execute the game entry point without a browser. The public shape
/// intentionally stays small: the runtime can be replaced by Embedded V8
/// without changing the window, asset, or renderer layers.
pub struct JsRuntime {
    context: Context,
}

impl JsRuntime {
    pub fn new() -> Self {
        Self {
            context: Context::default(),
        }
    }

    pub fn execute_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read JavaScript entry point {}", path.display()))?;
        self.execute_source(&source)
    }

    pub fn execute_source(&mut self, source: &str) -> Result<()> {
        self.context
            .eval(Source::from_bytes(source))
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("JavaScript evaluation failed: {error}"))
    }
}
