// ===============================================================================
// BUILDOS KERNEL - HTTP CLIENT
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================
//
// HTTP/1.1 client implementation for BuildOS.
// Supports GET, POST, PUT, DELETE methods with headers and body.
//
// ===============================================================================

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::format;
use core::fmt::Write;

use super::{
    NetworkError,
    Ipv4Address, tcp::TcpConnection, dns,
};

// =============================================================================
// HTTP CONSTANTS
// =============================================================================

/// Default HTTP port
pub const HTTP_PORT: u16 = 80;

/// Default HTTPS port
pub const HTTPS_PORT: u16 = 443;

/// HTTP version string
pub const HTTP_VERSION: &str = "HTTP/1.1";

/// Maximum header size
pub const MAX_HEADER_SIZE: usize = 8192;

/// Maximum body size for buffered reads
pub const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB

/// Connection timeout in milliseconds
pub const CONNECTION_TIMEOUT_MS: u64 = 30000;

/// Read timeout in milliseconds
pub const READ_TIMEOUT_MS: u64 = 30000;

/// User agent string
pub const USER_AGENT: &str = "BuildOS/1.0";

// =============================================================================
// HTTP METHOD
// =============================================================================

/// HTTP request methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
}

impl HttpMethod {
    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Head => "HEAD",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Connect => "CONNECT",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Trace => "TRACE",
            HttpMethod::Patch => "PATCH",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(HttpMethod::Get),
            "HEAD" => Some(HttpMethod::Head),
            "POST" => Some(HttpMethod::Post),
            "PUT" => Some(HttpMethod::Put),
            "DELETE" => Some(HttpMethod::Delete),
            "CONNECT" => Some(HttpMethod::Connect),
            "OPTIONS" => Some(HttpMethod::Options),
            "TRACE" => Some(HttpMethod::Trace),
            "PATCH" => Some(HttpMethod::Patch),
            _ => None,
        }
    }
}

// =============================================================================
// HTTP STATUS
// =============================================================================

/// HTTP status code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpStatus(pub u16);

impl HttpStatus {
    // 1xx Informational
    pub const CONTINUE: Self = Self(100);
    pub const SWITCHING_PROTOCOLS: Self = Self(101);
    pub const PROCESSING: Self = Self(102);
    pub const EARLY_HINTS: Self = Self(103);

    // 2xx Success
    pub const OK: Self = Self(200);
    pub const CREATED: Self = Self(201);
    pub const ACCEPTED: Self = Self(202);
    pub const NON_AUTHORITATIVE: Self = Self(203);
    pub const NO_CONTENT: Self = Self(204);
    pub const RESET_CONTENT: Self = Self(205);
    pub const PARTIAL_CONTENT: Self = Self(206);

    // 3xx Redirection
    pub const MULTIPLE_CHOICES: Self = Self(300);
    pub const MOVED_PERMANENTLY: Self = Self(301);
    pub const FOUND: Self = Self(302);
    pub const SEE_OTHER: Self = Self(303);
    pub const NOT_MODIFIED: Self = Self(304);
    pub const TEMPORARY_REDIRECT: Self = Self(307);
    pub const PERMANENT_REDIRECT: Self = Self(308);

    // 4xx Client Errors
    pub const BAD_REQUEST: Self = Self(400);
    pub const UNAUTHORIZED: Self = Self(401);
    pub const PAYMENT_REQUIRED: Self = Self(402);
    pub const FORBIDDEN: Self = Self(403);
    pub const NOT_FOUND: Self = Self(404);
    pub const METHOD_NOT_ALLOWED: Self = Self(405);
    pub const NOT_ACCEPTABLE: Self = Self(406);
    pub const REQUEST_TIMEOUT: Self = Self(408);
    pub const CONFLICT: Self = Self(409);
    pub const GONE: Self = Self(410);
    pub const LENGTH_REQUIRED: Self = Self(411);
    pub const PAYLOAD_TOO_LARGE: Self = Self(413);
    pub const URI_TOO_LONG: Self = Self(414);
    pub const UNSUPPORTED_MEDIA: Self = Self(415);
    pub const TOO_MANY_REQUESTS: Self = Self(429);

