use super::support::{Scenario, assert_success, command, run_command};
use serde_json::json;
use std::{fs, process::Command};

fn shells() -> Vec<(&'static str, &'static str)> {
    if cfg!(windows) {
        vec![
            ("powershell_7", "pwsh.exe"),
            ("windows_powershell_5_1", "powershell.exe"),
        ]
    } else {
        vec![("sh", "sh"), ("bash", "bash")]
    }
}

fn source(shell: &str, executable: &str, args: &[String]) -> String {
    format!(
        r#"source_format: 3
package_id: '{{package_id}}'
revision:
  actions:
    - id: run
      access: observe
      parameters: []
      hook:
        protocol_version: 1
        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}
        args: {}
        io: {{terminal: output}}
      outputs: []
runtime_content:
  files:
    - {{id: script, source: script.txt, path: scripts/script.{}, executable: false}}
"#,
        serde_json::to_string(args).unwrap(),
        if cfg!(windows) { "ps1" } else { "sh" }
    )
}

fn invoke(scenario: &Scenario) -> std::process::Output {
    let mut cmd = command(
        &scenario.storage,
        &scenario.path(""),
        ["invoke", "sample", "run", "--action-timeout-ms", "10000"],
    );
    // Operator-authorized acceptance-only Process policy; never shipped by Loader.
    if cfg!(windows)
        && fs::read_to_string(scenario.source.join("pactrun.yaml"))
            .unwrap()
            .contains("windows_powershell_5_1")
    {
        cmd.env("PSExecutionPolicyPreference", "RemoteSigned");
    }
    run_command(cmd)
}

fn execute(scenario: &Scenario, args: &[&str]) -> std::process::Output {
    let mut cmd = command(&scenario.storage, &scenario.path(""), args);
    if cfg!(windows)
        && fs::read_to_string(scenario.source.join("pactrun.yaml"))
            .unwrap()
            .contains("windows_powershell_5_1")
    {
        cmd.env("PSExecutionPolicyPreference", "RemoteSigned");
    }
    run_command(cmd)
}

fn helper_calls(commands: &[&str], final_code: i32) -> String {
    let mut script = String::new();
    for command in commands {
        if cfg!(windows) {
            script.push_str(&format!(
                "& $env:PACTRUN_EXECUTABLE hook {command}\nif ($LASTEXITCODE -ne 0) {{exit 91}}\n"
            ));
        } else {
            script.push_str(&format!(
                "\"$PACTRUN_EXECUTABLE\" hook {command} || exit 91\n"
            ));
        }
    }
    script.push_str(&format!("exit {final_code}\n"));
    script
}

