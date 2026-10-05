//! Generate only a target/ candidate. Updating the checked-in contract is an
//! explicit reviewed patch, never an automatic test side effect.
use super::*;
use schemars::{JsonSchema, Schema, SchemaGenerator, generate::SchemaSettings};
use serde_json::{Value, json};

fn shape<T: JsonSchema>(generator: &mut SchemaGenerator) -> Value {
    serde_json::to_value(generator.subschema_for::<T>()).unwrap()
}

fn generated() -> Value {
    use capability_presentation::Presented;
    use catalog_presentation as c;
    use definitions::Related;
    use execution_presentation as e;
    use presentation as p;
    use transport_presentation as t;
    let mut generator = SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator();
    let mut cases: Vec<(&str, Value)> = Vec::new();
    macro_rules! cases {
        ($ty:ty => $($name:literal),+ $(,)?) => {{
            let schema = shape::<$ty>(&mut generator);
            $(cases.push(($name, schema.clone()));)+
        }};
    }
    cases!(p::Help<'static> => "help");
    cases!(p::Version<'static> => "version");
    cases!(p::Package => "pack generate-id");
    cases!(t::PackInstall => "pack install");
    cases!(t::PackExport => "revision export");
    let instance = shape::<p::Instance>(&mut generator);
    let created = shape::<t::CreatedRestore>(&mut generator);
    cases.push(("instance create", json!({"anyOf":[instance,created]})));
    cases!(Related<p::Instance> => "instance show");
    cases!(Related<definitions::Instances> => "instance list");
    cases!(p::StateVersion => "instance resolve-manual-recovery", "input set", "input delete");
    cases!(Related<p::Inputs> => "input list");
    cases!(p::InputExport => "input export");
    cases!(Presented<e::Actions> => "action list");
    cases!(Presented<e::Action> => "action show");
    let inspection = shape::<e::Inspection>(&mut generator);
    let plan = shape::<Presented<e::Plan>>(&mut generator);
    cases.push(("invoke", json!({"anyOf":[inspection,plan]})));
    cases!(Related<e::Inspection> => "run show");
    cases!(e::Reconciled => "run reconcile");
    cases!(p::Collection => "storage gc");
    cases!(p::Deletion => "run delete", "revision delete", "snapshot delete");
    cases!(p::ArtifactResult => "run artifact export", "run artifact delete");
    cases!(c::Page<c::RevisionEntry> => "revision list");
    cases!(Presented<c::RevisionEntry> => "revision show");
    cases!(c::RevisionEntry => "revision metadata show");
    cases!(c::Page<c::History> => "instance history list");
    cases!(c::Page<c::Retirement> => "instance deletion list");
    cases!(c::History => "instance history show");
    cases!(Related<c::Page<definitions::InspectedRun>> => "run list");
    cases!(c::Alias => "revision alias show");
    cases!(c::Local => "revision note show", "revision trust show");
    cases!(c::Mutation => "revision alias set", "revision alias clear", "revision note set", "revision note clear", "revision trust set", "revision trust clear");
    cases!(service_storage::Storages => "service-storage list");
    cases!(service_storage::Resources => "resource list");
    cases!(service_storage::Resource => "resource show");
    cases!(service_storage::Observation => "resource observe");
    cases!(service_storage::Location => "resource locate");
    let plan = shape::<Presented<retirements::RetirementPlan>>(&mut generator);
    for command in ["instance delete", "instance abandon"] {
        cases.push((command, json!({"anyOf":[inspection,plan]})));
    }
    cases!(retirements::RetirementInspection => "instance deletion show");
    cases!(retirements::Confirmation => "instance deletion confirm-complete");
    cases!(retirements::DetachedList => "service-storage detached list");
    cases!(retirements::DetachedInspection => "service-storage detached show");
    cases!(retirements::Discard => "service-storage detached discard");
    cases!(Related<t::Snapshots> => "snapshot list");
    cases!(Related<t::Snapshot> => "snapshot show");
    cases!(t::SnapshotVerified => "snapshot verify");
    cases!(t::SnapshotImported => "snapshot import");
    cases!(t::SnapshotExported => "snapshot export");
    let plan = shape::<Presented<t::SnapshotPlan>>(&mut generator);
    for command in ["snapshot capture", "snapshot restore"] {
        cases.push((command, json!({"anyOf":[inspection,plan]})));
    }
    cases!(Presented<migration_presentation::Paths> => "instance migration-paths");
    let plan = shape::<Presented<migration_presentation::Plan>>(&mut generator);
    let completed = shape::<migration_presentation::Completed>(&mut generator);
    cases.push(("instance migrate", json!({"anyOf":[plan,completed]})));
    let partial = shape::<p::PartialResult>(&mut generator);
    let reference = shape::<p::ErrorReference>(&mut generator);
    let acquisition = shape::<p::AcquisitionDiagnostic>(&mut generator);
    let deletion_obligation = shape::<p::DeletionObligationDiagnostic>(&mut generator);
    let delivery = shape::<streaming::DeliveryResult>(&mut generator);
    let commands: Vec<_> = cases.iter().map(|(name, _)| *name).collect();
    let branches: Vec<_> = cases
        .iter()
        .map(|(name, schema)| {
            json!({
                "if":{"properties":{"command":{"const":name},"status":{"const":"success"}}},
                "then":{"properties":{"result":schema}}
            })
        })
        .collect();
    let mut schema = json!({
        "$schema":"https://json-schema.org/draft/2020-12/schema",
        "title":"Pactrun CLI machine interface 1.0-alpha.1", "type":"object",
        "required":["format","format_version","command","status","result","error"],
        "properties":{
            "format":{"const":"pactrun.cli"},
            "format_version":{"const":"1.0-alpha.1"},
            "command":{"anyOf":[{"enum":commands},{"type":"null"}]},
            "status":{"enum":["success","failure"]},
            "result":{"type":["object","null"]},
            "delivery":delivery,
            "error":{"anyOf":[{"type":"null"},{"type":"object","required":["kind","message","reference"],"properties":{
                "kind":{"enum":["usage","operation","output","startup"]},"message":{"type":"string"},"reference":{"anyOf":[reference,{"type":"null"}]},"diagnostic":acquisition,"deletion_obligation":deletion_obligation,"retirement_reason":{"enum":["permission_denied","allocation_busy","unsupported_entry_kind"]}
            }}]}
        },
        "oneOf":[
            {"properties":{"status":{"const":"success"},"command":{"type":"string"},"result":{"type":"object"},"error":{"type":"null"}}},
            {"properties":{"status":{"const":"failure"},"error":{"type":"object"},"result":{"anyOf":[partial,completed,{"type":"null"}]}}}
        ],
        "allOf":branches,
        "$defs": generator.take_definitions(true)
    });
    // Error identities retain the owning Frozen lexical boundary.
    let acquisition = &mut schema["$defs"]["AcquisitionDiagnostic"]["properties"];
    acquisition["kind"] = json!({"const":"migration_input_acquisition"});
    acquisition["phase"] = json!({"enum":["open_source","stage_input"]});
    acquisition["reason"] =
        json!({"enum":["not_found","permission_denied","too_large","io_error","unavailable"]});
    acquisition["run_acceptance"] = json!({"const":"not_accepted"});
    schema["$defs"]["Failure"]["properties"]["reason"] = json!({"enum":["permission_denied","allocation_busy","unsupported_entry_kind","deletion_obligation"]});
    // Additive producer fact: older valid plans may omit it, never infer observed
    // state from that absence. New producers always emit the explicit value.
    for definition in schema["$defs"].as_object_mut().unwrap().values_mut() {
        if definition["properties"]
            .get("operator_input_acquisition")
            .is_some()
        {
            definition["properties"]["operator_input_acquisition"] =
                json!({"const":"not_performed"});
            definition["required"]
                .as_array_mut()
                .unwrap()
                .retain(|name| name != "operator_input_acquisition");
        }
    }
    for name in ["owner", "code"] {
        schema["$defs"]["ErrorReference"]["properties"][name] = json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$"});
    }
    let record = &mut schema["$defs"]["Record"];
    record["properties"]["format"] = json!({"const":"pactrun.cli"});
    record["properties"]["format_version"] = json!({"const":"1.0-alpha.1"});
    record["properties"]["sequence"] = json!({"type":"string","pattern":"^[1-9][0-9]*$"});
    for branch in record["oneOf"].as_array_mut().unwrap() {
        if branch["properties"]["type"]["const"] == "output" {
            branch["properties"]["encoding"] = json!({"const":"base64"});
            branch["properties"]["offset"] = json!({"type":"string","pattern":"^(0|[1-9][0-9]*)$"});
            branch["properties"]["data"] = json!({"type":"string","maxLength":87384,"pattern":"^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"});
        }
    }
    // Ensure a generated schema is itself a valid schema object.
    let _: Schema = serde_json::from_value(schema.clone()).unwrap();
    schema
}

pub(super) fn assert_response(value: &Value) {
    static VALIDATOR: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        let schema: Value = serde_json::from_str(include_str!(
            "../../docs/spec/interfaces/cli-machine.schema.json"
        ))
        .unwrap();
        jsonschema::validator_for(&schema).unwrap()
    });
    if let Err(error) = validator.validate(value) {
        panic!("CLI response does not satisfy V1: {error}; response: {value}");
    }
}

