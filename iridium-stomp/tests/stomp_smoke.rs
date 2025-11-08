use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[test]
fn stomp_smoke_connects() -> Result<(), Box<dyn std::error::Error>> {
    // Try to connect for a short timeout in case the test runner starts before the container.
    let addr = "127.0.0.1:61613";
    let start = Instant::now();
    let stream = loop {
        match TcpStream::connect(addr) {
            Ok(s) => break s,
            Err(e) => {
                if start.elapsed() > Duration::from_secs(15) {
                    return Err(Box::new(e));
                }
                sleep(Duration::from_millis(200));
            }
        }
    };

    // Set reasonable timeouts so the test fails quickly on network issues.
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    // Send a STOMP CONNECT frame (include login/passcode) and expect CONNECTED in response.
    let mut stream = stream;
    // Use the default virtual host (/) instead of 'localhost' which RabbitMQ rejects by default
    let frame = "CONNECT\naccept-version:1.2\nhost:/\nlogin:guest\npasscode:guest\n\n\0";
    stream.write_all(frame.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    // Read the first line of the response (e.g. "CONNECTED\n")
    reader.read_line(&mut line)?;

    assert!(
        line.starts_with("CONNECTED"),
        "expected CONNECTED response from broker, got: {}",
        line
    );

    Ok(())
}
