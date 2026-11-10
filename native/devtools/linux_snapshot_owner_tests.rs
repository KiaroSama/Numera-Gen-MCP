use super::*;
use std::io::BufRead;

#[test]
fn ready_sampler_that_stops_reading_is_killed_at_its_hard_deadline_and_reaped() {
    const MODE: &str = "NUMERA_SAMPLE_OWNER_FIXTURE";
    if std::env::var_os(MODE).is_some() {
        // libtest writes a preamble. Drain through READY before constructing Capture.
        println!("READY");
        io::stdout().flush().unwrap();
        let until = Instant::now() + Duration::from_secs(6);
        while Instant::now() < until {
            std::thread::park_timeout(Duration::from_millis(10));
        }
        return;
    }
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "devtools::linux_snapshot::owner::tests::ready_sampler_that_stops_reading_is_killed_at_its_hard_deadline_and_reaped",
            "--nocapture",
        ])
        .env(MODE, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut children = Children(vec![command.spawn().unwrap()]);
    let stdout = children.0[0].stdout.take().unwrap();
    let mut reader = io::BufReader::new(stdout);
    // Nonblocking startup is bounded even when the fixture fails before becoming ready.
    super::super::super::capture::nonblocking(reader.get_ref()).unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let mut accumulated = Vec::new();
    loop {
        let _ = reader.read_until(b'\n', &mut accumulated);
        if accumulated.windows(6).any(|s| s == b"READY\n") {
            break;
        }
        assert!(Instant::now() < until, "Sampler fixture did not become ready.");
        assert!(accumulated.len() < 8192);
        std::thread::park_timeout(Duration::from_millis(5));
    }
    let stdout = reader.into_inner();
    let stderr = children.0[0].stderr.take().unwrap();
    let observer = Observer {
        child: 0,
        input: children.0[0].stdin.take().unwrap(),
        capture: Capture::new(stdout, stderr).unwrap(),
    };
    assert!(
        !observer
            .stop_expired(&mut children, Duration::from_millis(499))
            .unwrap()
    );
    assert!(children.0[0].try_wait().unwrap().is_none());
    assert!(
        observer
            .stop_expired(&mut children, Duration::from_millis(500))
            .unwrap()
    );
    children
        .stop(Instant::now() + Duration::from_secs(2))
        .unwrap();
    assert!(children.0[0].try_wait().unwrap().is_some());
}
