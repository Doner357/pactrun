use super::*;
use crate::domain::{InputDeclarationV1, InputIdentity, InputProtectionV1, SnapshotId};
use serde::Deserialize;
use std::{io::Cursor, path::Path, process::Command};

const V1: &str = include_str!("../tests/vectors/snapshot_integrity/content.json");
const V2: &str = include_str!("../tests/vectors/snapshot_integrity/protection.json");

#[derive(Deserialize)]
struct Corpus {
    format: String,
    status: String,
    valid: Vec<Fixture>,
    invalid: Vec<Fixture>,
    rfc8785: Vec<JcsFixture>,
}
#[derive(Deserialize)]
struct JcsFixture {
    input: Value,
    expected: String,
}
#[derive(Clone, Deserialize)]
struct Fixture {
    name: String,
    raw_manifest: String,
    #[serde(default)]
    normalized_manifest: Value,
    #[serde(default)]
    producer_context: Option<ProducerFixture>,
    #[serde(default)]
    blob_contents: BTreeMap<String, String>,
    #[serde(default)]
    expected: Option<Expected>,
    #[serde(default)]
    expected_error: Option<String>,
    #[serde(default)]
    relational: Option<RelationFixture>,
}
#[derive(Clone, Deserialize)]
struct Expected {
    manifest_jcs_hex: String,
    frame_hex: String,
    digest: String,
}
#[derive(Clone, Deserialize)]
struct RelationFixture {
    status: String,
}
#[derive(Clone, Deserialize)]
struct ProducerFixture {
    package_id: String,
    revision_content_digest: String,
    inputs: Vec<InputFixture>,
}
#[derive(Clone, Deserialize)]
struct InputFixture {
    id: String,
    protection: String,
}

fn decode_fixture(
    version: SnapshotIntegrityVersion,
    fixture: &Fixture,
) -> Result<VerifiedSnapshotManifest, SnapshotCodecError> {
    let producer = fixture.producer_context.as_ref().map(|p| {
        let identity = RevisionIdentity::new(
            p.package_id.parse().unwrap(),
            p.revision_content_digest.parse().unwrap(),
        );
        let inputs: Vec<_> = p
            .inputs
            .iter()
            .map(|input| InputDeclarationV1 {
                id: InputIdentity::parse(&input.id).unwrap(),
                required: false,
                protection: match input.protection.as_str() {
                    "normal" => InputProtectionV1::Normal,
                    "secret" => InputProtectionV1::Secret,
                    _ => panic!("bad producer fixture"),
                },
            })
            .collect();
        (identity, inputs)
    });
    let context = producer
        .as_ref()
        .map(|(revision, inputs)| SnapshotProducerContext { revision, inputs });
    decode_snapshot_manifest(version, fixture.raw_manifest.as_bytes(), context.as_ref())
}

fn verify_fixture(
    version: SnapshotIntegrityVersion,
    fixture: &Fixture,
) -> Result<VerifiedSnapshotManifest, SnapshotCodecError> {
    let decoded = decode_fixture(version, fixture)?;
    decoded.verify_content(|digest| {
        fixture
            .blob_contents
            .get(digest.as_str())
            .map(|bytes| Cursor::new(hex::decode(bytes).expect("public fixture hex")))
            .ok_or(SnapshotBlobReadError::Missing)
    })?;
    Ok(decoded)
}

