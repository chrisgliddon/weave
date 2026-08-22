use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use tempfile::NamedTempFile;
use weave_character::{
    AlignmentConfig, AlignmentPack, AlignmentProposal, AlignmentReceipt, AlignmentReview,
    AlignmentReviewDecision, CharacterAuthoringRevision, CharacterAuthoringWorkspace,
    CharacterCollection, CharacterFinalReview, CharacterFinalReviewDecision, CharacterJobProgress,
    CharacterOperationRequest, CharacterOverlay, CharacterProfile, CharacterProposal,
    CharacterProposalReview, CharacterQuestionnaireAnswers, CharacterQuestionnaireConflictDecision,
    CharacterQuestionnaireFacetDecision, CharacterQuestionnairePack,
    CharacterQuestionnaireProposal, CharacterQuestionnaireReceipt, CharacterQuestionnaireReview,
    CharacterReviewDecision, CharacterScope, CharacterSynthesisResult, CharacterTemplate,
    HexacoTrait, PresentationAllocationRequest, PresentationCatalog, PresentationLockRevision,
    PresentationProposal, PresentationReceipt, PresentationReview, PresentationReviewDecision,
    TemporalContextConfig, TemporalContextPack, TemporalContextProposal, TemporalContextReceipt,
    TemporalContextReview, TemporalReviewDecision, alignment_config_schema, alignment_pack_schema,
    alignment_profile_fingerprint, alignment_proposal_schema, alignment_receipt_schema,
    alignment_review_schema, apply_authoring_revision, apply_character_questionnaire_review,
    apply_presentation_lock_revision, apply_presentation_review, apply_reviewed_alignment,
    apply_reviewed_character_proposal, apply_reviewed_temporal_context,
    character_authoring_preview_schema, character_authoring_revision_schema,
    character_authoring_workspace_schema, character_collection_schema, character_diagnostic_schema,
    character_domain_pack, character_final_review_schema, character_module_manifest,
    character_operation_request_schema, character_overlay_schema, character_profile_schema,
    character_progress_schema, character_proposal_schema, character_questionnaire_answers_schema,
    character_questionnaire_pack_schema, character_questionnaire_proposal_schema,
    character_questionnaire_receipt_schema, character_questionnaire_review_schema,
    character_review_schema, character_synthesis_schema, character_template_schema,
    clone_authoring_draft, collection_fingerprint, create_alignment_review, create_authoring_draft,
    create_character_questionnaire_review, create_temporal_context_review,
    export_authoring_profile, list_authoring_drafts, list_characters, new_authoring_workspace,
    presentation_allocation_request_schema, presentation_authoring_revision,
    presentation_catalog_schema, presentation_lock_revision_schema, presentation_proposal_schema,
    presentation_receipt_schema, presentation_review_schema, preview_authoring_revision,
    propose_alignment, propose_character_operation, propose_character_questionnaire,
    propose_presentation_allocations, propose_temporal_context, questionnaire_authoring_revision,
    resume_character_operation, review_authoring_draft, review_character_proposal,
    review_presentation_proposal, show_authoring_draft, show_character, synthesize_character,
    temporal_context_config_schema, temporal_context_pack_schema, temporal_context_proposal_schema,
    temporal_context_receipt_schema, temporal_context_review_schema, temporal_profile_fingerprint,
};

