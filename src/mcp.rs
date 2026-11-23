//! Built-in MCP server, so AI clients can list, create and switch cats and change settings.
//!
//! Two transports share one JSON-RPC handler: stdio (`KuroCat.exe --mcp`, a separate process that
//! only edits files on disk) and a loopback HTTP endpoint inside the running pet.

use crate::cats::{
    all_cats, cat_exists, cat_schema_guide, cats_path, delete_custom_cat, sanitize_cat_id,
    upsert_custom_cat, CatSkinConfig,
};
use crate::config::{load_settings, save_settings, set_windows_autostart};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::Arc;

const SERVER_NAME: &str = "kurocat";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The tools this server offers, in MCP `tools/list` format.
///
/// @return The tool definitions as a JSON array.
fn tool_definitions() -> Value {
    json!([
        {
            "name": "list_cats",
            "description": "List all available KuroCat desktop pet skins (built-in and custom cats) and show which cat is currently active on screen.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "get_cat_schema",
            "description": "Get the full CatSkinConfig JSON schema, pattern options (solid, bicolor, tuxedo, tabby, calico, siamese), spot coordinate guide, and ready-to-use breed examples before creating or editing a custom cat.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "upsert_cat",
            "description": "Create a new custom cat or update an existing cat's fur colors, coat pattern, ears, eyes, tail, whiskers, keyboard highlight color, and unique pet spots/birthmarks. Changes hot-reload immediately on the running desktop pet!",
            "inputSchema": {
                "type": "object",
                "required": ["cat"],
                "properties": {
                    "cat": {
                        "type": "object",
                        "required": ["id", "name", "furColor"],
                        "properties": {
                            "id": { "type": "string", "description": "Unique lowercase slug ID, e.g. 'orange-tabby', 'my-calico', 'my-ragdoll'" },
                            "name": { "type": "string", "description": "Display name shown in menu, e.g. 'Orange Tabby', 'My Calico'" },
                            "emoji": { "type": "string", "description": "Optional emoji badge, e.g. '🍊'" },
                            "furColor": { "type": "string", "description": "Base coat hex color, e.g. '#f2a452'" },
                            "furSecondaryColor": { "type": "string" },
                            "outlineColor": { "type": "string", "description": "Outer stroke hex color for light coats, e.g. '#5d5148'" },
                            "legLineColor": { "type": "string" },
                            "pattern": { "type": "string", "enum": ["solid", "bicolor", "tuxedo", "tabby", "calico", "siamese"] },
                            "maskColor": { "type": "string" },
                            "foreheadBridgeColor": { "type": "string" },
                            "flankColor": { "type": "string" },
                            "bellyBibColor": { "type": "string" },
                            "stripeColor": { "type": "string" },
                            "calicoPatchColor": { "type": "string" },
                            "earOuterColor": { "type": "string" },
                            "earTipColor": { "type": "string" },
                            "earInnerColor": { "type": "string" },
                            "earTuftColor": { "type": "string" },
                            "tailColor": { "type": "string" },
                            "tailTipColor": { "type": "string" },
                            "tailFluffy": { "type": "boolean" },
                            "tailStripes": { "type": "boolean" },
                            "eyeColorTop": { "type": "string" },
                            "eyeColorBottom": { "type": "string" },
                            "pupilColor": { "type": "string" },
                            "eyeCatchlight": { "type": "boolean" },
                            "eyeRimColor": { "type": "string" },
                            "squintColor": { "type": "string" },
                            "squintGlowColor": { "type": "string" },
                            "noseColor": { "type": "string" },
                            "mouthColor": { "type": "string" },
                            "blushColor": { "type": "string" },
                            "whiskerColor": { "type": "string" },
                            "armColor": { "type": "string" },
                            "pawColor": { "type": "string" },
                            "pawOutlineColor": { "type": "string" },
                            "armCreaseColor": { "type": "string" },
                            "accentColor": { "type": "string", "description": "Pressed key & clicked mouse highlight hex color" },
                            "accentSideColor": { "type": "string" },
                            "spots": {
                                "type": "array",
                                "description": "Custom unique pet markings/spots on head, body, leftArm, or rightArm",
                                "items": {
                                    "type": "object",
                                    "required": ["part", "x", "y", "rx", "ry", "color"],
                                    "properties": {
                                        "part": { "type": "string", "enum": ["head", "body", "leftArm", "rightArm"] },
                                        "x": { "type": "number" },
                                        "y": { "type": "number" },
                                        "rx": { "type": "number" },
                                        "ry": { "type": "number" },
                                        "rotation": { "type": "number" },
                                        "color": { "type": "string" },
                                        "opacity": { "type": "number" }
                                    }
                                }
                            }
                        }
                    },
                    "activate": {
                        "type": "boolean",
                        "default": true,
                        "description": "Immediately switch the live desktop pet to this cat (default: true)."
                    }
                }
            }
        },
        {
            "name": "switch_cat",
            "description": "Switch the active cat displayed on the user's Windows desktop by ID (e.g. 'black', 'ragdoll', or any custom cat ID).",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string", "description": "Cat ID to activate" }
                }
            }
        },
        {
            "name": "delete_cat",
            "description": "Delete a custom cat skin (or reset a customized built-in cat back to default).",
            "inputSchema": {
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string", "description": "Custom cat ID to delete" }
                }
            }
        },
        {
            "name": "get_settings",
            "description": "Read current KuroCat desktop pet settings (active cat, scale, bubbles, petting purr sound, always-on-top, click-through lock, MCP status).",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "update_settings",
            "description": "Update KuroCat desktop pet settings live (scale 0.2-1.5, showKeyBubbles, soundEnabled, alwaysOnTop, clickThroughLocked, catBreed, keyboardLayout 65|108, keyboardOrientation cat|viewer, autostart).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "catBreed": { "type": "string" },
                    "scale": { "type": "number", "minimum": 0.2, "maximum": 1.5 },
                    "keyboardLayout": { "type": "string", "enum": ["65", "108"] },
                    "keyboardOrientation": { "type": "string", "enum": ["cat", "viewer"] },
                    "showKeyBubbles": { "type": "boolean" },
                    "soundEnabled": { "type": "boolean" },
                    "alwaysOnTop": { "type": "boolean" },
                    "clickThroughLocked": { "type": "boolean" },
                    "autostart": { "type": "boolean" }
                }
            }
        }
    ])
}