fn assert_corpus(version: SnapshotIntegrityVersion, text: &str) {
    let corpus: Corpus = serde_json::from_str(text).unwrap();
    assert_eq!(corpus.format, "snapshot_integrity_baseline_vectors");
    assert_eq!(corpus.status, "baseline");
    let mut names = BTreeSet::new();
    for fixture in &corpus.valid {
        assert!(names.insert(&fixture.name));
        let decoded =
            verify_fixture(version, fixture).unwrap_or_else(|e| panic!("{}: {e}", fixture.name));
        let expected = fixture.expected.as_ref().unwrap();
        assert_eq!(
            hex::encode(decoded.canonical_bytes()),
            expected.manifest_jcs_hex,
            "{}",
            fixture.name
        );
        assert_eq!(
            decoded.integrity_digest().as_str(),
            expected.digest,
            "{}",
            fixture.name
        );
        let normalized: Value = serde_json::from_slice(decoded.canonical_bytes()).unwrap();
        assert_eq!(normalized, fixture.normalized_manifest, "{}", fixture.name);
        let mut frame = Vec::from(&b"pactrun.snapshot-integrity-digest\0"[..]);
        frame.extend(b"snapshot-integrity-manifest\0");
        frame.extend((decoded.canonical_bytes().len() as u64).to_be_bytes());
        frame.extend(decoded.canonical_bytes());
        assert_eq!(hex::encode(frame), expected.frame_hex, "{}", fixture.name);
        assert_eq!(
            decoded.relational_verification(),
            match fixture.relational.as_ref().unwrap().status.as_str() {
                "valid" => SnapshotRelationalVerification::Valid,
                "not_evaluated" => SnapshotRelationalVerification::NotEvaluated,
                _ => panic!("bad relational fixture"),
            }
        );
        let again = decode_snapshot_manifest(version, decoded.canonical_bytes(), None).unwrap();
        assert_eq!(again.canonical_bytes(), decoded.canonical_bytes());
        assert_eq!(again.integrity_digest(), decoded.integrity_digest());
        assert_eq!(again.manifest(), decoded.manifest());
    }
    for fixture in &corpus.invalid {
        assert!(names.insert(&fixture.name));
        let error = verify_fixture(version, fixture).unwrap_err();
        let code = match error {
            SnapshotCodecError::Validation(value) => value.code(),
            SnapshotCodecError::UnsupportedVersion(required) => {
                assert!(!crate::domain::VersionDomain::Snapshot.supports(required));
                "invalid_format_version"
            }
            _ => panic!("{}: wrong failure class {error}", fixture.name),
        };
        assert_eq!(
            code,
            fixture.expected_error.as_ref().unwrap(),
            "{}",
            fixture.name
        );
    }
    for fixture in &corpus.rfc8785 {
        assert_eq!(
            serde_jcs::to_string(&fixture.input).unwrap(),
            fixture.expected
        );
    }
}

// Test-ID: PR-TEST-0184
// Verifies: PR-REQ-0149, PR-REQ-0297
#[test]
fn production_v1_preserves_every_frozen_golden_and_negative_code() {
    assert_corpus(SnapshotIntegrityVersion::BASELINE, V1);
}

// Test-ID: PR-TEST-0185
// Verifies: PR-REQ-0149, PR-REQ-0295, PR-REQ-0296, PR-REQ-0297
#[test]
fn production_v2_matches_independent_goldens_and_negative_contracts() {
    assert_corpus(SnapshotIntegrityVersion::BASELINE, V2);
}

fn fixture(name: &str) -> Fixture {
    serde_json::from_str::<Corpus>(V2)
        .unwrap()
        .valid
        .into_iter()
        .find(|v| v.name == name)
        .unwrap()
}

// Test-ID: PR-TEST-0186
// Verifies: PR-REQ-0296, PR-REQ-0297
#[test]
fn v2_relaxes_only_bound_active_normal_protection_and_never_upgrades_v1() {
    for version in [SnapshotIntegrityVersion::BASELINE] {
        for state in ["bound", "absent"] {
            for declared in ["normal", "secret"] {
                for recorded in ["normal", "secret"] {
                    let mut f = fixture("empty_bound_is_not_absence");
                    let m = &mut f.normalized_manifest;
                    m["format_version"] = json!(version.as_str());
                    m["managed_bindings"][0]["state"] = json!(state);
                    m["managed_bindings"][0]["protection"] = json!(recorded);
                    if state == "absent" {
                        m["managed_bindings"][0]
                            .as_object_mut()
                            .unwrap()
                            .remove("blob_digest");
                    }
                    f.producer_context.as_mut().unwrap().inputs[0].protection = declared.to_owned();
                    f.raw_manifest = serde_json::to_string(m).unwrap();
                    let result = decode_fixture(version, &f);
                    let allowed = declared == recorded
                        || (version == SnapshotIntegrityVersion::BASELINE
                            && state == "bound"
                            && declared == "normal"
                            && recorded == "secret");
                    if allowed {
                        assert_eq!(
                            result.unwrap().relational_verification(),
                            SnapshotRelationalVerification::Valid
                        );
                    } else {
                        assert_eq!(
                            result.unwrap_err(),
                            SnapshotCodecError::Validation(
                                SnapshotValidationError::InvalidBindingProtection
                            )
                        );
                    }
                    // Without producer semantics this is not automatically relationally valid.
                    f.producer_context = None;
                    assert_eq!(
                        decode_fixture(version, &f)
                            .unwrap()
                            .relational_verification(),
                        SnapshotRelationalVerification::NotEvaluated
                    );
                }
            }
        }
    }
    assert_eq!(
        SnapshotIntegrityVersion::current_writer(),
        SnapshotIntegrityVersion::BASELINE
    );
    for unsupported in [-1, 0, 3, i64::MAX] {
        assert_eq!(
            SnapshotIntegrityVersion::from_text(&unsupported.to_string()),
            Err(SnapshotValidationError::InvalidFormatVersion)
        );
    }
    let f = fixture("active_normal_sticky_secret");
    assert!(decode_fixture(SnapshotIntegrityVersion::BASELINE, &f).is_ok());
    for old in ["1", "2"] {
        let raw = f.raw_manifest.replace(
            "\"format_version\":\"1.0-alpha.1\"",
            &format!("\"format_version\":{old}"),
        );
        assert!(
            decode_snapshot_manifest(SnapshotIntegrityVersion::BASELINE, raw.as_bytes(), None)
                .is_err()
        );
    }
}

