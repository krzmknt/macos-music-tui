use std::process::Command;

fn main() {
    // Get git commit hash
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok();

    let commit_hash = output
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", commit_hash);

    // Get build timestamp for version (JST = UTC+9)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Convert to JST
    let ts_local = now + 9 * 3600;

    // Calculate date/time components
    let seconds_in_day = ((ts_local % 86400) + 86400) % 86400;
    let hour = seconds_in_day / 3600;
    let minute = (seconds_in_day % 3600) / 60;

    let mut days = ts_local / 86400;
    if ts_local < 0 && ts_local % 86400 != 0 {
        days -= 1;
    }

    // Calculate year/month/day from days since 1970-01-01
    let mut year = 1970i32;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let mut month = 1u32;
    loop {
        let dim = days_in_month(year, month) as i64;
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }

    let day = days + 1;

    let build_version = format!("{}.{:02}.{:02}.{:02}.{:02}", year, month, day, hour, minute);
    println!("cargo:rustc-env=BUILD_VERSION={}", build_version);

    // Rerun if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 30,
    }
}
