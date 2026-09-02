//! PackSourceYamlV1 schema-directed decoding and source-stage domain.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use serde_json::{Map, Number, Value};
use yaml_rust2::{
    parser::{Event, MarkedEventReceiver, Parser},
    scanner::{Marker, Scanner, TScalarStyle, TokenType},
};

mod source_acquisition;

pub(crate) use source_acquisition::{SecureSourceRoot, SourceAcquisitionError};

use crate::{
    domain::{
        ActionIdentity, AttributionText, ContentId, InputIdentity, ManagedOutputIdentity,
        PackageId, ParameterIdentity, PresentationField, PresentationTargetV1, PresentationValue,
        ProvenanceClaim, PublisherName, PublisherNamespace, ReferenceLabel, ReferenceLabelSource,
        RevisionContentDigest, RevisionCoreV1, RuntimeContentClosureIdentityV1, RuntimePath,
        SourceUri,
    },
    revision_core_v1::project_revision_core_source_v1,
};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct SourceRelativePathV1(String);

impl SourceRelativePathV1 {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, AuthoringError> {
        let value = value.into();
        if value.is_empty() || value.len() > 1024 || value.starts_with('/') {
            return Err(AuthoringError::new(
                "invalid SourceRelativePathV1 length or root",
            ));
        }
        for segment in value.split('/') {
            validate_source_segment(segment)?;
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeSourceRecordV1 {
    pub(crate) id: ContentId,
    pub(crate) source: SourceRelativePathV1,
    pub(crate) path: RuntimePath,
    pub(crate) executable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PortablePresentationTemplate {
    pub(crate) target: PresentationTargetV1,
    pub(crate) field: PresentationField,
    pub(crate) value: PresentationValue,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub(crate) struct PortableMetadataTemplate {
    pub(crate) reference_labels: Vec<(ReferenceLabel, ReferenceLabelSource)>,
    pub(crate) presentation: Vec<PortablePresentationTemplate>,
    pub(crate) provenance: Vec<ProvenanceClaim>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NormalizedPackSourceCandidate {
    pub(crate) package_id: PackageId,
    pub(crate) revision: RevisionCoreV1,
    pub(crate) runtime_sources: Vec<RuntimeSourceRecordV1>,
    pub(crate) portable_metadata: PortableMetadataTemplate,
}

/// Host-source-independent normalized semantic model. Source locators and
/// acquisition handles cannot cross into this type.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NormalizedPackDefinition {
    pub(crate) package_id: PackageId,
    pub(crate) revision: RevisionCoreV1,
    pub(crate) runtime_content: RuntimeContentClosureIdentityV1,
    pub(crate) portable_metadata: PortableMetadataTemplate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AuthoringError {
    message: String,
}

impl AuthoringError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AuthoringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AuthoringError {}

#[derive(Clone, Debug)]
enum Node {
    Scalar(ScalarNode),
    Sequence(Vec<Node>),
    Mapping(BTreeMap<String, Node>),
}

#[derive(Clone, Debug)]
struct ScalarNode {
    value: String,
    style: TScalarStyle,
    marker: Option<Marker>,
}

#[derive(Default)]
struct EventSink {
    events: Vec<(Event, Marker)>,
}

impl MarkedEventReceiver for EventSink {
    fn on_event(&mut self, event: Event, marker: Marker) {
        self.events.push((event, marker));
    }
}

pub(crate) fn parse_pack_source_yaml_v1(
    source: &[u8],
) -> Result<NormalizedPackSourceCandidate, AuthoringError> {
    let source = std::str::from_utf8(source)
        .map_err(|_| AuthoringError::new("pactrun.yaml must be valid UTF-8"))?;
    reject_forbidden_tokens(source)?;
    let root = parse_document(source)?;
    let mut root = closed_map(
        root,
        &["source_format", "package_id", "revision", "runtime_content"],
        &["portable_metadata"],
        "PackSourceYamlV1",
    )?;

    let source_format = take_scalar(&mut root, "source_format")?;
    if source_format.style != TScalarStyle::Plain
        || capture_numeric_token(source, &source_format)? != "1"
    {
        return Err(AuthoringError::new("source_format must use raw token 1"));
    }
    let package_id = PackageId::from_str(&string_scalar(take(&mut root, "package_id")?)?)
        .map_err(AuthoringError::new)?;

    let revision_json = revision_json(source, take(&mut root, "revision")?)?;
    let revision_bytes = serde_json::to_vec(&revision_json)
        .map_err(|error| AuthoringError::new(format!("encode projected Revision: {error}")))?;
    let revision = project_revision_core_source_v1(&revision_bytes)
        .map_err(|error| AuthoringError::new(format!("project RevisionCoreV1: {error}")))?;
    let runtime_sources = runtime_sources(take(&mut root, "runtime_content")?)?;
    let portable_metadata = match root.remove("portable_metadata") {
        Some(value) => portable_metadata(value)?,
        None => PortableMetadataTemplate::default(),
    };

    for item in &portable_metadata.presentation {
        if !revision.contains_presentation_target(&item.target) {
            return Err(AuthoringError::new(
                "presentation target is not declared by the normalized Revision",
            ));
        }
    }

    Ok(NormalizedPackSourceCandidate {
        package_id,
        revision,
        runtime_sources,
        portable_metadata,
    })
}

fn reject_forbidden_tokens(source: &str) -> Result<(), AuthoringError> {
    for token in Scanner::new(source.chars()) {
        if matches!(
            token.1,
            TokenType::VersionDirective(_, _)
                | TokenType::TagDirective(_, _)
                | TokenType::Tag(_, _)
                | TokenType::Anchor(_)
                | TokenType::Alias(_)
        ) {
            return Err(AuthoringError::new(format!(
                "forbidden YAML construct at line {} column {}",
                token.0.line(),
                token.0.col()
            )));
        }
    }
    Ok(())
}

fn parse_document(source: &str) -> Result<Node, AuthoringError> {
    let mut sink = EventSink::default();
    Parser::new_from_str(source)
        .load(&mut sink, true)
        .map_err(|error| AuthoringError::new(error.to_string()))?;
    let document_count = sink
        .events
        .iter()
        .filter(|(event, _)| matches!(event, Event::DocumentStart))
        .count();
    if document_count != 1 {
        return Err(AuthoringError::new(
            "PackSourceYamlV1 must contain exactly one document",
        ));
    }
    let mut cursor = 0;
    expect_event(&sink.events, &mut cursor, |event| {
        matches!(event, Event::StreamStart)
    })?;
    expect_event(&sink.events, &mut cursor, |event| {
        matches!(event, Event::DocumentStart)
    })?;
    let node = parse_node(&sink.events, &mut cursor)?;
    expect_event(&sink.events, &mut cursor, |event| {
        matches!(event, Event::DocumentEnd)
    })?;
    expect_event(&sink.events, &mut cursor, |event| {
        matches!(event, Event::StreamEnd)
    })?;
    if cursor != sink.events.len() {
        return Err(AuthoringError::new(
            "multiple YAML documents are not supported",
        ));
    }
    Ok(node)
}

fn parse_node(events: &[(Event, Marker)], cursor: &mut usize) -> Result<Node, AuthoringError> {
    let (event, marker) = events
        .get(*cursor)
        .ok_or_else(|| AuthoringError::new("unexpected end of YAML event stream"))?;
    *cursor += 1;
    match event {
        Event::Scalar(value, style, 0, None) => {
            if matches!(style, TScalarStyle::Literal | TScalarStyle::Folded) {
                return Err(AuthoringError::new("block scalars are not supported"));
            }
            Ok(Node::Scalar(ScalarNode {
                value: value.clone(),
                style: *style,
                marker: Some(*marker),
            }))
        }
        Event::SequenceStart(0, None) => {
            let mut values = Vec::new();
            while !matches!(
                events.get(*cursor).map(|value| &value.0),
                Some(Event::SequenceEnd)
            ) {
                values.push(parse_node(events, cursor)?);
            }
            *cursor += 1;
            Ok(Node::Sequence(values))
        }
        Event::MappingStart(0, None) => {
            let mut values = BTreeMap::new();
            while !matches!(
                events.get(*cursor).map(|value| &value.0),
                Some(Event::MappingEnd)
            ) {
                let key = string_scalar(parse_node(events, cursor)?)?;
                let value = parse_node(events, cursor)?;
                if values.insert(key.clone(), value).is_some() {
                    return Err(AuthoringError::new(format!(
                        "duplicate decoded YAML key {key:?}"
                    )));
                }
            }
            *cursor += 1;
            Ok(Node::Mapping(values))
        }
        Event::Alias(_) => Err(AuthoringError::new("YAML aliases are not supported")),
        Event::Scalar(_, _, _, Some(_))
        | Event::SequenceStart(_, Some(_))
        | Event::MappingStart(_, Some(_)) => {
            Err(AuthoringError::new("explicit YAML tags are not supported"))
        }
        Event::Scalar(_, _, _, _) | Event::SequenceStart(_, _) | Event::MappingStart(_, _) => {
            Err(AuthoringError::new("YAML anchors are not supported"))
        }
        _ => Err(AuthoringError::new("unexpected YAML node event")),
    }
}

fn expect_event(
    events: &[(Event, Marker)],
    cursor: &mut usize,
    predicate: impl FnOnce(&Event) -> bool,
) -> Result<(), AuthoringError> {
    let event = events
        .get(*cursor)
        .ok_or_else(|| AuthoringError::new("unexpected end of YAML event stream"))?;
    if !predicate(&event.0) {
        return Err(AuthoringError::new("unexpected YAML event ordering"));
    }
    *cursor += 1;
    Ok(())
}

fn revision_json(source: &str, node: Node) -> Result<Value, AuthoringError> {
    let mut revision = closed_map(
        node,
        &[],
        &["inputs", "actions", "snapshot", "migrations", "cleanup"],
        "revision",
    )?;
    for key in ["inputs", "actions", "migrations"] {
        revision
            .entry(key.to_owned())
            .or_insert_with(|| Node::Sequence(Vec::new()));
    }
    if let Some(Node::Sequence(inputs)) = revision.get_mut("inputs") {
        for input in inputs {
            let map = mapping_mut(input, "Input declaration")?;
            map.entry("required".to_owned())
                .or_insert_with(|| plain("false"));
            map.entry("protection".to_owned())
                .or_insert_with(|| plain("normal"));
        }
    }
    let mut json = Map::new();
    json.insert("format_version".to_owned(), Value::Number(Number::from(1)));
    for (key, value) in revision {
        json.insert(key, node_to_json(source, value, None)?);
    }
    Ok(Value::Object(json))
}

fn runtime_sources(node: Node) -> Result<Vec<RuntimeSourceRecordV1>, AuthoringError> {
    let mut runtime = closed_map(node, &[], &["files"], "runtime_content")?;
    let files = runtime
        .remove("files")
        .unwrap_or_else(|| Node::Sequence(Vec::new()));
    let Node::Sequence(files) = files else {
        return Err(AuthoringError::new(
            "runtime_content.files must be a sequence",
        ));
    };
    let mut result = Vec::new();
    let mut ids = BTreeSet::new();
    for file in files {
        let mut file = closed_map(
            file,
            &["id", "source", "path"],
            &["executable"],
            "runtime file source",
        )?;
        let id = ContentId::parse(string_scalar(take(&mut file, "id")?)?)
            .map_err(|error| AuthoringError::new(error.to_string()))?;
        if !ids.insert(id.clone()) {
            return Err(AuthoringError::new(format!(
                "duplicate authored ContentId {}",
                id.as_str()
            )));
        }
        result.push(RuntimeSourceRecordV1 {
            id,
            source: SourceRelativePathV1::parse(string_scalar(take(&mut file, "source")?)?)?,
            path: RuntimePath::parse(string_scalar(take(&mut file, "path")?)?)
                .map_err(|error| AuthoringError::new(error.to_string()))?,
            executable: match file.remove("executable") {
                Some(value) => bool_scalar(value)?,
                None => false,
            },
        });
    }
    result.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(result)
}

fn portable_metadata(node: Node) -> Result<PortableMetadataTemplate, AuthoringError> {
    let mut object = closed_map(
        node,
        &[],
        &["reference_labels", "presentation", "provenance"],
        "portable_metadata",
    )?;
    let labels = sequence_or_empty(object.remove("reference_labels"), "reference_labels")?;
    let presentations = sequence_or_empty(object.remove("presentation"), "presentation")?;
    let provenance = sequence_or_empty(object.remove("provenance"), "provenance")?;
    let mut result = PortableMetadataTemplate::default();
    let mut label_keys = BTreeSet::new();
    for value in labels {
        let mut value = closed_map(value, &["label", "source"], &[], "reference label")?;
        let label = ReferenceLabel::parse(string_scalar(take(&mut value, "label")?)?)
            .map_err(|error| AuthoringError::new(error.to_string()))?;
        let source = label_source(take(&mut value, "source")?)?;
        if !label_keys.insert((label.clone(), source.clone())) {
            return Err(AuthoringError::new(
                "duplicate reference-label semantic tuple",
            ));
        }
        result.reference_labels.push((label, source));
    }
    result.reference_labels.sort();
    let mut presentation_keys = BTreeSet::new();
    for value in presentations {
        let mut value = closed_map(
            value,
            &["target", "field", "value"],
            &[],
            "presentation metadata",
        )?;
        let target = presentation_target(take(&mut value, "target")?)?;
        let field = match string_scalar(take(&mut value, "field")?)?.as_str() {
            "display_name" => PresentationField::DisplayName,
            "summary" => PresentationField::Summary,
            "description" => PresentationField::Description,
            "help" => PresentationField::Help,
            other => {
                return Err(AuthoringError::new(format!(
                    "unknown presentation field {other:?}"
                )));
            }
        };
        if !presentation_keys.insert((target.clone(), field)) {
            return Err(AuthoringError::new("duplicate presentation semantic key"));
        }
        let value = PresentationValue::parse(string_scalar(take(&mut value, "value")?)?)
            .map_err(|error| AuthoringError::new(error.to_string()))?;
        result.presentation.push(PortablePresentationTemplate {
            target,
            field,
            value,
        });
    }
    result.presentation.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then(left.field.cmp(&right.field))
    });
    let mut claim_keys = BTreeSet::new();
    for value in provenance {
        let claim = provenance_claim(value)?;
        if !claim_keys.insert(claim.clone()) {
            return Err(AuthoringError::new("duplicate provenance semantic tuple"));
        }
        result.provenance.push(claim);
    }
    result.provenance.sort();
    Ok(result)
}

fn label_source(node: Node) -> Result<ReferenceLabelSource, AuthoringError> {
    let mut map = mapping(node, "reference-label source")?;
    let kind = string_scalar(take(&mut map, "kind")?)?;
    let result = match kind.as_str() {
        "unattributed" => ReferenceLabelSource::Unattributed,
        "source_uri" => ReferenceLabelSource::SourceUri(source_uri(take(&mut map, "source_uri")?)?),
        "publisher" | "publisher_source_uri" => {
            let name = PublisherName::parse(string_scalar(take(&mut map, "publisher_name")?)?)
                .map_err(|error| AuthoringError::new(error.to_string()))?;
            let namespace = map
                .remove("publisher_namespace")
                .map(string_scalar)
                .transpose()?
                .map(PublisherNamespace::parse)
                .transpose()
                .map_err(|error| AuthoringError::new(error.to_string()))?;
            let uri = map.remove("source_uri").map(source_uri).transpose()?;
            match (kind.as_str(), uri) {
                ("publisher", None) => ReferenceLabelSource::Publisher { name, namespace },
                ("publisher_source_uri", Some(source_uri)) => {
                    ReferenceLabelSource::PublisherSourceUri {
                        name,
                        namespace,
                        source_uri,
                    }
                }
                _ => {
                    return Err(AuthoringError::new(
                        "publisher source URI presence does not match kind",
                    ));
                }
            }
        }
        _ => return Err(AuthoringError::new("unknown reference-label source kind")),
    };
    require_no_fields(&map, "reference-label source")?;
    Ok(result)
}

fn presentation_target(node: Node) -> Result<PresentationTargetV1, AuthoringError> {
    let mut map = mapping(node, "presentation target")?;
    let kind = string_scalar(take(&mut map, "kind")?)?;
    let target = match kind.as_str() {
        "revision" => PresentationTargetV1::Revision,
        "input" => PresentationTargetV1::Input(parse_input_id(take(&mut map, "input_id")?)?),
        "action" => PresentationTargetV1::Action(parse_action_id(take(&mut map, "action_id")?)?),
        "action_parameter" => PresentationTargetV1::ActionParameter {
            action: parse_action_id(take(&mut map, "action_id")?)?,
            parameter: parse_parameter_id(take(&mut map, "parameter_id")?)?,
        },
        "managed_output" => PresentationTargetV1::ManagedOutput {
            action: parse_action_id(take(&mut map, "action_id")?)?,
            output: ManagedOutputIdentity::parse(string_scalar(take(&mut map, "output_id")?)?)
                .map_err(|error| AuthoringError::new(error.to_string()))?,
        },
        "snapshot_capture" => PresentationTargetV1::SnapshotCapture,
        "snapshot_capture_parameter" => PresentationTargetV1::SnapshotCaptureParameter(
            parse_parameter_id(take(&mut map, "parameter_id")?)?,
        ),
        "snapshot_restore" => PresentationTargetV1::SnapshotRestore,
        "snapshot_restore_parameter" => PresentationTargetV1::SnapshotRestoreParameter(
            parse_parameter_id(take(&mut map, "parameter_id")?)?,
        ),
        "migration" => PresentationTargetV1::MigrationEdge(
            RevisionContentDigest::from_str(&string_scalar(take(
                &mut map,
                "source_revision_digest",
            )?)?)
            .map_err(AuthoringError::new)?,
        ),
        "cleanup" => PresentationTargetV1::Cleanup,
        _ => return Err(AuthoringError::new("unknown presentation target kind")),
    };
    require_no_fields(&map, "presentation target")?;
    Ok(target)
}

fn provenance_claim(node: Node) -> Result<ProvenanceClaim, AuthoringError> {
    let mut map = mapping(node, "provenance claim")?;
    let kind = string_scalar(take(&mut map, "kind")?)?;
    let claim = match kind.as_str() {
        "source_uri" => ProvenanceClaim::SourceUri(source_uri(take(&mut map, "source_uri")?)?),
        "publisher_attribution" => ProvenanceClaim::PublisherAttribution {
            publisher: PublisherName::parse(string_scalar(take(&mut map, "publisher_name")?)?)
                .map_err(|error| AuthoringError::new(error.to_string()))?,
            namespace: map
                .remove("publisher_namespace")
                .map(string_scalar)
                .transpose()?
                .map(PublisherNamespace::parse)
                .transpose()
                .map_err(|error| AuthoringError::new(error.to_string()))?,
            source_uri: map.remove("source_uri").map(source_uri).transpose()?,
        },
        "attribution" => ProvenanceClaim::Attribution {
            text: AttributionText::parse(string_scalar(take(&mut map, "attribution_text")?)?)
                .map_err(|error| AuthoringError::new(error.to_string()))?,
            source_uri: map.remove("source_uri").map(source_uri).transpose()?,
        },
        _ => return Err(AuthoringError::new("unknown provenance claim kind")),
    };
    require_no_fields(&map, "provenance claim")?;
    Ok(claim)
}

fn node_to_json(source: &str, node: Node, field: Option<&str>) -> Result<Value, AuthoringError> {
    match node {
        Node::Scalar(scalar) => match field {
            Some("required" | "sensitive") => bool_scalar(Node::Scalar(scalar))
                .map(Value::Bool)
                .map_err(|error| {
                    AuthoringError::new(format!(
                        "{}: {error}",
                        field.expect("matched schema-directed Boolean field")
                    ))
                }),
            Some("protocol_version") => {
                let value = exact_integer(&capture_numeric_token(source, &scalar)?)?;
                if value <= 0 {
                    return Err(AuthoringError::new("protocol_version must be positive"));
                }
                Ok(Value::Number(Number::from(value)))
            }
            _ => Ok(Value::String(string_scalar(Node::Scalar(scalar))?)),
        },
        Node::Sequence(values) => values
            .into_iter()
            .map(|value| node_to_json(source, value, None))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Node::Mapping(values) => {
            let parameter_type = values.get("type").and_then(|node| match node {
                Node::Scalar(value) => Some(value.value.clone()),
                _ => None,
            });
            let mut object = Map::new();
            for (key, value) in values {
                let converted = if key == "default" {
                    parameter_default_json(source, value, parameter_type.as_deref())?
                } else {
                    node_to_json(source, value, Some(&key))?
                };
                object.insert(key, converted);
            }
            Ok(Value::Object(object))
        }
    }
}

fn parameter_default_json(
    source: &str,
    node: Node,
    parameter_type: Option<&str>,
) -> Result<Value, AuthoringError> {
    match parameter_type {
        Some("integer") => {
            let scalar = scalar(node)?;
            Ok(Value::Number(Number::from(exact_integer(
                &capture_numeric_token(source, &scalar)?,
            )?)))
        }
        Some("float") => {
            let scalar = scalar(node)?;
            let token = capture_numeric_token(source, &scalar)?;
            let value = token
                .parse::<f64>()
                .map_err(|_| AuthoringError::new("invalid binary64 default"))?;
            if !value.is_finite() {
                return Err(AuthoringError::new("float default must be finite"));
            }
            Number::from_f64(if value == 0.0 { 0.0 } else { value })
                .map(Value::Number)
                .ok_or_else(|| AuthoringError::new("invalid binary64 default"))
        }
        Some("boolean") => Ok(Value::Bool(bool_scalar(node)?)),
        Some("string") => Ok(Value::String(string_scalar(node)?)),
        _ => Err(AuthoringError::new(
            "parameter default requires a closed parameter type",
        )),
    }
}

fn capture_numeric_token(source: &str, scalar: &ScalarNode) -> Result<String, AuthoringError> {
    if scalar.style != TScalarStyle::Plain {
        return Err(AuthoringError::new(
            "numeric values must use plain scalar style",
        ));
    }
    let marker = scalar
        .marker
        .ok_or_else(|| AuthoringError::new("numeric default has no source marker"))?;
    let char_index = marker.index();
    let byte_index = source
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .or_else(|| (char_index == source.chars().count()).then_some(source.len()))
        .ok_or_else(|| AuthoringError::new("invalid scalar source marker"))?;
    let bytes = source.as_bytes();
    let mut end = byte_index;
    while end < bytes.len() && matches!(bytes[end], b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E')
    {
        end += 1;
    }
    let token = source
        .get(byte_index..end)
        .ok_or_else(|| AuthoringError::new("numeric token is not UTF-8"))?;
    if token != scalar.value {
        return Err(AuthoringError::new(
            "YAML scalar decoding did not preserve numeric spelling",
        ));
    }
    ensure_json_number(token)?;
    Ok(token.to_owned())
}

fn ensure_json_number(token: &str) -> Result<(), AuthoringError> {
    let bytes = token.as_bytes();
    let mut index = usize::from(bytes.first() == Some(&b'-'));
    if index >= bytes.len() {
        return Err(AuthoringError::new("invalid JSON number token"));
    }
    if bytes[index] == b'0' {
        index += 1;
        if bytes.get(index).is_some_and(u8::is_ascii_digit) {
            return Err(AuthoringError::new("invalid JSON number leading zero"));
        }
    } else if matches!(bytes[index], b'1'..=b'9') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
    } else {
        return Err(AuthoringError::new("invalid JSON number integer part"));
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return Err(AuthoringError::new("invalid JSON number fraction"));
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return Err(AuthoringError::new("invalid JSON number exponent"));
        }
    }
    if index != bytes.len() {
        return Err(AuthoringError::new("invalid JSON number token"));
    }
    Ok(())
}

fn exact_integer(token: &str) -> Result<i64, AuthoringError> {
    ensure_json_number(token)?;
    let negative = token.starts_with('-');
    let unsigned = token.strip_prefix('-').unwrap_or(token);
    let exponent_at = unsigned.find(['e', 'E']);
    let (mantissa, exponent) = if let Some(index) = exponent_at {
        (
            &unsigned[..index],
            unsigned[index + 1..]
                .parse::<i32>()
                .map_err(|_| AuthoringError::new("numeric exponent is out of range"))?,
        )
    } else {
        (unsigned, 0)
    };
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits = format!("{integer}{fraction}");
    let first_nonzero = digits
        .find(|value| value != '0')
        .unwrap_or(digits.len().saturating_sub(1));
    digits.drain(..first_nonzero);
    let scale = exponent
        .checked_sub(
            i32::try_from(fraction.len()).map_err(|_| AuthoringError::new("number is too long"))?,
        )
        .ok_or_else(|| AuthoringError::new("numeric scale is out of range"))?;
    if scale < 0 {
        let remove =
            usize::try_from(-scale).map_err(|_| AuthoringError::new("non-integral number"))?;
        if remove > digits.len()
            || !digits[digits.len() - remove..]
                .bytes()
                .all(|byte| byte == b'0')
        {
            return Err(AuthoringError::new(
                "integer default must be mathematically integral",
            ));
        }
        digits.truncate(digits.len() - remove);
        if digits.is_empty() {
            digits.push('0');
        }
    } else {
        let append =
            usize::try_from(scale).map_err(|_| AuthoringError::new("integer is out of range"))?;
        if digits.len().saturating_add(append) > 16 {
            return Err(AuthoringError::new("integer is outside the safe range"));
        }
        digits.extend(std::iter::repeat_n('0', append));
    }
    let magnitude = digits
        .parse::<i64>()
        .map_err(|_| AuthoringError::new("integer is outside the safe range"))?;
    let value = if negative { -magnitude } else { magnitude };
    if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&value) {
        return Err(AuthoringError::new("integer is outside the safe range"));
    }
    Ok(value)
}

fn validate_source_segment(segment: &str) -> Result<(), AuthoringError> {
    if segment.is_empty()
        || matches!(segment, "." | "..")
        || segment.ends_with(['.', ' '])
        || segment.chars().any(|scalar| {
            matches!(scalar, '\u{0000}'..='\u{001f}' | '\u{007f}'..='\u{009f}')
                || matches!(scalar, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
    {
        return Err(AuthoringError::new("invalid SourceRelativePathV1 segment"));
    }
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .to_ascii_uppercase();
    let reserved = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) || (stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        || [
            "COM\u{00b9}",
            "COM\u{00b2}",
            "COM\u{00b3}",
            "LPT\u{00b9}",
            "LPT\u{00b2}",
            "LPT\u{00b3}",
        ]
        .contains(&stem.as_str());
    if reserved {
        return Err(AuthoringError::new("reserved Windows source-path segment"));
    }
    Ok(())
}

fn plain(value: &str) -> Node {
    Node::Scalar(ScalarNode {
        value: value.to_owned(),
        style: TScalarStyle::Plain,
        marker: None,
    })
}

fn closed_map(
    node: Node,
    required: &[&str],
    optional: &[&str],
    name: &str,
) -> Result<BTreeMap<String, Node>, AuthoringError> {
    let map = mapping(node, name)?;
    for field in required {
        if !map.contains_key(*field) {
            return Err(AuthoringError::new(format!("{name} is missing {field}")));
        }
    }
    for field in map.keys() {
        if !required.contains(&field.as_str()) && !optional.contains(&field.as_str()) {
            return Err(AuthoringError::new(format!(
                "unknown {name} field {field:?}"
            )));
        }
    }
    Ok(map)
}

fn mapping(node: Node, name: &str) -> Result<BTreeMap<String, Node>, AuthoringError> {
    match node {
        Node::Mapping(value) => Ok(value),
        _ => Err(AuthoringError::new(format!("{name} must be a mapping"))),
    }
}

fn mapping_mut<'a>(
    node: &'a mut Node,
    name: &str,
) -> Result<&'a mut BTreeMap<String, Node>, AuthoringError> {
    match node {
        Node::Mapping(value) => Ok(value),
        _ => Err(AuthoringError::new(format!("{name} must be a mapping"))),
    }
}

fn sequence_or_empty(node: Option<Node>, name: &str) -> Result<Vec<Node>, AuthoringError> {
    match node.unwrap_or_else(|| Node::Sequence(Vec::new())) {
        Node::Sequence(values) => Ok(values),
        _ => Err(AuthoringError::new(format!("{name} must be a sequence"))),
    }
}

fn scalar(node: Node) -> Result<ScalarNode, AuthoringError> {
    match node {
        Node::Scalar(value) => Ok(value),
        _ => Err(AuthoringError::new("expected scalar value")),
    }
}

fn string_scalar(node: Node) -> Result<String, AuthoringError> {
    let scalar = scalar(node)?;
    if matches!(scalar.style, TScalarStyle::Literal | TScalarStyle::Folded)
        || (scalar.style == TScalarStyle::Plain && scalar.value.is_empty())
    {
        return Err(AuthoringError::new("invalid schema-directed string scalar"));
    }
    Ok(scalar.value)
}

fn bool_scalar(node: Node) -> Result<bool, AuthoringError> {
    let scalar = scalar(node)?;
    if scalar.style != TScalarStyle::Plain {
        return Err(AuthoringError::new("Boolean values must be unquoted"));
    }
    match scalar.value.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(AuthoringError::new("Boolean values must be true or false")),
    }
}

fn take(map: &mut BTreeMap<String, Node>, key: &str) -> Result<Node, AuthoringError> {
    map.remove(key)
        .ok_or_else(|| AuthoringError::new(format!("missing required field {key}")))
}

fn take_scalar(map: &mut BTreeMap<String, Node>, key: &str) -> Result<ScalarNode, AuthoringError> {
    scalar(take(map, key)?)
}

fn require_no_fields(map: &BTreeMap<String, Node>, name: &str) -> Result<(), AuthoringError> {
    if let Some(field) = map.keys().next() {
        Err(AuthoringError::new(format!(
            "unknown {name} field {field:?}"
        )))
    } else {
        Ok(())
    }
}

fn source_uri(node: Node) -> Result<SourceUri, AuthoringError> {
    SourceUri::parse(string_scalar(node)?).map_err(|error| AuthoringError::new(error.to_string()))
}
fn parse_input_id(node: Node) -> Result<InputIdentity, AuthoringError> {
    InputIdentity::parse(string_scalar(node)?)
        .map_err(|error| AuthoringError::new(error.to_string()))
}
fn parse_action_id(node: Node) -> Result<ActionIdentity, AuthoringError> {
    ActionIdentity::parse(string_scalar(node)?)
        .map_err(|error| AuthoringError::new(error.to_string()))
}
fn parse_parameter_id(node: Node) -> Result<ParameterIdentity, AuthoringError> {
    ParameterIdentity::parse(string_scalar(node)?)
        .map_err(|error| AuthoringError::new(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal(extra: &str) -> String {
        format!(
            "source_format: 1\npackage_id: 00000000000000000000000000000001\nrevision:\n  inputs: []\n  actions: []\n  migrations: []\nruntime_content:\n  files: []\n{extra}"
        )
    }

    fn parameter_default(parameter_type: &str, default: &str) -> String {
        format!(
            r#"source_format: 1
package_id: 00000000000000000000000000000001
revision:
  inputs: []
  actions:
    - id: inspect
      access: observe
      parameters:
        - {{ id: value, type: {parameter_type}, sensitive: false, default: {default} }}
      hook:
        protocol_version: 1
        launch: {{ kind: direct, executable: hook }}
        args: []
        io: {{ terminal: output }}
      outputs: []
  migrations: []
runtime_content:
  files:
    - id: hook
      source: hook.exe
      path: hook.exe
      executable: true
"#
        )
    }

    // Test-ID: PR-TEST-0068
    // Verifies: PR-REQ-0258
    #[test]
    fn schema_directed_scalars_and_forbidden_yaml_are_exact() {
        assert!(parse_pack_source_yaml_v1(minimal("").as_bytes()).is_ok());
        assert!(
            parse_pack_source_yaml_v1(
                minimal("")
                    .replace(
                        "source_format: 1\n",
                        "# 前置 Unicode\r\nsource_format: 1 # raw\r\n"
                    )
                    .as_bytes()
            )
            .is_ok()
        );
        let mut integer_revisions = Vec::new();
        for value in ["1", "1.0", "1e0", "1E+0"] {
            let parsed = parse_pack_source_yaml_v1(parameter_default("integer", value).as_bytes());
            assert!(parsed.is_ok(), "{value}: {parsed:?}");
            integer_revisions.push(parsed.unwrap().revision);
        }
        assert!(integer_revisions.windows(2).all(|pair| pair[0] == pair[1]));
        let negative_zero =
            parse_pack_source_yaml_v1(parameter_default("float", "-0.0").as_bytes()).unwrap();
        let positive_zero =
            parse_pack_source_yaml_v1(parameter_default("float", "0").as_bytes()).unwrap();
        assert_eq!(negative_zero.revision, positive_zero.revision);
        assert!(parse_pack_source_yaml_v1(parameter_default("string", "true").as_bytes()).is_ok());
        assert!(parse_pack_source_yaml_v1(parameter_default("string", "123").as_bytes()).is_ok());
        assert!(parse_pack_source_yaml_v1(parameter_default("boolean", "true").as_bytes()).is_ok());
        for invalid in ["01", "+1", ".5", "1.", "0x10", "1_000", ".nan", ".inf"] {
            assert!(
                parse_pack_source_yaml_v1(parameter_default("float", invalid).as_bytes()).is_err(),
                "{invalid}"
            );
        }
        assert!(
            parse_pack_source_yaml_v1(parameter_default("float", "1e9999").as_bytes()).is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(parameter_default("integer", "9007199254740992").as_bytes())
                .is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(parameter_default("integer", "\"1\"").as_bytes()).is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(parameter_default("boolean", "\"true\"").as_bytes()).is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(
                minimal("")
                    .replace("source_format: 1", "source_format: 1.0")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(
                minimal("")
                    .replace("source_format: 1", "source_format: !!int 1")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(format!("%YAML 1.2\n---\n{}", minimal("")).as_bytes())
                .is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(
                minimal("")
                    .replace("package_id:", "package_id: !!str")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            parse_pack_source_yaml_v1(format!("{}---\n{}", minimal(""), minimal("")).as_bytes())
                .is_err()
        );
        assert!(parse_pack_source_yaml_v1(minimal("unknown: value\n").as_bytes()).is_err());
    }

    // Test-ID: PR-TEST-0069
    // Verifies: PR-REQ-0121, PR-REQ-0122, PR-REQ-0258, PR-REQ-0260
    #[test]
    fn source_candidate_keeps_authored_content_identity_and_portable_metadata() {
        let yaml = r#"source_format: 1
package_id: 00000000000000000000000000000001
revision:
  inputs:
    - id: config
  actions:
    - id: inspect
      access: observe
      parameters:
        - { id: detail, type: string, sensitive: false }
      hook: &forbidden-anchor
        protocol_version: 1
        launch: { kind: direct, executable: launcher }
        args: []
        io: { terminal: output }
      outputs:
        - { id: report }
  snapshot:
    capture:
      parameters:
        - { id: capture_mode, type: string, sensitive: false }
      access: observe
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: launcher }
        args: []
        io: { terminal: output }
    restore:
      parameters:
        - { id: restore_mode, type: string, sensitive: false }
      hook:
        protocol_version: 1
        launch: { kind: direct, executable: launcher }
        args: []
        io: { terminal: output }
  migrations:
    - source_revision_digest: sha256:1111111111111111111111111111111111111111111111111111111111111111
      transitions: []
      requires_source: []
      requires_target: []
      produces_target: []
  cleanup:
    requires: []
    hook:
      protocol_version: 1
      launch: { kind: direct, executable: launcher }
      args: []
      io: { terminal: output }
runtime_content:
  files:
    - id: launcher
      source: bin/launcher.exe
      path: bin/launcher.exe
      executable: true
portable_metadata:
  reference_labels:
    - { label: stable, source: { kind: unattributed } }
    - { label: source, source: { kind: source_uri, source_uri: "https://example.test/source#part" } }
    - { label: publisher, source: { kind: publisher, publisher_name: Example, publisher_namespace: tools } }
    - { label: release, source: { kind: publisher_source_uri, publisher_name: Example, source_uri: "urn:example:release#v1" } }
  presentation:
    - { target: { kind: revision }, field: display_name, value: Revision }
    - { target: { kind: input, input_id: config }, field: summary, value: Input }
    - { target: { kind: action, action_id: inspect }, field: description, value: Action }
    - { target: { kind: action_parameter, action_id: inspect, parameter_id: detail }, field: help, value: Parameter }
    - { target: { kind: managed_output, action_id: inspect, output_id: report }, field: display_name, value: Output }
    - { target: { kind: snapshot_capture }, field: summary, value: Capture }
    - { target: { kind: snapshot_capture_parameter, parameter_id: capture_mode }, field: description, value: CaptureParameter }
    - { target: { kind: snapshot_restore }, field: help, value: Restore }
    - { target: { kind: snapshot_restore_parameter, parameter_id: restore_mode }, field: display_name, value: RestoreParameter }
    - { target: { kind: migration, source_revision_digest: sha256:1111111111111111111111111111111111111111111111111111111111111111 }, field: summary, value: Migration }
    - { target: { kind: cleanup }, field: description, value: Cleanup }
  provenance:
    - { kind: source_uri, source_uri: "https://example.test/pack#source" }
    - { kind: publisher_attribution, publisher_name: Example, publisher_namespace: tools, source_uri: "urn:example:publisher" }
    - { kind: attribution, attribution_text: "Built by Example", source_uri: "https://example.test/credits#team" }
"#;
        assert!(parse_pack_source_yaml_v1(yaml.as_bytes()).is_err());
        let yaml = yaml.replace("      hook: &forbidden-anchor\n", "      hook:\n");
        let candidate = parse_pack_source_yaml_v1(yaml.as_bytes()).unwrap();
        assert_eq!(candidate.runtime_sources[0].id.as_str(), "launcher");
        assert_eq!(
            candidate.runtime_sources[0].source.as_str(),
            "bin/launcher.exe"
        );
        assert_eq!(
            candidate.runtime_sources[0].path.as_str(),
            "bin/launcher.exe"
        );
        assert!(candidate.runtime_sources[0].executable);
        assert_eq!(candidate.portable_metadata.reference_labels.len(), 4);
        assert_eq!(candidate.portable_metadata.presentation.len(), 11);
        assert_eq!(candidate.portable_metadata.provenance.len(), 3);

        let duplicate = minimal(
            "portable_metadata:\n  reference_labels:\n    - { label: stable, source: { kind: unattributed } }\n    - { label: stable, source: { kind: unattributed } }\n",
        );
        assert!(parse_pack_source_yaml_v1(duplicate.as_bytes()).is_err());
    }

    // Supporting lexical coverage for PR-TEST-0070.
    // Verifies: PR-REQ-0259
    #[test]
    fn source_relative_path_rejects_windows_aliases_and_traversal() {
        for invalid in [
            "",
            "/root",
            "../file",
            "a\\b",
            "a:b",
            "name. ",
            "name.",
            "CON",
            "com1.txt",
            "COM\u{00b9}.txt",
        ] {
            assert!(SourceRelativePathV1::parse(invalid).is_err(), "{invalid:?}");
        }
        assert!(SourceRelativePathV1::parse("hooks/start.ps1").is_ok());
    }
}