pub(super) fn assert_event(value: &Value) {
    struct Contracts;
    impl jsonschema::Retrieve for Contracts {
        fn retrieve(
            &self,
            uri: &jsonschema::Uri<String>,
        ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
            if uri.as_str() == "https://pactrun.test/cli-machine.schema.json" {
                Ok(serde_json::from_str(include_str!(
                    "../../docs/spec/interfaces/cli-machine.schema.json"
                ))?)
            } else {
                Err(format!("unexpected schema reference: {uri}").into())
            }
        }
    }
    static VALIDATOR: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        let schema: Value = serde_json::from_str(include_str!(
            "../../docs/spec/interfaces/cli-events.schema.json"
        ))
        .unwrap();
        jsonschema::options()
            .with_base_uri("https://pactrun.test/cli-events.schema.json")
            .with_retriever(Contracts)
            .build(&schema)
            .unwrap()
    });
    if let Err(error) = validator.validate(value) {
        panic!("invalid event: {error}; {value}");
    }
}

// Test-ID: PR-TEST-0556
// Verifies: PR-REQ-0083, PR-REQ-0120, PR-REQ-0359
#[test]
fn cli_json_schema_contract_matches_explicit_projections() {
    let generated = generated();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/cli-json-evidence");
    fs::create_dir_all(&path).unwrap();
    fs::write(
        path.join("cli-machine.schema.json"),
        serde_json::to_string_pretty(&generated).unwrap() + "\n",
    )
    .unwrap();
    let checked: Value = serde_json::from_str(include_str!(
        "../../docs/spec/interfaces/cli-machine.schema.json"
    ))
    .unwrap();
    assert!(
        checked == generated,
        "schema drift: review target/cli-json-evidence/cli-machine.schema.json; no automatic contract update"
    );
    let validator = jsonschema::validator_for(&checked).unwrap();
    let success = json!({"format":"pactrun.cli","format_version":"1.0-alpha.1","command":"version","status":"success","result":{"product_version":"test","build_target":"test","rustc":"test","source_commit":null,"source_manifest_sha256":null,"supported_formats":{},"default_formats":{}},"error":null});
    assert!(validator.is_valid(&success));
    let mut missing = success.clone();
    missing.as_object_mut().unwrap().remove("format_version");
    assert!(!validator.is_valid(&missing));
    let mut numeric = success.clone();
    numeric["format_version"] = 1.into();
    assert!(!validator.is_valid(&numeric));
    let mut legacy = success.clone();
    legacy["format"] = "pactrun.cli.v1".into();
    assert!(!validator.is_valid(&legacy));
    let mut negative = success.clone();
    negative["format_version"] = "1.1".into();
    assert!(!validator.is_valid(&negative));
    let mut negative = success.clone();
    negative["result"] = json!({"product_version":42});
    assert!(!validator.is_valid(&negative));
    let mut negative = success.clone();
    negative["status"] = "failure".into();
    assert!(!validator.is_valid(&negative));
    let mut additive = success;
    additive["future_field"] = true.into();
    assert!(validator.is_valid(&additive));
    let mut acquisition = json!({"format":"pactrun.cli","format_version":"1.0-alpha.1","command":"instance migrate","status":"failure","result":null,"error":{"kind":"operation","message":"safe explanation","reference":null,"diagnostic":{"kind":"migration_input_acquisition","phase":"open_source","instance_id":"00000000000000000000000000000001","target_revision":{"package_id":"00000000000000000000000000000002","content_digest":format!("sha256:{}","0".repeat(64))},"input_id":"credentials","reason":"permission_denied","run_acceptance":"not_accepted"}}});
    assert!(validator.is_valid(&acquisition));
    let mut previous = checked.clone();
    previous["properties"]["error"]["anyOf"][1]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("diagnostic");
    for definition in previous["$defs"].as_object_mut().unwrap().values_mut() {
        if let Some(properties) = definition
            .get_mut("properties")
            .and_then(Value::as_object_mut)
        {
            properties.remove("operator_input_acquisition");
        }
    }
    assert!(
        jsonschema::validator_for(&previous)
            .unwrap()
            .is_valid(&acquisition),
        "the pre-extension open error projection must accept new diagnostic members"
    );
    acquisition["error"]["diagnostic"]["reason"] = "unreviewed_reason".into();
    assert!(!validator.is_valid(&acquisition));
    acquisition["error"]
        .as_object_mut()
        .unwrap()
        .remove("diagnostic");
    assert!(
        validator.is_valid(&acquisition),
        "older failure responses remain valid"
    );
}

