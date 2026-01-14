use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread::sleep;
use std::time::Duration;

/// Attempts a STOMP connection with retry logic for robustness.
/// Returns Ok if successful, Err with detailed message if all retries fail.
fn attempt_stomp_connection(
    addr: &str,
    max_attempts: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_error: Option<Box<dyn std::error::Error>> = None;

    for attempt in 1..=max_attempts {
        eprintln!("Connection attempt {}/{}", attempt, max_attempts);

        // Try to establish TCP connection
        let stream = match TcpStream::connect_timeout(
            &addr.parse()?,
            Duration::from_secs(5),
        ) {
            Ok(s) => s,
            Err(e) => {
                last_error = Some(Box::new(e));
                eprintln!("  TCP connect failed: {}", last_error.as_ref().unwrap());
                if attempt < max_attempts {
                    sleep(Duration::from_millis(500));
                }
                continue;
            }
        };

        // Set timeouts for read/write operations
        if let Err(e) = stream.set_read_timeout(Some(Duration::from_secs(5))) {
            last_error = Some(Box::new(e));
            eprintln!("  Failed to set read timeout: {}", last_error.as_ref().unwrap());
            continue;
        }
        if let Err(e) = stream.set_write_timeout(Some(Duration::from_secs(5))) {
            last_error = Some(Box::new(e));
            eprintln!("  Failed to set write timeout: {}", last_error.as_ref().unwrap());
            continue;
        }

        // Send STOMP CONNECT frame
        let mut stream = stream;
        let frame = "CONNECT\naccept-version:1.2\nhost:/\nlogin:guest\npasscode:guest\n\n\0";
        if let Err(e) = stream.write_all(frame.as_bytes()) {
            last_error = Some(Box::new(e));
            eprintln!("  Failed to send CONNECT frame: {}", last_error.as_ref().unwrap());
            if attempt < max_attempts {
                sleep(Duration::from_millis(500));
            }
            continue;
        }

        // Read response
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(_) => {
                if line.starts_with("CONNECTED") {
                    eprintln!("  ✓ Successfully connected to STOMP broker");
                    return Ok(());
                } else {
                    last_error = Some(format!(
                        "Unexpected response from broker: {}",
                        line.trim()
                    )
                    .into());
                    eprintln!("  {}", last_error.as_ref().unwrap());
                }
            }
            Err(e) => {
                last_error = Some(Box::new(e));
                eprintln!("  Failed to read response: {}", last_error.as_ref().unwrap());
            }
        }

        if attempt < max_attempts {
            sleep(Duration::from_millis(500));
        }
    }

    Err(last_error.unwrap_or_else(|| "All connection attempts failed".into()))
}

#[test]
fn stomp_smoke_connects() -> Result<(), Box<dyn std::error::Error>> {
    // Skip this smoke test unless explicitly enabled. Running an actual
    // broker is an external dependency and many runners (CI/local) won't
    // have one available by default. Set the environment variable
    // `RUN_STOMP_SMOKE=1` to enable this test.
    if env::var("RUN_STOMP_SMOKE").is_err() {
        eprintln!("skipping stomp_smoke_connects: RUN_STOMP_SMOKE not set");
        return Ok(());
    }

    let addr = "127.0.0.1:61613";
    eprintln!("Running STOMP smoke test against {}", addr);

    // Wait briefly for the broker to be ready (in case test starts immediately after container)
    eprintln!("Waiting 2 seconds for broker startup...");
    sleep(Duration::from_secs(2));

    // Attempt connection with retry logic
    let max_attempts = 5;
    attempt_stomp_connection(addr, max_attempts)?;

    eprintln!("✓ Smoke test passed: STOMP connection successful");
    Ok(())
}