/// Wraps a JSON payload as a successful tool result.
///
/// @param payload The data to return.
/// @return The MCP tool result.
fn ok_text(payload: Value) -> Value {
    let text = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    })
}

/// Builds a failed tool result.
///
/// @param msg The error message.
/// @return The MCP tool result with `isError: true`.
fn err_text(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true
    })
}

/// Runs one tool call.
///
/// @param name The tool name.
/// @param args The tool arguments.
/// @param on_change Called after the tool changed cats or settings on disk.
/// @return The MCP tool result.
fn execute_tool(name: &str, args: &Value, on_change: &dyn Fn()) -> Value {
    match name {
        "list_cats" => {
            let settings = load_settings();
            let cats = all_cats();
            ok_text(json!({
                "activeCatId": settings.cat_breed,
                "storagePath": cats_path().to_string_lossy(),
                "count": cats.len(),
                "cats": cats
            }))
        }
        "get_cat_schema" => ok_text(cat_schema_guide()),
        "upsert_cat" => {
            let Some(cat_val) = args.get("cat") else {
                return err_text("Missing required 'cat' object argument.");
            };
            let parsed: CatSkinConfig = match serde_json::from_value(cat_val.clone()) {
                Ok(c) => c,
                Err(e) => return err_text(&format!("Invalid cat skin JSON: {}", e)),
            };
            match upsert_custom_cat(parsed) {
                Ok(saved) => {
                    let activate = args
                        .get("activate")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    if activate {
                        let mut s = load_settings();
                        s.cat_breed = saved.id.clone();
                        save_settings(&s);
                    }
                    on_change();
                    ok_text(json!({
                        "status": "saved",
                        "activated": activate,
                        "cat": saved
                    }))
                }
                Err(e) => err_text(&format!("Failed to save custom cat: {}", e)),
            }
        }
        "switch_cat" => {
            let Some(id_raw) = args.get("id").and_then(|v| v.as_str()) else {
                return err_text("Missing required 'id' string argument.");
            };
            let id = sanitize_cat_id(id_raw);
            if !cat_exists(&id) {
                let ids: Vec<String> = all_cats().into_iter().map(|c| c.id).collect();
                return err_text(&format!(
                    "Cat '{}' not found. Available IDs: {:?}",
                    id, ids
                ));
            }
            let mut s = load_settings();
            s.cat_breed = id.clone();
            save_settings(&s);
            on_change();
            ok_text(json!({
                "status": "switched",
                "activeCatId": id
            }))
        }
        "delete_cat" => {
            let Some(id_raw) = args.get("id").and_then(|v| v.as_str()) else {
                return err_text("Missing required 'id' string argument.");
            };
            let id = sanitize_cat_id(id_raw);
            match delete_custom_cat(&id) {
                Ok(removed) => {
                    if removed {
                        let mut s = load_settings();
                        if s.cat_breed == id && !cat_exists(&id) {
                            s.cat_breed = "black".to_string();
                            save_settings(&s);
                        }
                        on_change();
                        ok_text(json!({
                            "status": "deleted",
                            "id": id
                        }))
                    } else {
                        err_text(&format!("No custom cat entry found for '{}'.", id))
                    }
                }
                Err(e) => err_text(&format!("Failed to delete cat: {}", e)),
            }
        }
        "get_settings" => ok_text(serde_json::to_value(load_settings()).unwrap_or(json!({}))),
        "update_settings" => {
            let mut s = load_settings();
            if let Some(breed) = args.get("catBreed").and_then(|v| v.as_str()) {
                let clean = sanitize_cat_id(breed);
                if cat_exists(&clean) {
                    s.cat_breed = clean;
                } else {
                    return err_text(&format!("Unknown catBreed '{}'.", clean));
                }
            }
            if let Some(scale) = args.get("scale").and_then(|v| v.as_f64()) {
                s.scale = scale.clamp(0.20, 1.50);
            }
            if let Some(layout) = args.get("keyboardLayout").and_then(|v| v.as_str()) {
                s.keyboard_layout = crate::config::normalize_keyboard_layout(layout);
            }
            if let Some(orient) = args.get("keyboardOrientation").and_then(|v| v.as_str()) {
                s.keyboard_orientation = crate::config::normalize_keyboard_orientation(orient);
            }
            if let Some(b) = args.get("showKeyBubbles").and_then(|v| v.as_bool()) {
                s.show_key_bubbles = b;
            }
            if let Some(b) = args.get("soundEnabled").and_then(|v| v.as_bool()) {
                s.sound_enabled = b;
            }
            if let Some(b) = args.get("alwaysOnTop").and_then(|v| v.as_bool()) {
                s.always_on_top = b;
            }
            if let Some(b) = args.get("clickThroughLocked").and_then(|v| v.as_bool()) {
                s.click_through_locked = b;
            }
            if let Some(b) = args.get("autostart").and_then(|v| v.as_bool()) {
                if set_windows_autostart(b).is_ok() {
                    s.autostart = b;
                }
            }
            save_settings(&s);
            on_change();
            ok_text(serde_json::to_value(&s).unwrap_or(json!({})))
        }
        _ => err_text(&format!("Unknown tool: {}", name)),
    }
}

