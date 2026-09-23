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
    cases!(p::StorageUpgrade => "storage upgrade");
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
        "title":"Pactrun CLI JSON V1", "type":"object",
        "required":["format","command","status","result","error"],
        "properties":{
            "format":{"const":"pactrun.cli.v1"},
            "command":{"anyOf":[{"enum":commands},{"type":"null"}]},
            "status":{"enum":["success","failure"]},
            "result":{"type":["object","null"]},
            "delivery":delivery,
            "error":{"anyOf":[{"type":"null"},{"type":"object","required":["kind","message","reference"],"properties":{
                "kind":{"enum":["usage","operation","output","startup"]},"message":{"type":"string"},"reference":{"anyOf":[reference,{"type":"null"}]}
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
    for name in ["owner", "code"] {
        schema["$defs"]["ErrorReference"]["properties"][name] = json!({"type":"string","minLength":1,"maxLength":128,"pattern":"^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$"});
    }
    let record = &mut schema["$defs"]["Record"];
    record["properties"]["format"] = json!({"const":"pactrun.cli.events.v1"});
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
            "../../docs/spec/contracts/cli-json-v1.schema.json"
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
            if uri.as_str() == "https://pactrun.test/cli-json-v1.schema.json" {
                Ok(serde_json::from_str(include_str!(
                    "../../docs/spec/contracts/cli-json-v1.schema.json"
                ))?)
            } else {
                Err(format!("unexpected schema reference: {uri}").into())
            }
        }
    }
    static VALIDATOR: std::sync::OnceLock<jsonschema::Validator> = std::sync::OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        let schema: Value = serde_json::from_str(include_str!(
            "../../docs/spec/contracts/cli-events-v1.schema.json"
        ))
        .unwrap();
        jsonschema::options()
            .with_base_uri("https://pactrun.test/cli-events-v1.schema.json")
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
        path.join("cli-json-v1.schema.json"),
        serde_json::to_string_pretty(&generated).unwrap() + "\n",
    )
    .unwrap();
    let checked: Value = serde_json::from_str(include_str!(
        "../../docs/spec/contracts/cli-json-v1.schema.json"
    ))
    .unwrap();
    assert!(
        checked == generated,
        "schema drift: review target/cli-json-evidence/cli-json-v1.schema.json; no automatic contract update"
    );
    let validator = jsonschema::validator_for(&checked).unwrap();
    let success = json!({"format":"pactrun.cli.v1","command":"version","status":"success","result":{"product_version":"test"},"error":null});
    assert!(validator.is_valid(&success));
    let mut negative = success.clone();
    negative["format"] = "pactrun.cli.v2".into();
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
}
