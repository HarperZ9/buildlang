# Getting Started with BuildLang

> Current status (2026-06-15): historical/aspirational release-bundle guide.
> It does not describe an available binary release, install script, package
> manager, or hosted documentation site. Use the repository root `README.md` and
> `STATUS.md` for current verified compiler status.

Welcome to BuildLang! This guide will help you get up and running with the language quickly.

## Table of Contents

1. [Installation](#installation)
2. [Your First Program](#your-first-program)
3. [Basic Concepts](#basic-concepts)
4. [Working with the Compiler](#working-with-the-compiler)
5. [Package Management](#package-management)
6. [Next Steps](#next-steps)

## Installation

### From Binary Release (Recommended)

Download the latest release for your platform:

```bash
# Linux/macOS
curl -sSL https://buildlang.org/install.sh | sh

# Or using wget
wget -qO- https://buildlang.org/install.sh | sh
```

### From Source

```bash
# Clone the repository
git clone https://github.com/HarperZ9/buildlang.git
cd buildlang

# Build the compiler
./build.sh release

# Add to PATH
export PATH="$PWD/target/release:$PATH"
```

### Verify Installation

```bash
build --version
# BuildLang 1.0.0
```

## Your First Program

Create a file named `hello.bld`:

```build
fn main() {
    println!("Hello, World!");
}
```

Compile and run:

```bash
build run hello.bld
# Hello, World!
```

Or compile to an executable:

```bash
build build hello.bld -o hello
./hello
# Hello, World!
```

## Basic Concepts

### Variables and Types

BuildLang uses type inference with optional explicit annotations:

```build
// Immutable by default
let x = 42;              // i32 inferred
let name = "Alice";      // String inferred
let pi: f64 = 3.14159;   // Explicit type

// Mutable variables
let mut count = 0;
count += 1;

// Constants (compile-time)
const MAX_SIZE: usize = 1024;
```

### Functions

```build
// Basic function
fn greet(name: String) {
    println!("Hello, {}!", name);
}

// Function with return type
fn add(a: i32, b: i32) -> i32 {
    a + b  // No semicolon = implicit return
}

// Generic function
fn first<T>(items: &[T]) -> Option<&T> {
    items.get(0)
}
```

### Control Flow

```build
// If expressions
let max = if a > b { a } else { b };

// Pattern matching
match value {
    0 => println!("zero"),
    1..=9 => println!("single digit"),
    n if n < 0 => println!("negative"),
    _ => println!("other"),
}

// Loops
for i in 0..10 {
    println!("{}", i);
}

while condition {
    // ...
}

loop {
    if done { break; }
}
```

### Structs and Enums

```build
// Struct definition
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    // Constructor
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }
    
    // Method
    fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}

// Enum with variants
enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
    Triangle { base: f64, height: f64 },
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => 3.14159 * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Triangle { base, height } => 0.5 * base * height,
        }
    }
}
```

### Error Handling

BuildLang uses `Result` and `Option` for error handling:

```build
use std::io::{File, Read};

fn read_file(path: &str) -> Result<String, std::io::Error> {
    let mut file = File::open(path)?;  // ? propagates errors
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

fn main() {
    match read_file("config.txt") {
        Ok(contents) => println!("{}", contents),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### Collections

```build
use std::vec::Vec;
use std::hashmap::HashMap;

// Vectors
let mut numbers = vec![1, 2, 3, 4, 5];
numbers.push(6);
let sum: i32 = numbers.iter().sum();

// Hash maps
let mut scores = HashMap::new();
scores.insert("Alice", 100);
scores.insert("Bob", 85);

if let Some(score) = scores.get("Alice") {
    println!("Alice's score: {}", score);
}
```

## Working with the Compiler

### Compilation Modes

```bash
# Debug build (faster compilation, slower runtime)
build build main.bld

# Release build (optimized)
build build main.bld --release

# Run directly
build run main.bld

# Run with arguments
build run main.bld -- arg1 arg2
```

### Compiler Targets

```bash
# Native (default)
build build main.bld

# WebAssembly
build build main.bld --target wasm32

# Specific architecture
build build main.bld --target x86_64-linux
build build main.bld --target aarch64-macos
```

### Useful Commands

```bash
# Format code
build fmt src/

# Run linter
build lint src/

# Generate documentation
build doc src/ -o docs/

# Run tests
build test

# Check without building
build check main.bld

# Show dependencies
build deps

# Start REPL
build repl
```

## Package Management

### Creating a Project

```bash
build new my-project
cd my-project
```

This creates:
```
my-project/
├── build.toml       # Project configuration
├── src/
│   └── main.bld   # Main source file
└── tests/
    └── test_main.bld
```

### Project Configuration (build.toml)

```toml
[package]
name = "my-project"
version = "0.1.0"
authors = ["Your Name <you@example.com>"]
edition = "2024"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }

[dev-dependencies]
test-utils = "0.5"

[build]
target = "native"
opt-level = 2
```

### Adding Dependencies

```bash
# Add from registry
build add serde

# Add with version
build add serde@1.0

# Add with features
build add tokio --features full

# Remove dependency
build remove serde
```

### Building and Running

```bash
# Build project
build build

# Run project
build run

# Run specific binary
build run --bin my-binary

# Run tests
build test

# Run benchmarks
build bench
```

## Next Steps

Now that you have the basics, explore these topics:

1. **[Language Reference](reference/language.md)** - Complete language specification
2. **[Standard Library](api/std.md)** - Full API documentation
3. **[Tutorials](tutorials/README.md)** - Step-by-step guides
4. **[Best Practices](guide/best-practices.md)** - Idiomatic BuildLang
5. **[Concurrency](guide/concurrency.md)** - Async/await and threads
6. **[FFI](guide/ffi.md)** - Interoperating with C/C++

### Community Resources

- **Documentation**: https://docs.buildlang.org
- **Forum**: https://forum.buildlang.org
- **Discord**: https://discord.gg/buildlang
- **GitHub**: https://github.com/HarperZ9/buildlang

### Getting Help

```bash
# Built-in help
build help
build help <command>

# Search documentation
build doc --search "HashMap"
```

Welcome to the BuildLang community! 🎉