/// Handles a JSON-RPC message from a client: a single request or a batch (array) of them.
///
/// @param message The parsed message.
/// @param on_change Called after a tool changed cats or settings on disk.
/// @return The response (an array for a batch), or None when nothing should be sent back.
pub fn handle_jsonrpc_message(message: &Value, on_change: &dyn Fn()) -> Option<Value> {
    let Some(batch) = message.as_array() else {
        return handle_jsonrpc(message, on_change);
    };
    let mut responses = Vec::new();
    for item in batch {
        if let Some(response) = handle_jsonrpc(item, on_change) {
            responses.push(response);
        }
    }
    if responses.is_empty() {
        None
    } else {
        Some(Value::Array(responses))
    }
}

/// Handles one JSON-RPC request or notification (not a batch).
///
/// @param req The parsed message.
/// @param on_change Called after a tool changed cats or settings on disk.
/// @return The response, or None for notifications and client responses.
pub fn handle_jsonrpc(req: &Value, on_change: &dyn Fn()) -> Option<Value> {
    // A message without a method is the client's answer to a request; nothing is sent back.
    if req.get("method").is_none() {
        return None;
    }
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));

    // Notifications (no id) must not get a JSON-RPC response.
    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }
    let id = id.unwrap();

    let result = match method {
        "initialize" => {
            let client_ver = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05");
            let ver = match client_ver {
                "2025-06-18" | "2025-03-26" | "2024-11-05" => client_ver,
                _ => "2024-11-05",
            };
            Ok(json!({
                "protocolVersion": ver,
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                },
                "instructions": "Use get_cat_schema to inspect the custom cat design format, then call upsert_cat (with activate: true) to create or customize desktop pet cats in real time on the user's Windows desktop."
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            Ok(execute_tool(tool_name, &args, on_change))
        }
        _ => Err((-32601, format!("Method not found: {}", method))),
    };

    Some(match result {
        Ok(res) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": res
        }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }),
    })
}

