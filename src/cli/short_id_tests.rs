// Test-ID: PR-TEST-0586
// Verifies: PR-REQ-0038, PR-REQ-0086, PR-REQ-0369
#[test]
fn short_id_grammar_preserves_full_tokens_names_and_preflight_ordering() {
    use short_ids::Selector;
    for text in ["abcdef12", "abcdef123", "0123456789abcdef0123456789abcdef"] {
        assert!(text.parse::<Selector<RunId>>().is_ok());
    }
    for text in ["abcdef1", "ABCDEF12", "abcdef1g", "0123456789abcdef0123456789abcdef0", "abcdef12..."] {
        assert!(text.parse::<Selector<RunId>>().is_err());
    }
    assert!("mp1-abcdef12".parse::<Selector<crate::domain::MigrationPathId>>().is_ok());
    assert!("sha256:abcdef123".parse::<Selector<RevisionContentDigest>>().is_ok());
    assert!(parse_state_version("abcdef12".into()).is_err());
    assert!(parse_input_id("abcdef12").is_ok());
    let mut command=parse_command(["action","show","abcdef12","abcdef12"].map(Into::into).to_vec()).unwrap();
    short_ids::resolve(&mut command,None,&ActionCancellation::default()).unwrap();
    let mut command=parse_command(["run","show","0123456789abcdef0123456789abcdef"].map(Into::into).to_vec()).unwrap();
    short_ids::resolve(&mut command,None,&ActionCancellation::default()).unwrap(); // Full identities need no lookup.
    let (temp,_,_)=cli_roots(); let missing=temp.path().join("uncreated");
    struct MustNotRead;
    impl Read for MustNotRead {fn read(&mut self,_:&mut[u8])->io::Result<usize>{panic!("prefix preflight must not acquire stdin")}}
    let mut stdin=CancellableStdin::new(MustNotRead,ActionCancellation::default());
    let code=run(["--format","json","instance","create","new","--revision","exact:abcdef12/sha256:abcdef12","--input-stdin","config"].map(Into::into).to_vec(),Some(missing.as_os_str().into()),&mut stdin,&mut Vec::new(),&mut Vec::new());
    assert_eq!(code,1);assert!(stdin.worker.is_none());assert!(!missing.exists());
    for args in [vec!["run","show","abcdef12","--bogus"],vec!["instance","deletion","confirm-complete","abcdef12","--attempt","abcdef12","--if-version","abcdef12","--assert-cleanup-complete"]] {
        let(code,_,_)=json_invoke(&missing,args.iter().map(OsString::from).collect());assert_eq!(code,2);assert!(!missing.exists());
    }
}