// Test-ID: PR-TEST-0187
// Verifies: PR-REQ-0295, PR-REQ-0297
#[test]
fn content_verification_reads_exact_referenced_bytes_once_and_is_not_manifest_only() {
    let f = fixture("shared_logical_service_blob");
    let manifest = decode_fixture(SnapshotIntegrityVersion::BASELINE, &f).unwrap();
    let mut opens = 0;
    let verified = manifest
        .verify_content(|digest| {
            opens += 1;
            Ok(ShortReads {
                cursor: Cursor::new(hex::decode(&f.blob_contents[digest.as_str()]).unwrap()),
                interrupt_once: true,
            })
        })
        .unwrap();
    assert_eq!(opens, 1);
    assert_eq!(verified.lengths().len(), 1);
    assert_eq!(
        manifest
            .verify_content::<Cursor<Vec<u8>>>(|_| Err(SnapshotBlobReadError::Missing))
            .unwrap_err(),
        SnapshotCodecError::Validation(SnapshotValidationError::MissingContentBlob)
    );
    assert_eq!(
        manifest
            .verify_content(|_| Ok(Cursor::new(b"changed")))
            .unwrap_err(),
        SnapshotCodecError::Validation(SnapshotValidationError::ContentDigestMismatch)
    );
    assert_eq!(
        manifest
            .verify_content::<Cursor<Vec<u8>>>(|_| Err(SnapshotBlobReadError::Io(
                io::ErrorKind::PermissionDenied
            )))
            .unwrap_err(),
        SnapshotCodecError::ContentIo(io::ErrorKind::PermissionDenied)
    );
    let error = manifest.verify_content(|_| Ok(FailingRead)).unwrap_err();
    assert_eq!(error, SnapshotCodecError::ContentIo(io::ErrorKind::Other));
    assert!(!format!("{error:?} {error}").contains("secret-host-path"));
    let empty = fixture("empty_bound_is_not_absence");
    let empty_manifest = decode_fixture(SnapshotIntegrityVersion::BASELINE, &empty).unwrap();
    assert!(
        empty_manifest
            .verify_content::<Cursor<Vec<u8>>>(|_| Err(SnapshotBlobReadError::Missing))
            .is_err()
    );
    assert_eq!(
        *empty_manifest
            .verify_content(|_| Ok(Cursor::new(Vec::<u8>::new())))
            .unwrap()
            .lengths()
            .values()
            .next()
            .unwrap(),
        0
    );
    let minimal = decode_fixture(
        SnapshotIntegrityVersion::BASELINE,
        &fixture("v2_minimal_manifest"),
    )
    .unwrap();
    assert!(
        minimal
            .verify_content::<Cursor<Vec<u8>>>(|_| panic!("must not browse unrelated content"))
            .unwrap()
            .lengths()
            .is_empty()
    );
}

struct ShortReads {
    cursor: Cursor<Vec<u8>>,
    interrupt_once: bool,
}
impl Read for ShortReads {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.interrupt_once {
            self.interrupt_once = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        let maximum = buffer.len().min(7);
        self.cursor.read(&mut buffer[..maximum])
    }
}
struct FailingRead;
impl Read for FailingRead {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("secret-host-path"))
    }
}