/// Runs the stdio MCP server (`KuroCat.exe --mcp`): one JSON-RPC message per line on stdin and
/// stdout. Does not need the pet's app state; the running pet picks up file changes by itself.
pub fn run_stdio_mcp() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let noop = || {};

    for line_res in stdin.lock().lines() {
        let Ok(line) = line_res else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                });
                let _ = writeln!(out, "{}", err_resp);
                let _ = out.flush();
                continue;
            }
        };

        if let Some(resp) = handle_jsonrpc_message(&parsed, &noop) {
            let _ = writeln!(out, "{}", resp);
            let _ = out.flush();
        }
    }
}

/// Starts the loopback-only HTTP MCP endpoint (`http://127.0.0.1:<port>/mcp`) on a background thread.
/// Requests are refused while the MCP switch is off in the settings.
///
/// @param port The TCP port to listen on.
/// @param on_change Called after a tool changed cats or settings on disk.
pub fn start_http_mcp_server<F>(port: u16, on_change: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let on_change = Arc::new(on_change);
    std::thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", port);
        let Ok(listener) = TcpListener::bind(&addr) else {
            return;
        };
        for stream_res in listener.incoming() {
            let Ok(stream) = stream_res else {
                continue;
            };
            let notify = on_change.clone();
            std::thread::spawn(move || {
                handle_http_conn(stream, &*notify);
            });
        }
    });
}

/// Reports whether a host name (without port) is this machine.
///
/// @param name The host name.
/// @return True for localhost, 127.0.0.1 and [::1].
fn is_loopback_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "localhost" || lower == "127.0.0.1" || lower == "[::1]"
}

/// Removes an optional ":port" suffix from a host value such as "127.0.0.1:27420" or "[::1]:80".
///
/// @param value The host with or without a port.
/// @return The host part.
fn strip_port(value: &str) -> &str {
    if value.starts_with('[') {
        match value.find(']') {
            Some(end) => &value[..=end],
            None => value,
        }
    } else {
        match value.rsplit_once(':') {
            Some((name, _port)) => name,
            None => value,
        }
    }
}

/// Reports whether an HTTP Origin header belongs to a page served from this machine.
///
/// @param origin The Origin header value, e.g. "http://localhost:3000".
/// @return True for http(s) origins on localhost, 127.0.0.1 or [::1].
fn is_local_origin(origin: &str) -> bool {
    let without_scheme = if let Some(rest) = origin.strip_prefix("http://") {
        rest
    } else if let Some(rest) = origin.strip_prefix("https://") {
        rest
    } else {
        return false;
    };
    is_loopback_name(strip_port(without_scheme.trim_end_matches('/')))
}

/// Reports whether an HTTP Host header names this machine.
///
/// @param host The Host header value, e.g. "127.0.0.1:27420".
/// @return True for localhost, 127.0.0.1 or [::1], with any port.
fn is_local_host(host: &str) -> bool {
    is_loopback_name(strip_port(host))
}

