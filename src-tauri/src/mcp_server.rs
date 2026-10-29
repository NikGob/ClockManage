//! Local MCP endpoint: `POST http://127.0.0.1:<port>/mcp` (Streamable HTTP, JSON responses).
//!
//! Listens on loopback only. Requests carrying a foreign `Origin` or `Host` header are refused,
//! which blocks DNS-rebinding attempts from web pages in a browser.

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clockmanage_core::mcp;
use serde::Serialize;
use tiny_http::{Header, Method, Response, Server};

use crate::app::Shared;
use clockmanage_core::tr;

#[derive(Debug, Clone, Default, Serialize)]
pub struct McpStatus {
    pub enabled: bool,
    pub port: u16,
    pub running: bool,
    pub url: String,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct McpServer {
    /// The running server, and a count of stops: a thread still waiting for its port gives up
    /// once the count moves on.
    server: Mutex<(u64, Option<Arc<Server>>)>,
    pub status: Mutex<McpStatus>,
}

/// How often a busy port is tried again.
const RETRY: Duration = Duration::from_secs(2);

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

/// No `null`: a sandboxed iframe or a data: page sends that, so it is any web page.
fn allowed_origin(origin: &str, port: u16) -> bool {
    origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}")
}

impl McpServer {
    pub fn stop(&self) {
        let mut slot = self.server.lock().unwrap();
        slot.0 += 1;
        if let Some(s) = slot.1.take() {
            s.unblock();
        }
        drop(slot);
        self.status.lock().unwrap().running = false;
    }

    /// (Re)start on `port`. The port never moves by itself, or MCP clients would keep knocking
    /// on the old address: while another program (or the copy of the app that is closing) holds
    /// it, the status says so and the port is tried again until it is free.
    pub fn start(&self, shared: Arc<Shared>, enabled: bool, port: u16) {
        self.stop();
        let url = format!("http://127.0.0.1:{port}/mcp");
        *self.status.lock().unwrap() = McpStatus { enabled, port, url, ..Default::default() };
        if !enabled {
            return;
        }
        let run = self.server.lock().unwrap().0;
        thread::Builder::new()
            .name("mcp".into())
            .spawn(move || {
                let Some(server) = shared.mcp.bind(port, run) else { return };
                let host = crate::app::McpBridge(shared);
                for mut req in server.incoming_requests() {
                    let origin_ok = req
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("Origin"))
                        .map(|h| allowed_origin(h.value.as_str(), port))
                        .unwrap_or(true);
                    let host_ok = req
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("Host"))
                        .map(|h| {
                            let v = h.value.as_str();
                            v == format!("127.0.0.1:{port}") || v == format!("localhost:{port}")
                        })
                        .unwrap_or(false);
                    if !origin_ok || !host_ok {
                        let _ = req.respond(Response::from_string("Forbidden").with_status_code(403));
                        continue;
                    }
                    let path = req.url().split('?').next().unwrap_or("").to_string();
                    if path != "/mcp" && path != "/" {
                        let _ = req.respond(Response::from_string("Not found").with_status_code(404));
                        continue;
                    }
                    if *req.method() != Method::Post {
                        let _ = req.respond(
                            Response::from_string("Method not allowed").with_status_code(405).with_header(header("Allow", "POST")),
                        );
                        continue;
                    }
                    // MCP clients always send JSON. A page can only POST text/plain or a form
                    // without a preflight, and the preflight (OPTIONS) gets 405 above.
                    let json_body = req
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("Content-Type"))
                        .is_some_and(|h| h.value.as_str().to_ascii_lowercase().starts_with("application/json"));
                    if !json_body {
                        let _ = req.respond(Response::from_string("Unsupported media type").with_status_code(415));
                        continue;
                    }
                    let mut body = String::new();
                    if req.as_reader().take(1 << 20).read_to_string(&mut body).is_err() {
                        let _ = req.respond(Response::from_string("Bad request").with_status_code(400));
                        continue;
                    }
                    match mcp::handle(&body, &host) {
                        Some(json) => {
                            let _ = req.respond(Response::from_string(json).with_header(header("Content-Type", "application/json")));
                        }
                        None => {
                            let _ = req.respond(Response::empty(202));
                        }
                    }
                }
            })
            .expect("spawn mcp thread");
    }

    /// `port` bound, waiting while it is busy. None once a stop came after start number `run`.
    fn bind(&self, port: u16, run: u64) -> Option<Arc<Server>> {
        loop {
            let bound = Server::http(("127.0.0.1", port));
            {
                let mut slot = self.server.lock().unwrap();
                if slot.0 != run {
                    return None;
                }
                let mut st = self.status.lock().unwrap();
                match bound {
                    Ok(s) => {
                        let s = Arc::new(s);
                        slot.1 = Some(s.clone());
                        st.running = true;
                        st.error = None;
                        return Some(s);
                    }
                    Err(e) => st.error = Some(tr!("mcp_port_is_busy", port = port, e = e)),
                }
            }
            thread::sleep(RETRY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_port_is_waited_for_not_replaced() {
        let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = other.local_addr().unwrap().port();
        let mcp = Arc::new(McpServer::default());
        let run = mcp.server.lock().unwrap().0;
        let waiting = thread::spawn({
            let mcp = mcp.clone();
            move || mcp.bind(port, run).is_some()
        });
        thread::sleep(Duration::from_millis(300));
        assert!(mcp.status.lock().unwrap().error.is_some());
        drop(other);
        assert!(waiting.join().unwrap());
        let st = mcp.status.lock().unwrap().clone();
        assert!(st.running && st.error.is_none());

        // A stop ends the wait: the old port is given up, not taken later.
        let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = other.local_addr().unwrap().port();
        mcp.stop();
        let run = mcp.server.lock().unwrap().0;
        let waiting = thread::spawn({
            let mcp = mcp.clone();
            move || mcp.bind(port, run).is_some()
        });
        thread::sleep(Duration::from_millis(300));
        mcp.stop();
        assert!(!waiting.join().unwrap());
        drop(other);
    }
}