    // 5xx Server Errors
    pub const INTERNAL_ERROR: Self = Self(500);
    pub const NOT_IMPLEMENTED: Self = Self(501);
    pub const BAD_GATEWAY: Self = Self(502);
    pub const SERVICE_UNAVAILABLE: Self = Self(503);
    pub const GATEWAY_TIMEOUT: Self = Self(504);
    pub const VERSION_NOT_SUPPORTED: Self = Self(505);

    /// Check if status is informational (1xx)
    pub fn is_informational(&self) -> bool {
        self.0 >= 100 && self.0 < 200
    }

    /// Check if status is success (2xx)
    pub fn is_success(&self) -> bool {
        self.0 >= 200 && self.0 < 300
    }

    /// Check if status is redirection (3xx)
    pub fn is_redirection(&self) -> bool {
        self.0 >= 300 && self.0 < 400
    }

    /// Check if status is client error (4xx)
    pub fn is_client_error(&self) -> bool {
        self.0 >= 400 && self.0 < 500
    }

    /// Check if status is server error (5xx)
    pub fn is_server_error(&self) -> bool {
        self.0 >= 500 && self.0 < 600
    }

    /// Check if status is error (4xx or 5xx)
    pub fn is_error(&self) -> bool {
        self.0 >= 400
    }

    /// Get reason phrase
    pub fn reason(&self) -> &'static str {
        match self.0 {
            100 => "Continue",
            101 => "Switching Protocols",
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            204 => "No Content",
            206 => "Partial Content",
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            304 => "Not Modified",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "Unknown",
        }
    }
}

// =============================================================================
// HTTP HEADERS
// =============================================================================

/// HTTP headers collection
#[derive(Debug, Clone, Default)]
pub struct HttpHeaders {
    headers: BTreeMap<String, String>,
}

impl HttpHeaders {
    pub fn new() -> Self {
        Self {
            headers: BTreeMap::new(),
        }
    }

    /// Set a header (case-insensitive)
    pub fn set(&mut self, name: &str, value: &str) {
        self.headers.insert(name.to_lowercase(), value.to_string());
    }

    /// Get a header (case-insensitive)
    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|s| s.as_str())
    }

    /// Remove a header
    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.headers.remove(&name.to_lowercase())
    }

    /// Check if header exists
    pub fn contains(&self, name: &str) -> bool {
        self.headers.contains_key(&name.to_lowercase())
    }

    /// Get Content-Length header value
    pub fn content_length(&self) -> Option<usize> {
        self.get("content-length").and_then(|v| v.parse().ok())
    }

    /// Get Content-Type header value
    pub fn content_type(&self) -> Option<&str> {
        self.get("content-type")
    }

    /// Check if Transfer-Encoding is chunked
    pub fn is_chunked(&self) -> bool {
        self.get("transfer-encoding")
            .map(|v| v.to_lowercase().contains("chunked"))
            .unwrap_or(false)
    }

    /// Check if Connection should be closed
    pub fn should_close(&self) -> bool {
        self.get("connection")
            .map(|v| v.to_lowercase() == "close")
            .unwrap_or(false)
    }

    /// Get Location header (for redirects)
    pub fn location(&self) -> Option<&str> {
        self.get("location")
    }

    /// Iterate over headers
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.headers.iter()
    }

    /// Serialize headers to string
    pub fn serialize(&self) -> String {
        let mut result = String::new();
        for (name, value) in &self.headers {
            let _ = write!(result, "{}: {}\r\n", name, value);
        }
        result
    }

    /// Parse headers from raw string
    pub fn parse(data: &str) -> Self {
        let mut headers = Self::new();

        for line in data.lines() {
            if line.is_empty() {
                break;
            }

            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim();
                let value = line[colon_pos + 1..].trim();
                headers.set(name, value);
            }
        }

        headers
    }
}

// =============================================================================
// HTTP REQUEST
// =============================================================================

/// HTTP request builder
pub struct HttpRequest {
    method: HttpMethod,
    url: Url,
    headers: HttpHeaders,
    body: Option<Vec<u8>>,
}

impl HttpRequest {
    /// Create new request
    pub fn new(method: HttpMethod, url: &str) -> Result<Self, HttpError> {
        let url = Url::parse(url)?;

        let mut headers = HttpHeaders::new();
        headers.set("Host", &url.host);
        headers.set("User-Agent", USER_AGENT);
        headers.set("Accept", "*/*");
        headers.set("Connection", "close");

        Ok(Self {
            method,
            url,
            headers,
            body: None,
        })
    }

