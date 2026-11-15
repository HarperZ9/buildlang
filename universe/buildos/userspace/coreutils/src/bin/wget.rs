//! BuildOS wget - Network file download utility
//!
//! A command-line utility for downloading files from HTTP/HTTPS servers.
//! Similar to GNU wget with essential functionality.

#![no_std]
#![no_main]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_assignments)]
#![allow(dead_code)]
#![allow(static_mut_refs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::vec;
use alloc::format;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

// =============================================================================
// SIMPLE BUMP ALLOCATOR
// =============================================================================

const HEAP_SIZE: usize = 1024 * 1024; // 1 MB heap
static mut HEAP: [u8; HEAP_SIZE] = [0; HEAP_SIZE];
static HEAP_POS: AtomicUsize = AtomicUsize::new(0);

struct BumpAllocator;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        loop {
            let pos = HEAP_POS.load(Ordering::Relaxed);
            let aligned = (pos + align - 1) & !(align - 1);
            let new_pos = aligned + size;

            if new_pos > HEAP_SIZE {
                return core::ptr::null_mut();
            }

            if HEAP_POS.compare_exchange_weak(pos, new_pos, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                return HEAP.as_mut_ptr().add(aligned);
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't deallocate
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

// =============================================================================
// PANIC HANDLER
// =============================================================================

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // Simple panic - just exit
    extern "C" {
        fn syscall_exit(code: i32) -> !;
    }
    unsafe { syscall_exit(1) }
}

// Maximum buffer sizes
const MAX_URL_LENGTH: usize = 2048;
const MAX_HEADER_SIZE: usize = 8192;
const DOWNLOAD_BUFFER_SIZE: usize = 65536;
const MAX_REDIRECTS: usize = 10;
const MAX_FILENAME_LENGTH: usize = 256;

// =============================================================================
// OUTPUT FUNCTIONS AND MACROS
// =============================================================================

extern "C" {
    fn syscall_write(fd: i32, data: *const u8, len: usize) -> isize;
}

/// Print to stdout with newline
fn println(s: &str) {
    unsafe {
        syscall_write(1, s.as_ptr(), s.len());
        syscall_write(1, b"\n".as_ptr(), 1);
    }
}

/// Print to stdout (no newline)
fn print(s: &str) {
    unsafe {
        syscall_write(1, s.as_ptr(), s.len());
    }
}

/// Print formatted string with newline
macro_rules! println {
    () => { println("") };
    ($s:expr) => { println($s) };
    ($fmt:expr, $($arg:tt)*) => {
        println(&format!($fmt, $($arg)*))
    };
}

/// Print formatted string
macro_rules! print {
    ($s:expr) => { print($s) };
    ($fmt:expr, $($arg:tt)*) => {
        print(&format!($fmt, $($arg)*))
    };
}

// =============================================================================
// DATA STRUCTURES
// =============================================================================

/// URL components
#[derive(Clone)]
struct Url {
    scheme: String,
    host: String,
    port: u16,
    path: String,
    query: Option<String>,
}

impl Url {
    /// Parse a URL string
    fn parse(url_str: &str) -> Option<Self> {
        let url_str = url_str.trim();

        // Parse scheme
        let (scheme, rest) = if url_str.starts_with("https://") {
            ("https".to_string(), &url_str[8..])
        } else if url_str.starts_with("http://") {
            ("http".to_string(), &url_str[7..])
        } else {
            // Default to http
            ("http".to_string(), url_str)
        };

        // Find host/port/path separation
        let (host_port, path_query) = match rest.find('/') {
            Some(idx) => (&rest[..idx], &rest[idx..]),
            None => (rest, "/"),
        };

        // Parse host and port
        let (host, port) = match host_port.find(':') {
            Some(idx) => {
                let port_str = &host_port[idx + 1..];
                let port = port_str.parse().ok()?;
                (host_port[..idx].to_string(), port)
            }
            None => {
                let default_port = if scheme == "https" { 443 } else { 80 };
                (host_port.to_string(), default_port)
            }
        };

        if host.is_empty() {
            return None;
        }

        // Parse path and query
        let (path, query) = match path_query.find('?') {
            Some(idx) => (
                path_query[..idx].to_string(),
                Some(path_query[idx + 1..].to_string()),
            ),
            None => (path_query.to_string(), None),
        };

        Some(Self {
            scheme,
            host,
            port,
            path,
            query,
        })
    }