// Test-ID: PR-TEST-0587
// Verifies: PR-REQ-0038, PR-REQ-0086, PR-REQ-0369, PR-REQ-0370
#[test]
fn short_id_resolution_and_display_cover_hidden_history_collisions_without_writes() {
    let (_temp,root,source)=cli_roots();let revision=json_install(&root,&source);
    let view=json_ok(&root,&["instance","create","node","--revision",&revision]);
    let id=view["instance_id"].as_str().unwrap();
    let mut other=id.to_owned();other.replace_range(12..13,if &id[12..13]=="0"{"1"}else{"0"});
    let database=root.join("database/pactrun.sqlite3");
    let db=rusqlite::Connection::open(&database).unwrap();
    db.execute("INSERT INTO instance_history_identities(instance_id,instance_name) VALUES(?1,?2)",rusqlite::params![hex::decode(&other).unwrap(),b"historical".as_slice()]).unwrap();drop(db);
    let before=fs::read(&database).unwrap();
    let(code,error,_)=json_invoke(&root,["instance","history","show",&id[..12]].map(OsString::from).to_vec());
    assert_eq!(code,1);assert_eq!(error["result"]["selector_kind"],"instance");
    assert_eq!(error["result"]["candidates"].as_array().unwrap().len(),2);
    assert!(error["result"]["candidates"].as_array().unwrap().contains(&serde_json::json!(id)));
    let(code,_,_)=json_invoke(&root,["instance","deletion","show",&id[..12]].map(OsString::from).to_vec());assert_eq!(code,1); // Eligibility cannot disambiguate.
    let mut out=Vec::new();let mut err=Vec::new();
    assert_eq!(run(["instance","history","list","--limit","1"].map(Into::into).to_vec(),Some(root.as_os_str().into()),&mut io::empty(),&mut out,&mut err),0);
    let text=String::from_utf8(out).unwrap();let short=text.lines().nth(1).unwrap().split_whitespace().next().unwrap();
    assert_eq!(short.len(),13); // Other collision is outside the displayed page.
    let shown=json_ok(&root,&["instance","history","show",short]);
    assert!(shown["instance_id"]==id || shown["instance_id"]==other);
    let(code,_,_)=json_invoke(&root,["run","show",short].map(Into::into).to_vec());assert_eq!(code,1);
    assert!(before==fs::read(&database).unwrap());
    let mut command=parse_command(["instance","history","show",short].map(Into::into).to_vec()).unwrap();
    let root_arg=root.as_os_str().to_owned();
    short_ids::resolve(&mut command,Some(&root_arg),&ActionCancellation::default()).unwrap();
    let fixed=shown["instance_id"].as_str().unwrap();
    let mut third=fixed.to_string();third.replace_range(13..14,if &fixed[13..14]=="0"{"1"}else{"0"});
    let db=rusqlite::Connection::open(&database).unwrap();db.execute("INSERT INTO instance_history_identities(instance_id,instance_name) VALUES(?1,?2)",rusqlite::params![hex::decode(&third).unwrap(),b"later".as_slice()]).unwrap();drop(db);
    let mut out=Vec::new();
    assert_eq!(run_selected(command,Some(root_arg),&mut io::empty(),&mut out,&mut Vec::new(),&ActionCancellation::default(),presentation::Format::Json),0);
    let result:serde_json::Value=serde_json::from_slice(&out).unwrap();assert_eq!(result["result"]["instance_id"],fixed);
    let(code,_,_)=json_invoke(&root,["instance","history","show",short].map(OsString::from).to_vec());assert_eq!(code,1);
    let ids=json_ok(&root,&["instance","history","list","--limit","1"]);
    assert_eq!(ids["items"][0]["instance_id"].as_str().unwrap().len(),32);
    let full="ffffffffffffffffffffffffffffffff";
    assert_eq!(json_ok(&root,&["run","delete",full])["outcome"],"already_absent");
    let db=rusqlite::Connection::open(&database).unwrap();
    for n in 0..21 {db.execute("INSERT INTO instance_history_identities(instance_id,instance_name) VALUES(?1,?2)",rusqlite::params![hex::decode(format!("fffffffff{n:023x}")).unwrap(),b"bounded".as_slice()]).unwrap();}drop(db);
    let(code,ambiguous,_)=json_invoke(&root,["instance","history","show","fffffffff"].map(Into::into).to_vec());
    assert_eq!(code,1);assert_eq!(ambiguous["result"]["candidates"].as_array().unwrap().len(),20);assert_eq!(ambiguous["result"]["has_more"],true);
}

// Test-ID: PR-TEST-0589
// Verifies: PR-REQ-0369, PR-REQ-0370
#[test]
fn short_revision_selectors_preserve_machine_ids_and_exact_metadata_cas() {
    let (_temp,root,source)=cli_roots();let revision=json_install(&root,&source);
    let (package,digest)=revision.strip_prefix("exact:").unwrap().split_once('/').unwrap();
    let short=format!("exact:{}/sha256:{}",&package[..8],&digest[7..16]);
    let shown=json_ok(&root,&["revision","show",&short]);
    assert_eq!(shown["revision"]["package_id"],package);assert_eq!(shown["revision"]["content_digest"],digest);
    let named_short=format!("{}:{}",&package[..8],&digest[7..16]);
    json_ok(&root,&["revision","rename",&named_short,"prod"]);
    assert_eq!(json_ok(&root,&["revision","show",&format!("{package}:prod")])["revision"],shown["revision"]);
    json_ok(&root,&["revision","unname",&named_short]);
    json_ok(&root,&["revision","note","set",&short,"--value","note","--expect-absent"]);
    let path=source.join("pactrun.yaml");
    fs::write(&path,fs::read_to_string(&path).unwrap().replace(package,"00000000000000000000000000000022")).unwrap();
    let second=json_install(&root,&source);
    let (code,error,_)=json_invoke(&root,["revision","show",&short].map(OsString::from).to_vec());
    assert_eq!(code,1);assert_eq!(error["result"]["candidates"].as_array().unwrap().len(),2);
    assert!(error["result"]["candidates"].as_array().unwrap().contains(&serde_json::json!(second)));
    assert_eq!(json_ok(&root,&["revision","show",&revision])["revision"],shown["revision"]);
}