    /// Create GET request
    pub fn get(url: &str) -> Result<Self, HttpError> {
        Self::new(HttpMethod::Get, url)
    }

    /// Create POST request
    pub fn post(url: &str) -> Result<Self, HttpError> {
        Self::new(HttpMethod::Post, url)
    }

    /// Create PUT request
    pub fn put(url: &str) -> Result<Self, HttpError> {
        Self::new(HttpMethod::Put, url)
    }

    /// Create DELETE request
    pub fn delete(url: &str) -> Result<Self, HttpError> {
        Self::new(HttpMethod::Delete, url)
    }

    /// Set a header
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.set(name, value);
        self
    }

    /// Set request body
    pub fn body(mut self, body: Vec<u8>) -> Self {
        let len = body.len();
        self.body = Some(body);
        self.headers.set("Content-Length", &len.to_string());
        self
    }

    /// Set JSON body
    pub fn json(self, json: &str) -> Self {
        self.header("Content-Type", "application/json")
            .body(json.as_bytes().to_vec())
    }

    /// Set form body
    pub fn form(self, form: &str) -> Self {
        self.header("Content-Type", "application/x-www-form-urlencoded")
            .body(form.as_bytes().to_vec())
    }

    /// Set text body
    pub fn text(self, text: &str) -> Self {
        self.header("Content-Type", "text/plain")
            .body(text.as_bytes().to_vec())
    }

    /// Serialize the request
    fn serialize(&self) -> Vec<u8> {
        let mut request = String::new();

        // Request line
        let _ = write!(
            request,
            "{} {} {}\r\n",
            self.method.as_str(),
            self.url.path_with_query(),
            HTTP_VERSION
        );

        // Headers
        request.push_str(&self.headers.serialize());

        // End of headers
        request.push_str("\r\n");

        let mut data = request.into_bytes();

        // Body
        if let Some(ref body) = self.body {
            data.extend_from_slice(body);
        }

        data
    }

    /// Send the request and get response
    pub fn send(self) -> Result<HttpResponse, HttpError> {
        send_request(self)
    }
}

// =============================================================================
// HTTP RESPONSE
// =============================================================================

/// HTTP response
pub struct HttpResponse {
    /// HTTP status
    pub status: HttpStatus,
    /// Response headers
    pub headers: HttpHeaders,
    /// Response body
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Get status code
    pub fn status_code(&self) -> u16 {
        self.status.0
    }

    /// Check if response is success
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Get body as string
    pub fn text(&self) -> Option<String> {
        String::from_utf8(self.body.clone()).ok()
    }

    /// Get body as bytes
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Get content length
    pub fn content_length(&self) -> usize {
        self.body.len()
    }
}

// =============================================================================
// URL PARSING
// =============================================================================

/// Parsed URL
#[derive(Debug, Clone)]
pub struct Url {
    /// Scheme (http or https)
    pub scheme: String,
    /// Host name or IP
    pub host: String,
    /// Port number
    pub port: u16,
    /// Path
    pub path: String,
    /// Query string (without ?)
    pub query: Option<String>,
    /// Fragment (without #)
    pub fragment: Option<String>,
}

impl Url {
    /// Parse a URL string
    pub fn parse(url: &str) -> Result<Self, HttpError> {
        let url = url.trim();

        // Extract scheme
        let (scheme, rest) = if let Some(pos) = url.find("://") {
            (&url[..pos], &url[pos + 3..])
        } else {
            ("http", url)
        };

        let scheme = scheme.to_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(HttpError::InvalidUrl);
        }

        // Default port based on scheme
        let default_port = if scheme == "https" { HTTPS_PORT } else { HTTP_PORT };

        // Extract fragment
        let (rest, fragment) = if let Some(pos) = rest.find('#') {
            (&rest[..pos], Some(rest[pos + 1..].to_string()))
        } else {
            (rest, None)
        };

        // Extract query
        let (rest, query) = if let Some(pos) = rest.find('?') {
            (&rest[..pos], Some(rest[pos + 1..].to_string()))
        } else {
            (rest, None)
        };