// Test-ID: PR-TEST-0188
// Verifies: PR-REQ-0293, PR-REQ-0295
#[test]
fn parser_limits_are_capabilities_while_exact_numbers_and_unicode_remain_format_rules() {
    let over = vec![b' '; SnapshotCapability::RawManifest.maximum().unwrap() as usize + 1];
    assert_eq!(
        decode_snapshot_manifest(SnapshotIntegrityVersion::BASELINE, &over, None).unwrap_err(),
        SnapshotCodecError::Capability(CapabilityRefusal {
            capability: SnapshotCapability::RawManifest
        })
    );
    let deep = format!("{}0{}", "[".repeat(17), "]".repeat(17));
    assert_eq!(
        decode_snapshot_manifest(SnapshotIntegrityVersion::BASELINE, deep.as_bytes(), None)
            .unwrap_err(),
        SnapshotCodecError::Capability(CapabilityRefusal {
            capability: SnapshotCapability::JsonDepth
        })
    );
    assert!(check_json_depth(format!("{}0{}", "[".repeat(16), "]".repeat(16)).as_bytes()).is_ok());
    let mut f = fixture("v2_minimal_manifest");
    f.normalized_manifest["captured_at"]["unix_seconds"] = json!(0);
    let raw = serde_json::to_string(&f.normalized_manifest).unwrap();
    for (token, expected) in [
        ("-0.0", Some(0)),
        ("1e0", Some(1)),
        ("0e400", Some(0)),
        ("9007199254740991.0", Some(9_007_199_254_740_991)),
        ("1.00000000000000001", None),
        ("9007199254740991.1", None),
        ("1e-9223372036854775808", None),
    ] {
        let bytes = raw.replace("\"unix_seconds\":0", &format!("\"unix_seconds\":{token}"));
        let result =
            decode_snapshot_manifest(SnapshotIntegrityVersion::BASELINE, bytes.as_bytes(), None);
        if let Some(expected) = expected {
            assert_eq!(
                result.unwrap().manifest().captured_at().unix_seconds(),
                expected
            );
        } else {
            assert_eq!(
                result.unwrap_err(),
                SnapshotCodecError::Validation(SnapshotValidationError::InvalidNumber)
            );
        }
    }
    assert_eq!(
        decode_snapshot_manifest(SnapshotIntegrityVersion::BASELINE, &[0xff], None).unwrap_err(),
        SnapshotCodecError::Validation(SnapshotValidationError::InvalidUnicodeScalar)
    );
    f.normalized_manifest["unknown"] = json!("[".repeat(100));
    assert_eq!(
        decode_snapshot_manifest(
            SnapshotIntegrityVersion::BASELINE,
            serde_json::to_string(&f.normalized_manifest)
                .unwrap()
                .as_bytes(),
            None
        )
        .unwrap_err(),
        SnapshotCodecError::Validation(SnapshotValidationError::UnknownField)
    );
}

// Test-ID: PR-TEST-0189
// Verifies: PR-REQ-0295, PR-REQ-0296
#[test]
fn typed_manifest_constructor_normalizes_sets_and_debug_projections_withhold_digests() {
    let decoded = decode_fixture(
        SnapshotIntegrityVersion::BASELINE,
        &fixture("active_normal_sticky_secret"),
    )
    .unwrap();
    let manifest = decoded.manifest();
    let parts = || SnapshotManifestParts {
        version: manifest.version(),
        snapshot_id: manifest.snapshot_id(),
        producer: manifest.producer().clone(),
        origin_instance_id: manifest.origin_instance_id(),
        captured_at: manifest.captured_at(),
        managed_bindings: manifest.managed_bindings().iter().cloned().rev().collect(),
        service_content: manifest.service_content().iter().cloned().rev().collect(),
    };
    let reordered =
        encode_snapshot_manifest(SnapshotManifest::new(parts()).unwrap(), None).unwrap();
    assert_eq!(reordered.canonical_bytes(), decoded.canonical_bytes());
    let mut duplicate = parts();
    duplicate
        .managed_bindings
        .push(duplicate.managed_bindings[0].clone());
    assert_eq!(
        SnapshotManifest::new(duplicate).unwrap_err(),
        SnapshotValidationError::DuplicateSemanticKey
    );
    let mut duplicate = parts();
    duplicate
        .service_content
        .push(duplicate.service_content[0].clone());
    assert_eq!(
        SnapshotManifest::new(duplicate).unwrap_err(),
        SnapshotValidationError::DuplicateSemanticKey
    );
    let mut different_id = parts();
    different_id.snapshot_id = SnapshotId::from_bytes([42; 16]);
    assert_ne!(
        encode_snapshot_manifest(SnapshotManifest::new(different_id).unwrap(), None)
            .unwrap()
            .integrity_digest(),
        decoded.integrity_digest()
    );
    assert!(!format!("{decoded:?} {:?}", decoded.integrity_digest()).contains("sha256:"));
    assert!(SnapshotTimestamp::new(0, -1).is_err());
    assert!(SnapshotTimestamp::new(i64::MAX, 0).is_err());
}