    /// Get the full path including query string
    fn full_path(&self) -> String {
        match &self.query {
            Some(q) => format!("{}?{}", self.path, q),
            None => self.path.clone(),
        }
    }

    /// Extract filename from URL path
    fn filename(&self) -> String {
        let path = self.path.trim_end_matches('/');
        match path.rfind('/') {
            Some(idx) => {
                let name = &path[idx + 1..];
                if name.is_empty() {
                    "index.html".to_string()
                } else {
                    name.to_string()
                }
            }
            None => "index.html".to_string(),
        }
    }
}

/// HTTP response status
#[derive(Clone, Copy, Debug)]
struct HttpStatus {
    code: u16,
    version_major: u8,
    version_minor: u8,
}

impl HttpStatus {
    fn is_success(&self) -> bool {
        self.code >= 200 && self.code < 300
    }

    fn is_redirect(&self) -> bool {
        self.code == 301 || self.code == 302 || self.code == 303 ||
        self.code == 307 || self.code == 308
    }
}

/// HTTP header
#[derive(Clone)]
struct HttpHeader {
    name: String,
    value: String,
}

/// HTTP response
struct HttpResponse {
    status: HttpStatus,
    headers: Vec<HttpHeader>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn new() -> Self {
        Self {
            status: HttpStatus { code: 0, version_major: 1, version_minor: 1 },
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Get header value by name (case-insensitive)
    fn get_header(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_ascii_lowercase();
        for header in &self.headers {
            if header.name.to_ascii_lowercase() == name_lower {
                return Some(&header.value);
            }
        }
        None
    }

    /// Get content length
    fn content_length(&self) -> Option<usize> {
        self.get_header("content-length")
            .and_then(|v| v.parse().ok())
    }

    /// Get redirect location
    fn redirect_location(&self) -> Option<&str> {
        self.get_header("location")
    }
}

/// Download options
struct WgetOptions {
    /// Output filename (None = derive from URL)
    output_file: Option<String>,
    /// Continue partial download
    continue_download: bool,
    /// Quiet mode (no output)
    quiet: bool,
    /// Number of retries
    tries: usize,
    /// Timeout in seconds
    timeout: u32,
    /// User agent string
    user_agent: String,
    /// Follow redirects
    follow_redirects: bool,
    /// Max redirects
    max_redirects: usize,
    /// Print headers only
    headers_only: bool,
    /// Spider mode (check existence)
    spider: bool,
    /// Verbose output
    verbose: bool,
    /// Server response headers
    save_headers: bool,
    /// Show progress bar
    progress: bool,
    /// HTTP username
    http_user: Option<String>,
    /// HTTP password
    http_password: Option<String>,
    /// Referer URL
    referer: Option<String>,
    /// Custom headers
    custom_headers: Vec<(String, String)>,
}

impl Default for WgetOptions {
    fn default() -> Self {
        Self {
            output_file: None,
            continue_download: false,
            quiet: false,
            tries: 3,
            timeout: 30,
            user_agent: "BuildOS-wget/1.0".to_string(),
            follow_redirects: true,
            max_redirects: MAX_REDIRECTS,
            headers_only: false,
            spider: false,
            verbose: false,
            save_headers: false,
            progress: true,
            http_user: None,
            http_password: None,
            referer: None,
            custom_headers: Vec::new(),
        }
    }
}

/// Wget downloader
struct Wget {
    options: WgetOptions,
    redirect_count: usize,
}

impl Wget {
    fn new(options: WgetOptions) -> Self {
        Self {
            options,
            redirect_count: 0,
        }
    }

    /// Build HTTP request
    fn build_request(&self, url: &Url, start_pos: usize) -> String {
        let method = if self.options.headers_only { "HEAD" } else { "GET" };

        let mut request = format!(
            "{} {} HTTP/1.1\r\n\
             Host: {}\r\n\
             User-Agent: {}\r\n\
             Accept: */*\r\n\
             Connection: close\r\n",
            method,
            url.full_path(),
            url.host,
            self.options.user_agent
        );

        // Add range header for resume
        if start_pos > 0 {
            request.push_str(&format!("Range: bytes={}-\r\n", start_pos));
        }

        // Add referer
        if let Some(ref referer) = self.options.referer {
            request.push_str(&format!("Referer: {}\r\n", referer));
        }

        // Add basic authentication
        if let (Some(ref user), Some(ref password)) = (&self.options.http_user, &self.options.http_password) {
            let credentials = format!("{}:{}", user, password);
            let encoded = base64_encode(credentials.as_bytes());
            request.push_str(&format!("Authorization: Basic {}\r\n", encoded));
        }

        // Add custom headers
        for (name, value) in &self.options.custom_headers {
            request.push_str(&format!("{}: {}\r\n", name, value));
        }

        request.push_str("\r\n");
        request
    }

    /// Parse HTTP response headers
    fn parse_response_headers(&self, data: &[u8]) -> Option<(HttpResponse, usize)> {
        // Find header end
        let header_end = find_header_end(data)?;

        let header_str = core::str::from_utf8(&data[..header_end]).ok()?;
        let mut lines = header_str.lines();

        // Parse status line
        let status_line = lines.next()?;
        let status = self.parse_status_line(status_line)?;

        let mut response = HttpResponse::new();
        response.status = status;

        // Parse headers
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some(idx) = line.find(':') {
                let name = line[..idx].trim().to_string();
                let value = line[idx + 1..].trim().to_string();
                response.headers.push(HttpHeader { name, value });
            }
        }

        Some((response, header_end + 4)) // +4 for \r\n\r\n
    }

    /// Parse HTTP status line
    fn parse_status_line(&self, line: &str) -> Option<HttpStatus> {
        // HTTP/1.1 200 OK
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() < 2 {
            return None;
        }

        let version = parts[0].strip_prefix("HTTP/")?;
        let version_parts: Vec<&str> = version.split('.').collect();
        let version_major = version_parts.get(0)?.parse().ok()?;
        let version_minor = version_parts.get(1).unwrap_or(&"0").parse().ok()?;

        let code = parts[1].parse().ok()?;

        Some(HttpStatus {
            code,
            version_major,
            version_minor,
        })
    }

    /// Download file from URL
    fn download(&mut self, url_str: &str) -> Result<(), WgetError> {
        let url = Url::parse(url_str).ok_or(WgetError::InvalidUrl)?;

        if !self.options.quiet {
            println!("--{}", timestamp());
            println!("  {}", url_str);
            println!("Resolving {}...", url.host);
        }

        // Check for HTTPS (we'd need TLS support)
        if url.scheme == "https" {
            if !self.options.quiet {
                println!("Note: HTTPS support requires TLS. Attempting connection...");
            }
        }

        // Connect to server
        let socket = self.connect(&url)?;

        // Get existing file size for resume
        let start_pos = if self.options.continue_download {
            self.get_existing_file_size(&url)
        } else {
            0
        };

        // Build and send request
        let request = self.build_request(&url, start_pos);
        self.send_data(&socket, request.as_bytes())?;

        // Receive response
        let response = self.receive_response(&socket)?;

        // Handle redirect
        if response.status.is_redirect() && self.options.follow_redirects {
            if self.redirect_count >= self.options.max_redirects {
                return Err(WgetError::TooManyRedirects);
            }

            if let Some(location) = response.redirect_location() {
                self.redirect_count += 1;
                if !self.options.quiet {
                    println!("Location: {} [following]", location);
                }

                // Handle relative redirects
                let new_url = if location.starts_with("http://") || location.starts_with("https://") {
                    location.to_string()
                } else if location.starts_with('/') {
                    format!("{}://{}:{}{}", url.scheme, url.host, url.port, location)
                } else {
                    let base_path = url.path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                    format!("{}://{}:{}/{}/{}", url.scheme, url.host, url.port, base_path, location)
                };

                self.close_socket(&socket);
                return self.download(&new_url);
            }
        }

        // Check status
        if !response.status.is_success() && response.status.code != 206 {
            if !self.options.quiet {
                println!("Error: Server returned status {}", response.status.code);
            }
            return Err(WgetError::HttpError(response.status.code));
        }

        // Spider mode - just check existence
        if self.options.spider {
            if !self.options.quiet {
                println!("Remote file exists.");
            }
            self.close_socket(&socket);
            return Ok(());
        }

        // Headers only mode
        if self.options.headers_only {
            self.print_headers(&response);
            self.close_socket(&socket);
            return Ok(());
        }

        // Determine output filename
        let output_file = self.options.output_file.clone()
            .unwrap_or_else(|| url.filename());

        // Get content length
        let content_length = response.content_length();

        if !self.options.quiet {
            println!("Length: {} [{}]",
                content_length.map(|l| format!("{}", l)).unwrap_or_else(|| "unspecified".to_string()),
                response.get_header("content-type").unwrap_or("unknown")
            );
            println!("Saving to: '{}'", output_file);
            println!();
        }

        // Save the body to file
        let bytes_written = self.save_to_file(&output_file, &response.body, start_pos, content_length)?;

        if !self.options.quiet {
            println!();
            println!("'{}' saved [{}/{}]",
                output_file,
                bytes_written + start_pos,
                content_length.unwrap_or(bytes_written + start_pos)
            );
        }

        self.close_socket(&socket);
        Ok(())
    }

    /// Connect to server
    fn connect(&self, url: &Url) -> Result<Socket, WgetError> {
        // Resolve hostname to IP
        let ip = dns_resolve(&url.host).ok_or(WgetError::DnsError)?;

        if !self.options.quiet && self.options.verbose {
            println!("Connecting to {}:{}...", ip, url.port);
        }

        // Create socket
        let socket = socket_create(SocketType::Tcp)?;

        // Set timeout
        socket_set_timeout(&socket, self.options.timeout * 1000)?;

        // Connect
        socket_connect(&socket, ip, url.port)?;

        if !self.options.quiet && self.options.verbose {
            println!("Connected.");
        }

        Ok(socket)
    }

    /// Send data through socket
    fn send_data(&self, socket: &Socket, data: &[u8]) -> Result<(), WgetError> {
        socket_send(socket, data)
            .map(|_| ())
            .map_err(|_| WgetError::NetworkError)
    }

    /// Receive HTTP response
    fn receive_response(&self, socket: &Socket) -> Result<HttpResponse, WgetError> {
        let mut buffer = vec![0u8; DOWNLOAD_BUFFER_SIZE];
        let mut total_data = Vec::new();
        let mut response: Option<HttpResponse> = None;
        let mut header_offset = 0;
        let mut content_length: Option<usize> = None;
        let mut bytes_received = 0usize;

        loop {
            let n = socket_recv(socket, &mut buffer)?;
            if n == 0 {
                break; // Connection closed
            }

            total_data.extend_from_slice(&buffer[..n]);

            // Parse headers if not done yet
            if response.is_none() {
                if let Some((resp, offset)) = self.parse_response_headers(&total_data) {
                    content_length = resp.content_length();
                    response = Some(resp);
                    header_offset = offset;
                }
            }

            if let Some(ref mut resp) = response {
                // Calculate body bytes received
                let body_bytes = total_data.len() - header_offset;
                bytes_received = body_bytes;

                // Show progress
                if self.options.progress && !self.options.quiet {
                    self.show_progress(bytes_received, content_length);
                }

                // Check if we've received all content
                if let Some(len) = content_length {
                    if body_bytes >= len {
                        break;
                    }
                }
            }
        }

        match response {
            Some(mut resp) => {
                resp.body = total_data[header_offset..].to_vec();
                Ok(resp)
            }
            None => Err(WgetError::InvalidResponse),
        }
    }

    /// Show download progress
    fn show_progress(&self, current: usize, total: Option<usize>) {
        if let Some(total_len) = total {
            let percent = (current * 100) / total_len.max(1);
            let bar_width = 50;
            let filled = (bar_width * percent) / 100;

            let bar: String = (0..bar_width)
                .map(|i| if i < filled { '=' } else { ' ' })
                .collect();

            print!("\r[{}] {}% ({}/{})", bar, percent, format_size(current), format_size(total_len));
        } else {
            print!("\r{} downloaded", format_size(current));
        }
    }

    /// Save data to file
    fn save_to_file(
        &self,
        filename: &str,
        data: &[u8],
        start_pos: usize,
        _content_length: Option<usize>,
    ) -> Result<usize, WgetError> {
        let flags = if start_pos > 0 {
            FileFlags::WRITE | FileFlags::APPEND
        } else {
            FileFlags::WRITE | FileFlags::CREATE | FileFlags::TRUNCATE
        };

        let fd = file_open(filename, flags)?;

        // Write data
        let written = file_write(fd, data)?;

        file_close(fd)?;

        Ok(written)
    }

    /// Get existing file size for resume
    fn get_existing_file_size(&self, url: &Url) -> usize {
        let filename = self.options.output_file.clone()
            .unwrap_or_else(|| url.filename());

        match file_stat(&filename) {
            Ok(stat) => stat.size as usize,
            Err(_) => 0,
        }
    }

    /// Print response headers
    fn print_headers(&self, response: &HttpResponse) {
        println!("HTTP/{}.{} {}",
            response.status.version_major,
            response.status.version_minor,
            response.status.code
        );
        for header in &response.headers {
            println!("{}: {}", header.name, header.value);
        }
    }

    /// Close socket
    fn close_socket(&self, socket: &Socket) {
        let _ = socket_close(socket);
    }
}

/// Wget errors
#[derive(Debug)]
enum WgetError {
    InvalidUrl,
    DnsError,
    NetworkError,
    SocketError,
    ConnectionFailed,
    Timeout,
    InvalidResponse,
    HttpError(u16),
    TooManyRedirects,
    FileError,
    IoError,
}

impl core::fmt::Display for WgetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidUrl => write!(f, "Invalid URL"),
            Self::DnsError => write!(f, "DNS resolution failed"),
            Self::NetworkError => write!(f, "Network error"),
            Self::SocketError => write!(f, "Socket error"),
            Self::ConnectionFailed => write!(f, "Connection failed"),
            Self::Timeout => write!(f, "Connection timed out"),
            Self::InvalidResponse => write!(f, "Invalid HTTP response"),
            Self::HttpError(code) => write!(f, "HTTP error: {}", code),
            Self::TooManyRedirects => write!(f, "Too many redirects"),
            Self::FileError => write!(f, "File error"),
            Self::IoError => write!(f, "I/O error"),
        }
    }
}