        // Extract path
        let (host_port, path) = if let Some(pos) = rest.find('/') {
            (&rest[..pos], rest[pos..].to_string())
        } else {
            (rest, "/".to_string())
        };

        // Extract port
        let (host, port) = if let Some(pos) = host_port.rfind(':') {
            let port_str = &host_port[pos + 1..];
            if let Ok(port) = port_str.parse::<u16>() {
                (&host_port[..pos], port)
            } else {
                (host_port, default_port)
            }
        } else {
            (host_port, default_port)
        };

        if host.is_empty() {
            return Err(HttpError::InvalidUrl);
        }

        Ok(Self {
            scheme,
            host: host.to_string(),
            port,
            path,
            query,
            fragment,
        })
    }

    /// Get full path with query string
    pub fn path_with_query(&self) -> String {
        if let Some(ref query) = self.query {
            format!("{}?{}", self.path, query)
        } else {
            self.path.clone()
        }
    }

    /// Get host with port
    pub fn host_with_port(&self) -> String {
        if (self.scheme == "http" && self.port == HTTP_PORT)
            || (self.scheme == "https" && self.port == HTTPS_PORT)
        {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

// =============================================================================
// HTTP ERRORS
// =============================================================================

/// HTTP errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    /// Invalid URL
    InvalidUrl,
    /// DNS resolution failed
    DnsError,
    /// Connection failed
    ConnectionFailed,
    /// Connection timed out
    Timeout,
    /// Network error
    NetworkError(NetworkError),
    /// Invalid response
    InvalidResponse,
    /// Response too large
    ResponseTooLarge,
    /// Redirect limit exceeded
    TooManyRedirects,
    /// SSL/TLS not supported
    TlsNotSupported,
    /// I/O error
    IoError,
}

impl From<NetworkError> for HttpError {
    fn from(e: NetworkError) -> Self {
        HttpError::NetworkError(e)
    }
}

// =============================================================================
// HTTP CLIENT
// =============================================================================

/// Maximum redirects to follow
const MAX_REDIRECTS: u32 = 10;

/// Send an HTTP request
fn send_request(request: HttpRequest) -> Result<HttpResponse, HttpError> {
    let mut current_url = request.url.clone();
    let mut redirect_count = 0;

    loop {
        // Resolve DNS
        let ip = resolve_host(&current_url.host)?;

        // Connect to server
        let socket = connect(ip, current_url.port)?;

        // Send request data
        let request_data = if redirect_count == 0 {
            request.serialize()
        } else {
            // Rebuild request for redirect
            let mut headers = request.headers.clone();
            headers.set("Host", &current_url.host);

            let mut req_str = String::new();
            let _ = write!(
                req_str,
                "{} {} {}\r\n",
                request.method.as_str(),
                current_url.path_with_query(),
                HTTP_VERSION
            );
            req_str.push_str(&headers.serialize());
            req_str.push_str("\r\n");

            let mut data = req_str.into_bytes();
            if let Some(ref body) = request.body {
                data.extend_from_slice(body);
            }
            data
        };

        socket_send(&socket, &request_data)?;

        // Receive response
        let response = receive_response(&socket)?;

        // Handle redirects
        if response.status.is_redirection() && redirect_count < MAX_REDIRECTS {
            if let Some(location) = response.headers.location() {
                // Parse redirect URL
                let new_url = if location.starts_with("http://") || location.starts_with("https://") {
                    Url::parse(location)?
                } else if location.starts_with('/') {
                    Url {
                        scheme: current_url.scheme.clone(),
                        host: current_url.host.clone(),
                        port: current_url.port,
                        path: location.to_string(),
                        query: None,
                        fragment: None,
                    }
                } else {
                    // Relative URL
                    let base_path = current_url.path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                    Url {
                        scheme: current_url.scheme.clone(),
                        host: current_url.host.clone(),
                        port: current_url.port,
                        path: format!("{}/{}", base_path, location),
                        query: None,
                        fragment: None,
                    }
                };

                current_url = new_url;
                redirect_count += 1;
                continue;
            }
        }

        return Ok(response);
    }
}

/// Resolve hostname to IP address
fn resolve_host(host: &str) -> Result<Ipv4Address, HttpError> {
    // First try to parse as IP address
    if let Some(ip) = parse_ipv4(host) {
        return Ok(ip);
    }

    // DNS lookup
    dns::resolve(host).map_err(|_| HttpError::DnsError)
}

/// Parse IPv4 address from string
fn parse_ipv4(s: &str) -> Option<Ipv4Address> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }

    let a = parts[0].parse::<u8>().ok()?;
    let b = parts[1].parse::<u8>().ok()?;
    let c = parts[2].parse::<u8>().ok()?;
    let d = parts[3].parse::<u8>().ok()?;

    Some(Ipv4Address::new(a, b, c, d))
}