/// Writes a complete HTTP response and flushes it.
///
/// @param stream The client connection.
/// @param status Status line text, e.g. "200 OK".
/// @param content_type The Content-Type header value.
/// @param body The response body.
fn write_http_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) {
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        body.as_bytes().len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

/// Serves one HTTP connection.
///
/// @param stream The client connection.
/// @param on_change Called after a tool changed cats or settings on disk.
fn handle_http_conn(mut stream: TcpStream, on_change: &dyn Fn()) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    let mut content_length: usize = 0;
    let mut origin: Option<String> = None;
    let mut host: Option<String> = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            return;
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            } else if k.eq_ignore_ascii_case("origin") {
                origin = Some(v.trim().to_string());
            } else if k.eq_ignore_ascii_case("host") {
                host = Some(v.trim().to_string());
            }
        }
    }

    // Only local clients may talk to the server. Web pages send an Origin header, and DNS
    // rebinding attacks send a foreign Host header; both are refused.
    let origin_allowed = match origin.as_deref() {
        None => true,
        Some(value) => is_local_origin(value),
    };
    let host_allowed = match host.as_deref() {
        None => true,
        Some(value) => is_local_host(value),
    };
    if !origin_allowed || !host_allowed {
        write_http_response(
            &mut stream,
            "403 Forbidden",
            "application/json; charset=utf-8",
            r#"{"error":"Only local clients may use the KuroCat MCP server"}"#,
        );
        return;
    }

    if method == "OPTIONS" {
        write_http_response(&mut stream, "204 No Content", "text/plain", "");
        return;
    }

    if !crate::app::state().mcp_enabled.load(Ordering::Relaxed) {
        write_http_response(
            &mut stream,
            "403 Forbidden",
            "application/json; charset=utf-8",
            r#"{"error":"MCP server is disabled in KuroCat settings"}"#,
        );
        return;
    }

    if method == "GET" && (path == "/health" || path == "/") {
        let body = json!({
            "ok": true,
            "name": SERVER_NAME,
            "version": SERVER_VERSION,
            "mcpEndpoint": "/mcp"
        })
        .to_string();
        write_http_response(&mut stream, "200 OK", "application/json; charset=utf-8", &body);
        return;
    }

    if method == "POST" && (path == "/mcp" || path == "/") {
        if content_length > 2 * 1024 * 1024 {
            write_http_response(
                &mut stream,
                "413 Payload Too Large",
                "application/json",
                r#"{"error":"Payload too large"}"#,
            );
            return;
        }
        let mut buf = vec![0u8; content_length];
        if content_length > 0 && reader.read_exact(&mut buf).is_err() {
            return;
        }
        let parsed: Value = match serde_json::from_slice(&buf) {
            Ok(v) => v,
            Err(e) => {
                let err = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                })
                .to_string();
                write_http_response(&mut stream, "400 Bad Request", "application/json; charset=utf-8", &err);
                return;
            }
        };

        if let Some(resp) = handle_jsonrpc_message(&parsed, on_change) {
            let body = resp.to_string();
            write_http_response(&mut stream, "200 OK", "application/json; charset=utf-8", &body);
        } else {
            write_http_response(&mut stream, "202 Accepted", "application/json", "");
        }
        return;
    }

    if path == "/mcp" {
        // No server-sent event stream is offered; the Streamable HTTP spec asks for 405 here.
        let _ = stream.write_all(
            b"HTTP/1.1 405 Method Not Allowed\r\nAllow: POST\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        let _ = stream.flush();
        return;
    }

    write_http_response(
        &mut stream,
        "404 Not Found",
        "application/json; charset=utf-8",
        r#"{"error":"Not found. Use POST /mcp or GET /health"}"#,
    );
}

#[cfg(test)]
mod origin_tests {
    use super::*;

    #[test]
    fn accepts_only_local_origins_and_hosts() {
        assert!(is_local_origin("http://localhost:3000"));
        assert!(is_local_origin("http://127.0.0.1"));
        assert!(is_local_origin("https://[::1]:8443"));
        assert!(!is_local_origin("https://evil.example"));
        assert!(!is_local_origin("http://localhost.evil.example"));
        assert!(!is_local_origin("null"));
        assert!(is_local_host("127.0.0.1:27420"));
        assert!(is_local_host("localhost"));
        assert!(!is_local_host("rebind.example:27420"));
    }
}
