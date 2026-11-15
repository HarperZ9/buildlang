#!/bin/bash
# =============================================================================
# BUILDOS QEMU TEST SCRIPT
# =============================================================================
# Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration
MEMORY="512M"
CPU="max"
SMP="2"
OVMF_PATH="/usr/share/OVMF/OVMF_CODE.fd"
OVMF_VARS="/usr/share/OVMF/OVMF_VARS.fd"

# Paths
BOOTLOADER_EFI="$PROJECT_ROOT/target/x86_64-unknown-uefi/release/bootloader.efi"
KERNEL_ELF="$PROJECT_ROOT/target/x86_64-buildos/release/kernel"
ESP_DIR="$PROJECT_ROOT/target/esp"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${BLUE}============================================${NC}"
    echo -e "${BLUE}  BuildOS QEMU Launcher${NC}"
    echo -e "${BLUE}============================================${NC}"
}

print_step() {
    echo -e "${GREEN}[*]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    print_step "Checking prerequisites..."

    if ! command -v qemu-system-x86_64 &> /dev/null; then
        print_error "QEMU not found. Install with: sudo apt install qemu-system-x86"
        exit 1
    fi

    if [ ! -f "$OVMF_PATH" ]; then
        print_warning "OVMF not found at $OVMF_PATH"
        print_warning "Trying alternative paths..."

        # Try common OVMF locations
        for path in /usr/share/edk2/ovmf/OVMF_CODE.fd \
                    /usr/share/qemu/OVMF.fd \
                    /usr/share/edk2-ovmf/x64/OVMF_CODE.fd; do
            if [ -f "$path" ]; then
                OVMF_PATH="$path"
                print_step "Found OVMF at $OVMF_PATH"
                break
            fi
        done

        if [ ! -f "$OVMF_PATH" ]; then
            print_error "OVMF firmware not found. Install with: sudo apt install ovmf"
            exit 1
        fi
    fi
}

# Build the bootloader
build_bootloader() {
    print_step "Building bootloader..."
    cd "$PROJECT_ROOT/bootloader"
    cargo build --release --target x86_64-unknown-uefi
    cd "$PROJECT_ROOT"
}

# Build the kernel
build_kernel() {
    print_step "Building kernel..."
    cd "$PROJECT_ROOT/kernel"
    cargo build --release --target "$PROJECT_ROOT/kernel/x86_64-buildos.json"
    cd "$PROJECT_ROOT"
}

# Create ESP (EFI System Partition) structure
create_esp() {
    print_step "Creating EFI System Partition structure..."

    rm -rf "$ESP_DIR"
    mkdir -p "$ESP_DIR/EFI/BOOT"
    mkdir -p "$ESP_DIR/EFI/BUILDOS"

    # Copy bootloader
    if [ -f "$BOOTLOADER_EFI" ]; then
        cp "$BOOTLOADER_EFI" "$ESP_DIR/EFI/BOOT/BOOTX64.EFI"
        cp "$BOOTLOADER_EFI" "$ESP_DIR/EFI/BUILDOS/BOOTX64.EFI"
        print_step "Bootloader copied to ESP"
    else
        print_error "Bootloader not found at $BOOTLOADER_EFI"
        print_error "Run 'cargo build --release' in bootloader directory first"
        exit 1
    fi

    # Copy kernel
    if [ -f "$KERNEL_ELF" ]; then
        cp "$KERNEL_ELF" "$ESP_DIR/EFI/BUILDOS/KERNEL.ELF"
        print_step "Kernel copied to ESP"
    else
        print_warning "Kernel not found at $KERNEL_ELF"
        print_warning "QEMU will start but kernel won't load"
    fi

    # Create boot configuration
    cat > "$ESP_DIR/EFI/BUILDOS/BOOT.CFG" << EOF
# BuildOS Boot Configuration
# ============================

# Kernel command line
cmdline=console=tty0 loglevel=7 debug

# Video settings
video_width=1920
video_height=1080

# Boot options
verbose=true
debug=true
timeout=3

# Serial console (for debugging)
serial=true
serial_port=0
serial_baud=115200
EOF

    print_step "Boot configuration created"
}

# Run QEMU
run_qemu() {
    print_step "Starting QEMU..."
    echo ""
    echo -e "${YELLOW}QEMU Controls:${NC}"
    echo "  Ctrl+Alt+G  - Release mouse grab"
    echo "  Ctrl+Alt+2  - Switch to QEMU monitor"
    echo "  Ctrl+Alt+1  - Switch back to VM"
    echo "  Ctrl+C      - Terminate QEMU"
    echo ""

    qemu-system-x86_64 \
        -machine q35 \
        -cpu "$CPU" \
        -smp "$SMP" \
        -m "$MEMORY" \
        -drive if=pflash,format=raw,readonly=on,file="$OVMF_PATH" \
        -drive format=raw,file=fat:rw:"$ESP_DIR" \
        -serial stdio \
        -device virtio-net-pci,netdev=net0 \
        -netdev user,id=net0 \
        -device virtio-gpu-pci \
        -display sdl \
        -no-reboot \
        -no-shutdown \
        "$@"
}

# Run QEMU in debug mode (with GDB server)
run_qemu_debug() {
    print_step "Starting QEMU in debug mode (GDB on port 1234)..."

    qemu-system-x86_64 \
        -machine q35 \
        -cpu "$CPU" \
        -smp "$SMP" \
        -m "$MEMORY" \
        -drive if=pflash,format=raw,readonly=on,file="$OVMF_PATH" \
        -drive format=raw,file=fat:rw:"$ESP_DIR" \
        -serial stdio \
        -device virtio-net-pci,netdev=net0 \
        -netdev user,id=net0 \
        -device virtio-gpu-pci \
        -display sdl \
        -no-reboot \
        -no-shutdown \
        -s -S \
        "$@"
}

# Run QEMU headless (for CI/testing)
run_qemu_headless() {
    print_step "Starting QEMU in headless mode..."

    timeout 60 qemu-system-x86_64 \
        -machine q35 \
        -cpu "$CPU" \
        -smp "$SMP" \
        -m "$MEMORY" \
        -drive if=pflash,format=raw,readonly=on,file="$OVMF_PATH" \
        -drive format=raw,file=fat:rw:"$ESP_DIR" \
        -serial stdio \
        -display none \
        -no-reboot \
        -no-shutdown \
        "$@" || true
}

# Main
main() {
    print_header

    case "${1:-run}" in
        build)
            check_prerequisites
            build_bootloader
            build_kernel
            create_esp
            print_step "Build complete!"
            ;;
        run)
            check_prerequisites
            create_esp
            run_qemu "${@:2}"
            ;;
        debug)
            check_prerequisites
            create_esp
            run_qemu_debug "${@:2}"
            ;;
        headless)
            check_prerequisites
            create_esp
            run_qemu_headless "${@:2}"
            ;;
        help|--help|-h)
            echo "Usage: $0 [command] [qemu-args...]"
            echo ""
            echo "Commands:"
            echo "  build     Build bootloader, kernel, and create ESP"
            echo "  run       Run QEMU with display (default)"
            echo "  debug     Run QEMU with GDB server on port 1234"
            echo "  headless  Run QEMU without display (for CI)"
            echo "  help      Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0 build"
            echo "  $0 run"
            echo "  $0 debug"
            echo "  $0 run -m 1G -smp 4"
            ;;
        *)
            print_error "Unknown command: $1"
            echo "Run '$0 help' for usage information"
            exit 1
            ;;
    esac
}

main "$@"
