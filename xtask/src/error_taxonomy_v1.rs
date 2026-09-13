use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const CATALOG_PATH: &str = "tests/vectors/error_taxonomy_v1/catalog.json";
const FIXTURE_PATH: &str = "tests/vectors/error_taxonomy_v1/vectors.json";
const CATALOG_FORMAT: &str = "PactrunErrorTaxonomyCatalogV1";
const FIXTURE_FORMAT: &str = "PactrunErrorTaxonomyV1Fixtures";

const CATEGORIES: &[&str] = &[
    "admission",
    "compilation",
    "execution",
    "intrinsic_semantic_validation",
    "persistence",
    "protocol",
    "recovery",
    "relational_semantic_validation",
    "representation_validation",
    "resolution",
];

const INITIAL_OWNERS: &[&str] = &[
    "admission",
    "compilation",
    "domain_validation",
    "execution",
    "hook_protocol_v1",
    "persistence",
    "recovery",
    "resolution",
    "revision_core_format_v1",
    "snapshot_integrity_format_v1",
];

/// Initial owners that still have no appended code. `execution` left this set
/// when M3 Slice 4 appended its runtime codes under PR-REQ-0222.
const EMPTY_INITIAL_OWNERS: &[&str] = &[
    "compilation",
    "domain_validation",
    "persistence",
    "recovery",
];

/// PR-REQ-0221: no owner receives a placeholder code.
const PLACEHOLDER_CODES: &[&str] = &["failed", "internal_failure", "unknown_failure"];

const EXCLUDED_REVISION_CORE_VOCABULARY: &[&str] = &[
    "duplicate_semantic_key",
    "invalid_digest",
    "invalid_identifier",
    "invalid_json_syntax",
    "invalid_reference",
    "invalid_runtime_path",
    "invalid_snapshot",
    "invalid_transition",
    "invalid_type",
    "missing_field",
    "unsupported_format_version",
];

