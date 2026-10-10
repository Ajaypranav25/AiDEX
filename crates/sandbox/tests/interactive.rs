mod common;
use aidex_sandbox::{CommandSpec, SandboxLevel};
use std::{
    thread,
    time::{Duration, Instant},
};
#[test]
fn interactive_input_stays_open_and_tree_is_tracked() {
    let work = common::workspace();
    let sandbox = common::sandbox();
    let plan = sandbox
        .negotiate(&common::request(SandboxLevel::Brokered, &work))
        .unwrap();
    let command = if cfg!(windows) {
        CommandSpec::new(plan.resolve_program("powershell.exe").unwrap()).args(["-NoProfile", "-NonInteractive", "-Command", "while ($null -ne ($line = [Console]::ReadLine())) { [Console]::WriteLine('reply:' + $line) }"])
    } else {
        CommandSpec::new("/bin/cat")
    };
    let process = sandbox.spawn_interactive(&plan, command).unwrap();
    assert!(process.is_running());
    process.write_stdin(b"one\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !process.output_so_far().stdout.text().contains("one") {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
    process.write_stdin(b"two\n").unwrap();
    process.close_stdin();
    assert!(common::finish(&process).success());
    assert!(process.output_so_far().stdout.text().contains("two"));
    assert!(process.write_stdin(b"late").is_err());
    assert!(sandbox.registry().live_ids().is_empty());
}
