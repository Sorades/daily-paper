use tokio::sync::broadcast;
use tracing::Subscriber;
use tracing_subscriber::Layer;

use crate::commands::serve::LogBuffer;

/// Tracing layer that feeds formatted log lines into a [`LogBuffer`]
/// (for history replay) and a [`broadcast::Sender<String>`] (for real-time
/// SSE push to connected clients).
pub struct WebLogLayer {
    buffer: LogBuffer,
    tx: broadcast::Sender<String>,
    max_lines: usize,
}

impl WebLogLayer {
    pub fn new(buffer: LogBuffer, tx: broadcast::Sender<String>, max_lines: usize) -> Self {
        Self {
            buffer,
            tx,
            max_lines,
        }
    }
}

impl<S: Subscriber> Layer<S> for WebLogLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        let level = meta.level();
        let target = meta.target();

        // Collect fields into a simple message
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        let message = visitor.0;

        if message.is_empty() {
            return;
        }

        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
        let line = format!("{} {} {}: {}", timestamp, level, target, message);

        // Push into buffer (evict oldest if full)
        if let Ok(mut buf) = self.buffer.try_lock() {
            if buf.len() >= self.max_lines {
                buf.pop_front();
            }
            buf.push_back(line.clone());
        }

        // Broadcast to SSE clients (ignore if no receivers)
        let _ = self.tx.send(line);
    }
}

/// Simple visitor that concatenates all fields into a single string.
struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if self.0.is_empty() {
            self.0 = format!("{:?}", value);
        } else {
            self.0.push_str(&format!(" {}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            if self.0.is_empty() {
                self.0 = value.to_string();
            } else {
                self.0 = format!("{} {}", value, self.0);
            }
        } else if self.0.is_empty() {
            self.0 = format!("{}={}", field.name(), value);
        } else {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
    }
}
