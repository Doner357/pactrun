// Included in cli::tests; fixtures are isolated under the repository target/.
fn json_invoke(root: &Path, args: Vec<OsString>) -> (i32, serde_json::Value, String) {
    let mut all = vec![OsString::from("--format"), OsString::from("json")];
    all.extend(args);
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run(all, Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut err);
    let value = serde_json::from_slice(&out).unwrap_or_else(|e| panic!("invalid JSON: {e}; stdout={}; stderr={}", String::from_utf8_lossy(&out), String::from_utf8_lossy(&err)));
    schema_tests::assert_response(&value);
    (code, value, String::from_utf8(err).unwrap())
}
fn json_ok(root: &Path, args: &[&str]) -> serde_json::Value {
    let (code, value, err) = json_invoke(root, args.iter().map(OsString::from).collect());
    assert_eq!(code, 0, "{args:?}: {value}; {err}");
    assert_eq!(value["status"], "success");
    value["result"].clone()
}
fn json_install(root: &Path, source: &Path) -> String {
    let (code, value, err) = json_invoke(root, vec!["pack".into(), "install".into(), source.as_os_str().into()]);
    assert_eq!(code, 0, "{value}; {err}");
    format!("exact:{}/{}", value["result"]["revision"]["package_id"].as_str().unwrap(), value["result"]["revision"]["content_digest"].as_str().unwrap())
}

// Test-ID: PR-TEST-0557
// Verifies: PR-REQ-0077, PR-REQ-0083, PR-REQ-0358, PR-REQ-0359
#[test]
fn json_selection_parse_and_startup_errors_precede_storage() {
    let (temp, _, _) = cli_roots();
    let missing = temp.path().join("must-not-create");
    for args in [vec!["unknown"], vec!["--format", "human", "version"], vec!["instance", "list", "--format", "human"]] {
        let (code, response, _) = json_invoke(&missing, args.iter().map(OsString::from).collect());
        assert_eq!(code, 2); assert_eq!(response["error"]["kind"], "usage");
        assert!(!missing.exists());
    }
    struct Fail;
    impl CancellationHandlerInstaller for Fail {
        fn install(&self, _: ActionCancellation) -> Result<(), ()> { Err(()) }
    }
    let mut out = Vec::new(); let mut err = Vec::new();
    let code = run_with_handler_installer(&Fail, ["--format","json","instance","list"].map(OsString::from).to_vec(), Some(missing.as_os_str().into()), &mut io::empty(), &mut out, &mut err);
    assert_eq!(code, 1);
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
    schema_tests::assert_response(&value); assert_eq!(value["error"]["kind"], "startup"); assert!(!missing.exists());
    json_ok(&missing, &["--version"]); json_ok(&missing, &["--help"]); json_ok(&missing, &["pack","generate-id"]);
}