const EXCLUDED_SNAPSHOT_VOCABULARY: &[&str] = &[
    "invalid_blob_fixture",
    "invalid_identifier",
    "invalid_json_syntax",
    "invalid_producer_context",
    "invalid_type",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    format: String,
    status: String,
    owners: Vec<OwnerEntry>,
    codes: Vec<CodeEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerEntry {
    owner: String,
    normative_sources: Vec<NormativeSource>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeEntry {
    owner: String,
    code: String,
    category: String,
    normative_sources: Vec<NormativeSource>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum NormativeSource {
    Requirement { id: String },
    FrozenVector { suite: String, name: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
struct ErrorRef {
    owner: String,
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixtures {
    format: String,
    valid_references: Vec<ValidReferenceFixture>,
    invalid_references: Vec<InvalidReferenceFixture>,
    invalid_catalogs: Vec<InvalidCatalogFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidReferenceFixture {
    name: String,
    input: ErrorRef,
    expected: ErrorRef,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidReferenceFixture {
    name: String,
    input: ErrorRef,
    expected_violation: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidCatalogFixture {
    name: String,
    raw_json: String,
    expected_violation: String,
}

#[derive(Debug)]
struct Violation {
    code: &'static str,
    detail: String,
}

impl Violation {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

#[derive(Debug)]
struct SourceIndex {
    requirements: BTreeSet<String>,
    suites: BTreeMap<String, BTreeMap<String, String>>,
}

pub fn verify(workspace_root: &Path) -> Result<(), String> {
    let source_index = build_source_index(workspace_root)?;
    let catalog = load_catalog(workspace_root)?;
    let fixtures = load_fixtures(workspace_root)?;

    validate_catalog(&catalog, &source_index).map_err(format_violation)?;
    verify_initial_registry(&catalog, &source_index).map_err(format_violation)?;
    verify_reference_fixtures(&fixtures).map_err(format_violation)?;
    verify_invalid_catalog_fixtures(&fixtures, &source_index).map_err(format_violation)?;
    verify_append_only_examples(&catalog, &source_index).map_err(format_violation)?;
    verify_semantic_boundaries(&catalog).map_err(format_violation)?;
    super::revision_core_v1::verify_traceability(workspace_root)?;

    eprintln!(
        "PactrunErrorTaxonomyV1: {} owners, {} registered codes, {} reference fixtures, and {} single-fault catalog fixtures passed Rust and traceability",
        catalog.owners.len(),
        catalog.codes.len(),
        fixtures.valid_references.len() + fixtures.invalid_references.len(),
        fixtures.invalid_catalogs.len()
    );
    Ok(())
}

fn load_catalog(workspace_root: &Path) -> Result<Catalog, String> {
    let path = workspace_root.join(CATALOG_PATH);
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    parse_catalog(&raw).map_err(|error| {
        format!(
            "could not parse taxonomy catalog {}: {}: {}",
            path.display(),
            error.code,
            error.detail
        )
    })
}

fn load_fixtures(workspace_root: &Path) -> Result<Fixtures, String> {
    let path = workspace_root.join(FIXTURE_PATH);
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let fixtures: Fixtures = serde_json::from_str(&raw)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    if fixtures.format != FIXTURE_FORMAT {
        return Err(format!(
            "unexpected taxonomy fixture format {}",
            fixtures.format
        ));
    }
    Ok(fixtures)
}

fn parse_catalog(raw: &str) -> Result<Catalog, Violation> {
    serde_json::from_str(raw).map_err(|error| {
        let message = error.to_string();
        let code = if message.contains("duplicate field") {
            "duplicate_property"
        } else if message.contains("unknown field") {
            "unknown_field"
        } else if message.contains("invalid type") {
            "invalid_type"
        } else {
            "invalid_catalog_json"
        };
        Violation::new(code, message)
    })
}

fn validate_catalog(catalog: &Catalog, sources: &SourceIndex) -> Result<(), Violation> {
    if catalog.format != CATALOG_FORMAT {
        return Err(Violation::new(
            "invalid_catalog_format",
            format!("unexpected catalog format {}", catalog.format),
        ));
    }
    if !matches!(catalog.status.as_str(), "candidate" | "frozen") {
        return Err(Violation::new(
            "invalid_catalog_status",
            "catalog status must be candidate or frozen",
        ));
    }

    let mut owners = BTreeSet::new();
    for entry in &catalog.owners {
        validate_identifier(&entry.owner, "invalid_error_owner")?;
        if !owners.insert(entry.owner.clone()) {
            return Err(Violation::new(
                "duplicate_owner",
                format!("duplicate owner {}", entry.owner),
            ));
        }
        validate_sources(&entry.normative_sources, None, sources)?;
    }

    let mut codes = BTreeSet::new();
    for entry in &catalog.codes {
        validate_identifier(&entry.owner, "invalid_error_owner")?;
        validate_identifier(&entry.code, "invalid_error_code")?;
        if !owners.contains(&entry.owner) {
            return Err(Violation::new(
                "unregistered_owner",
                format!(
                    "catalog code {}.{} refers to an unregistered owner",
                    entry.owner, entry.code
                ),
            ));
        }
        if !CATEGORIES.contains(&entry.category.as_str()) {
            return Err(Violation::new(
                "invalid_category",
                format!("unknown category {}", entry.category),
            ));
        }
        validate_owner_category(entry)?;
        if !codes.insert((entry.owner.clone(), entry.code.clone())) {
            return Err(Violation::new(
                "duplicate_error_identity",
                format!("duplicate error identity {}.{}", entry.owner, entry.code),
            ));
        }
        validate_sources(&entry.normative_sources, Some(entry), sources)?;
    }
    Ok(())
}

fn validate_owner_category(entry: &CodeEntry) -> Result<(), Violation> {
    let category = entry.category.as_str();
    let valid = match entry.owner.as_str() {
        "revision_core_format_v1" | "snapshot_integrity_format_v1" => matches!(
            category,
            "representation_validation"
                | "intrinsic_semantic_validation"
                | "relational_semantic_validation"
        ),
        "hook_protocol_v1" => category == "protocol",
        "domain_validation" => matches!(
            category,
            "intrinsic_semantic_validation" | "relational_semantic_validation"
        ),
        "resolution" => category == "resolution",
        "compilation" => category == "compilation",
        "admission" => category == "admission",
        "execution" => category == "execution",
        "persistence" => category == "persistence",
        "recovery" => category == "recovery",
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(Violation::new(
            "invalid_category",
            format!(
                "category {} is not owned by {}",
                entry.category, entry.owner
            ),
        ))
    }
}

fn validate_sources(
    entries: &[NormativeSource],
    code: Option<&CodeEntry>,
    sources: &SourceIndex,
) -> Result<(), Violation> {
    if entries.is_empty() {
        return Err(Violation::new(
            "missing_normative_source",
            "catalog entry has no normative source",
        ));
    }
    let mut unique = BTreeSet::new();
    for source in entries {
        if !unique.insert(source.clone()) {
            return Err(Violation::new(
                "duplicate_normative_source",
                "catalog entry repeats a normative source",
            ));
        }
        match source {
            NormativeSource::Requirement { id } => {
                if !valid_requirement_id(id) || !sources.requirements.contains(id) {
                    return Err(Violation::new(
                        "unresolved_normative_source",
                        format!("unknown requirement {id}"),
                    ));
                }
            }
            NormativeSource::FrozenVector { suite, name } => {
                let Some(code) = code else {
                    return Err(Violation::new(
                        "invalid_normative_source",
                        "owner entries cannot use an error vector as their semantic source",
                    ));
                };
                if suite_owner(suite) != Some(code.owner.as_str()) {
                    return Err(Violation::new(
                        "invalid_normative_source",
                        format!("suite {suite} does not own {}.{}", code.owner, code.code),
                    ));
                }
                let expected = sources
                    .suites
                    .get(suite)
                    .and_then(|vectors| vectors.get(name));
                if expected.map(String::as_str) != Some(code.code.as_str()) {
                    return Err(Violation::new(
                        "unresolved_normative_source",
                        format!(
                            "Frozen vector {suite}/{name} does not establish {}",
                            code.code
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_error_ref(value: ErrorRef) -> Result<ErrorRef, Violation> {
    validate_identifier(&value.owner, "invalid_error_owner")?;
    validate_identifier(&value.code, "invalid_error_code")?;
    Ok(value)
}

fn validate_identifier(value: &str, violation: &'static str) -> Result<(), Violation> {
    let bytes = value.as_bytes();
    let first_valid = bytes.first().is_some_and(u8::is_ascii_lowercase);
    let segments_valid = value.split('_').all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    });
    if bytes.is_empty() || bytes.len() > 128 || !value.is_ascii() || !first_valid || !segments_valid
    {
        Err(Violation::new(
            violation,
            format!("invalid taxonomy identifier {value:?}"),
        ))
    } else {
        Ok(())
    }
}

fn verify_initial_registry(catalog: &Catalog, sources: &SourceIndex) -> Result<(), Violation> {
    let owners = catalog
        .owners
        .iter()
        .map(|entry| entry.owner.as_str())
        .collect::<BTreeSet<_>>();
    for owner in INITIAL_OWNERS {
        if !owners.contains(owner) {
            return Err(Violation::new(
                "missing_initial_owner",
                format!("missing initial owner {owner}"),
            ));
        }
    }

    for owner in EMPTY_INITIAL_OWNERS {
        if catalog.codes.iter().any(|entry| entry.owner == *owner) {
            return Err(Violation::new(
                "placeholder_runtime_code",
                format!("initial owner {owner} must remain intentionally empty"),
            ));
        }
    }
    if let Some(entry) = catalog
        .codes
        .iter()
        .find(|entry| PLACEHOLDER_CODES.contains(&entry.code.as_str()))
    {
        return Err(Violation::new(
            "placeholder_runtime_code",
            format!("{}.{} is a placeholder code", entry.owner, entry.code),
        ));
    }

    for (owner, suite) in [
        ("revision_core_format_v1", "revision_core_format_v1"),
        (
            "snapshot_integrity_format_v1",
            "snapshot_integrity_format_v1",
        ),
        ("hook_protocol_v1", "hook_protocol_v1"),
    ] {
        let catalog_codes = catalog
            .codes
            .iter()
            .filter(|entry| entry.owner == owner)
            .map(|entry| entry.code.clone())
            .collect::<BTreeSet<_>>();
        let vector_codes = sources
            .suites
            .get(suite)
            .expect("all Frozen suites are indexed")
            .values()
            .cloned()
            .collect::<BTreeSet<_>>();
        if catalog_codes != vector_codes {
            return Err(Violation::new(
                "frozen_code_set_mismatch",
                format!(
                    "catalog codes for {owner} differ from Frozen vector errors: catalog={catalog_codes:?}, vectors={vector_codes:?}"
                ),
            ));
        }
        for entry in catalog.codes.iter().filter(|entry| entry.owner == owner) {
            let has_vector = entry.normative_sources.iter().any(|source| {
                matches!(
                    source,
                    NormativeSource::FrozenVector { suite: source_suite, .. }
                        if source_suite == suite
                )
            });
            if !has_vector {
                return Err(Violation::new(
                    "missing_frozen_vector_source",
                    format!("{}.{} has no Frozen vector source", entry.owner, entry.code),
                ));
            }
        }
    }

    verify_excluded(
        catalog,
        "revision_core_format_v1",
        EXCLUDED_REVISION_CORE_VOCABULARY,
    )?;
    verify_excluded(
        catalog,
        "snapshot_integrity_format_v1",
        EXCLUDED_SNAPSHOT_VOCABULARY,
    )?;

    require_code(catalog, "resolution", "ambiguous_reference", "resolution")?;
    require_code(catalog, "admission", "plan_invalidated", "admission")?;
    Ok(())
}

fn verify_excluded(catalog: &Catalog, owner: &str, excluded: &[&str]) -> Result<(), Violation> {
    for code in excluded {
        if catalog
            .codes
            .iter()
            .any(|entry| entry.owner == owner && entry.code == *code)
        {
            return Err(Violation::new(
                "implementation_vocabulary_registered",
                format!("{owner}.{code} has no Frozen product-code source"),
            ));
        }
    }
    Ok(())
}

fn require_code(
    catalog: &Catalog,
    owner: &str,
    code: &str,
    category: &str,
) -> Result<(), Violation> {
    if catalog
        .codes
        .iter()
        .any(|entry| entry.owner == owner && entry.code == code && entry.category == category)
    {
        Ok(())
    } else {
        Err(Violation::new(
            "missing_initial_code",
            format!("missing initial error identity {owner}.{code}"),
        ))
    }
}

fn verify_reference_fixtures(fixtures: &Fixtures) -> Result<(), Violation> {
    for fixture in &fixtures.valid_references {
        let actual = validate_error_ref(fixture.input.clone()).map_err(|error| {
            Violation::new(
                error.code,
                format!("valid reference fixture {}: {}", fixture.name, error.detail),
            )
        })?;
        if actual != fixture.expected {
            return Err(Violation::new(
                "reference_not_preserved",
                format!("fixture {} changed owner or code spelling", fixture.name),
            ));
        }
    }
    for fixture in &fixtures.invalid_references {
        match validate_error_ref(fixture.input.clone()) {
            Ok(_) => {
                return Err(Violation::new(
                    "invalid_reference_accepted",
                    format!("fixture {} unexpectedly passed", fixture.name),
                ));
            }
            Err(error) if error.code == fixture.expected_violation => {}
            Err(error) => {
                return Err(Violation::new(
                    "fixture_mismatch",
                    format!(
                        "fixture {} expected {}, got {}",
                        fixture.name, fixture.expected_violation, error.code
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn verify_invalid_catalog_fixtures(
    fixtures: &Fixtures,
    sources: &SourceIndex,
) -> Result<(), Violation> {
    for fixture in &fixtures.invalid_catalogs {
        let result = parse_catalog(&fixture.raw_json)
            .and_then(|catalog| validate_catalog(&catalog, sources));
        match result {
            Ok(()) => {
                return Err(Violation::new(
                    "invalid_catalog_accepted",
                    format!("fixture {} unexpectedly passed", fixture.name),
                ));
            }
            Err(error) if error.code == fixture.expected_violation => {}
            Err(error) => {
                return Err(Violation::new(
                    "fixture_mismatch",
                    format!(
                        "fixture {} expected {}, got {}: {}",
                        fixture.name, fixture.expected_violation, error.code, error.detail
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn verify_append_only_examples(catalog: &Catalog, sources: &SourceIndex) -> Result<(), Violation> {
    let baseline = semantic_projection(catalog);

    let mut code_append = catalog.clone();
    code_append.codes.push(CodeEntry {
        owner: "resolution".to_owned(),
        code: "future_resolution_condition".to_owned(),
        category: "resolution".to_owned(),
        normative_sources: vec![NormativeSource::Requirement {
            id: "PR-REQ-0222".to_owned(),
        }],
    });
    validate_catalog(&code_append, sources)?;
    if !semantic_projection(&code_append).is_superset(&baseline) {
        return Err(Violation::new(
            "append_changed_existing_entry",
            "code append changed an existing catalog entry",
        ));
    }

    let mut owner_append = catalog.clone();
    owner_append.owners.push(OwnerEntry {
        owner: "future_subsystem".to_owned(),
        normative_sources: vec![NormativeSource::Requirement {
            id: "PR-REQ-0220".to_owned(),
        }],
    });
    owner_append.codes.push(CodeEntry {
        owner: "future_subsystem".to_owned(),
        code: "future_condition".to_owned(),
        category: "execution".to_owned(),
        normative_sources: vec![NormativeSource::Requirement {
            id: "PR-REQ-0222".to_owned(),
        }],
    });
    validate_catalog(&owner_append, sources)?;
    if !semantic_projection(&owner_append).is_superset(&baseline) {
        return Err(Violation::new(
            "append_changed_existing_entry",
            "owner append changed an existing catalog entry",
        ));
    }

    let mut reordered = catalog.clone();
    reordered.owners.reverse();
    reordered.codes.reverse();
    validate_catalog(&reordered, sources)?;
    if semantic_projection(&reordered) != baseline {
        return Err(Violation::new(
            "array_order_changed_semantics",
            "catalog array order changed the semantic projection",
        ));
    }
    Ok(())
}

fn semantic_projection(catalog: &Catalog) -> BTreeSet<String> {
    let mut projection = catalog
        .owners
        .iter()
        .map(|entry| {
            format!(
                "owner:{}:{:?}",
                entry.owner,
                source_set(&entry.normative_sources)
            )
        })
        .collect::<BTreeSet<_>>();
    projection.extend(catalog.codes.iter().map(|entry| {
        format!(
            "code:{}:{}:{}:{:?}",
            entry.owner,
            entry.code,
            entry.category,
            source_set(&entry.normative_sources)
        )
    }));
    projection
}

fn source_set(sources: &[NormativeSource]) -> BTreeSet<NormativeSource> {
    sources.iter().cloned().collect()
}

fn verify_semantic_boundaries(catalog: &Catalog) -> Result<(), Violation> {
    for reserved in [
        "cancelled",
        "failed",
        "hook_result",
        "interrupted",
        "manual_recovery_required",
        "not_evaluated",
        "primary_failure",
        "secondary_failure",
        "succeeded",
        "timed_out",
    ] {
        if catalog.codes.iter().any(|entry| entry.code == reserved) {
            return Err(Violation::new(
                "non_error_concept_registered",
                format!("{reserved} is not a Pactrun error identity"),
            ));
        }
    }
    if catalog
        .owners
        .iter()
        .any(|entry| entry.owner == "hook_code_v1")
    {
        return Err(Violation::new(
            "hook_code_registered",
            "HookCodeV1 is not a Pactrun-owned taxonomy owner",
        ));
    }
    Ok(())
}

fn build_source_index(workspace_root: &Path) -> Result<SourceIndex, String> {
    let requirements = collect_requirement_ids(&workspace_root.join("docs"))?;
    let mut suites = BTreeMap::new();
    for suite in [
        "revision_core_format_v1",
        "snapshot_integrity_format_v1",
        "hook_protocol_v1",
    ] {
        suites.insert(
            suite.to_owned(),
            load_frozen_error_vectors(workspace_root, suite)?,
        );
    }
    Ok(SourceIndex {
        requirements,
        suites,
    })
}

fn load_frozen_error_vectors(
    workspace_root: &Path,
    suite: &str,
) -> Result<BTreeMap<String, String>, String> {
    let path = workspace_root
        .join("tests")
        .join("vectors")
        .join(suite)
        .join("vectors.json");
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))?;
    if value["status"] != "frozen" {
        return Err(format!("normative vector suite {suite} is not Frozen"));
    }
    let invalid = value["invalid"]
        .as_array()
        .ok_or_else(|| format!("{suite} has no invalid vector array"))?;
    let mut vectors = BTreeMap::new();
    for vector in invalid {
        let name = vector["name"]
            .as_str()
            .ok_or_else(|| format!("{suite} invalid vector has no name"))?;
        let code = vector["expected_error"]
            .as_str()
            .ok_or_else(|| format!("{suite}/{name} has no expected_error"))?;
        if vectors.insert(name.to_owned(), code.to_owned()).is_some() {
            return Err(format!("{suite} has duplicate vector name {name}"));
        }
    }
    Ok(vectors)
}

fn collect_requirement_ids(root: &Path) -> Result<BTreeSet<String>, String> {
    let files = super::revision_core_v1::normative_markdown_files(root)?;
    let mut requirements = BTreeSet::new();
    for path in files {
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        for line in text.lines() {
            if let Some(id) = requirement_definition(line)
                && !requirements.insert(id.clone())
            {
                return Err(format!("duplicate requirement definition {id}"));
            }
        }
    }
    Ok(requirements)
}

fn requirement_definition(line: &str) -> Option<String> {
    if !line.starts_with('#') {
        return None;
    }
    let text = line.trim_start_matches('#').trim_start();
    let rest = text.strip_prefix("PR-REQ-")?;
    let digits = rest.get(..4)?;
    if digits.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(format!("PR-REQ-{digits}"))
    } else {
        None
    }
}

fn valid_requirement_id(value: &str) -> bool {
    value.len() == 11
        && value.starts_with("PR-REQ-")
        && value[7..].bytes().all(|byte| byte.is_ascii_digit())
}

fn suite_owner(suite: &str) -> Option<&'static str> {
    match suite {
        "revision_core_format_v1" => Some("revision_core_format_v1"),
        "snapshot_integrity_format_v1" => Some("snapshot_integrity_format_v1"),
        "hook_protocol_v1" => Some("hook_protocol_v1"),
        _ => None,
    }
}

fn format_violation(error: Violation) -> String {
    format!("{}: {}", error.code, error.detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask is a direct workspace child")
            .to_path_buf()
    }

    fn loaded() -> (Catalog, Fixtures, SourceIndex) {
        let root = workspace_root();
        (
            load_catalog(&root).unwrap(),
            load_fixtures(&root).unwrap(),
            build_source_index(&root).unwrap(),
        )
    }

    // Test-ID: PR-TEST-0033
    // Verifies: PR-REQ-0219, PR-REQ-0220, PR-REQ-0224
    #[test]
    fn catalog_schema_and_identifier_profile_are_closed() {
        let (catalog, fixtures, sources) = loaded();
        validate_catalog(&catalog, &sources).unwrap();
        verify_invalid_catalog_fixtures(&fixtures, &sources).unwrap();
        verify_reference_fixtures(&fixtures).unwrap();
    }

    // Test-ID: PR-TEST-0034
    // Verifies: PR-REQ-0220, PR-REQ-0222, PR-REQ-0224
    #[test]
    fn owner_and_code_registries_are_unique_and_append_only() {
        let (catalog, _, sources) = loaded();
        verify_append_only_examples(&catalog, &sources).unwrap();
        assert!(
            catalog
                .codes
                .iter()
                .filter(|entry| entry.code == "duplicate_property")
                .count()
                >= 2
        );
    }

    // Test-ID: PR-TEST-0035
    // Verifies: PR-REQ-0221, PR-REQ-0224
    #[test]
    fn registered_local_codes_have_normative_sources() {
        let (catalog, _, sources) = loaded();
        verify_initial_registry(&catalog, &sources).unwrap();
    }

    // Test-ID: PR-TEST-0036
    // Verifies: PR-REQ-0219, PR-REQ-0222, PR-REQ-0224
    #[test]
    fn unknown_references_are_preserved_without_catalog_lookup() {
        let (catalog, fixtures, sources) = loaded();
        verify_reference_fixtures(&fixtures).unwrap();
        verify_append_only_examples(&catalog, &sources).unwrap();
    }

    // Test-ID: PR-TEST-0037
    // Verifies: PR-REQ-0219, PR-REQ-0221, PR-REQ-0223, PR-REQ-0224
    #[test]
    fn error_identity_is_separate_from_outcome_and_hook_results() {
        let (catalog, _, _) = loaded();
        verify_semantic_boundaries(&catalog).unwrap();
        require_code(&catalog, "resolution", "ambiguous_reference", "resolution").unwrap();
        require_code(&catalog, "admission", "plan_invalidated", "admission").unwrap();
    }

    // Test-ID: PR-TEST-0038
    // Verifies: PR-REQ-0223, PR-REQ-0224
    #[test]
    fn status_only_freeze_and_traceability_are_valid() {
        let (catalog, fixtures, _) = loaded();
        assert!(matches!(catalog.status.as_str(), "candidate" | "frozen"));
        assert!(
            fixtures
                .invalid_catalogs
                .iter()
                .all(|fixture| !fixture.expected_violation.is_empty())
        );
        let mut frozen = catalog.clone();
        frozen.status = "frozen".to_owned();
        assert_eq!(semantic_projection(&catalog), semantic_projection(&frozen));
        super::super::revision_core_v1::verify_traceability(&workspace_root()).unwrap();
    }
}
