// ===============================================================================
// DATE - PRINT OR SET THE SYSTEM DATE AND TIME
// ===============================================================================
// Copyright (c) 2024-2025 Zain Dana Harper. All Rights Reserved.
// ===============================================================================

#![no_std]
#![no_main]

use coreutils_common::*;

entry!(main);

fn main(argc: usize, argv: *const *const u8) -> i32 {
    let mut utc = false;
    let args = unsafe { ArgIter::new(argc, argv) };
    let mut skip_first = true;

    for arg in args {
        if skip_first {
            skip_first = false;
            continue;
        }

        if arg.len() > 1 && arg[0] == b'-' {
            if str_eq(arg, b"--utc") || str_eq(arg, b"--universal") {
                utc = true;
            } else if str_eq(arg, b"--help") {
                println("Usage: date [OPTION]... [+FORMAT]");
                println("Display the current time.");
                println("");
                println("  -u, --utc, --universal  print Coordinated Universal Time");
                return 0;
            } else {
                for i in 1..arg.len() {
                    match arg[i] {
                        b'u' => utc = true,
                        _ => {}
                    }
                }
            }
        }
    }

    let mut ts = Timespec::zeroed();
    let result = clock_gettime(CLOCK_REALTIME, &mut ts);

    if result < 0 {
        eprintln("date: cannot get time");
        return 1;
    }

    // Convert Unix timestamp to date components
    // This is a simplified implementation
    let mut seconds = ts.tv_sec;

    // Days since epoch
    let mut days = seconds / 86400;
    seconds %= 86400;

    let hours = seconds / 3600;
    seconds %= 3600;
    let minutes = seconds / 60;
    let secs = seconds % 60;

    // Calculate year, month, day from days since 1970-01-01
    let mut year: i64 = 1970;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0;
    while month < 12 && days >= month_days[month] {
        days -= month_days[month];
        month += 1;
    }

    let day = days + 1;
    month += 1;

    // Day of week (0 = Thursday for epoch)
    let total_days = ts.tv_sec / 86400;
    let dow = ((total_days + 4) % 7) as usize;  // 0 = Sunday

    let day_names = [b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];
    let month_names = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun",
        b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec"
    ];

    // Print: "Wed Dec 20 14:30:00 UTC 2024"
    print_bytes(day_names[dow]);
    print(" ");
    print_bytes(month_names[month as usize - 1]);
    print(" ");
    if day < 10 {
        print(" ");
    }
    print_num(day as u64);
    print(" ");

    if hours < 10 {
        print("0");
    }
    print_num(hours as u64);
    print(":");
    if minutes < 10 {
        print("0");
    }
    print_num(minutes as u64);
    print(":");
    if secs < 10 {
        print("0");
    }
    print_num(secs as u64);
    print(" ");

    if utc {
        print("UTC ");
    }

    print_num(year as u64);
    println("");

    0
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