// Test-ID: PR-TEST-0558
// Verifies: PR-REQ-0007, PR-REQ-0027, PR-REQ-0035, PR-REQ-0116, PR-REQ-0120, PR-REQ-0358, PR-REQ-0359, PR-REQ-0360
#[test]
fn json_management_catalog_and_raw_input_share_backend_facts() {
    let (temp, root, source) = cli_roots();
    let revision = json_install(&root, &source);
    let created = json_ok(&root, &["instance","create","node","--revision",&revision]);
    let shown = json_ok(&root, &["instance","show","node"]);
    for (key, value) in created.as_object().unwrap() {
        assert_eq!(value, &shown[key]);
    }
    assert_eq!(shown["related_revisions"].as_array().unwrap().len(), 1);
    let list = json_ok(&root, &["instance","list"]);
    assert_eq!(list["items"][0]["instance_id"], created["instance_id"]);
    let input = temp.path().join("--format"); fs::write(&input, [0,255,10,13,123]).unwrap();
    let (code, _, _) = json_invoke(&root, vec!["input".into(),"set".into(),"node".into(),"config".into(),"--file".into(),input.as_os_str().into()]); assert_eq!(code,0);
    let mut out = Vec::new(); let mut err = Vec::new();
    let code = run(["--format","json","input","export","node","config","--output","-"].map(OsString::from).to_vec(), Some(root.as_os_str().into()), &mut io::empty(), &mut out, &mut err);
    assert_eq!(code,0); assert_eq!(out, [0,255,10,13,123]); assert!(err.is_empty());
    let exported = temp.path().join("exported");
    assert_eq!(json_invoke(&root, vec!["input".into(),"export".into(),"node".into(),"config".into(),"--output".into(),exported.as_os_str().into()]).0,0);
    assert_eq!(fs::read(exported).unwrap(),out);
    json_ok(&root,&["input","list","node"]); json_ok(&root,&["input","delete","node","config"]);
    let before_inspection = json_ok(&root, &["instance", "show", "node"]);
    for args in [vec!["revision","list"],vec!["revision","show",&revision],vec!["revision","metadata","show",&revision],vec!["instance","history","list"],vec!["instance","deletion","list"],vec!["run","list"],vec!["snapshot","list"],vec!["action","list","node"],vec!["storage","gc","--plan"]] { json_ok(&root,&args); }
    for field in ["note","trust"] {
        let value = if field == "note" { "not absent\n\"text\"" } else { "trusted" };
        let mut args = vec!["revision",field,"set",&revision];
        if field == "note" { args.push("--value"); }
        args.extend([value,"--expect-absent"]); json_ok(&root,&args);
        json_ok(&root,&["revision",field,"show",&revision]);
        json_ok(&root,&["revision",field,"clear",&revision,"--expect",value]);
    }
    json_ok(&root,&["revision","alias","set","alias",&revision,"--expect-absent"]);
    json_ok(&root,&["revision","alias","show","alias"]);
    json_ok(&root,&["revision","alias","clear","alias","--expect",&revision]);
    let base = temp.path().join("transport");
    assert_eq!(json_invoke(&root,vec!["revision".into(),"export".into(),revision.into(),"--output".into(),base.as_os_str().into()]).0,0);
    assert!(temp.path().join("transport.pack").exists());
    let after_inspection = json_ok(&root, &["instance", "show", "node"]);
    assert_eq!(after_inspection["state_version"], before_inspection["state_version"]);
    let database = rusqlite::Connection::open(root.join("database/pactrun.sqlite3")).unwrap();
    let runs: i64 = database.query_row("SELECT count(*) FROM runs", [], |row| row.get(0)).unwrap();
    assert_eq!(runs, 0);
}

// Test-ID: PR-TEST-0559
// Verifies: PR-REQ-0359, PR-REQ-0360, PR-REQ-0120
#[test]
fn json_secret_denial_is_not_a_payload_or_extra_disclosure() {
    let (temp, root, source) = cli_roots(); let revision = json_install(&root,&source);
    json_ok(&root,&["instance","create","node","--revision",&revision]);
    let file=temp.path().join("secret"); fs::write(&file,b"json-secret-sentinel").unwrap();
    assert_eq!(json_invoke(&root,vec!["input".into(),"set".into(),"node".into(),"secret".into(),"--file".into(),file.as_os_str().into()]).0,0);
    let mut out=Vec::new(); let mut err=Vec::new();
    let code=run(["--format","json","input","export","node","secret","--output","-"].map(OsString::from).to_vec(),Some(root.as_os_str().into()),&mut io::empty(),&mut out,&mut err);
    assert_eq!(code,1); assert!(out.is_empty());
    let error:serde_json::Value=serde_json::from_slice(&err).unwrap(); schema_tests::assert_response(&error);
    assert!(!String::from_utf8(err).unwrap().contains("json-secret-sentinel"));
    let shown=json_ok(&root,&["instance","show","node"]); assert!(!shown.to_string().contains("json-secret-sentinel"));
}