// ============================================================================
// System call wrappers (these would be implemented by the OS)
// ============================================================================

/// Socket type
#[derive(Clone, Copy)]
enum SocketType {
    Tcp,
    Udp,
}

/// Socket handle
struct Socket {
    fd: i32,
}

/// File flags
struct FileFlags;
impl FileFlags {
    const READ: u32 = 0x01;
    const WRITE: u32 = 0x02;
    const CREATE: u32 = 0x04;
    const TRUNCATE: u32 = 0x08;
    const APPEND: u32 = 0x10;
}

/// File stat
struct FileStat {
    size: u64,
    mode: u32,
}

/// IPv4 address
#[derive(Clone, Copy)]
struct Ipv4Addr {
    octets: [u8; 4],
}

impl core::fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}.{}",
            self.octets[0], self.octets[1], self.octets[2], self.octets[3])
    }
}

// System call stubs - these would be implemented by syscall interface
fn dns_resolve(_hostname: &str) -> Option<Ipv4Addr> {
    // Would call kernel DNS resolver
    extern "C" {
        fn syscall_dns_resolve(hostname: *const u8, hostname_len: usize, ip_out: *mut u8) -> i32;
    }
    let mut ip = [0u8; 4];
    unsafe {
        let hostname_bytes = _hostname.as_bytes();
        if syscall_dns_resolve(hostname_bytes.as_ptr(), hostname_bytes.len(), ip.as_mut_ptr()) == 0 {
            Some(Ipv4Addr { octets: ip })
        } else {
            None
        }
    }
}