// Test-ID: PR-TEST-0488
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_risk_acknowledgments_and_nonzero_exit_preserve_recovery() {
    for (shell, executable) in shells() {
        for (calls, success, open) in [
            (vec!["risk enter", "risk resolve"], true, false),
            (vec!["risk enter"], false, true),
        ] {
            let yaml = source(shell, executable, &[]).replace("access: observe", "access: mutate");
            let scenario = Scenario::new(703, &yaml);
            fs::write(scenario.source.join("script.txt"), helper_calls(&calls, 0)).unwrap();
            scenario.install_and_create("sample");
            let result = invoke(&scenario);
            assert_eq!(
                result.status.success(),
                success,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let text = String::from_utf8_lossy(&result.stderr);
            assert_eq!(text.contains("terminal_risk: open"), open, "{text}");
        }
    }
}

// Test-ID: PR-TEST-0489
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_plain_capture_restore_and_cleanup_use_operation_completion() {
    for (shell, executable) in shells() {
        let mut yaml = source(shell, executable, &[]);
        let hook = format!(
            "        protocol_version: 2\n        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}\n        args: []\n        io: {{terminal: none}}\n"
        );
        let capabilities = format!(
            "  snapshot:\n    capture:\n      parameters: []\n      access: observe\n      hook:\n{hook}    restore:\n      parameters: []\n      hook:\n{hook}  cleanup:\n    requires: []\n    hook:\n{}",
            hook.lines()
                .map(|l| format!("{}\n", &l[2..]))
                .collect::<String>()
        );
        yaml = yaml.replace(
            "runtime_content:",
            &format!("{capabilities}runtime_content:"),
        );
        let scenario = Scenario::new(704, &yaml);
        fs::write(scenario.source.join("script.txt"), "exit 0\n").unwrap();
        scenario.install_and_create("sample");
        let capture = execute(
            &scenario,
            &[
                "snapshot",
                "capture",
                "sample",
                "--execution-timeout-ms",
                "10000",
            ],
        );
        assert_success(&capture);
        let text = String::from_utf8(capture.stdout).unwrap();
        let snapshot = text
            .lines()
            .find_map(|l| l.strip_prefix("snapshot: "))
            .expect(&text);
        assert_success(&execute(
            &scenario,
            &[
                "snapshot",
                "restore",
                "sample",
                snapshot,
                "--execution-timeout-ms",
                "10000",
            ],
        ));
        assert_success(&execute(
            &scenario,
            &[
                "instance",
                "delete",
                "sample",
                "--execution-timeout-ms",
                "10000",
            ],
        ));
    }
}

// Test-ID: PR-TEST-0495
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_plain_migration_and_missing_target_output_are_distinct() {
    for (shell, executable) in shells() {
        for missing in [false, true] {
            let scenario = Scenario::new(705, &source(shell, executable, &[]));
            fs::write(scenario.source.join("script.txt"), "exit 0\n").unwrap();
            let revision = scenario.install_and_create("sample");
            let digest = revision.rsplit('/').next().unwrap();
            let base =
                source(shell, executable, &[]).replace("{package_id}", &format!("{:032x}", 705));
            let migration = format!(
                "  migrations:\n    - source_revision_digest: '{digest}'\n      transitions: []\n      requires_source: []\n      requires_target: []\n      produces_target: {}\n      hook:\n        protocol_version: 2\n        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}\n        args: []\n        io: {{terminal: none}}\n",
                if missing { "[data]" } else { "[]" }
            );
            let mut target =
                base.replace("runtime_content:", &format!("{migration}runtime_content:"));
            if missing {
                target = target.replace(
                    "revision:\n",
                    "revision:\n  inputs: [{id: data, required: false, protection: normal}]\n",
                );
            }
            fs::write(scenario.source.join("pactrun.yaml"), target).unwrap();
            let target = scenario.install();
            let result = execute(
                &scenario,
                &[
                    "instance",
                    "migrate",
                    "sample",
                    "--to",
                    &target,
                    "--execution-timeout-ms",
                    "10000",
                ],
            );
            assert_eq!(
                result.status.success(),
                !missing,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

// Test-ID: PR-TEST-0496
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_timeout_does_not_become_success() {
    for (shell, executable) in shells() {
        let scenario = Scenario::new(706, &source(shell, executable, &[]));
        let script = if cfg!(windows) {
            "Start-Sleep -Seconds 30\nexit 0\n"
        } else {
            "sleep 30\nexit 0\n"
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let result = execute(
            &scenario,
            &[
                "invoke",
                "sample",
                "run",
                "--action-timeout-ms",
                "1000",
                "--termination-grace-ms",
                "100",
            ],
        );
        assert!(!result.status.success());
    }
}

// Test-ID: PR-TEST-0497
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_capture_registered_binary_content_round_trips() {
    for (shell, executable) in shells() {
        let hook = format!(
            "        protocol_version: 2\n        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}\n        io: {{terminal: none}}\n"
        );
        let capabilities = format!(
            "  snapshot:\n    capture:\n      parameters: []\n      access: observe\n      hook:\n{hook}        args: [capture]\n    restore:\n      parameters: []\n      hook:\n{hook}        args: [restore]\n"
        );
        let yaml = source(shell, executable, &[])
            .replace(
                "runtime_content:",
                &format!("{capabilities}runtime_content:"),
            )
            .replace("terminal: none", "terminal: output");
        let scenario = Scenario::new(707, &yaml);
        let bytes = vec![0, 255, 128, 10, 13, 0, 42];
        let payload = scenario.path("payload.bin");
        fs::write(&payload, &bytes).unwrap();
        let restored = scenario.path("restored.bin");
        let descriptor = scenario.path("descriptor.json");
        fs::write(
            &descriptor,
            br#"{"role":"state","path":"service/data","candidate_path":"data"}"#,
        )
        .unwrap();
        let script = if cfg!(windows) {
            format!(
                r#"if ($args[0] -eq 'capture') {{
  $root = & $env:PACTRUN_EXECUTABLE hook candidate
  if ($LASTEXITCODE -ne 0) {{exit 1}}
  [IO.File]::Copy('{}', [IO.Path]::Combine($root, 'data'))
  & $env:PACTRUN_EXECUTABLE hook capture-register --file '{}'
  exit $LASTEXITCODE
}}
$content = (& $env:PACTRUN_EXECUTABLE hook snapshot-content | ConvertFrom-Json)
if ($LASTEXITCODE -ne 0) {{exit 2}}
[IO.File]::Copy([IO.Path]::Combine($content.readonly_root_path, $content.logical_descriptors[0].materialized_path.Replace('/', '\')), '{}')
exit 0
"#,
                payload.display(),
                descriptor.display(),
                restored.display()
            )
        } else {
            // The descriptor path is fixed by this fixture; Session JSON parsing
            // is delegated to Python only in the test script, never required by Loader.
            format!(
                r#"if [ "$1" = capture ]; then
  root=$("$PACTRUN_EXECUTABLE" hook candidate) || exit 1
  cp '{}' "$root/data" || exit 2
  "$PACTRUN_EXECUTABLE" hook capture-register --file '{}'
  exit $?
fi
"$PACTRUN_EXECUTABLE" hook snapshot-content | python3 -c 'import sys,json,shutil; c=json.load(sys.stdin); shutil.copyfile(c["readonly_root_path"]+"/"+c["logical_descriptors"][0]["materialized_path"],sys.argv[1])' '{}'
"#,
                payload.display(),
                descriptor.display(),
                restored.display()
            )
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let capture = execute(
            &scenario,
            &[
                "snapshot",
                "capture",
                "sample",
                "--execution-timeout-ms",
                "10000",
            ],
        );
        assert_success(&capture);
        let text = String::from_utf8(capture.stdout).unwrap();
        let snapshot = text
            .lines()
            .find_map(|l| l.strip_prefix("snapshot: "))
            .expect(&text);
        assert_success(&execute(
            &scenario,
            &[
                "snapshot",
                "restore",
                "sample",
                snapshot,
                "--execution-timeout-ms",
                "10000",
            ],
        ));
        assert_eq!(fs::read(restored).unwrap(), bytes);
    }
}

// Test-ID: PR-TEST-0490
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_transform_proposal_receipt_and_exit_preserve_target_boundary() {
    for (shell, executable) in shells() {
        for code in [0, 8] {
            let mut yaml = source(shell, executable, &[]).replace("revision:\n", "revision:\n  service_storages: [{id: state}]\n  service_resources: [{id: data, storage_id: state, locator: data, kind: file}]\n");
            let scenario = Scenario::new(708, &yaml);
            fs::write(scenario.source.join("script.txt"), "exit 0\n").unwrap();
            let old = scenario.install_and_create("sample");
            let digest = old.rsplit('/').next().unwrap();
            yaml = yaml.replace("{package_id}", &format!("{:032x}", 708));
            let migration = format!(
                r#"  migrations:
    - source_revision_digest: '{digest}'
      transitions: []
      requires_source: []
      requires_target: []
      produces_target: []
      storage_transitions:
        - {{kind: reuse, source: {{role: active, storage_id: state}}, target_storage_id: state}}
      resource_transitions:
        - {{kind: transform, sources: [{{role: active, resource_id: data}}], targets: [data]}}
      hook:
        protocol_version: 2
        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}
        args: []
        io: {{terminal: none}}
        service_access:
          - reference: {{view: source, role: active, kind: resource, id: data}}
            mode: read
          - reference: {{view: target, role: active, kind: resource, id: data}}
            mode: write
"#
            );
            yaml = yaml.replace("runtime_content:", &format!("{migration}runtime_content:"));
            fs::write(scenario.source.join("pactrun.yaml"), yaml).unwrap();
            let mut script = helper_calls(&["risk enter"], 0);
            script = script.replace("exit 0\n", "");
            if cfg!(windows) {
                script.push_str("$session = (& $env:PACTRUN_EXECUTABLE hook session | ConvertFrom-Json)\nif ($LASTEXITCODE -ne 0) {exit 3}\n$path = & $env:PACTRUN_EXECUTABLE hook resource $session.service_authorities[1].handle\nif ($LASTEXITCODE -ne 0) {exit 4}\n[IO.File]::WriteAllText($path, 'target')\n");
            } else {
                script.push_str("handle=$(\"$PACTRUN_EXECUTABLE\" hook session | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"service_authorities\"][1][\"handle\"])') || exit 3\npath=$(\"$PACTRUN_EXECUTABLE\" hook resource \"$handle\") || exit 4\nprintf target > \"$path\" || exit 5\n");
            }
            script.push_str(&helper_calls(&["target-ready"], code));
            fs::write(scenario.source.join("script.txt"), script).unwrap();
            let target = scenario.install();
            let result = execute(
                &scenario,
                &[
                    "instance",
                    "migrate",
                    "sample",
                    "--to",
                    &target,
                    "--execution-timeout-ms",
                    "10000",
                ],
            );
            assert_eq!(
                result.status.success(),
                code == 0,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let shown = scenario.run(["instance", "show", "sample"]);
            assert_success(&shown);
            let text = String::from_utf8_lossy(&shown.stdout);
            assert!(
                text.contains(if code == 0 { &target } else { &old }),
                "{text}"
            );
        }
    }
}

// Test-ID: PR-TEST-0486
// Verifies: PR-REQ-0348, PR-REQ-0349
#[test]
fn shell_loader_plain_scripts_install_reload_and_complete_without_helpers() {
    for (shell, executable) in shells() {
        for code in [0, 7] {
            let scenario = Scenario::new(700, &source(shell, executable, &[]));
            fs::write(scenario.source.join("script.txt"), format!("exit {code}\n")).unwrap();
            scenario.install_and_create("sample");
            let result = invoke(&scenario);
            assert_eq!(
                result.status.success(),
                code == 0,
                "{shell}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

// Test-ID: PR-TEST-0498
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_preserves_special_arguments_without_evaluation() {
    for (shell, executable) in shells() {
        let arguments = [
            "",
            "two words",
            "a\"b",
            "trailing\\",
            "a'b",
            "$HOME; echo injected",
            "line\nbreak",
            "\u{4e2d}\u{6587}",
        ]
        .map(str::to_owned);
        let scenario = Scenario::new(701, &source(shell, executable, &arguments));
        let output = scenario.path("arguments.json");
        let script = if cfg!(windows) {
            format!(
                "[IO.File]::WriteAllText('{}', (ConvertTo-Json -InputObject @($args) -Compress), (New-Object Text.UTF8Encoding($false)))\nexit 0\n",
                output.display().to_string().replace('\'', "''")
            )
        } else {
            // Exact byte comparison avoids depending on a JSON interpreter in sh.
            format!("printf '%s\\000' \"$@\" > '{}'\nexit 0\n", output.display())
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        assert_success(&invoke(&scenario));
        let bytes = fs::read(output).unwrap();
        if cfg!(windows) {
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                json!(arguments),
                "{shell}"
            );
        } else {
            let expected = arguments
                .iter()
                .flat_map(|v| v.bytes().chain([0]))
                .collect::<Vec<_>>();
            assert_eq!(bytes, expected, "{shell}");
        }
    }
}

// Test-ID: PR-TEST-0499
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_helpers_reuse_session_and_preserve_binary_output() {
    for (shell, executable) in shells() {
        let yaml = source(shell, executable, &[]).replace("outputs: []", "outputs: [{id: result}]");
        let scenario = Scenario::new(702, &yaml);
        let payload = scenario.path("bytes.bin");
        // Exceeds the canonical 16 MiB frame bound: bytes travel through files,
        // not helper IPC or text encoding.
        let bytes = (0..=255)
            .cycle()
            .take(16 * 1024 * 1024 + 3)
            .collect::<Vec<u8>>();
        fs::write(&payload, &bytes).unwrap();
        let script = if cfg!(windows) {
            format!(
                "& $env:PACTRUN_EXECUTABLE hook workspace\nif ($LASTEXITCODE -ne 0) {{exit 1}}\n& $env:PACTRUN_EXECUTABLE hook output result --file '{}'\nif ($LASTEXITCODE -ne 0) {{exit 2}}\n& $env:PACTRUN_EXECUTABLE hook output-register result\nexit $LASTEXITCODE\n",
                payload.display()
            )
        } else {
            format!(
                "\"$PACTRUN_EXECUTABLE\" hook workspace || exit 1\n\"$PACTRUN_EXECUTABLE\" hook output result --file '{}' || exit 2\n\"$PACTRUN_EXECUTABLE\" hook output-register result\n",
                payload.display()
            )
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let result = invoke(&scenario);
        assert_success(&result);
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let run = text
            .lines()
            .find_map(|line| line.strip_prefix("run: "))
            .expect(&text);
        let exported = scenario.path("export.bin");
        let mut export = command(
            &scenario.storage,
            &scenario.path(""),
            ["run", "artifact", "export", run, "result", "--output"],
        );
        export.arg(&exported).arg("--authorize-sensitive-export");
        assert_success(&run_command(export));
        assert_eq!(fs::read(exported).unwrap(), bytes);
    }
}

// Test-ID: PR-TEST-0500
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_helper_without_session_fails_without_creating_storage() {
    let result = Command::new(env!("CARGO_BIN_EXE_pactrun"))
        .args(["hook", "workspace"])
        .env_remove("PACTRUN_SHELL_HELPER_ENDPOINT")
        .output()
        .unwrap();
    assert!(!result.status.success());
}

// Test-ID: PR-TEST-0501
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_terminal_contracts_keep_helper_ipc_out_of_terminal_streams() {
    for (shell, executable) in shells() {
        for terminal in ["none", "output", "interactive"] {
            let yaml = source(shell, executable, &[])
                .replace("terminal: output", &format!("terminal: {terminal}"));
            let scenario = Scenario::new(709, &yaml);
            let script = if cfg!(windows) {
                "$null = & $env:PACTRUN_EXECUTABLE hook workspace\nif ($LASTEXITCODE -ne 0) {exit 2}\n[Console]::Out.WriteLine('loader-stdout')\n[Console]::Error.WriteLine('loader-stderr')\nexit 0\n"
            } else {
                "\"$PACTRUN_EXECUTABLE\" hook workspace >/dev/null || exit 2\nprintf 'loader-stdout\\n'\nprintf 'loader-stderr\\n' >&2\nexit 0\n"
            };
            fs::write(scenario.source.join("script.txt"), script).unwrap();
            scenario.install_and_create("sample");
            let result = invoke(&scenario);
            assert_success(&result);
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            let all = format!("{stdout}{stderr}");
            assert_eq!(
                all.contains("loader-stdout"),
                terminal != "none",
                "{terminal}: {all}"
            );
            assert_eq!(
                all.contains("loader-stderr"),
                terminal != "none",
                "{terminal}: {all}"
            );
            if terminal == "output" {
                assert!(!stdout.contains("loader-stderr"));
                assert!(!stderr.contains("loader-stdout"));
            }
            assert!(!all.contains("session_start"));
            assert!(!all.contains("PACTRUN_SHELL_HELPER_ENDPOINT"));
        }
    }
}

// Test-ID: PR-TEST-0502
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_secret_inputs_and_typed_parameters_use_explicit_file_interfaces() {
    for (shell, executable) in shells() {
        let yaml = source(shell, executable, &[]).replace("revision:\n", "revision:\n  inputs: [{id: secret, required: false, protection: secret}]\n")
            .replace("parameters: []", "parameters: [{id: enabled, type: boolean, sensitive: false, default: true}, {id: secret_parameter, type: string, sensitive: true, default: private-canary}]");
        let scenario = Scenario::new(710, &yaml);
        let source = scenario.path("source.bin");
        let copied = scenario.path("copied.bin");
        let boolean = scenario.path("bool.json");
        fs::write(&source, [0, 255, 13, 10, 128]).unwrap();
        let script = helper_calls(
            &[
                &format!("input secret --copy-to '{}'", copied.display()),
                &format!("parameter enabled --output '{}'", boolean.display()),
            ],
            0,
        );
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let mut set = command(
            &scenario.storage,
            &scenario.path(""),
            ["input", "set", "sample", "secret", "--file"],
        );
        set.arg(&source);
        assert_success(&run_command(set));
        let result = invoke(&scenario);
        assert_success(&result);
        assert_eq!(fs::read(copied).unwrap(), fs::read(source).unwrap());
        assert_eq!(fs::read(boolean).unwrap(), b"true");
        let run = super::support::run_id(&result);
        let shown = scenario.run(["run", "show", &run]);
        assert_success(&shown);
        for bytes in [&result.stdout, &result.stderr, &shown.stdout, &shown.stderr] {
            assert!(!String::from_utf8_lossy(bytes).contains("private-canary"));
        }
    }
}

// Test-ID: PR-TEST-0503
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_concurrent_helpers_share_one_session_and_local_errors_are_handleable() {
    for (shell, executable) in shells() {
        let scenario = Scenario::new(711, &source(shell, executable, &[]));
        let script = if cfg!(windows) {
            r#"$children = @()
for ($i = 0; $i -lt 8; $i++) {
  $p = New-Object Diagnostics.Process
  $p.StartInfo.FileName = $env:PACTRUN_EXECUTABLE
  $p.StartInfo.Arguments = 'hook workspace'
  $p.StartInfo.UseShellExecute = $false
  $null = $p.Start()
  $children += $p
}
foreach ($p in $children) { $p.WaitForExit(); if ($p.ExitCode -ne 0) {exit 3} }
& $env:PACTRUN_EXECUTABLE hook parameter missing
if ($LASTEXITCODE -eq 0) {exit 4}
Write-Error 'handled-fixture-error'
exit 0
"#
        } else {
            "pids=''\nfor i in 1 2 3 4 5 6 7 8; do \"$PACTRUN_EXECUTABLE\" hook workspace & pids=\"$pids $!\"; done\nfor p in $pids; do wait \"$p\" || exit 3; done\nif \"$PACTRUN_EXECUTABLE\" hook parameter missing; then exit 4; fi\nfalse\nexit 0\n"
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        assert_success(&invoke(&scenario));
    }
}

// Test-ID: PR-TEST-0504
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_abnormal_script_loss_cannot_request_success() {
    for (shell, executable) in shells() {
        let scenario = Scenario::new(712, &source(shell, executable, &[]));
        let script = if cfg!(windows) {
            "Stop-Process -Id $PID -Force\nexit 0\n"
        } else {
            "kill -KILL $$\nexit 0\n"
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        assert!(!invoke(&scenario).status.success());
    }
}

#[cfg(unix)]
// Test-ID: PR-TEST-0505
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_loss_is_failed_and_core_removes_its_private_helper_socket() {
    for (shell, executable) in shells() {
        let scenario = Scenario::new(713, &source(shell, executable, &[]));
        let marker = scenario.path("helper-endpoint");
        let script = format!(
            "printf '%s' \"$PACTRUN_SHELL_HELPER_ENDPOINT\" > '{}'\nkill -KILL \"$PPID\"\nsleep 30\n",
            marker.display()
        );
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        assert!(!invoke(&scenario).status.success());
        let endpoint = fs::read_to_string(marker).unwrap();
        assert!(!std::path::Path::new(&endpoint).exists());
        assert!(!std::path::Path::new(&endpoint).parent().unwrap().exists());
    }
}

// Test-ID: PR-TEST-0506
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_explicit_failure_with_open_risk_is_valid_failure_not_success() {
    for (shell, executable) in shells() {
        let yaml = source(shell, executable, &[]).replace("access: observe", "access: mutate");
        let scenario = Scenario::new(714, &yaml);
        let result = scenario.path("failure.json");
        fs::write(
            &result,
            br#"{"status":"failure","code":"intentional","message":"test failure"}"#,
        )
        .unwrap();
        let script = helper_calls(
            &[
                "risk enter",
                &format!("completion --file '{}'", result.display()),
            ],
            0,
        );
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let output = invoke(&scenario);
        assert!(!output.status.success());
        let text = String::from_utf8_lossy(&output.stderr);
        assert!(text.contains("hook_completion_status: failure"), "{text}");
        assert!(text.contains("terminal_risk: open"), "{text}");
    }
}

// Test-ID: PR-TEST-0507
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_migration_reads_pinned_source_and_explicitly_registers_target_bytes() {
    for (shell, executable) in shells() {
        let base = source(shell, executable, &[]).replace(
            "revision:\n",
            "revision:\n  inputs: [{id: data, required: false, protection: normal}]\n",
        );
        let scenario = Scenario::new(715, &base);
        fs::write(scenario.source.join("script.txt"), "exit 0\n").unwrap();
        let old = scenario.install_and_create("sample");
        let bytes = [0, 255, 128, 10, 0, 42];
        let initial = scenario.path("initial.bin");
        fs::write(&initial, bytes).unwrap();
        let mut set = command(
            &scenario.storage,
            &scenario.path(""),
            ["input", "set", "sample", "data", "--file"],
        );
        set.arg(initial);
        assert_success(&run_command(set));
        let digest = old.rsplit('/').next().unwrap();
        let migration = format!(
            "  migrations:\n    - source_revision_digest: '{digest}'\n      transitions: []\n      requires_source: [{{role: active, input_id: data}}]\n      requires_target: []\n      produces_target: [result]\n      hook:\n        protocol_version: 2\n        launch: {{kind: shell_loader, shell: {shell}, command: {executable}, script: script}}\n        args: []\n        io: {{terminal: output}}\n"
        );
        let target_source = base
            .replace("inputs: [{id: data,", "inputs: [{id: result,")
            .replace("{package_id}", &format!("{:032x}", 715))
            .replace("runtime_content:", &format!("{migration}runtime_content:"));
        fs::write(scenario.source.join("pactrun.yaml"), target_source).unwrap();
        let copy = scenario.path("source-view.bin");
        let script = helper_calls(
            &[
                &format!("input data --view source --copy-to '{}'", copy.display()),
                &format!("output result --file '{}'", copy.display()),
                "output-register result",
            ],
            0,
        );
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        let target = scenario.install();
        assert_success(&execute(
            &scenario,
            &[
                "instance",
                "migrate",
                "sample",
                "--to",
                &target,
                "--execution-timeout-ms",
                "10000",
            ],
        ));
        let exported = scenario.path("target.bin");
        let mut export = command(
            &scenario.storage,
            &scenario.path(""),
            ["input", "export", "sample", "result", "--output"],
        );
        export.arg(&exported);
        assert_success(&run_command(export));
        assert_eq!(fs::read(exported).unwrap(), bytes);
    }
}

// Test-ID: PR-TEST-0508
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_local_result_file_failure_can_be_handled_without_replay() {
    for (shell, executable) in shells() {
        let scenario = Scenario::new(716, &source(shell, executable, &[]));
        let file = scenario.path("existing.json");
        fs::write(&file, "unchanged").unwrap();
        let script = if cfg!(windows) {
            format!(
                "& $env:PACTRUN_EXECUTABLE hook session --output '{}'\nif ($LASTEXITCODE -eq 0) {{exit 1}}\n{}",
                file.display(),
                helper_calls(&["workspace"], 0)
            )
        } else {
            format!(
                "if \"$PACTRUN_EXECUTABLE\" hook session --output '{}'; then exit 1; fi\n{}",
                file.display(),
                helper_calls(&["workspace"], 0)
            )
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        assert_success(&invoke(&scenario));
        assert_eq!(fs::read_to_string(file).unwrap(), "unchanged");
    }
}

// Test-ID: PR-TEST-0509
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_interactive_input_remains_available_to_script_not_helpers() {
    for (shell, executable) in shells() {
        let yaml =
            source(shell, executable, &[]).replace("terminal: output", "terminal: interactive");
        let scenario = Scenario::new(717, &yaml);
        let received = scenario.path("received.txt");
        let input = scenario.path("input.txt");
        fs::write(&input, b"terminal-input\n").unwrap();
        let script = if cfg!(windows) {
            format!(
                "$null = & $env:PACTRUN_EXECUTABLE hook workspace\nif ($LASTEXITCODE -ne 0) {{exit 2}}\n$line = [Console]::ReadLine()\n[IO.File]::WriteAllText('{}', $line)\nexit 0\n",
                received.display()
            )
        } else {
            format!(
                "\"$PACTRUN_EXECUTABLE\" hook workspace >/dev/null || exit 2\nIFS= read -r line || exit 3\nprintf '%s' \"$line\" > '{}'\n",
                received.display()
            )
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        let mut cmd = command(
            &scenario.storage,
            &scenario.path(""),
            ["invoke", "sample", "run", "--action-timeout-ms", "10000"],
        );
        cmd.stdin(fs::File::open(input).unwrap());
        if shell == "windows_powershell_5_1" {
            cmd.env("PSExecutionPolicyPreference", "RemoteSigned");
        }
        assert_success(&run_command(cmd));
        assert_eq!(fs::read(received).unwrap(), b"terminal-input");
    }
}

// Test-ID: PR-TEST-0510
// Verifies: PR-REQ-0348
#[test]
fn shell_loader_unsupported_host_pair_fails_before_script_start() {
    let (shell, executable) = if cfg!(windows) {
        ("sh", "powershell.exe")
    } else {
        ("powershell_7", "sh")
    };
    let scenario = Scenario::new(718, &source(shell, executable, &[]));
    fs::write(scenario.source.join("script.txt"), "exit 0\n").unwrap();
    scenario.install_and_create("sample");
    let result = invoke(&scenario);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsupported on this host"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("run: "));
}

// Test-ID: PR-TEST-0517
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_busy_helpers_wait_for_owner_controlled_completion_without_replaying() {
    for (shell, executable) in shells() {
        let parameter = serde_json::to_string(&"x".repeat(256 * 1024)).unwrap();
        let yaml = source(shell, executable, &[]).replace(
            "parameters: []",
            &format!(
                "parameters: [{{id: blob, type: string, sensitive: false, default: {parameter}}}]"
            ),
        );
        let scenario = Scenario::new(720, &yaml);
        let script = if cfg!(windows) {
            r#"$children = @()
foreach ($arguments in @('hook parameter blob', 'hook workspace', 'hook workspace')) {
  $p = New-Object Diagnostics.Process
  $p.StartInfo.FileName = $env:PACTRUN_EXECUTABLE
  $p.StartInfo.Arguments = $arguments
  $p.StartInfo.UseShellExecute = $false
  $p.StartInfo.CreateNoWindow = $true
  $p.StartInfo.RedirectStandardOutput = $true
  $null = $p.Start()
  $children += $p
  Start-Sleep -Milliseconds 300
}
Start-Sleep -Seconds 3
foreach ($p in $children) {
  $null = $p.StandardOutput.ReadToEnd()
  $p.WaitForExit()
  if ($p.ExitCode -ne 0) {exit 2}
}
exit 0
"#
        } else {
            r#"python3 -c 'import os,subprocess,time
children=[]
for args in [("parameter","blob"),("workspace",),("workspace",)]:
 children.append(subprocess.Popen([os.environ["PACTRUN_EXECUTABLE"],"hook",*args],stdout=subprocess.PIPE))
 time.sleep(.3)
time.sleep(3)
for child in children:
 child.communicate()
 if child.returncode: raise SystemExit(2)
'
"#
        };
        fs::write(scenario.source.join("script.txt"), script).unwrap();
        scenario.install_and_create("sample");
        assert_success(&invoke(&scenario));
    }
}

#[cfg(windows)]
// Test-ID: PR-TEST-0516
// Verifies: PR-REQ-0349
#[test]
fn shell_loader_rejects_ambient_file_locator_without_touching_its_bytes() {
    let scenario = Scenario::new(719, &source("powershell_7", "pwsh.exe", &[]));
    let path = scenario.path("not-a-pipe");
    fs::write(&path, b"untouched-by-helper").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_pactrun"))
        .args(["hook", "workspace"])
        .env("PACTRUN_SHELL_HELPER_ENDPOINT", &path)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&path).unwrap(), b"untouched-by-helper");
    assert_eq!(
        fs::read_dir(scenario.storage.join("database"))
            .unwrap()
            .count(),
        0
    );
}