// Test-ID: PR-TEST-0566
// Verifies: PR-REQ-0358, PR-REQ-0359, PR-REQ-0360
#[test]
fn json_raw_usage_errors_and_stable_error_references_are_not_inferred_from_text() {
    let (temp,_,_)=cli_roots(); let root=temp.path().join("absent");
    let mut out=Vec::new(); let mut err=Vec::new();
    let code=run(["--format","json","input","export","node","config","--output","-","--bogus"].map(OsString::from).to_vec(),Some(root.as_os_str().into()),&mut io::empty(),&mut out,&mut err);
    assert_eq!(code,2); assert!(out.is_empty());
    let value:serde_json::Value=serde_json::from_slice(&err).unwrap(); schema_tests::assert_response(&value); assert!(!root.exists());
    let error=app_error(ApplicationError::Revision(crate::domain::RevisionError::stable("duplicate_property","presentation only")));
    let reference=error.reference.as_ref().unwrap(); assert_eq!(reference.owner,"revision_core"); assert_eq!(reference.code,"duplicate_property");
    let fake=CliError::operation("revision_core.duplicate_property"); assert!(fake.reference.is_none());
    let id=RunId::generate().unwrap(); let associated=fake.with_run_context(id); out.clear(); err.clear();
    assert_eq!(report_cli_failure(presentation::Format::Json,Some("invoke"),false,associated,&mut out,&mut err),1);
    let value:serde_json::Value=serde_json::from_slice(&out).unwrap(); schema_tests::assert_response(&value);
    assert_eq!(value["result"]["run_id"],id.to_string()); assert!(value["error"]["reference"].is_null());
}

// Test-ID: PR-TEST-0560
// Verifies: PR-REQ-0027, PR-REQ-0120, PR-REQ-0133, PR-REQ-0359, PR-REQ-0360
#[test]
fn json_real_execution_plan_and_run_inspection_are_structured_and_redacted() {
    let (temp,root,expected,sensitive)=prepared_cli_hook_instance();
    let before_instance = json_ok(&root, &["instance", "show", "node"]);
    assert!(before_instance["state_version"].is_string());
    let before_inputs = json_ok(&root, &["input", "list", "node"]);
    let marker=temp.path().join("json-hook-marker");
    let mut args=successful_invoke_args(&marker,&expected,&sensitive); args.push("--plan".into());
    let (code,plan,_)=json_invoke(&root,args); assert_eq!(code,0,"{plan}"); assert_eq!(plan["result"]["preview"],"not_admitted"); assert!(!marker.exists());
    json_ok(&root,&["action","show","node","execute"]);
    let mut execution=successful_invoke_args(&marker,&expected,&sensitive);
    *execution.iter_mut().find(|a| **a == "mode=success").unwrap()="mode=output".into();
    let (code,executed,err)=json_invoke(&root,execution);
    assert_eq!(code,0,"{executed}; {err}"); assert!(marker.with_extension("session").exists());
    assert_eq!(executed["result"]["run"]["state"]["outcome"],"succeeded");
    assert!(!executed.to_string().contains("slice4-secret-binding")); assert!(!executed.to_string().contains("slice4-sensitive-parameter"));
    let id=executed["result"]["run"]["run_id"].as_str().unwrap();
    let shown=json_ok(&root,&["run","show",id]); assert_eq!(shown["run"],executed["result"]["run"]);
    json_ok(&root,&["run","list"]); json_ok(&root,&["run","reconcile"]);
    let destination=temp.path().join("artifact");
    let (code,response,_)=json_invoke(&root,vec!["run".into(),"artifact".into(),"export".into(),id.into(),"report".into(),"--output".into(),destination.as_os_str().into(),"--authorize-sensitive-export".into()]);
    assert_eq!(code,0,"{response}"); assert!(destination.exists());
    json_ok(&root,&["run","artifact","delete",id,"report"]);
    let after_instance = json_ok(&root, &["instance", "show", "node"]);
    assert_eq!(after_instance["instance_id"], before_instance["instance_id"]);
    assert_eq!(after_instance["state_version"], before_instance["state_version"]);
    assert_eq!(json_ok(&root, &["input", "list", "node"]), before_inputs);
    // Invocation parameters and Run/Artifact publication are not managed Input mutations.
    json_ok(&root,&["run","delete",id]);
}

