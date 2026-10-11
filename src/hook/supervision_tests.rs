use super::*;
use std::net::{TcpListener, TcpStream};

#[test]
fn bounded_descendant() {
    let Some(marker) = env::var_os("PACTRUN_TEST_DESCENDANT_MARKER") else { return; };
    let marker = PathBuf::from(marker);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    fs::write(marker_variant(&marker, "child-ready"), listener.local_addr().unwrap().to_string()).unwrap();
    // Finite lifetime even if a regression prematurely drops supervision.
    thread::sleep(Duration::from_millis(1800));
    io::stdout().write_all(b"child-tail-0684\n").unwrap();
    io::stdout().flush().unwrap();
    fs::write(marker_variant(&marker, "child-done"), b"done").unwrap();
    drop(listener);
}

// Test-ID: PR-TEST-0684
// Verifies: PR-REQ-0073, PR-REQ-0052, PR-REQ-0286
#[test]
fn completion_waits_for_descendants_independently_of_output_pipe_ownership() {
    let fixture = RuntimeFixture::new();
    for mode in ["descendant_closed", "descendant_pipes", "descendant_cancel", "descendant_timeout"] {
        let marker = fixture.marker(mode);
        let cancellation = ActionCancellation::default();
        let delivery = crate::hook::delivery::Delivery::start(&fixture.storage, "invoke").unwrap();
        cancellation.delivery.set(delivery.clone());
        let admitted = fixture.admit("output_terminal", mode, &marker);
        let run = fixture.application.execute_admitted_action(
            admitted, policy(None, Some(800), Some(30)), cancellation.clone(),
        );
        let address = fs::read_to_string(marker_variant(&marker, "child-ready")).unwrap();
        let mut observed_wait = false;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let guard = fixture.application.take_owner_continuation(run).unwrap();
            if matches!(guard.continuation(), OwnerContinuation::ReadyToFinalize(_)) {
                drop(guard);
                break;
            }
            observed_wait = true;
            assert!(matches!(load_run(&fixture.storage, run).state, RunState::Running(_)));
            drop(guard);
            if mode == "descendant_cancel" { cancellation.request(); }
            assert!(Instant::now() < deadline, "process tree never reached termination");
            thread::sleep(Duration::from_millis(10));
            assert!(fixture.application.resume_owner_continuation(run));
        }
        let facts = take_facts(&fixture.application, run);
        delivery.finish();
        assert!(delivery.summary().complete);
        let mut cursor = delivery.cursor(false);
        let mut stdout = Vec::new();
        while let Some(record) = cursor.next().unwrap() {
            if let crate::hook::delivery::Event::Output { channel: crate::hook::delivery::Channel::Stdout, data, .. } = record.event {
                use base64::Engine as _;
                stdout.extend(base64::engine::general_purpose::STANDARD.decode(data).unwrap());
            }
        }
        assert_eq!(contains(&stdout, b"child-tail-0684"), mode == "descendant_pipes", "descendant tail must not be discarded or invented");
        let child_still_alive = TcpStream::connect(&address).is_ok();
        // Wait before asserting so a failing regression leaves no live fixture.
        let cleanup_deadline = Instant::now() + Duration::from_secs(5);
        while TcpStream::connect(&address).is_ok() && Instant::now() < cleanup_deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!child_still_alive, "terminal facts published while a descendant was alive: {mode}");
        if mode != "descendant_timeout" {
            assert!(observed_wait, "root exit must retain process-tree supervision: {mode}");
            assert!(facts.completion_accepted && facts.process_terminated);
            assert_eq!(facts.outcome, RunOutcome::Succeeded, "late cancellation cannot rewrite completion");
        } else {
            assert!(!facts.completion_accepted && facts.process_terminated);
            assert_eq!(facts.outcome, RunOutcome::TimedOut);
        }
        if matches!(mode, "descendant_closed" | "descendant_pipes") {
            assert!(marker_variant(&marker, "child-done").exists());
        }
    }
}
