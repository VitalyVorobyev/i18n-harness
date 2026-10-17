//! Minimal blocking mock HTTP server for backend integration tests.
//!
//! Each test spawns a `TcpListener` on a random loopback port that answers
//! a fixed sequence of connections with canned responses. No async runtime,
//! no external mock crate — just `std::net`.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpListener;

/// A canned HTTP/1.1 200 response with the given JSON body.
pub(crate) fn ok_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

/// A canned HTTP/1.1 response with an arbitrary status and no body.
pub(crate) fn status_response(code: u16, reason: &str) -> String {
    format!("HTTP/1.1 {code} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
}

/// Read an HTTP request from `stream` until the header block ends.
/// Also reads the body if `Content-Length` is present in the headers.
pub(crate) fn drain_request(stream: &mut std::net::TcpStream) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1];
    // Read byte-by-byte until \r\n\r\n.
    loop {
        if stream.read(&mut tmp).unwrap_or(0) == 0 {
            break;
        }
        buf.push(tmp[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    // Parse Content-Length and drain the body so the response is not
    // sent before the client has finished sending its request.
    let headers = String::from_utf8_lossy(&buf);
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().unwrap_or(0))
        })
        .unwrap_or(0);
    if content_length > 0 {
        let mut body_buf = vec![0u8; content_length];
        let _ = stream.read_exact(&mut body_buf);
    }
}

/// Spawn a mock server that handles `conn_count` connections sequentially.
///
/// `responses` must have exactly `conn_count` entries. The server sends
/// `responses[i]` for the i-th connection, in order.
///
/// Returns `(host_url, join_handle)`.
pub(crate) fn spawn_mock_server(responses: Vec<String>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let host = format!("http://{addr}");
    let handle = std::thread::spawn(move || {
        for response in responses {
            if let Ok((mut stream, _)) = listener.accept() {
                drain_request(&mut stream);
                let _ = stream.write_all(response.as_bytes());
            }
        }
    });
    (host, handle)
}

/// Like [`spawn_mock_server`], but also returns each request (first line and
/// body) in the order received.
pub(crate) fn spawn_capturing_server(
    responses: Vec<String>,
) -> (String, std::sync::mpsc::Receiver<(String, String)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            let length: usize = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap_or(0))
                })
                .unwrap_or(0);
            let mut body = vec![0u8; length];
            let _ = stream.read_exact(&mut body);
            let first = head.lines().next().unwrap_or_default().to_owned();
            let _ = tx.send((first, String::from_utf8_lossy(&body).into_owned()));
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (host, rx)
}