// Test-ID: PR-TEST-0561
// Verifies: PR-REQ-0358, PR-REQ-0360, PR-REQ-0120
#[test]
fn json_interactive_is_refused_before_launch_or_parameter_acquisition() {
    for terminal in ["interactive"] {
        let (_temp,root,source)=cli_action_roots();
        let yaml=fs::read_to_string(source.join("pactrun.yaml")).unwrap().replace("terminal: none",&format!("terminal: {terminal}"));
        fs::write(source.join("pactrun.yaml"),yaml).unwrap();
        let revision=json_install(&root,&source); let created=json_ok(&root,&["instance","create","node","--revision",&revision]);
        let (code,value,_)=json_invoke(&root,["invoke","node","inspect","--param-file","value=does-not-exist"].map(OsString::from).to_vec());
        assert_eq!(code,2,"{value}"); assert!(value["error"]["message"].as_str().unwrap().contains("interactive"));
        assert!(json_ok(&root,&["run","list"])["items"].as_array().unwrap().is_empty());
        let shown = json_ok(&root,&["instance","show","node"]);
        for (key, value) in created.as_object().unwrap() { assert_eq!(value, &shown[key]); }
        json_ok(&root,&["invoke","node","inspect","--param","value=x","--plan"]);
    }
}

// Test-ID: PR-TEST-0562
// Verifies: PR-REQ-0359, PR-REQ-0360
#[test]
fn json_broken_response_does_not_undo_committed_input_or_append_an_envelope() {
    let (temp,root,source)=cli_roots(); let revision=json_install(&root,&source);
    json_ok(&root,&["instance","create","node","--revision",&revision]);
    let file=temp.path().join("value"); fs::write(&file,b"committed-once").unwrap();
    let mut out=PrefixFailWriter { bytes:Vec::new(), remaining:12 }; let mut err=Vec::new();
    let code=run(vec!["--format".into(),"json".into(),"input".into(),"set".into(),"node".into(),"config".into(),"--file".into(),file.into_os_string()],Some(root.as_os_str().into()),&mut io::empty(),&mut out,&mut err);
    assert_eq!(code,1); assert_eq!(out.bytes.len(),12);
    let app=PactrunApplication::open(&root).unwrap();
    let id=app.resolve_instance_name(&InstanceName::parse("node").unwrap()).unwrap().unwrap();
    let observed=app.export_input(id,&InputIdentity::parse("config").unwrap(),false).unwrap();
    let mut bytes=Vec::new(); observed.bytes.try_clone_reader().unwrap().read_to_end(&mut bytes).unwrap(); assert_eq!(bytes,b"committed-once");
}

// Test-ID: PR-TEST-0569
// Verifies: PR-REQ-0359, PR-REQ-0360
#[test]
fn json_publication_failure_preserves_known_destination_facts_without_rollback() {
    let (temp,root,source)=cli_roots(); let revision=json_install(&root,&source);
    json_ok(&root,&["instance","create","node","--revision",&revision]);
    let file=temp.path().join("input"); fs::write(&file,b"published-before-failure").unwrap();
    assert_eq!(json_invoke(&root,vec!["input".into(),"set".into(),"node".into(),"config".into(),"--file".into(),file.into_os_string()]).0,0);
    let app=PactrunApplication::open(&root).unwrap(); let id=app.resolve_instance_name(&InstanceName::parse("node").unwrap()).unwrap().unwrap();
    let observed=app.export_input(id,&InputIdentity::parse("config").unwrap(),false).unwrap(); let destination=temp.path().join("published");
    let failure=crate::output_publication::fail_after_publication(&observed.bytes,&destination).unwrap_err(); assert!(failure.destination_published);
    let error=publication_error(failure,&destination); let mut out=Vec::new(); let mut stderr=Vec::new();
    assert_eq!(report_cli_failure(presentation::Format::Json,Some("input export"),false,error,&mut out,&mut stderr),1);
    let value:serde_json::Value=serde_json::from_slice(&out).unwrap(); schema_tests::assert_response(&value); assert_eq!(value["result"]["destination_published"],true);
    assert_eq!(fs::read(&destination).unwrap(),b"published-before-failure");
    let collision=crate::output_publication::publish_reported(&observed.bytes,&destination).unwrap_err(); assert!(!collision.destination_published); assert!(destination.exists());
    for mapper in [app_error, snapshots::safe_error] {
        let error=mapper(ApplicationError::Publication { operation:"test publication",destination:destination.clone(),destination_published:true,source:io::Error::other("injected") });
        out.clear();
        assert_eq!(report_cli_failure(presentation::Format::Json,Some("snapshot export"),false,error,&mut out,&mut stderr),1);
        let value:serde_json::Value=serde_json::from_slice(&out).unwrap(); schema_tests::assert_response(&value); assert_eq!(value["result"]["destination_published"],true);
    }
}