// Test-ID: PR-TEST-0655
// Verifies: PR-REQ-0359
#[test]
fn retirement_extensions_are_optional_members_not_existing_enum_changes() {
    let current = generated();
    let validator = jsonschema::validator_for(&current).unwrap();
    let mut previous = current.clone();
    let error_properties = previous["properties"]["error"]["anyOf"][1]["properties"]
        .as_object_mut()
        .unwrap();
    error_properties.remove("deletion_obligation");
    error_properties.remove("retirement_reason");
    previous["$defs"]["Failure"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("reason");
    let previous_validator = jsonschema::validator_for(&previous).unwrap();
    let mut response = json!({"format":"pactrun.cli","format_version":"1.0-alpha.1","command":"invoke","status":"failure","result":{"run_id":"00000000000000000000000000000001"},"error":{"kind":"operation","message":"safe guidance","reference":{"owner":"admission","code":"plan_invalidated"},"deletion_obligation":{"instance_id":"00000000000000000000000000000002","run_id":"00000000000000000000000000000001"}}});
    assert!(validator.is_valid(&response));
    assert!(previous_validator.is_valid(&response));
    response["error"]
        .as_object_mut()
        .unwrap()
        .remove("deletion_obligation");
    assert!(validator.is_valid(&response));
    response["command"] = "service-storage detached discard".into();
    response["result"] = Value::Null;
    response["error"]["retirement_reason"] = "unsupported_entry_kind".into();
    assert!(validator.is_valid(&response));
    assert!(previous_validator.is_valid(&response));
    response["error"]["retirement_reason"] = "unreviewed_reason".into();
    assert!(!validator.is_valid(&response));
    let failure_schema = json!({"$ref":"#/$defs/Failure","$defs":current["$defs"]});
    let old_failure_schema = json!({"$ref":"#/$defs/Failure","$defs":previous["$defs"]});
    let validator = jsonschema::validator_for(&failure_schema).unwrap();
    let old_validator = jsonschema::validator_for(&old_failure_schema).unwrap();
    let mut failure = json!({"reference":{"owner":"service_storage","code":"allocation_unavailable"},"explanation":"safe explanation","detail":"safe detail","step":"finalize_storage","reason":"unsupported_entry_kind"});
    assert!(validator.is_valid(&failure));
    assert!(old_validator.is_valid(&failure));
    failure["reason"] = "unreviewed_reason".into();
    assert!(!validator.is_valid(&failure));
    failure.as_object_mut().unwrap().remove("reason");
    assert!(validator.is_valid(&failure));
}