fn socket_create(sock_type: SocketType) -> Result<Socket, WgetError> {
    extern "C" {
        fn syscall_socket_create(sock_type: u32) -> i32;
    }
    let type_val = match sock_type {
        SocketType::Tcp => 1,
        SocketType::Udp => 2,
    };
    unsafe {
        let fd = syscall_socket_create(type_val);
        if fd >= 0 {
            Ok(Socket { fd })
        } else {
            Err(WgetError::SocketError)
        }
    }
}

fn socket_connect(socket: &Socket, ip: Ipv4Addr, port: u16) -> Result<(), WgetError> {
    extern "C" {
        fn syscall_socket_connect(fd: i32, ip: *const u8, port: u16) -> i32;
    }
    unsafe {
        if syscall_socket_connect(socket.fd, ip.octets.as_ptr(), port) == 0 {
            Ok(())
        } else {
            Err(WgetError::ConnectionFailed)
        }
    }
}

fn socket_send(socket: &Socket, data: &[u8]) -> Result<usize, WgetError> {
    extern "C" {
        fn syscall_socket_send(fd: i32, data: *const u8, len: usize) -> isize;
    }
    unsafe {
        let n = syscall_socket_send(socket.fd, data.as_ptr(), data.len());
        if n >= 0 {
            Ok(n as usize)
        } else {
            Err(WgetError::NetworkError)
        }
    }
}