// Test-ID: PR-TEST-0564
// Verifies: PR-REQ-0359, PR-REQ-0360, PR-REQ-0120
#[test]
fn json_snapshot_transport_verification_and_plan_preserve_archive_bytes() {
    let (temp,root,revision,snapshot)=create_restore_fixture();
    let id=snapshot.to_string(); let reference=format_revision(&revision);
    json_ok(&root,&["snapshot","list"]); json_ok(&root,&["snapshot","show",&id]);
    let verified=json_ok(&root,&["snapshot","verify",&id]); assert_eq!(verified["content_verification"],"valid");
    let base=temp.path().join("json-snapshot");
    let (code,value,_)=json_invoke(&root,vec!["snapshot".into(),"export".into(),id.clone().into(),"--output".into(),base.as_os_str().into(),"--authorize-sensitive-export".into()]); assert_eq!(code,0,"{value}");
    let archive=temp.path().join("json-snapshot.snapshot"); assert_eq!(&fs::read(&archive).unwrap()[..2],b"PK");
    let (code,value,_)=json_invoke(&root,vec!["snapshot".into(),"import".into(),archive.into_os_string()]); assert_eq!(code,0,"{value}"); assert_eq!(value["result"]["outcome"],"already_present");
    json_ok(&root,&["instance","create","node","--revision",&reference]);
    let plan=json_ok(&root,&["snapshot","restore","node",&id,"--plan"]); assert_eq!(plan["preview"],"not_admitted");
    let current=json_ok(&root,&["instance","show","node"]);
    let (code,failed,_)=json_invoke(&root,["instance","create","node","--revision",&reference,"--restore-from",&id].map(OsString::from).to_vec()); assert_eq!(code,1,"{failed}");
    assert_eq!(json_ok(&root,&["instance","show","node"]),current);
    json_ok(&root,&["snapshot","delete",&id]);
}

// Test-ID: PR-TEST-0625
// Verifies: PR-REQ-0077, PR-REQ-0331
#[test]
fn version_reports_the_eight_explicit_supported_contracts_without_storage_access() {
    let (temp, _, _) = cli_roots();
    let root = temp.path().join("version-must-not-initialize");
    let mut out=Vec::new(); let mut err=Vec::new();
    assert_eq!(run(["--format", "json", "--version"].into_iter().map(OsString::from).collect(), Some(root.as_os_str().to_owned()), &mut io::empty(), &mut out, &mut err), 0);
    let result: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(result["result"]["product_version"], env!("CARGO_PKG_VERSION"));
    let formats = result["result"]["supported_formats"].as_object().unwrap();
    assert_eq!(formats.len(), 8);
    for domain in crate::domain::VersionDomain::ALL {
        assert_eq!(formats[domain.name()], serde_json::json!([domain.current_text()]));
    }
    assert_eq!(result["result"]["default_formats"].as_object().unwrap().len(), 8);
    for domain in crate::domain::VersionDomain::ALL {
        assert_eq!(result["result"]["default_formats"][domain.name()], domain.current_text());
    }
    assert!(!root.exists()); assert!(err.is_empty());
}
