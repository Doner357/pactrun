//! Exact path selection and pure Managed Input transition semantics.
//! No host paths, payload bytes, persistence adapters, or execution authority.

use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MigrationError {
    CrossLineage,
    ZeroEdges,
    RepeatedRevision,
    MissingRevision,
    DuplicateRevision,
    NoPath,
    AmbiguousPath,
    InvalidPathId,
    PathContextMismatch,
    InvalidPageSize,
    InvalidEdge,
    InvalidObservation,
    MissingSource(InputIdentity),
    MissingTarget(InputIdentity),
    MultipleWriters(InputIdentity),
    InvalidOperatorTarget,
    DeclassificationNotAuthorized,
    IllegalProtection(InputIdentity),
    OutputsWithoutHook,
    InvalidCompletion,
    UnsupportedProtocol(FormatVersion),
    ServiceMapping(ServiceMigrationError),
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CrossLineage => f.write_str("Migration must stay within one Package lineage"),
            Self::ZeroEdges => f.write_str("Migration needs at least one edge"),
            Self::RepeatedRevision => f.write_str("Migration path repeats a Revision"),
            Self::MissingRevision => f.write_str("exact Migration Revision is not installed"),
            Self::DuplicateRevision => f.write_str("duplicate exact Revision observation"),
            Self::NoPath => f.write_str("no declared executable Migration path"),
            Self::AmbiguousPath => f.write_str("multiple Migration paths; select a listed path with --path"),
            Self::InvalidPathId => f.write_str("invalid Migration path ID"),
            Self::PathContextMismatch => f.write_str("Migration path ID does not match this source, target, or lineage; list paths again"),
            Self::InvalidPageSize => f.write_str("Migration path page size must be between 1 and 100"),
            Self::InvalidEdge => f.write_str("Migration edge needs Valid exact-source semantics"),
            Self::InvalidObservation => f.write_str("Migration binding observations are inconsistent"),
            Self::MissingSource(id) => write!(f, "Migration requires source Input {}", id.as_str()),
            Self::MissingTarget(id) => write!(f, "Migration requires staged target Input {}", id.as_str()),
            Self::MultipleWriters(id) => write!(f, "Migration target Input {} has multiple writers", id.as_str()),
            Self::InvalidOperatorTarget => f.write_str("operator Input must name a declared target on this path"),
            Self::DeclassificationNotAuthorized => f.write_str("Migration requires explicit declassification authorization"),
            Self::IllegalProtection(id) => write!(f, "illegal Migration protection transition for Input {}", id.as_str()),
            Self::OutputsWithoutHook => f.write_str("Migration outputs require a Hook"),
            Self::InvalidCompletion => f.write_str("Migration completion must submit every declared output once on success, none on failure"),
            Self::UnsupportedProtocol(version) => f.write_str(&VersionDomain::Hook.unsupported_message(*version)),
            Self::ServiceMapping(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for MigrationError {}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MigrationRevision {
    pub(crate) identity: RevisionIdentity,
    pub(crate) content: ValidatedRevisionContent,
}

#[derive(Clone, Debug)]
pub(crate) struct MigrationCompilationObservation {
    pub(crate) instance: InstanceId,
    pub(crate) state_version: InstanceStateVersion,
    pub(crate) active_revision: RevisionIdentity,
    pub(crate) revisions: Vec<MigrationRevision>,
    pub(crate) bindings: Vec<MigrationBinding>,
    pub(crate) service_state: Option<InstanceServiceState>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MigrationCompiledEdge {
    pub(crate) bindings: MigrationEdgePlan,
    pub(crate) runtime: Vec<RuntimeFileV1>,
    pub(crate) launch: Option<CompiledHookLaunch>,
    pub(crate) service: Option<ServiceMigrationEdge>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MigrationExecutionPlan {
    binding_plan: MigrationBindingPlan,
    compiled_edges: Vec<MigrationCompiledEdge>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationRunIdentity {
    path: Vec<RevisionIdentity>,
    authorize_declassification: bool,
}
impl MigrationRunIdentity {
    pub(crate) fn new(
        path: Vec<RevisionIdentity>,
        authorize: bool,
    ) -> Result<Self, MigrationError> {
        MigrationPathId::for_path(&path)?;
        Ok(Self {
            path,
            authorize_declassification: authorize,
        })
    }
    pub(crate) fn path(&self) -> &[RevisionIdentity] {
        &self.path
    }
    pub(crate) fn source(&self) -> &RevisionIdentity {
        &self.path[0]
    }
    pub(crate) fn target(&self) -> &RevisionIdentity {
        self.path.last().expect("nonempty exact path")
    }
    pub(crate) fn authorized(&self) -> bool {
        self.authorize_declassification
    }
    pub(crate) fn edge_count(&self) -> usize {
        self.path.len() - 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationPlanStep {
    EstablishSession,
    LaunchHook,
    AcceptCompletion,
    PublishManagedResult,
    Finalize,
}
impl MigrationPlanStep {
    pub(crate) fn rank(self) -> i64 {
        match self {
            Self::EstablishSession => 0,
            Self::LaunchHook => 1,
            Self::AcceptCompletion => 2,
            Self::PublishManagedResult => 3,
            Self::Finalize => 4,
        }
    }
    pub(crate) fn from_rank(rank: i64) -> Result<Self, MigrationError> {
        match rank {
            0 => Ok(Self::EstablishSession),
            1 => Ok(Self::LaunchHook),
            2 => Ok(Self::AcceptCompletion),
            3 => Ok(Self::PublishManagedResult),
            4 => Ok(Self::Finalize),
            _ => Err(MigrationError::InvalidObservation),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationRunProgress {
    pub(crate) committed_edges: usize,
    pub(crate) step: MigrationPlanStep,
    pub(crate) boundary_revision: RevisionIdentity,
    pub(crate) boundary_state_version: InstanceStateVersion,
}

impl MigrationExecutionPlan {
    pub(crate) fn invocation(&self) -> MigrationRunIdentity {
        let path = std::iter::once(self.edges()[0].bindings.source().clone())
            .chain(self.edges().iter().map(|e| e.bindings.target().clone()))
            .collect();
        MigrationRunIdentity::new(path, self.binding_plan.authorize_declassification)
            .expect("validated compiled path")
    }
    pub(crate) fn operator_inputs(&self) -> BTreeSet<MigrationTargetInput> {
        self.edges()
            .iter()
            .flat_map(|edge| {
                edge.bindings
                    .committed_bindings()
                    .iter()
                    .filter_map(|binding| match &binding.origin {
                        MigrationValueOrigin::Operator(input) => Some(input.clone()),
                        _ => None,
                    })
            })
            .collect()
    }
    pub(crate) fn new(
        binding_plan: MigrationBindingPlan,
        compiled_edges: Vec<MigrationCompiledEdge>,
    ) -> Result<Self, MigrationError> {
        if binding_plan.edges.len() != compiled_edges.len()
            || binding_plan
                .edges
                .iter()
                .zip(&compiled_edges)
                .any(|(edge, compiled)| {
                    edge != &compiled.bindings
                        || edge.declaration.hook.is_some() != compiled.launch.is_some()
                })
        {
            return Err(MigrationError::InvalidObservation);
        }
        Ok(Self {
            binding_plan,
            compiled_edges,
        })
    }
    pub(crate) fn instance(&self) -> InstanceId {
        self.binding_plan.instance()
    }
    pub(crate) fn expected_state_version(&self) -> InstanceStateVersion {
        self.binding_plan.expected_state_version()
    }
    pub(crate) fn edges(&self) -> &[MigrationCompiledEdge] {
        &self.compiled_edges
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MigrationPathSelection {
    Automatic,
    /// Includes both endpoints. An exact direct path is distinct from automatic.
    Exact(Vec<RevisionIdentity>),
}

/// Typed intent, never a raw CLI path ID. Exact paths include both endpoints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TransitionRevision {
    pub(crate) instance: InstanceId,
    pub(crate) expected_state_version: InstanceStateVersion,
    pub(crate) source: RevisionIdentity,
    pub(crate) target: RevisionIdentity,
    pub(crate) path: MigrationPathSelection,
    pub(crate) operator_inputs: Vec<MigrationTargetInput>,
    pub(crate) authorize_declassification: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct MigrationTargetInput {
    pub(crate) revision: RevisionContentDigest,
    pub(crate) input: InputIdentity,
}

/// Content provenance is typed and never includes bytes or host acquisition paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MigrationValueOrigin {
    Existing(ManagedInputPayloadId),
    Operator(MigrationTargetInput),
    Hook(MigrationTargetInput),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationBinding {
    pub(crate) input: InputIdentity,
    pub(crate) origin: MigrationValueOrigin,
    pub(crate) protection: ManagedInputProtection,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MigrationEdgePlan {
    source: RevisionIdentity,
    target: RevisionIdentity,
    declaration: MigrationV1,
    source_bindings: Vec<MigrationBinding>,
    staged_bindings: Vec<MigrationBinding>,
    committed_bindings: Vec<MigrationBinding>,
    required_inputs_satisfied: bool,
}

impl MigrationEdgePlan {
    pub(crate) fn source(&self) -> &RevisionIdentity {
        &self.source
    }
    pub(crate) fn target(&self) -> &RevisionIdentity {
        &self.target
    }
    pub(crate) fn declaration(&self) -> &MigrationV1 {
        &self.declaration
    }
    pub(crate) fn source_bindings(&self) -> &[MigrationBinding] {
        &self.source_bindings
    }
    /// Full registry before Hook output; callers derive active target Session roles.
    pub(crate) fn staged_bindings(&self) -> &[MigrationBinding] {
        &self.staged_bindings
    }
    /// Symbolic successful result; Hook values are conditional on valid completion.
    pub(crate) fn committed_bindings(&self) -> &[MigrationBinding] {
        &self.committed_bindings
    }
    pub(crate) fn required_inputs_satisfied(&self) -> bool {
        self.required_inputs_satisfied
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MigrationBindingPlan {
    instance: InstanceId,
    expected_state_version: InstanceStateVersion,
    edges: Vec<MigrationEdgePlan>,
    authorize_declassification: bool,
}
impl MigrationBindingPlan {
    pub(crate) fn instance(&self) -> InstanceId {
        self.instance
    }
    pub(crate) fn expected_state_version(&self) -> InstanceStateVersion {
        self.expected_state_version
    }
    pub(crate) fn edges(&self) -> &[MigrationEdgePlan] {
        &self.edges
    }
}

/// Build executable edges from intrinsic-valid installed content. A missing
/// source leaves a target installable but cannot establish an executable edge.
pub(crate) fn resolve_migration_path(
    revisions: &[MigrationRevision],
    source: &RevisionIdentity,
    target: &RevisionIdentity,
    intermediates: &[RevisionIdentity],
) -> Result<Vec<RevisionIdentity>, MigrationError> {
    let successors = migration_successors(revisions, source, target)?;
    if !intermediates.is_empty() {
        let path: Vec<_> = std::iter::once(source)
            .chain(intermediates)
            .chain(std::iter::once(target))
            .cloned()
            .collect();
        validate_exact_path(&successors, source, target, &path)?;
        return Ok(path);
    }
    let path = find_path(&successors, source, target, None).ok_or(MigrationError::NoPath)?;
    // A distinct simple path omits a witness edge. This proves uniqueness
    // without enumerating dead-end permutations or choosing a shortest route.
    for pair in path.windows(2) {
        if find_path(&successors, source, target, Some((&pair[0], &pair[1]))).is_some() {
            return Err(MigrationError::AmbiguousPath);
        }
    }
    Ok(path)
}

pub(super) fn migration_successors(
    revisions: &[MigrationRevision],
    source: &RevisionIdentity,
    target: &RevisionIdentity,
) -> Result<BTreeMap<RevisionIdentity, Vec<RevisionIdentity>>, MigrationError> {
    if source.package_id != target.package_id {
        return Err(MigrationError::CrossLineage);
    }
    if source == target {
        return Err(MigrationError::ZeroEdges);
    }
    let nodes = revision_map(revisions)?;
    for id in [source, target] {
        if !nodes.contains_key(id) {
            return Err(MigrationError::MissingRevision);
        }
    }
    let mut successors: BTreeMap<RevisionIdentity, Vec<RevisionIdentity>> = BTreeMap::new();
    for node in revisions
        .iter()
        .filter(|n| n.identity.package_id == source.package_id)
    {
        for edge in node.content.core.migrations() {
            let from = RevisionIdentity::new(
                source.package_id,
                RevisionContentDigest::from_bytes(edge.source_revision_digest.to_bytes()),
            );
            if let Some(previous) = nodes.get(&from)
                && relationally_valid(edge, &previous.content.core, &node.content.core)
            {
                successors
                    .entry(from)
                    .or_default()
                    .push(node.identity.clone());
            }
        }
    }
    for next in successors.values_mut() {
        next.sort();
        next.dedup();
    }
    Ok(successors)
}

pub(super) fn validate_exact_path(
    graph: &BTreeMap<RevisionIdentity, Vec<RevisionIdentity>>,
    source: &RevisionIdentity,
    target: &RevisionIdentity,
    path: &[RevisionIdentity],
) -> Result<(), MigrationError> {
    if path.len() < 2 || path.first() != Some(source) || path.last() != Some(target) {
        return Err(MigrationError::InvalidEdge);
    }
    if path.iter().any(|node| node.package_id != source.package_id) {
        return Err(MigrationError::CrossLineage);
    }
    if path.iter().collect::<BTreeSet<_>>().len() != path.len() {
        return Err(MigrationError::RepeatedRevision);
    }
    if path.windows(2).any(|pair| {
        !graph
            .get(&pair[0])
            .is_some_and(|next| next.contains(&pair[1]))
    }) {
        return Err(MigrationError::InvalidEdge);
    }
    Ok(())
}

fn find_path(
    successors: &BTreeMap<RevisionIdentity, Vec<RevisionIdentity>>,
    source: &RevisionIdentity,
    target: &RevisionIdentity,
    omitted: Option<(&RevisionIdentity, &RevisionIdentity)>,
) -> Option<Vec<RevisionIdentity>> {
    let mut queue = VecDeque::from([source.clone()]);
    let mut previous = BTreeMap::from([(source.clone(), source.clone())]);
    while let Some(current) = queue.pop_front() {
        if &current == target {
            let mut path = vec![current.clone()];
            let mut cursor = &current;
            while cursor != source {
                cursor = previous.get(cursor)?;
                path.push(cursor.clone());
            }
            path.reverse();
            return Some(path);
        }
        for next in successors.get(&current).map(Vec::as_slice).unwrap_or(&[]) {
            if omitted == Some((&current, next)) || previous.contains_key(next) {
                continue;
            }
            previous.insert(next.clone(), current.clone());
            queue.push_back(next.clone());
        }
    }
    None
}

fn revision_map(
    revisions: &[MigrationRevision],
) -> Result<BTreeMap<&RevisionIdentity, &MigrationRevision>, MigrationError> {
    let mut result = BTreeMap::new();
    for revision in revisions {
        if result.insert(&revision.identity, revision).is_some() {
            return Err(MigrationError::DuplicateRevision);
        }
    }
    Ok(result)
}

fn relationally_valid(edge: &MigrationV1, source: &RevisionCore, target: &RevisionCore) -> bool {
    let inputs = edge
        .requires_source
        .iter()
        .chain(edge.transitions.iter().map(MigrationTransitionV1::source))
        .all(|reference| {
            let declared = source.inputs().iter().any(|i| i.id == reference.input_id);
            declared == (reference.role == InputBindingRoleV1::Active)
        });
    inputs
        && target.service_core().is_none_or(|target| {
            validate_service_sources_v2(
                target,
                &BTreeMap::from([(edge.source_revision_digest.clone(), source.service_source())]),
            )
            .is_ok()
        })
}

/// Symbolic preflight is not acceptance, Admission, or successful completion.
pub(crate) fn build_migration_binding_plan(
    intent: &TransitionRevision,
    revisions: &[MigrationRevision],
    observed: &[MigrationBinding],
) -> Result<MigrationBindingPlan, MigrationError> {
    let path = match &intent.path {
        MigrationPathSelection::Automatic => {
            resolve_migration_path(revisions, &intent.source, &intent.target, &[])?
        }
        MigrationPathSelection::Exact(path) => {
            let graph = migration_successors(revisions, &intent.source, &intent.target)?;
            validate_exact_path(&graph, &intent.source, &intent.target, path)?;
            path.clone()
        }
    };
    let nodes = revision_map(revisions)?;
    let mut operators = BTreeSet::new();
    for supplied in &intent.operator_inputs {
        let target = path
            .iter()
            .skip(1)
            .find(|r| r.content_digest == supplied.revision)
            .ok_or(MigrationError::InvalidOperatorTarget)?;
        if !nodes[target]
            .content
            .core
            .inputs()
            .iter()
            .any(|d| d.id == supplied.input)
        {
            return Err(MigrationError::InvalidOperatorTarget);
        }
        if !operators.insert(supplied.clone()) {
            return Err(MigrationError::MultipleWriters(supplied.input.clone()));
        }
    }
    let mut bindings = BTreeMap::new();
    for binding in observed {
        if !matches!(binding.origin, MigrationValueOrigin::Existing(_))
            || bindings
                .insert(binding.input.clone(), binding.clone())
                .is_some()
        {
            return Err(MigrationError::InvalidObservation);
        }
    }
    let mut edges = Vec::new();
    for pair in path.windows(2) {
        let source = nodes[&pair[0]];
        let target = nodes[&pair[1]];
        // Protection on active bindings must include the current declaration.
        promote_declared_protection(&mut bindings, source.content.core.inputs());
        let declaration = target
            .content
            .core
            .migrations()
            .iter()
            .find(|edge| {
                edge.source_revision_digest.to_bytes() == *source.identity.content_digest.as_bytes()
            })
            .ok_or(MigrationError::InvalidEdge)?;
        let edge = evaluate_edge(
            source,
            target,
            declaration,
            &bindings,
            &operators,
            intent.authorize_declassification,
        )?;
        bindings = edge
            .committed_bindings
            .iter()
            .map(|b| (b.input.clone(), b.clone()))
            .collect();
        edges.push(edge);
    }
    Ok(MigrationBindingPlan {
        instance: intent.instance,
        expected_state_version: intent.expected_state_version,
        edges,
        authorize_declassification: intent.authorize_declassification,
    })
}

/// Re-evaluate the already selected immutable edge over the committed registry.
/// This resolves payload references after our own commits, never reselects a path.
pub(crate) fn evaluate_migration_edge(
    source: &MigrationRevision,
    target: &MigrationRevision,
    bindings: &[MigrationBinding],
    operators: &BTreeSet<MigrationTargetInput>,
    authorize: bool,
) -> Result<MigrationEdgePlan, MigrationError> {
    let edge = target
        .content
        .core
        .migrations()
        .iter()
        .find(|e| e.source_revision_digest.to_bytes() == *source.identity.content_digest.as_bytes())
        .ok_or(MigrationError::InvalidEdge)?;
    for input in operators {
        if input.revision != target.identity.content_digest
            || !target
                .content
                .core
                .inputs()
                .iter()
                .any(|d| d.id == input.input)
        {
            return Err(MigrationError::InvalidOperatorTarget);
        }
    }
    let mut registry = BTreeMap::new();
    for b in bindings {
        if registry.insert(b.input.clone(), b.clone()).is_some() {
            return Err(MigrationError::InvalidObservation);
        }
    }
    promote_declared_protection(&mut registry, source.content.core.inputs());
    evaluate_edge(source, target, edge, &registry, operators, authorize)
}

fn declared_protection(input: &InputDeclarationV1) -> ManagedInputProtection {
    match input.protection {
        InputProtectionV1::Normal => ManagedInputProtection::Normal,
        InputProtectionV1::Secret => ManagedInputProtection::Secret,
    }
}

fn promote_declared_protection(
    bindings: &mut BTreeMap<InputIdentity, MigrationBinding>,
    inputs: &[InputDeclarationV1],
) {
    for input in inputs {
        if let Some(binding) = bindings.get_mut(&input.id) {
            binding.protection = binding.protection.sticky(declared_protection(input));
        }
    }
}

fn reserve(
    writers: &mut BTreeSet<InputIdentity>,
    input: &InputIdentity,
) -> Result<(), MigrationError> {
    if !writers.insert(input.clone()) {
        return Err(MigrationError::MultipleWriters(input.clone()));
    }
    Ok(())
}

fn evaluate_edge(
    source: &MigrationRevision,
    target: &MigrationRevision,
    edge: &MigrationV1,
    bindings: &BTreeMap<InputIdentity, MigrationBinding>,
    operators: &BTreeSet<MigrationTargetInput>,
    authorize: bool,
) -> Result<MigrationEdgePlan, MigrationError> {
    if !relationally_valid(edge, &source.content.core, &target.content.core) {
        return Err(MigrationError::InvalidEdge);
    }
    if let Some(hook) = edge.hook.as_ref()
        && !VersionDomain::Hook.supports(hook.protocol_version)
    {
        return Err(MigrationError::UnsupportedProtocol(hook.protocol_version));
    }
    if !edge.produces_target.is_empty() && edge.hook.is_none() {
        return Err(MigrationError::OutputsWithoutHook);
    }
    for required in &edge.requires_source {
        if !bindings.contains_key(&required.input_id) {
            return Err(MigrationError::MissingSource(required.input_id.clone()));
        }
    }
    let targets: BTreeMap<_, _> = target
        .content
        .core
        .inputs()
        .iter()
        .map(|i| (&i.id, i))
        .collect();
    let mut next = bindings.clone();
    let mut writers = BTreeSet::new();
    // Remove consumed sources first, so swaps are simultaneous and order-free.
    for transition in &edge.transitions {
        if !matches!(transition, MigrationTransitionV1::Keep { .. }) {
            next.remove(&transition.source().input_id);
        }
    }
    // Unconsumed continuity/reactivation is a writer, not an overwrite fallback.
    for input in next.keys().filter(|input| targets.contains_key(input)) {
        reserve(&mut writers, input)?;
    }
    for transition in &edge.transitions {
        let Some(id) = transition.target() else {
            continue;
        };
        reserve(&mut writers, id)?;
        let target_input = targets.get(id).ok_or(MigrationError::InvalidEdge)?;
        let declassify = matches!(transition, MigrationTransitionV1::Declassify { .. });
        if declassify && !authorize {
            return Err(MigrationError::DeclassificationNotAuthorized);
        }
        if declassify && declared_protection(target_input) != ManagedInputProtection::Normal {
            return Err(MigrationError::IllegalProtection(id.clone()));
        }
        if let Some(value) = bindings.get(&transition.source().input_id) {
            let protection = declared_protection(target_input);
            if declassify && value.protection != ManagedInputProtection::Secret {
                return Err(MigrationError::IllegalProtection(id.clone()));
            }
            let mut value = value.clone();
            value.input = id.clone();
            // Normal is a declaration floor, not permission to erase sticky
            // protection. Carry into a Normal declaration remains Secret when
            // the source is Secret (PR-REQ-0034); only Declassify lowers it.
            value.protection = if declassify {
                protection
            } else {
                value.protection.sticky(protection)
            };
            next.insert(id.clone(), value);
        }
    }
    for supplied in operators
        .iter()
        .filter(|o| o.revision == target.identity.content_digest)
    {
        reserve(&mut writers, &supplied.input)?;
        next.insert(
            supplied.input.clone(),
            MigrationBinding {
                input: supplied.input.clone(),
                origin: MigrationValueOrigin::Operator(supplied.clone()),
                protection: declared_protection(targets[&supplied.input]),
            },
        );
    }
    for output in &edge.produces_target {
        reserve(&mut writers, output)?;
    }
    promote_declared_protection(&mut next, target.content.core.inputs());
    for required in &edge.requires_target {
        if !next.contains_key(required) {
            return Err(MigrationError::MissingTarget(required.clone()));
        }
    }
    let staged_bindings = next.values().cloned().collect();
    for output in &edge.produces_target {
        next.insert(
            output.clone(),
            MigrationBinding {
                input: output.clone(),
                origin: MigrationValueOrigin::Hook(MigrationTargetInput {
                    revision: target.identity.content_digest,
                    input: output.clone(),
                }),
                protection: declared_protection(targets[output]),
            },
        );
    }
    let ready = target
        .content
        .core
        .inputs()
        .iter()
        .filter(|i| i.required)
        .all(|i| next.contains_key(&i.id));
    Ok(MigrationEdgePlan {
        source: source.identity.clone(),
        target: target.identity.clone(),
        declaration: edge.clone(),
        source_bindings: bindings.values().cloned().collect(),
        staged_bindings,
        committed_bindings: next.into_values().collect(),
        required_inputs_satisfied: ready,
    })
}

pub(crate) fn validate_migration_completion(
    edge: &MigrationV1,
    success: bool,
    submitted: &[InputIdentity],
) -> Result<(), MigrationError> {
    let unique: BTreeSet<_> = submitted.iter().collect();
    if (!success && !submitted.is_empty())
        || unique.len() != submitted.len()
        || (success && unique != edge.produces_target.iter().collect())
    {
        return Err(MigrationError::InvalidCompletion);
    }
    Ok(())
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