#[derive(Debug, Parser)]
#[command(
    name = "weave-character",
    about = "Validate and synthesize portable Weave Character profiles"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write one canonical Character JSON Schema.
    Schema {
        #[arg(value_enum)]
        kind: DocumentKind,
        #[arg(long)]
        output: PathBuf,
    },
    /// Strictly parse and validate one Character document.
    Validate {
        #[arg(value_enum)]
        kind: DocumentKind,
        input: PathBuf,
    },
    /// Apply one sparse overlay over an optional immutable template.
    Synthesize {
        overlay: PathBuf,
        #[arg(long)]
        template: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Write the canonical declarative Weave Character module manifest.
    ModuleManifest {
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate a profile and project it into one immutable domain pack.
    DomainPack {
        profile: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        title: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// List deterministic summaries from one validated Character collection.
    CollectionList {
        collection: PathBuf,
        /// Select one or more exact stable ids.
        #[arg(long = "id")]
        ids: Vec<String>,
        /// Select stable ids with this prefix.
        #[arg(long)]
        id_prefix: Option<String>,
        /// Select profiles containing this extension namespace.
        #[arg(long)]
        extension_namespace: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show one exact profile from a validated Character collection.
    CollectionShow {
        collection: PathBuf,
        id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Produce a deterministic review proposal without changing the collection.
    CollectionPropose {
        collection: PathBuf,
        request: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Advance a payload-free resumable collection operation.
    CollectionResume {
        collection: PathBuf,
        request: PathBuf,
        #[arg(long)]
        progress: Option<PathBuf>,
        #[arg(long, default_value_t = 100)]
        max_items: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        progress_output: PathBuf,
        #[arg(long)]
        proposal_output: Option<PathBuf>,
    },
    /// Record a whole-proposal review without changing the collection.
    CollectionReview {
        proposal: PathBuf,
        #[arg(long, value_enum)]
        decision: ReviewDecision,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Independently reproduce and atomically apply one accepted whole proposal.
    CollectionApply {
        collection: PathBuf,
        proposal: PathBuf,
        review: PathBuf,
        /// Validate and reproduce the complete apply without writing any file.
        #[arg(long)]
        dry_run: bool,
        /// Destination collection; defaults to atomically replacing the input collection.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Output encoding; defaults to the destination file extension.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Rank temporal authoring cues from a profile and exact offline packs.
    ContextPropose {
        profile: PathBuf,
        #[arg(long = "pack", required = true)]
        packs: Vec<PathBuf>,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        seed: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Create one complete temporal review from a candidate-decision map.
    ContextReview {
        proposal: PathBuf,
        decisions: PathBuf,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Reproduce and atomically apply reviewed temporal context.
    ContextApply {
        profile: PathBuf,
        #[arg(long = "pack", required = true)]
        packs: Vec<PathBuf>,
        proposal: PathBuf,
        review: PathBuf,
        /// Reproduce the complete receipt without writing any file.
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Score one exact declarative alignment pack without changing the profile.
    AlignmentPropose {
        profile: PathBuf,
        #[arg(long)]
        pack: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        seed: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Create one complete reviewed alignment manifest from explicit axis decisions.
    AlignmentReview {
        proposal: PathBuf,
        #[arg(long)]
        pack: PathBuf,
        decisions: PathBuf,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Reproduce and atomically apply one fully reviewed alignment manifest.
    AlignmentApply {
        profile: PathBuf,
        #[arg(long)]
        pack: PathBuf,
        proposal: PathBuf,
        review: PathBuf,
        /// Reproduce the complete receipt without writing any file.
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Initialize an empty guided-authoring workspace.
    AuthoringInit {
        #[arg(long)]
        id: String,
        #[arg(long)]
        provenance: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Create a draft from a blank or exact versioned template overlay.
    AuthoringCreate {
        workspace: PathBuf,
        overlay: PathBuf,
        #[arg(long)]
        template: Option<PathBuf>,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// List deterministic guided-authoring drafts.
    AuthoringList {
        workspace: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show one exact retained draft source document.
    AuthoringShow {
        workspace: PathBuf,
        id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Clone a reference-safe draft under another stable character id.
    AuthoringClone {
        workspace: PathBuf,
        source_id: String,
        #[arg(long)]
        new_character_id: String,
        #[arg(long)]
        new_overlay_id: String,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Preview base/effective differences, migration effects, and invalidations.
    AuthoringPreview {
        workspace: PathBuf,
        revision: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Apply one already-previewable revision atomically.
    AuthoringRevise {
        workspace: PathBuf,
        revision: PathBuf,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Validate and independently reopen a workspace without rewriting it.
    AuthoringValidate { workspace: PathBuf },
    /// Save a complete final authoring review.
    AuthoringReview {
        workspace: PathBuf,
        id: String,
        #[arg(long, value_enum)]
        decision: FinalReviewDecision,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Export one validated effective profile.
    AuthoringExport {
        workspace: PathBuf,
        id: String,
        #[arg(long)]
        allow_unreviewed: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Parse, validate, and canonically rewrite a workspace to prove reopen parity.
    AuthoringReopen {
        workspace: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Produce a balanced deterministic presentation-catalog dry-run.
    PresentationPropose {
        collection: PathBuf,
        catalog: PathBuf,
        request: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Create a complete per-allocation review from a decision map.
    PresentationReview {
        proposal: PathBuf,
        decisions: PathBuf,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Replay a reviewed proposal and atomically apply its output collection.
    PresentationApply {
        collection: PathBuf,
        proposal: PathBuf,
        review: PathBuf,
        /// Produce the exact receipt without changing the collection file.
        #[arg(long)]
        dry_run: bool,
        /// Receipt destination, always written after successful replay.
        #[arg(long)]
        receipt_output: PathBuf,
        /// Collection destination; defaults to atomically replacing the input.
        #[arg(long)]
        collection_output: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },
    /// Deterministically lock or unlock exact presentation assignments.
    PresentationLock {
        collection: PathBuf,
        revision: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Convert one reviewed presentation receipt into sparse authoring revision source.
    PresentationRevision {
        workspace: PathBuf,
        draft_id: String,
        receipt: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Score one exact original narrative-questionnaire pack.
    QuestionnairePropose {
        profile: PathBuf,
        pack: PathBuf,
        answers: PathBuf,
        #[arg(long)]
        seed: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Create a complete questionnaire review from facet and conflict decision maps.
    QuestionnaireReview {
        proposal: PathBuf,
        facet_decisions: PathBuf,
        conflict_decisions: PathBuf,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Reproduce and apply one complete questionnaire review.
    QuestionnaireApply {
        proposal: PathBuf,
        review: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
    /// Convert one questionnaire receipt into the exact workspace revision source document.
    QuestionnaireRevision {
        workspace: PathBuf,
        draft_id: String,
        receipt: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        rationale: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DocumentKind {
    Profile,
    Template,
    Overlay,
    Synthesis,
    Diagnostic,
    Collection,
    Request,
    Proposal,
    Review,
    Progress,
    TemporalPack,
    TemporalConfig,
    TemporalProposal,
    TemporalReview,
    TemporalReceipt,
    AlignmentPack,
    AlignmentConfig,
    AlignmentProposal,
    AlignmentReview,
    AlignmentReceipt,
    AuthoringWorkspace,
    AuthoringRevision,
    AuthoringPreview,
    PresentationCatalog,
    PresentationRequest,
    PresentationProposal,
    PresentationReview,
    PresentationReceipt,
    PresentationLockRevision,
    QuestionnairePack,
    QuestionnaireAnswers,
    QuestionnaireProposal,
    QuestionnaireReview,
    QuestionnaireReceipt,
    FinalReview,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Ron,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReviewDecision {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FinalReviewDecision {
    Accepted,
    NeedsChanges,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Schema { kind, output } => {
            let schema = match kind {
                DocumentKind::Profile => character_profile_schema()?,
                DocumentKind::Template => character_template_schema()?,
                DocumentKind::Overlay => character_overlay_schema()?,
                DocumentKind::Synthesis => character_synthesis_schema()?,
                DocumentKind::Diagnostic => character_diagnostic_schema()?,
                DocumentKind::Collection => character_collection_schema()?,
                DocumentKind::Request => character_operation_request_schema()?,
                DocumentKind::Proposal => character_proposal_schema()?,
                DocumentKind::Review => character_review_schema()?,
                DocumentKind::Progress => character_progress_schema()?,
                DocumentKind::TemporalPack => temporal_context_pack_schema()?,
                DocumentKind::TemporalConfig => temporal_context_config_schema()?,
                DocumentKind::TemporalProposal => temporal_context_proposal_schema()?,
                DocumentKind::TemporalReview => temporal_context_review_schema()?,
                DocumentKind::TemporalReceipt => temporal_context_receipt_schema()?,
                DocumentKind::AlignmentPack => alignment_pack_schema()?,
                DocumentKind::AlignmentConfig => alignment_config_schema()?,
                DocumentKind::AlignmentProposal => alignment_proposal_schema()?,
                DocumentKind::AlignmentReview => alignment_review_schema()?,
                DocumentKind::AlignmentReceipt => alignment_receipt_schema()?,
                DocumentKind::AuthoringWorkspace => character_authoring_workspace_schema()?,
                DocumentKind::AuthoringRevision => character_authoring_revision_schema()?,
                DocumentKind::AuthoringPreview => character_authoring_preview_schema()?,
                DocumentKind::PresentationCatalog => presentation_catalog_schema()?,
                DocumentKind::PresentationRequest => presentation_allocation_request_schema()?,
                DocumentKind::PresentationProposal => presentation_proposal_schema()?,
                DocumentKind::PresentationReview => presentation_review_schema()?,
                DocumentKind::PresentationReceipt => presentation_receipt_schema()?,
                DocumentKind::PresentationLockRevision => presentation_lock_revision_schema()?,
                DocumentKind::QuestionnairePack => character_questionnaire_pack_schema()?,
                DocumentKind::QuestionnaireAnswers => character_questionnaire_answers_schema()?,
                DocumentKind::QuestionnaireProposal => character_questionnaire_proposal_schema()?,
                DocumentKind::QuestionnaireReview => character_questionnaire_review_schema()?,
                DocumentKind::QuestionnaireReceipt => character_questionnaire_receipt_schema()?,
                DocumentKind::FinalReview => character_final_review_schema()?,
            };
            atomic_write(&output, schema.as_bytes())?;
        }
        Command::Validate { kind, input } => {
            let source = fs::read_to_string(&input)?;
            match kind {
                DocumentKind::Profile => {
                    parse_profile(&input, &source)?;
                }
                DocumentKind::Template => {
                    parse_template(&input, &source)?;
                }
                DocumentKind::Overlay => {
                    parse_overlay(&input, &source)?;
                }
                DocumentKind::Synthesis => {
                    parse_synthesis(&input, &source)?;
                }
                DocumentKind::Diagnostic => {
                    return Err(
                        "diagnostic documents have a schema but no standalone validator".into(),
                    );
                }
                DocumentKind::Collection => {
                    parse_collection(&input, &source)?;
                }
                DocumentKind::Request => {
                    parse_request(&input, &source)?;
                }
                DocumentKind::Proposal => {
                    parse_proposal(&input, &source)?;
                }
                DocumentKind::Review => {
                    parse_review(&input, &source)?;
                }
                DocumentKind::Progress => {
                    parse_progress(&input, &source)?;
                }
                DocumentKind::TemporalPack => {
                    parse_temporal_pack(&input, &source)?;
                }
                DocumentKind::TemporalConfig => {
                    parse_temporal_config(&input, &source)?;
                }
                DocumentKind::TemporalProposal => {
                    parse_temporal_proposal(&input, &source)?;
                }
                DocumentKind::TemporalReview => {
                    parse_temporal_review(&input, &source)?;
                }
                DocumentKind::TemporalReceipt => {
                    parse_temporal_receipt(&input, &source)?;
                }
                DocumentKind::AlignmentPack => {
                    parse_alignment_pack(&input, &source)?;
                }
                DocumentKind::AlignmentConfig => {
                    parse_alignment_config(&input, &source)?;
                }
                DocumentKind::AlignmentProposal => {
                    parse_alignment_proposal(&input, &source)?;
                }
                DocumentKind::AlignmentReview => {
                    parse_alignment_review(&input, &source)?;
                }
                DocumentKind::AlignmentReceipt => {
                    parse_alignment_receipt(&input, &source)?;
                }
                DocumentKind::AuthoringWorkspace => {
                    parse_authoring_workspace(&input, &source)?;
                }
                DocumentKind::AuthoringRevision => {
                    parse_authoring_revision(&input, &source)?;
                }
                DocumentKind::AuthoringPreview => {
                    return Err(
                        "authoring previews are generated receipts without a standalone state validator"
                            .into(),
                    );
                }
                DocumentKind::PresentationCatalog => {
                    parse_presentation_catalog(&input, &source)?;
                }
                DocumentKind::PresentationRequest => {
                    parse_presentation_request(&input, &source)?;
                }
                DocumentKind::PresentationProposal => {
                    parse_presentation_proposal(&input, &source)?;
                }
                DocumentKind::PresentationReview => {
                    parse_presentation_review(&input, &source)?;
                }
                DocumentKind::PresentationReceipt => {
                    parse_presentation_receipt(&input, &source)?;
                }
                DocumentKind::PresentationLockRevision => {
                    parse_presentation_lock_revision(&input, &source)?;
                }
                DocumentKind::QuestionnairePack => {
                    parse_questionnaire_pack(&input, &source)?;
                }
                DocumentKind::QuestionnaireAnswers => {
                    parse_questionnaire_answers(&input, &source)?;
                }
                DocumentKind::QuestionnaireProposal => {
                    parse_questionnaire_proposal(&input, &source)?;
                }
                DocumentKind::QuestionnaireReview => {
                    parse_questionnaire_review(&input, &source)?;
                }
                DocumentKind::QuestionnaireReceipt => {
                    parse_questionnaire_receipt(&input, &source)?;
                }
                DocumentKind::FinalReview => {
                    parse_final_review(&input, &source)?;
                }
            }
            println!("validated {}", input.display());
        }
        Command::Synthesize {
            overlay,
            template,
            format,
            output,
        } => {
            let overlay_source = fs::read_to_string(&overlay)?;
            let overlay = parse_overlay(&overlay, &overlay_source)?;
            let template = template
                .as_ref()
                .map(|path| {
                    fs::read_to_string(path)
                        .map_err(Box::<dyn std::error::Error>::from)
                        .and_then(|source| {
                            parse_template(path, &source)
                                .map_err(Box::<dyn std::error::Error>::from)
                        })
                })
                .transpose()?;
            let result = synthesize_character(template.as_ref(), &overlay)?;
            let serialized = match format {
                OutputFormat::Json => result.to_json()?,
                OutputFormat::Ron => result.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("synthesized {}", output.display());
        }
        Command::ModuleManifest { format, output } => {
            let manifest = character_module_manifest()?;
            let serialized = match format {
                OutputFormat::Json => manifest.to_json()?,
                OutputFormat::Ron => manifest.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character module manifest {}", output.display());
        }
        Command::DomainPack {
            profile,
            id,
            version,
            title,
            format,
            output,
        } => {
            let profile_source = fs::read_to_string(&profile)?;
            let profile = parse_profile(&profile, &profile_source)?;
            let pack = character_domain_pack(&profile, id, version, title)?;
            let serialized = match format {
                OutputFormat::Json => pack.to_json()?,
                OutputFormat::Ron => pack.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character domain pack {}", output.display());
        }
        Command::CollectionList {
            collection,
            ids,
            id_prefix,
            extension_namespace,
            format,
            output,
        } => {
            let collection_value = read_collection(&collection)?;
            let scope = collection_scope(ids, id_prefix, extension_namespace)?;
            let summaries = list_characters(&collection_value, &scope)?;
            let serialized = match format {
                OutputFormat::Json => weave_domain::to_pretty_json(&summaries)?,
                OutputFormat::Ron => weave_domain::to_pretty_ron(&summaries)?,
            };
            write_or_print(output.as_deref(), &serialized)?;
        }
        Command::CollectionShow {
            collection,
            id,
            format,
            output,
        } => {
            let collection = read_collection(&collection)?;
            let profile = show_character(&collection, &id)?;
            let serialized = match format {
                OutputFormat::Json => profile.to_json()?,
                OutputFormat::Ron => profile.to_ron()?,
            };
            write_or_print(output.as_deref(), &serialized)?;
        }
        Command::CollectionPropose {
            collection,
            request,
            format,
            output,
        } => {
            let collection = read_collection(&collection)?;
            let request = read_request(&request)?;
            let proposal = propose_character_operation(&collection, &request)?;
            let serialized = serialize_proposal(&proposal, format)?;
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character proposal {}", output.display());
        }
        Command::CollectionResume {
            collection,
            request,
            progress,
            max_items,
            format,
            progress_output,
            proposal_output,
        } => {
            let collection = read_collection(&collection)?;
            let request = read_request(&request)?;
            let progress = progress
                .as_ref()
                .map(|path| read_progress(path))
                .transpose()?;
            let step =
                resume_character_operation(&collection, &request, progress.as_ref(), max_items)?;
            if step.proposal.is_some() && proposal_output.is_none() {
                return Err(
                    "completed resume step requires --proposal-output for its proposal".into(),
                );
            }
            let serialized_progress = match format {
                OutputFormat::Json => step.progress.to_json()?,
                OutputFormat::Ron => step.progress.to_ron()?,
            };
            let serialized_proposal = step
                .proposal
                .as_ref()
                .map(|proposal| serialize_proposal(proposal, format))
                .transpose()?;
            atomic_write(&progress_output, serialized_progress.as_bytes())?;
            if let (Some(path), Some(serialized)) =
                (proposal_output.as_ref(), serialized_proposal.as_ref())
            {
                atomic_write(path, serialized.as_bytes())?;
            }
            println!("wrote Character progress {}", progress_output.display());
        }
        Command::CollectionReview {
            proposal,
            decision,
            reviewer,
            rationale,
            format,
            output,
        } => {
            let proposal = read_proposal(&proposal)?;
            let decision = match decision {
                ReviewDecision::Accepted => CharacterReviewDecision::Accepted,
                ReviewDecision::Rejected => CharacterReviewDecision::Rejected,
            };
            let review = review_character_proposal(&proposal, decision, reviewer, rationale)?;
            let serialized = match format {
                OutputFormat::Json => review.to_json()?,
                OutputFormat::Ron => review.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character review {}", output.display());
        }
        Command::CollectionApply {
            collection,
            proposal,
            review,
            dry_run,
            output,
            format,
        } => {
            let collection_value = read_collection(&collection)?;
            let proposal = read_proposal(&proposal)?;
            let review = read_review(&review)?;
            let applied = apply_reviewed_character_proposal(&collection_value, &proposal, &review)?;
            if dry_run {
                println!(
                    "validated Character apply {}",
                    collection_fingerprint(&applied)?
                );
            } else {
                let destination = output.as_deref().unwrap_or(&collection);
                let format = format.unwrap_or_else(|| output_format_for(destination));
                let serialized = match format {
                    OutputFormat::Json => applied.to_json()?,
                    OutputFormat::Ron => applied.to_ron()?,
                };
                atomic_write(destination, serialized.as_bytes())?;
                println!("applied Character proposal {}", destination.display());
            }
        }
        Command::ContextPropose {
            profile,
            packs,
            config,
            seed,
            format,
            output,
        } => {
            let profile_value = parse_profile(&profile, &fs::read_to_string(&profile)?)?;
            let pack_values = read_temporal_packs(&packs)?;
            let config_value = parse_temporal_config(&config, &fs::read_to_string(&config)?)?;
            let proposal =
                propose_temporal_context(&profile_value, &pack_values, &config_value, seed)?;
            let serialized = match format {
                OutputFormat::Json => proposal.to_json()?,
                OutputFormat::Ron => proposal.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote temporal context proposal {}", output.display());
        }
        Command::ContextReview {
            proposal,
            decisions,
            reviewer,
            rationale,
            format,
            output,
        } => {
            let proposal_value =
                parse_temporal_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let decision_values = read_temporal_decisions(&decisions)?;
            let review = create_temporal_context_review(
                &proposal_value,
                reviewer,
                rationale,
                decision_values,
            )?;
            let serialized = match format {
                OutputFormat::Json => review.to_json()?,
                OutputFormat::Ron => review.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote temporal context review {}", output.display());
        }
        Command::ContextApply {
            profile,
            packs,
            proposal,
            review,
            dry_run,
            format,
            output,
        } => {
            let profile_value = parse_profile(&profile, &fs::read_to_string(&profile)?)?;
            let pack_values = read_temporal_packs(&packs)?;
            let proposal_value =
                parse_temporal_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let review_value = parse_temporal_review(&review, &fs::read_to_string(&review)?)?;
            let receipt = apply_reviewed_temporal_context(
                &profile_value,
                &pack_values,
                &proposal_value,
                &review_value,
            )?;
            if dry_run {
                println!(
                    "validated temporal context apply {}",
                    temporal_profile_fingerprint(&receipt.output_profile)?
                );
            } else {
                let destination = output
                    .as_deref()
                    .ok_or("context apply requires --output unless --dry-run is set")?;
                let serialized = match format {
                    OutputFormat::Json => receipt.to_json()?,
                    OutputFormat::Ron => receipt.to_ron()?,
                };
                atomic_write(destination, serialized.as_bytes())?;
                println!("applied temporal context {}", destination.display());
            }
        }
        Command::AlignmentPropose {
            profile,
            pack,
            config,
            seed,
            format,
            output,
        } => {
            let profile_value = parse_profile(&profile, &fs::read_to_string(&profile)?)?;
            let pack_value = parse_alignment_pack(&pack, &fs::read_to_string(&pack)?)?;
            let config_value = parse_alignment_config(&config, &fs::read_to_string(&config)?)?;
            let proposal = propose_alignment(&profile_value, &pack_value, &config_value, seed)?;
            let serialized = match format {
                OutputFormat::Json => proposal.to_json()?,
                OutputFormat::Ron => proposal.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote alignment proposal {}", output.display());
        }
        Command::AlignmentReview {
            proposal,
            pack,
            decisions,
            reviewer,
            rationale,
            format,
            output,
        } => {
            let proposal_value =
                parse_alignment_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let pack_value = parse_alignment_pack(&pack, &fs::read_to_string(&pack)?)?;
            let decision_values = read_alignment_decisions(&decisions)?;
            let review = create_alignment_review(
                &proposal_value,
                &pack_value,
                reviewer,
                rationale,
                decision_values,
            )?;
            let serialized = match format {
                OutputFormat::Json => review.to_json()?,
                OutputFormat::Ron => review.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote alignment review {}", output.display());
        }
        Command::AlignmentApply {
            profile,
            pack,
            proposal,
            review,
            dry_run,
            format,
            output,
        } => {
            let profile_value = parse_profile(&profile, &fs::read_to_string(&profile)?)?;
            let pack_value = parse_alignment_pack(&pack, &fs::read_to_string(&pack)?)?;
            let proposal_value =
                parse_alignment_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let review_value = parse_alignment_review(&review, &fs::read_to_string(&review)?)?;
            let receipt = apply_reviewed_alignment(
                &profile_value,
                &pack_value,
                &proposal_value,
                &review_value,
            )?;
            if dry_run {
                println!(
                    "validated alignment apply {}",
                    alignment_profile_fingerprint(&receipt.output_profile)?
                );
            } else {
                let destination = output
                    .as_deref()
                    .ok_or("alignment apply requires --output unless --dry-run is set")?;
                let serialized = match format {
                    OutputFormat::Json => receipt.to_json()?,
                    OutputFormat::Ron => receipt.to_ron()?,
                };
                atomic_write(destination, serialized.as_bytes())?;
                println!("applied alignment view {}", destination.display());
            }
        }
        Command::AuthoringInit {
            id,
            provenance,
            format,
            output,
        } => {
            let provenance = read_provenance(&provenance)?;
            let workspace = new_authoring_workspace(id, provenance)?;
            let serialized = serialize_authoring_workspace(&workspace, format)?;
            atomic_write(&output, serialized.as_bytes())?;
            println!(
                "initialized Character authoring workspace {}",
                output.display()
            );
        }
        Command::AuthoringCreate {
            workspace,
            overlay,
            template,
            format,
            output,
        } => {
            let workspace_value = read_authoring_workspace(&workspace)?;
            let overlay_value = parse_overlay(&overlay, &fs::read_to_string(&overlay)?)?;
            let template_value = match template.as_ref() {
                Some(path) => Some(parse_template(path, &fs::read_to_string(path)?)?),
                None => None,
            };
            let created = create_authoring_draft(&workspace_value, template_value, overlay_value)?;
            let destination = output.as_deref().unwrap_or(&workspace);
            let format = format.unwrap_or_else(|| output_format_for(destination));
            let serialized = serialize_authoring_workspace(&created, format)?;
            atomic_write(destination, serialized.as_bytes())?;
            println!(
                "created Character authoring draft {}",
                destination.display()
            );
        }
        Command::AuthoringList {
            workspace,
            format,
            output,
        } => {
            let workspace = read_authoring_workspace(&workspace)?;
            let summaries = list_authoring_drafts(&workspace)?;
            let serialized = serialize_value(&summaries, format)?;
            write_or_print(output.as_deref(), &serialized)?;
        }
        Command::AuthoringShow {
            workspace,
            id,
            format,
            output,
        } => {
            let workspace = read_authoring_workspace(&workspace)?;
            let draft = show_authoring_draft(&workspace, &id)?;
            let serialized = serialize_value(draft, format)?;
            write_or_print(output.as_deref(), &serialized)?;
        }
        Command::AuthoringClone {
            workspace,
            source_id,
            new_character_id,
            new_overlay_id,
            format,
            output,
        } => {
            let workspace_value = read_authoring_workspace(&workspace)?;
            let cloned = clone_authoring_draft(
                &workspace_value,
                &source_id,
                new_character_id,
                new_overlay_id,
            )?;
            let destination = output.as_deref().unwrap_or(&workspace);
            let format = format.unwrap_or_else(|| output_format_for(destination));
            let serialized = serialize_authoring_workspace(&cloned, format)?;
            atomic_write(destination, serialized.as_bytes())?;
            println!("cloned Character authoring draft {}", destination.display());
        }
        Command::AuthoringPreview {
            workspace,
            revision,
            format,
            output,
        } => {
            let workspace = read_authoring_workspace(&workspace)?;
            let revision = read_authoring_revision(&revision)?;
            let preview = preview_authoring_revision(&workspace, &revision)?;
            let serialized = match format {
                OutputFormat::Json => preview.to_json()?,
                OutputFormat::Ron => preview.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character authoring preview {}", output.display());
        }
        Command::AuthoringRevise {
            workspace,
            revision,
            format,
            output,
        } => {
            let workspace_value = read_authoring_workspace(&workspace)?;
            let revision = read_authoring_revision(&revision)?;
            let revised = apply_authoring_revision(&workspace_value, &revision)?;
            let destination = output.as_deref().unwrap_or(&workspace);
            let format = format.unwrap_or_else(|| output_format_for(destination));
            let serialized = serialize_authoring_workspace(&revised, format)?;
            atomic_write(destination, serialized.as_bytes())?;
            println!(
                "revised Character authoring draft {}",
                destination.display()
            );
        }
        Command::AuthoringValidate { workspace } => {
            read_authoring_workspace(&workspace)?;
            println!(
                "validated Character authoring workspace {}",
                workspace.display()
            );
        }
        Command::AuthoringReview {
            workspace,
            id,
            decision,
            reviewer,
            rationale,
            format,
            output,
        } => {
            let workspace_value = read_authoring_workspace(&workspace)?;
            let decision = match decision {
                FinalReviewDecision::Accepted => CharacterFinalReviewDecision::Accepted,
                FinalReviewDecision::NeedsChanges => CharacterFinalReviewDecision::NeedsChanges,
            };
            let reviewed =
                review_authoring_draft(&workspace_value, &id, reviewer, rationale, decision)?;
            let destination = output.as_deref().unwrap_or(&workspace);
            let format = format.unwrap_or_else(|| output_format_for(destination));
            let serialized = serialize_authoring_workspace(&reviewed, format)?;
            atomic_write(destination, serialized.as_bytes())?;
            println!(
                "reviewed Character authoring draft {}",
                destination.display()
            );
        }
        Command::AuthoringExport {
            workspace,
            id,
            allow_unreviewed,
            format,
            output,
        } => {
            let workspace = read_authoring_workspace(&workspace)?;
            let profile = export_authoring_profile(&workspace, &id, !allow_unreviewed)?;
            let serialized = match format {
                OutputFormat::Json => profile.to_json()?,
                OutputFormat::Ron => profile.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("exported Character profile {}", output.display());
        }
        Command::AuthoringReopen {
            workspace,
            format,
            output,
        } => {
            let workspace = read_authoring_workspace(&workspace)?;
            let serialized = serialize_authoring_workspace(&workspace, format)?;
            atomic_write(&output, serialized.as_bytes())?;
            println!(
                "reopened Character authoring workspace {}",
                output.display()
            );
        }
        Command::PresentationPropose {
            collection,
            catalog,
            request,
            format,
            output,
        } => {
            let collection = parse_collection(&collection, &fs::read_to_string(&collection)?)?;
            let catalog = parse_presentation_catalog(&catalog, &fs::read_to_string(&catalog)?)?;
            let request = parse_presentation_request(&request, &fs::read_to_string(&request)?)?;
            let proposal = propose_presentation_allocations(&collection, &catalog, &request)?;
            let serialized = match format {
                OutputFormat::Json => proposal.to_json()?,
                OutputFormat::Ron => proposal.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character presentation proposal {}", output.display());
        }
        Command::PresentationReview {
            proposal,
            decisions,
            reviewer,
            rationale,
            format,
            output,
        } => {
            let proposal = parse_presentation_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let decisions = read_presentation_decisions(&decisions)?;
            let review = review_presentation_proposal(&proposal, reviewer, rationale, decisions)?;
            let serialized = match format {
                OutputFormat::Json => review.to_json()?,
                OutputFormat::Ron => review.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character presentation review {}", output.display());
        }
        Command::PresentationApply {
            collection,
            proposal,
            review,
            dry_run,
            receipt_output,
            collection_output,
            format,
        } => {
            let collection_value =
                parse_collection(&collection, &fs::read_to_string(&collection)?)?;
            let proposal = parse_presentation_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let review = parse_presentation_review(&review, &fs::read_to_string(&review)?)?;
            let receipt = apply_presentation_review(&collection_value, &proposal, &review)?;
            let serialized_receipt = match format {
                OutputFormat::Json => receipt.to_json()?,
                OutputFormat::Ron => receipt.to_ron()?,
            };
            atomic_write(&receipt_output, serialized_receipt.as_bytes())?;
            if !dry_run {
                let destination = collection_output.as_deref().unwrap_or(&collection);
                let collection_format = collection_output
                    .as_deref()
                    .map_or_else(|| output_format_for(destination), output_format_for);
                let serialized_collection = match collection_format {
                    OutputFormat::Json => receipt.output_collection.to_json()?,
                    OutputFormat::Ron => receipt.output_collection.to_ron()?,
                };
                atomic_write(destination, serialized_collection.as_bytes())?;
            }
            println!(
                "replayed Character presentation receipt {}",
                receipt_output.display()
            );
        }
        Command::PresentationLock {
            collection,
            revision,
            dry_run,
            output,
            format,
        } => {
            let collection_value =
                parse_collection(&collection, &fs::read_to_string(&collection)?)?;
            let revision =
                parse_presentation_lock_revision(&revision, &fs::read_to_string(&revision)?)?;
            let candidate = apply_presentation_lock_revision(&collection_value, &revision)?;
            if !dry_run {
                let destination = output.as_deref().unwrap_or(&collection);
                let format = format.unwrap_or_else(|| output_format_for(destination));
                let serialized = match format {
                    OutputFormat::Json => candidate.to_json()?,
                    OutputFormat::Ron => candidate.to_ron()?,
                };
                atomic_write(destination, serialized.as_bytes())?;
                println!(
                    "applied Character presentation lock revision {}",
                    destination.display()
                );
            } else {
                println!("validated Character presentation lock revision dry-run");
            }
        }
        Command::PresentationRevision {
            workspace,
            draft_id,
            receipt,
            id,
            rationale,
            format,
            output,
        } => {
            let workspace = read_authoring_workspace(&workspace)?;
            let draft = show_authoring_draft(&workspace, &draft_id)?;
            let receipt = parse_presentation_receipt(&receipt, &fs::read_to_string(&receipt)?)?;
            let revision = presentation_authoring_revision(draft, id, rationale, receipt)?;
            let serialized = match format {
                OutputFormat::Json => revision.to_json()?,
                OutputFormat::Ron => revision.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote presentation authoring revision {}", output.display());
        }
        Command::QuestionnairePropose {
            profile,
            pack,
            answers,
            seed,
            format,
            output,
        } => {
            let profile = parse_profile(&profile, &fs::read_to_string(&profile)?)?;
            let pack = parse_questionnaire_pack(&pack, &fs::read_to_string(&pack)?)?;
            let answers = parse_questionnaire_answers(&answers, &fs::read_to_string(&answers)?)?;
            let proposal = propose_character_questionnaire(&profile, &pack, &answers, seed)?;
            let serialized = match format {
                OutputFormat::Json => proposal.to_json()?,
                OutputFormat::Ron => proposal.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!(
                "wrote Character questionnaire proposal {}",
                output.display()
            );
        }
        Command::QuestionnaireReview {
            proposal,
            facet_decisions,
            conflict_decisions,
            reviewer,
            rationale,
            format,
            output,
        } => {
            let proposal =
                parse_questionnaire_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let facet_decisions = read_questionnaire_facet_decisions(&facet_decisions)?;
            let conflict_decisions = read_questionnaire_conflict_decisions(&conflict_decisions)?;
            let review = create_character_questionnaire_review(
                &proposal,
                reviewer,
                rationale,
                facet_decisions,
                conflict_decisions,
            )?;
            let serialized = match format {
                OutputFormat::Json => review.to_json()?,
                OutputFormat::Ron => review.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("wrote Character questionnaire review {}", output.display());
        }
        Command::QuestionnaireApply {
            proposal,
            review,
            format,
            output,
        } => {
            let proposal =
                parse_questionnaire_proposal(&proposal, &fs::read_to_string(&proposal)?)?;
            let review = parse_questionnaire_review(&review, &fs::read_to_string(&review)?)?;
            let receipt = apply_character_questionnaire_review(&proposal, &review)?;
            let serialized = match format {
                OutputFormat::Json => receipt.to_json()?,
                OutputFormat::Ron => receipt.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!("applied Character questionnaire {}", output.display());
        }
        Command::QuestionnaireRevision {
            workspace,
            draft_id,
            receipt,
            id,
            rationale,
            format,
            output,
        } => {
            let workspace = read_authoring_workspace(&workspace)?;
            let draft = show_authoring_draft(&workspace, &draft_id)?;
            let receipt = parse_questionnaire_receipt(&receipt, &fs::read_to_string(&receipt)?)?;
            let revision = questionnaire_authoring_revision(draft, id, rationale, receipt)?;
            let serialized = match format {
                OutputFormat::Json => revision.to_json()?,
                OutputFormat::Ron => revision.to_ron()?,
            };
            atomic_write(&output, serialized.as_bytes())?;
            println!(
                "wrote questionnaire authoring revision {}",
                output.display()
            );
        }
    }
    Ok(())
}

fn parse_profile(
    path: &Path,
    source: &str,
) -> Result<CharacterProfile, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterProfile::from_ron(source)
    } else {
        CharacterProfile::from_json(source)
    }
}

fn parse_template(
    path: &Path,
    source: &str,
) -> Result<CharacterTemplate, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterTemplate::from_ron(source)
    } else {
        CharacterTemplate::from_json(source)
    }
}

fn parse_overlay(
    path: &Path,
    source: &str,
) -> Result<CharacterOverlay, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterOverlay::from_ron(source)
    } else {
        CharacterOverlay::from_json(source)
    }
}

fn parse_synthesis(
    path: &Path,
    source: &str,
) -> Result<CharacterSynthesisResult, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterSynthesisResult::from_ron(source)
    } else {
        CharacterSynthesisResult::from_json(source)
    }
}

fn parse_collection(
    path: &Path,
    source: &str,
) -> Result<CharacterCollection, weave_character::CharacterCorpusError> {
    if is_ron(path) {
        CharacterCollection::from_ron(source)
    } else {
        CharacterCollection::from_json(source)
    }
}

fn parse_request(
    path: &Path,
    source: &str,
) -> Result<CharacterOperationRequest, weave_character::CharacterCorpusError> {
    if is_ron(path) {
        CharacterOperationRequest::from_ron(source)
    } else {
        CharacterOperationRequest::from_json(source)
    }
}

fn parse_proposal(
    path: &Path,
    source: &str,
) -> Result<CharacterProposal, weave_character::CharacterCorpusError> {
    if is_ron(path) {
        CharacterProposal::from_ron(source)
    } else {
        CharacterProposal::from_json(source)
    }
}

fn parse_review(
    path: &Path,
    source: &str,
) -> Result<CharacterProposalReview, weave_character::CharacterCorpusError> {
    if is_ron(path) {
        CharacterProposalReview::from_ron(source)
    } else {
        CharacterProposalReview::from_json(source)
    }
}

fn parse_progress(
    path: &Path,
    source: &str,
) -> Result<CharacterJobProgress, weave_character::CharacterCorpusError> {
    if is_ron(path) {
        CharacterJobProgress::from_ron(source)
    } else {
        CharacterJobProgress::from_json(source)
    }
}

fn parse_temporal_pack(
    path: &Path,
    source: &str,
) -> Result<TemporalContextPack, weave_character::CharacterError> {
    if is_ron(path) {
        TemporalContextPack::from_ron(source)
    } else {
        TemporalContextPack::from_json(source)
    }
}

fn parse_temporal_config(
    path: &Path,
    source: &str,
) -> Result<TemporalContextConfig, weave_character::CharacterError> {
    if is_ron(path) {
        TemporalContextConfig::from_ron(source)
    } else {
        TemporalContextConfig::from_json(source)
    }
}

fn parse_temporal_proposal(
    path: &Path,
    source: &str,
) -> Result<TemporalContextProposal, weave_character::CharacterError> {
    if is_ron(path) {
        TemporalContextProposal::from_ron(source)
    } else {
        TemporalContextProposal::from_json(source)
    }
}

fn parse_temporal_review(
    path: &Path,
    source: &str,
) -> Result<TemporalContextReview, weave_character::CharacterError> {
    if is_ron(path) {
        TemporalContextReview::from_ron(source)
    } else {
        TemporalContextReview::from_json(source)
    }
}

fn parse_temporal_receipt(
    path: &Path,
    source: &str,
) -> Result<TemporalContextReceipt, weave_character::CharacterError> {
    if is_ron(path) {
        TemporalContextReceipt::from_ron(source)
    } else {
        TemporalContextReceipt::from_json(source)
    }
}

fn parse_alignment_pack(
    path: &Path,
    source: &str,
) -> Result<AlignmentPack, weave_character::CharacterError> {
    if is_ron(path) {
        AlignmentPack::from_ron(source)
    } else {
        AlignmentPack::from_json(source)
    }
}

fn parse_alignment_config(
    path: &Path,
    source: &str,
) -> Result<AlignmentConfig, weave_character::CharacterError> {
    if is_ron(path) {
        AlignmentConfig::from_ron(source)
    } else {
        AlignmentConfig::from_json(source)
    }
}

fn parse_alignment_proposal(
    path: &Path,
    source: &str,
) -> Result<AlignmentProposal, weave_character::CharacterError> {
    if is_ron(path) {
        AlignmentProposal::from_ron(source)
    } else {
        AlignmentProposal::from_json(source)
    }
}

fn parse_alignment_review(
    path: &Path,
    source: &str,
) -> Result<AlignmentReview, weave_character::CharacterError> {
    if is_ron(path) {
        AlignmentReview::from_ron(source)
    } else {
        AlignmentReview::from_json(source)
    }
}

fn parse_alignment_receipt(
    path: &Path,
    source: &str,
) -> Result<AlignmentReceipt, weave_character::CharacterError> {
    if is_ron(path) {
        AlignmentReceipt::from_ron(source)
    } else {
        AlignmentReceipt::from_json(source)
    }
}

fn parse_authoring_workspace(
    path: &Path,
    source: &str,
) -> Result<CharacterAuthoringWorkspace, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterAuthoringWorkspace::from_ron(source)
    } else {
        CharacterAuthoringWorkspace::from_json(source)
    }
}

fn parse_authoring_revision(
    path: &Path,
    source: &str,
) -> Result<CharacterAuthoringRevision, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterAuthoringRevision::from_ron(source)
    } else {
        CharacterAuthoringRevision::from_json(source)
    }
}

fn parse_questionnaire_pack(
    path: &Path,
    source: &str,
) -> Result<CharacterQuestionnairePack, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterQuestionnairePack::from_ron(source)
    } else {
        CharacterQuestionnairePack::from_json(source)
    }
}

fn parse_presentation_catalog(
    path: &Path,
    source: &str,
) -> Result<PresentationCatalog, weave_character::CharacterError> {
    if is_ron(path) {
        PresentationCatalog::from_ron(source)
    } else {
        PresentationCatalog::from_json(source)
    }
}

fn parse_presentation_request(
    path: &Path,
    source: &str,
) -> Result<PresentationAllocationRequest, weave_character::CharacterError> {
    if is_ron(path) {
        PresentationAllocationRequest::from_ron(source)
    } else {
        PresentationAllocationRequest::from_json(source)
    }
}

fn parse_presentation_proposal(
    path: &Path,
    source: &str,
) -> Result<PresentationProposal, weave_character::CharacterError> {
    if is_ron(path) {
        PresentationProposal::from_ron(source)
    } else {
        PresentationProposal::from_json(source)
    }
}

fn parse_presentation_review(
    path: &Path,
    source: &str,
) -> Result<PresentationReview, weave_character::CharacterError> {
    if is_ron(path) {
        PresentationReview::from_ron(source)
    } else {
        PresentationReview::from_json(source)
    }
}

fn parse_presentation_receipt(
    path: &Path,
    source: &str,
) -> Result<PresentationReceipt, weave_character::CharacterError> {
    if is_ron(path) {
        PresentationReceipt::from_ron(source)
    } else {
        PresentationReceipt::from_json(source)
    }
}

fn parse_presentation_lock_revision(
    path: &Path,
    source: &str,
) -> Result<PresentationLockRevision, weave_character::CharacterError> {
    if is_ron(path) {
        PresentationLockRevision::from_ron(source)
    } else {
        PresentationLockRevision::from_json(source)
    }
}

fn parse_questionnaire_answers(
    path: &Path,
    source: &str,
) -> Result<CharacterQuestionnaireAnswers, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterQuestionnaireAnswers::from_ron(source)
    } else {
        CharacterQuestionnaireAnswers::from_json(source)
    }
}

fn parse_questionnaire_proposal(
    path: &Path,
    source: &str,
) -> Result<CharacterQuestionnaireProposal, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterQuestionnaireProposal::from_ron(source)
    } else {
        CharacterQuestionnaireProposal::from_json(source)
    }
}

fn parse_questionnaire_review(
    path: &Path,
    source: &str,
) -> Result<CharacterQuestionnaireReview, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterQuestionnaireReview::from_ron(source)
    } else {
        CharacterQuestionnaireReview::from_json(source)
    }
}

fn parse_questionnaire_receipt(
    path: &Path,
    source: &str,
) -> Result<CharacterQuestionnaireReceipt, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterQuestionnaireReceipt::from_ron(source)
    } else {
        CharacterQuestionnaireReceipt::from_json(source)
    }
}

fn parse_final_review(
    path: &Path,
    source: &str,
) -> Result<CharacterFinalReview, weave_character::CharacterError> {
    if is_ron(path) {
        CharacterFinalReview::from_ron(source)
    } else {
        CharacterFinalReview::from_json(source)
    }
}

fn read_authoring_workspace(
    path: &Path,
) -> Result<CharacterAuthoringWorkspace, Box<dyn std::error::Error>> {
    Ok(parse_authoring_workspace(path, &fs::read_to_string(path)?)?)
}

fn read_authoring_revision(
    path: &Path,
) -> Result<CharacterAuthoringRevision, Box<dyn std::error::Error>> {
    Ok(parse_authoring_revision(path, &fs::read_to_string(path)?)?)
}

fn read_provenance(path: &Path) -> Result<weave_domain::Provenance, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    let provenance = if is_ron(path) {
        ron::from_str(&source).map_err(|_| "invalid provenance RON")?
    } else {
        weave_domain::parse_strict_json(&source).map_err(|_| "invalid provenance JSON")?
    };
    weave_domain::validate_provenance(&provenance)?;
    Ok(provenance)
}

fn read_collection(path: &Path) -> Result<CharacterCollection, Box<dyn std::error::Error>> {
    Ok(parse_collection(path, &fs::read_to_string(path)?)?)
}

fn read_request(path: &Path) -> Result<CharacterOperationRequest, Box<dyn std::error::Error>> {
    Ok(parse_request(path, &fs::read_to_string(path)?)?)
}

fn read_proposal(path: &Path) -> Result<CharacterProposal, Box<dyn std::error::Error>> {
    Ok(parse_proposal(path, &fs::read_to_string(path)?)?)
}

fn read_review(path: &Path) -> Result<CharacterProposalReview, Box<dyn std::error::Error>> {
    Ok(parse_review(path, &fs::read_to_string(path)?)?)
}

fn read_progress(path: &Path) -> Result<CharacterJobProgress, Box<dyn std::error::Error>> {
    Ok(parse_progress(path, &fs::read_to_string(path)?)?)
}

fn read_temporal_packs(
    paths: &[PathBuf],
) -> Result<Vec<TemporalContextPack>, Box<dyn std::error::Error>> {
    paths
        .iter()
        .map(|path| Ok(parse_temporal_pack(path, &fs::read_to_string(path)?)?))
        .collect()
}

fn read_temporal_decisions(
    path: &Path,
) -> Result<BTreeMap<String, TemporalReviewDecision>, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    if is_ron(path) {
        ron::from_str(&source).map_err(|_| "invalid temporal decision RON".into())
    } else {
        weave_domain::parse_strict_json(&source)
            .map_err(|_| "invalid temporal decision JSON".into())
    }
}

fn read_alignment_decisions(
    path: &Path,
) -> Result<BTreeMap<String, AlignmentReviewDecision>, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    if is_ron(path) {
        ron::from_str(&source).map_err(|_| "invalid alignment decision RON".into())
    } else {
        weave_domain::parse_strict_json(&source)
            .map_err(|_| "invalid alignment decision JSON".into())
    }
}

fn read_presentation_decisions(
    path: &Path,
) -> Result<
    BTreeMap<String, BTreeMap<String, PresentationReviewDecision>>,
    Box<dyn std::error::Error>,
> {
    let source = fs::read_to_string(path)?;
    if is_ron(path) {
        ron::from_str(&source).map_err(|_| "invalid presentation decision RON".into())
    } else {
        weave_domain::parse_strict_json(&source)
            .map_err(|_| "invalid presentation decision JSON".into())
    }
}

fn read_questionnaire_facet_decisions(
    path: &Path,
) -> Result<BTreeMap<HexacoTrait, CharacterQuestionnaireFacetDecision>, Box<dyn std::error::Error>>
{
    let source = fs::read_to_string(path)?;
    if is_ron(path) {
        ron::from_str(&source).map_err(|_| "invalid questionnaire facet-decision RON".into())
    } else {
        weave_domain::parse_strict_json(&source)
            .map_err(|_| "invalid questionnaire facet-decision JSON".into())
    }
}

fn read_questionnaire_conflict_decisions(
    path: &Path,
) -> Result<BTreeMap<String, CharacterQuestionnaireConflictDecision>, Box<dyn std::error::Error>> {
    let source = fs::read_to_string(path)?;
    if is_ron(path) {
        ron::from_str(&source).map_err(|_| "invalid questionnaire conflict-decision RON".into())
    } else {
        weave_domain::parse_strict_json(&source)
            .map_err(|_| "invalid questionnaire conflict-decision JSON".into())
    }
}

fn collection_scope(
    mut ids: Vec<String>,
    id_prefix: Option<String>,
    extension_namespace: Option<String>,
) -> Result<CharacterScope, Box<dyn std::error::Error>> {
    if !ids.is_empty() && (id_prefix.is_some() || extension_namespace.is_some()) {
        return Err("--id cannot be combined with collection filters".into());
    }
    if !ids.is_empty() {
        ids.sort();
        ids.dedup();
        Ok(CharacterScope::Characters { ids })
    } else if id_prefix.is_some() || extension_namespace.is_some() {
        Ok(CharacterScope::Filter {
            id_prefix,
            extension_namespace,
        })
    } else {
        Ok(CharacterScope::All)
    }
}

fn serialize_proposal(
    proposal: &CharacterProposal,
    format: OutputFormat,
) -> Result<String, weave_character::CharacterCorpusError> {
    match format {
        OutputFormat::Json => proposal.to_json(),
        OutputFormat::Ron => proposal.to_ron(),
    }
}

fn serialize_authoring_workspace(
    workspace: &CharacterAuthoringWorkspace,
    format: OutputFormat,
) -> Result<String, weave_character::CharacterError> {
    match format {
        OutputFormat::Json => workspace.to_json(),
        OutputFormat::Ron => workspace.to_ron(),
    }
}

fn serialize_value<T: Serialize>(
    value: &T,
    format: OutputFormat,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(match format {
        OutputFormat::Json => weave_domain::to_pretty_json(value)?,
        OutputFormat::Ron => weave_domain::to_pretty_ron(value)?,
    })
}

fn output_format_for(path: &Path) -> OutputFormat {
    if is_ron(path) {
        OutputFormat::Ron
    } else {
        OutputFormat::Json
    }
}

fn write_or_print(
    output: Option<&Path>,
    serialized: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(output) = output {
        atomic_write(output, serialized.as_bytes())?;
    } else {
        print!("{serialized}");
    }
    Ok(())
}

fn is_ron(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "ron")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}