// Test-ID: PR-TEST-0191
// Verifies: PR-REQ-0297
#[test]
fn independent_node_oracle_matches_v2_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("node")
        .arg(root.join("tests/oracles/snapshot_integrity_protection.mjs"))
        .arg(root.join("tests/vectors/snapshot_integrity/protection.json"))
        .output()
        .expect("Node 24 is required for conformance");
    assert!(
        output.status.success(),
        "Node oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// Test-ID: PR-TEST-0192
// Verifies: PR-REQ-0296
#[test]
fn required_absence_is_format_representable_not_capture_admission_permission() {
    let f = fixture("active_normal_absent");
    let decoded = decode_fixture(SnapshotIntegrityVersion::BASELINE, &f).unwrap();
    let inputs = [InputDeclarationV1 {
        id: InputIdentity::parse("config").unwrap(),
        required: true,
        protection: InputProtectionV1::Normal,
    }];
    let context = SnapshotProducerContext {
        revision: decoded.manifest().producer(),
        inputs: &inputs,
    };
    assert_eq!(
        decoded
            .manifest()
            .validate_producer(Some(&context))
            .unwrap(),
        SnapshotRelationalVerification::Valid,
    );
    assert!(matches!(
        decoded.manifest().managed_bindings()[0].state,
        SnapshotBindingState::Absent,
    ));
    // Capture's independent RequiredInputsSatisfied check belongs to S4/S5.
    // No execution or admission claim is made by a format verifier.
}

// Test-ID: PR-TEST-0193
// Verifies: PR-REQ-0297
#[test]
fn specification_and_corpora_select_the_supported_integrity_format() {
    let specification = include_str!("../docs/spec/snapshots/integrity.md");
    let version = SnapshotIntegrityVersion::BASELINE.as_str();
    let marker = format!("`format_version: \"{version}\"`");
    assert!(specification.contains(&marker));
    for source in [V1, V2] {
        let corpus: Corpus = serde_json::from_str(source).unwrap();
        assert_eq!(corpus.status, "baseline");
        for fixture in corpus.valid {
            let manifest: Value = serde_json::from_str(&fixture.raw_manifest).unwrap();
            assert_eq!(manifest["format_version"].as_str(), Some(version));
        }
    }
}

// Test-ID: PR-TEST-0194
// Verifies: PR-REQ-0295
#[test]
fn bounded_hostile_mutations_never_panic_and_accepted_values_remain_canonical() {
    for name in ["v2_minimal_manifest", "active_normal_sticky_secret"] {
        let original = fixture(name).raw_manifest.into_bytes();
        for iteration in 0..512 {
            let mut mutated = original.clone();
            let position = (iteration * 7919 + 23) % mutated.len();
            mutated[position] = [0, b'"', b'[', b'\\', 0xff, b'0', b'9'][iteration % 7];
            let result = std::panic::catch_unwind(|| {
                decode_snapshot_manifest(SnapshotIntegrityVersion::BASELINE, &mutated, None)
            });
            assert!(result.is_ok(), "decoder panicked for bounded mutation");
            if let Ok(decoded) = result.unwrap() {
                let again = decode_snapshot_manifest(
                    SnapshotIntegrityVersion::BASELINE,
                    decoded.canonical_bytes(),
                    None,
                )
                .unwrap();
                assert_eq!(again.canonical_bytes(), decoded.canonical_bytes());
                assert_eq!(again.integrity_digest(), decoded.integrity_digest());
            }
        }
    }
}

// Test-ID: PR-TEST-0633
// Verifies: PR-REQ-0331
#[test]
fn snapshot_refusal_preserves_the_required_contract_before_decoding_future_content() {
    let error = decode_snapshot_manifest(
        SnapshotIntegrityVersion::BASELINE,
        br#"{"format_version":"1.0-alpha.2","future_body":{}}"#,
        None,
    )
    .unwrap_err();
    assert!(matches!(error, SnapshotCodecError::UnsupportedVersion(_)));
    for part in [
        "snapshot_integrity",
        "1.0-alpha.2",
        "1.0-alpha.1",
        "supporting release",
    ] {
        assert!(error.to_string().contains(part));
    }
}