/// Connect to server
fn connect(ip: Ipv4Address, port: u16) -> Result<TcpSocket, HttpError> {
    let mut socket = TcpSocket::new()?;
    socket.connect(ip, port)?;
    Ok(socket)
}

/// Send data over socket
fn socket_send(socket: &TcpSocket, data: &[u8]) -> Result<(), HttpError> {
    socket.send(data)
}

/// Receive HTTP response
fn receive_response(socket: &TcpSocket) -> Result<HttpResponse, HttpError> {
    let mut buffer = Vec::with_capacity(MAX_HEADER_SIZE);

    // Read until we find end of headers
    let header_end = loop {
        let mut chunk = [0u8; 1024];
        let n = socket.recv(&mut chunk)?;

        if n == 0 {
            return Err(HttpError::InvalidResponse);
        }

        buffer.extend_from_slice(&chunk[..n]);

        // Look for end of headers
        if let Some(pos) = find_header_end(&buffer) {
            break pos;
        }

        if buffer.len() > MAX_HEADER_SIZE {
            return Err(HttpError::ResponseTooLarge);
        }
    };

    // Parse status line and headers
    let header_str = core::str::from_utf8(&buffer[..header_end])
        .map_err(|_| HttpError::InvalidResponse)?;

    let mut lines = header_str.lines();

    // Parse status line
    let status_line = lines.next().ok_or(HttpError::InvalidResponse)?;
    let status = parse_status_line(status_line)?;

    // Parse headers
    let header_text: String = lines.collect::<Vec<_>>().join("\n");
    let headers = HttpHeaders::parse(&header_text);

    // Read body
    let body_start = header_end + 4; // Skip \r\n\r\n
    let mut body = buffer[body_start..].to_vec();

    // Determine how to read body
    if headers.is_chunked() {
        // Chunked transfer encoding
        body = read_chunked_body(socket, &body)?;
    } else if let Some(content_length) = headers.content_length() {
        // Known content length
        while body.len() < content_length {
            let mut chunk = [0u8; 4096];
            let n = socket.recv(&mut chunk)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);

            if body.len() > MAX_BODY_SIZE {
                return Err(HttpError::ResponseTooLarge);
            }
        }
    } else {
        // Read until connection closed
        loop {
            let mut chunk = [0u8; 4096];
            let n = match socket.recv(&mut chunk) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            body.extend_from_slice(&chunk[..n]);

            if body.len() > MAX_BODY_SIZE {
                return Err(HttpError::ResponseTooLarge);
            }
        }
    }

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// Find end of HTTP headers
fn find_header_end(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if data[i] == b'\r' && data[i + 1] == b'\n' && data[i + 2] == b'\r' && data[i + 3] == b'\n' {
            return Some(i);
        }
    }
    None
}

/// Parse HTTP status line
fn parse_status_line(line: &str) -> Result<HttpStatus, HttpError> {
    let parts: Vec<&str> = line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(HttpError::InvalidResponse);
    }

    let code: u16 = parts[1].parse().map_err(|_| HttpError::InvalidResponse)?;
    Ok(HttpStatus(code))
}

