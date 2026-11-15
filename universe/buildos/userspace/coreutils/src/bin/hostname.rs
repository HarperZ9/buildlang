// ===============================================================================
// HOSTNAME - SHOW OR SET THE SYSTEM'S HOST NAME
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if str_eq(arg, b"--help") {
            println("Usage: hostname [NAME]");
            println("Show or set the system's host name.");
            return 0;
        }
    }

    // Get hostname via uname
    let mut buf = Utsname::zeroed();
    let result = uname(&mut buf);

    if result < 0 {
        eprintln("hostname: cannot get hostname");
        return 1;
    }

    // Print nodename
    for &c in buf.nodename.iter() {
        if c == 0 {
            break;
        }
        write(STDOUT, &[c]);
    }
    println("");

    0
}