fn socket_recv(socket: &Socket, buffer: &mut [u8]) -> Result<usize, WgetError> {
    extern "C" {
        fn syscall_socket_recv(fd: i32, data: *mut u8, len: usize) -> isize;
    }
    unsafe {
        let n = syscall_socket_recv(socket.fd, buffer.as_mut_ptr(), buffer.len());
        if n >= 0 {
            Ok(n as usize)
        } else {
            Err(WgetError::NetworkError)
        }
    }
}

fn socket_set_timeout(socket: &Socket, timeout_ms: u32) -> Result<(), WgetError> {
    extern "C" {
        fn syscall_socket_set_option(fd: i32, option: u32, value: u32) -> i32;
    }
    unsafe {
        if syscall_socket_set_option(socket.fd, 1, timeout_ms) == 0 {
            Ok(())
        } else {
            Err(WgetError::SocketError)
        }
    }
}

fn socket_close(socket: &Socket) -> Result<(), WgetError> {
    extern "C" {
        fn syscall_socket_close(fd: i32) -> i32;
    }
    unsafe {
        if syscall_socket_close(socket.fd) == 0 {
            Ok(())
        } else {
            Err(WgetError::SocketError)
        }
    }
}

fn file_open(path: &str, flags: u32) -> Result<i32, WgetError> {
    extern "C" {
        fn syscall_file_open(path: *const u8, path_len: usize, flags: u32) -> i32;
    }
    unsafe {
        let path_bytes = path.as_bytes();
        let fd = syscall_file_open(path_bytes.as_ptr(), path_bytes.len(), flags);
        if fd >= 0 {
            Ok(fd)
        } else {
            Err(WgetError::FileError)
        }
    }
}

