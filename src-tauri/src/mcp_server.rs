//! Local MCP endpoint: `POST http://127.0.0.1:<port>/mcp` (Streamable HTTP, JSON responses).
//!
//! Listens on loopback only. Requests carrying a foreign `Origin` or `Host` header are refused,
//! which blocks DNS-rebinding attempts from web pages in a browser.

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;

use clockmanage_core::mcp;
use serde::Serialize;
use tiny_http::{Header, Method, Response, Server};

use crate::app::Shared;

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
    server: Mutex<Option<Arc<Server>>>,
    pub status: Mutex<McpStatus>,
}

pub fn random_port() -> u16 {
    use rand::Rng;
    rand::thread_rng().gen_range(20_000..=64_999)
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn allowed_origin(origin: &str, port: u16) -> bool {
    origin == "null" || origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}")
}

impl McpServer {
    pub fn stop(&self) {
        if let Some(s) = self.server.lock().unwrap().take() {
            s.unblock();
        }
        let mut st = self.status.lock().unwrap();
        st.running = false;
    }

    /// (Re)start on `port`. Returns the port actually used (a busy port is replaced).
    pub fn start(&self, shared: Arc<Shared>, enabled: bool, mut port: u16) -> u16 {
        self.stop();
        let mut st = McpStatus { enabled, port, ..Default::default() };
        if !enabled {
            *self.status.lock().unwrap() = st;
            return port;
        }
        let mut server = None;
        for attempt in 0..8 {
            if port == 0 || attempt > 0 {
                port = random_port();
            }
            match Server::http(("127.0.0.1", port)) {
                Ok(s) => {
                    server = Some(Arc::new(s));
                    break;
                }
                Err(e) => st.error = Some(format!("Порт {port} занят: {e}")),
            }
        }
        st.port = port;
        st.url = format!("http://127.0.0.1:{port}/mcp");
        let Some(server) = server else {
            *self.status.lock().unwrap() = st;
            return port;
        };
        st.running = true;
        st.error = None;
        *self.status.lock().unwrap() = st;
        *self.server.lock().unwrap() = Some(server.clone());

        thread::Builder::new()
            .name("mcp".into())
            .spawn(move || {
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
        port
    }
}