/// Read chunked transfer encoding body
fn read_chunked_body(socket: &TcpSocket, initial: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut buffer = initial.to_vec();
    let mut body = Vec::new();

    loop {
        // Ensure we have at least one line
        while !contains_crlf(&buffer) {
            let mut chunk = [0u8; 1024];
            let n = socket.recv(&mut chunk)?;
            if n == 0 {
                return Err(HttpError::InvalidResponse);
            }
            buffer.extend_from_slice(&chunk[..n]);
        }

        // Parse chunk size
        let line_end = find_crlf(&buffer).ok_or(HttpError::InvalidResponse)?;
        let size_str = core::str::from_utf8(&buffer[..line_end])
            .map_err(|_| HttpError::InvalidResponse)?
            .trim();

        let chunk_size = usize::from_str_radix(size_str.split(';').next().unwrap_or("0"), 16)
            .map_err(|_| HttpError::InvalidResponse)?;

        // Remove chunk size line
        buffer = buffer[line_end + 2..].to_vec();

        if chunk_size == 0 {
            // Final chunk
            break;
        }

        // Read chunk data
        while buffer.len() < chunk_size + 2 {
            let mut chunk = [0u8; 4096];
            let n = socket.recv(&mut chunk)?;
            if n == 0 {
                return Err(HttpError::InvalidResponse);
            }
            buffer.extend_from_slice(&chunk[..n]);
        }

        body.extend_from_slice(&buffer[..chunk_size]);
        buffer = buffer[chunk_size + 2..].to_vec(); // Skip chunk data + CRLF

        if body.len() > MAX_BODY_SIZE {
            return Err(HttpError::ResponseTooLarge);
        }
    }

    Ok(body)
}

fn contains_crlf(data: &[u8]) -> bool {
    data.windows(2).any(|w| w == b"\r\n")
}

fn find_crlf(data: &[u8]) -> Option<usize> {
    data.windows(2).position(|w| w == b"\r\n")
}

// =============================================================================
// TCP SOCKET WRAPPER
// =============================================================================

/// Simple TCP socket wrapper for HTTP
struct TcpSocket {
    connection: Option<Arc<TcpConnection>>,
}

impl TcpSocket {
    fn new() -> Result<Self, HttpError> {
        Ok(Self { connection: None })
    }

    fn connect(&mut self, ip: Ipv4Address, port: u16) -> Result<(), HttpError> {
        // Get network stack
        let stack = super::get_stack().ok_or(HttpError::NetworkError(NetworkError::NoRoute))?;

        // Create TCP connection
        match TcpConnection::connect(stack, ip, port) {
            Ok(conn) => {
                self.connection = Some(conn);
                Ok(())
            }
            Err(e) => Err(HttpError::NetworkError(e)),
        }
    }

    fn send(&self, data: &[u8]) -> Result<(), HttpError> {
        let conn = self.connection.as_ref().ok_or(HttpError::IoError)?;
        conn.send(data).map(|_| ()).map_err(|e| HttpError::NetworkError(e))
    }

    fn recv(&self, buf: &mut [u8]) -> Result<usize, HttpError> {
        let conn = self.connection.as_ref().ok_or(HttpError::IoError)?;
        conn.recv(buf).map_err(|e| HttpError::NetworkError(e))
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
        if let Some(conn) = self.connection.take() {
            let _ = conn.close();
        }
    }
}

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/// Perform a GET request
pub fn get(url: &str) -> Result<HttpResponse, HttpError> {
    HttpRequest::get(url)?.send()
}

/// Perform a POST request with body
pub fn post(url: &str, body: &[u8]) -> Result<HttpResponse, HttpError> {
    HttpRequest::post(url)?.body(body.to_vec()).send()
}

/// Perform a POST request with JSON body
pub fn post_json(url: &str, json: &str) -> Result<HttpResponse, HttpError> {
    HttpRequest::post(url)?.json(json).send()
}

/// Perform a PUT request with body
pub fn put(url: &str, body: &[u8]) -> Result<HttpResponse, HttpError> {
    HttpRequest::put(url)?.body(body.to_vec()).send()
}

/// Perform a DELETE request
pub fn delete(url: &str) -> Result<HttpResponse, HttpError> {
    HttpRequest::delete(url)?.send()
}

// =============================================================================
// URL ENCODING
// =============================================================================

/// URL encode a string
pub fn url_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);

    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            ' ' => {
                result.push('+');
            }
            _ => {
                for byte in c.to_string().bytes() {
                    let _ = write!(result, "%{:02X}", byte);
                }
            }
        }
    }

    result
}

/// URL decode a string
pub fn url_decode(s: &str) -> String {
    let mut result = Vec::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '+' => result.push(b' '),
            '%' => {
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte);
                }
            }
            _ => {
                for b in c.to_string().bytes() {
                    result.push(b);
                }
            }
        }
    }

    String::from_utf8_lossy(&result).into_owned()
}

/// Build query string from key-value pairs
pub fn build_query(params: &[(&str, &str)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}