fn file_write(fd: i32, data: &[u8]) -> Result<usize, WgetError> {
    extern "C" {
        fn syscall_file_write(fd: i32, data: *const u8, len: usize) -> isize;
    }
    unsafe {
        let n = syscall_file_write(fd, data.as_ptr(), data.len());
        if n >= 0 {
            Ok(n as usize)
        } else {
            Err(WgetError::IoError)
        }
    }
}

fn file_close(fd: i32) -> Result<(), WgetError> {
    extern "C" {
        fn syscall_file_close(fd: i32) -> i32;
    }
    unsafe {
        if syscall_file_close(fd) == 0 {
            Ok(())
        } else {
            Err(WgetError::FileError)
        }
    }
}

fn file_stat(path: &str) -> Result<FileStat, WgetError> {
    extern "C" {
        fn syscall_file_stat(path: *const u8, path_len: usize, size_out: *mut u64, mode_out: *mut u32) -> i32;
    }
    unsafe {
        let path_bytes = path.as_bytes();
        let mut size = 0u64;
        let mut mode = 0u32;
        if syscall_file_stat(path_bytes.as_ptr(), path_bytes.len(), &mut size, &mut mode) == 0 {
            Ok(FileStat { size, mode })
        } else {
            Err(WgetError::FileError)
        }
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Find end of HTTP headers (double CRLF)
fn find_header_end(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

/// Simple base64 encoding
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i];
        let b1 = if i + 1 < data.len() { data[i + 1] } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] } else { 0 };

        result.push(ALPHABET[(b0 >> 2) as usize] as char);
        result.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);

        if i + 1 < data.len() {
            result.push(ALPHABET[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            result.push('=');
        }

        if i + 2 < data.len() {
            result.push(ALPHABET[(b2 & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        i += 3;
    }

    result
}

/// Format byte size for display
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Get current timestamp (stub)
fn timestamp() -> String {
    extern "C" {
        fn syscall_get_time(hours: *mut u32, minutes: *mut u32, seconds: *mut u32) -> i32;
    }

    unsafe {
        let mut hours = 0u32;
        let mut minutes = 0u32;
        let mut seconds = 0u32;
        syscall_get_time(&mut hours, &mut minutes, &mut seconds);
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }
}

/// Print usage information
fn print_usage() {
    println!("Usage: wget [OPTION]... [URL]...");
    println!();
    println!("BuildOS wget - Download files from the network");
    println!();
    println!("Options:");
    println!("  -O, --output-document=FILE  write documents to FILE");
    println!("  -c, --continue              resume getting a partially-downloaded file");
    println!("  -q, --quiet                 quiet (no output)");
    println!("  -v, --verbose               be verbose");
    println!("  -t, --tries=NUMBER          set number of retries to NUMBER (0 = unlimited)");
    println!("  -T, --timeout=SECONDS       set all timeout values to SECONDS");
    println!("  -S, --server-response       print server response headers");
    println!("  --spider                    don't download, just check if URL exists");
    println!("  --no-check-certificate      don't validate server certificate (HTTPS)");
    println!("  -U, --user-agent=AGENT      identify as AGENT");
    println!("  --referer=URL               include 'Referer: URL' header");
    println!("  --header=STRING             insert STRING among the headers");
    println!("  --http-user=USER            set http user to USER");
    println!("  --http-password=PASS        set http password to PASS");
    println!("  --max-redirect=NUM          maximum number of redirections (default: 10)");
    println!("  --no-verbose                turn off verboseness (default)");
    println!("  --progress=TYPE             select progress bar type (bar, dot, none)");
    println!("  -h, --help                  display this help and exit");
    println!("  -V, --version               display version information and exit");
    println!();
    println!("Examples:");
    println!("  wget http://example.com/file.txt");
    println!("  wget -O output.html http://example.com/");
    println!("  wget -c http://example.com/largefile.zip");
}

/// Print version information
fn print_version() {
    println!("BuildOS wget 1.0");
    println!("Copyright (c) 2024-2025 BUILD-UNIVERSE");
    println!("Network file download utility");
}

/// Parse command line arguments
fn parse_args(args: &[String]) -> Result<(WgetOptions, Vec<String>), String> {
    let mut options = WgetOptions::default();
    let mut urls = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        if arg == "-h" || arg == "--help" {
            print_usage();
            return Err(String::new()); // Exit without error
        } else if arg == "-V" || arg == "--version" {
            print_version();
            return Err(String::new());
        } else if arg == "-q" || arg == "--quiet" {
            options.quiet = true;
            options.progress = false;
        } else if arg == "-v" || arg == "--verbose" {
            options.verbose = true;
        } else if arg == "-c" || arg == "--continue" {
            options.continue_download = true;
        } else if arg == "-S" || arg == "--server-response" {
            options.save_headers = true;
        } else if arg == "--spider" {
            options.spider = true;
        } else if arg == "-O" || arg == "--output-document" {
            i += 1;
            if i >= args.len() {
                return Err("Option -O requires an argument".to_string());
            }
            options.output_file = Some(args[i].clone());
        } else if arg.starts_with("-O") {
            options.output_file = Some(arg[2..].to_string());
        } else if arg.starts_with("--output-document=") {
            options.output_file = Some(arg[18..].to_string());
        } else if arg == "-t" || arg == "--tries" {
            i += 1;
            if i >= args.len() {
                return Err("Option -t requires an argument".to_string());
            }
            options.tries = args[i].parse().map_err(|_| "Invalid tries value")?;
        } else if arg.starts_with("-t") {
            options.tries = arg[2..].parse().map_err(|_| "Invalid tries value")?;
        } else if arg.starts_with("--tries=") {
            options.tries = arg[8..].parse().map_err(|_| "Invalid tries value")?;
        } else if arg == "-T" || arg == "--timeout" {
            i += 1;
            if i >= args.len() {
                return Err("Option -T requires an argument".to_string());
            }
            options.timeout = args[i].parse().map_err(|_| "Invalid timeout value")?;
        } else if arg.starts_with("-T") {
            options.timeout = arg[2..].parse().map_err(|_| "Invalid timeout value")?;
        } else if arg.starts_with("--timeout=") {
            options.timeout = arg[10..].parse().map_err(|_| "Invalid timeout value")?;
        } else if arg == "-U" || arg == "--user-agent" {
            i += 1;
            if i >= args.len() {
                return Err("Option -U requires an argument".to_string());
            }
            options.user_agent = args[i].clone();
        } else if arg.starts_with("--user-agent=") {
            options.user_agent = arg[13..].to_string();
        } else if arg.starts_with("--referer=") {
            options.referer = Some(arg[10..].to_string());
        } else if arg.starts_with("--header=") {
            let header = &arg[9..];
            if let Some(idx) = header.find(':') {
                let name = header[..idx].trim().to_string();
                let value = header[idx + 1..].trim().to_string();
                options.custom_headers.push((name, value));
            }
        } else if arg.starts_with("--http-user=") {
            options.http_user = Some(arg[12..].to_string());
        } else if arg.starts_with("--http-password=") {
            options.http_password = Some(arg[16..].to_string());
        } else if arg.starts_with("--max-redirect=") {
            options.max_redirects = arg[15..].parse().map_err(|_| "Invalid max-redirect value")?;
        } else if arg.starts_with("--progress=") {
            match &arg[11..] {
                "none" => options.progress = false,
                "bar" | "dot" => options.progress = true,
                _ => {}
            }
        } else if arg == "--no-verbose" {
            options.verbose = false;
        } else if arg.starts_with('-') {
            return Err(format!("Unknown option: {}", arg));
        } else {
            urls.push(arg.clone());
        }

        i += 1;
    }

    Ok((options, urls))
}

/// Main entry point
#[no_mangle]
pub extern "C" fn main(argc: i32, argv: *const *const u8) -> i32 {
    // Parse command line arguments
    let args: Vec<String> = unsafe {
        (0..argc as usize)
            .map(|i| {
                let ptr = *argv.add(i);
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let slice = core::slice::from_raw_parts(ptr, len);
                String::from_utf8_lossy(slice).to_string()
            })
            .collect()
    };

    // Skip program name
    let args = if args.len() > 0 { &args[1..] } else { &args[..] };

    if args.is_empty() {
        print_usage();
        return 1;
    }

    // Parse options
    let (options, urls) = match parse_args(&args.to_vec()) {
        Ok((opts, urls)) => (opts, urls),
        Err(e) => {
            if !e.is_empty() {
                println!("wget: {}", e);
                return 1;
            }
            return 0; // Help/version requested
        }
    };

    if urls.is_empty() {
        println!("wget: missing URL");
        println!("Usage: wget [OPTION]... [URL]...");
        return 1;
    }

    // Download each URL
    let mut exit_code = 0;
    for url in urls {
        let mut wget = Wget::new(options.clone());
        if let Err(e) = wget.download(&url) {
            if !options.quiet {
                println!("wget: {}", e);
            }
            exit_code = 1;
        }
    }

    exit_code
}

impl Clone for WgetOptions {
    fn clone(&self) -> Self {
        Self {
            output_file: self.output_file.clone(),
            continue_download: self.continue_download,
            quiet: self.quiet,
            tries: self.tries,
            timeout: self.timeout,
            user_agent: self.user_agent.clone(),
            follow_redirects: self.follow_redirects,
            max_redirects: self.max_redirects,
            headers_only: self.headers_only,
            spider: self.spider,
            verbose: self.verbose,
            save_headers: self.save_headers,
            progress: self.progress,
            http_user: self.http_user.clone(),
            http_password: self.http_password.clone(),
            referer: self.referer.clone(),
            custom_headers: self.custom_headers.clone(),
        }
    }
}
