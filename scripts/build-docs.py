#!/usr/bin/env python3
"""Build and verify the versioned Weave documentation site."""

from __future__ import annotations

import html.parser
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib
import urllib.parse
from dataclasses import dataclass, field
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
BOOK = ROOT / "target" / "book"
DOCS_CARGO_TARGET = ROOT / "target" / "docs-cargo"
MDBOOK_VERSION = "0.5.4"
SITE_VERSION = "0.1.0"

REQUIRED_CHAPTERS = {
    "index.md",
    "versioning.md",
    "getting_started.md",
    "language_guide.md",
    "pattern_systems.md",
    "community_patterns.md",
    "domain_modules.md",
    "domain_module_tutorial.md",
    "world_module.md",
    "character_module.md",
    "tabletop_adapters.md",
    "editor_guide.md",
    "json_format.md",
    "api_reference.md",
    "bevy_integration.md",
    "web_player.md",
    "language_server.md",
    "tree_sitter.md",
    "examples.md",
    "architecture.md",
    "contributing.md",
    "security.md",
}

RUSTDOC_PACKAGES = (
    "weave-core",
    "weave-compiler",
    "weave-runtime",
    "weave-patterns",
    "weave-domain",
    "weave-fmt",
    "weave-bevy",
    "weave-web",
    "weave-lsp",
    "weave-world",
    "weave-world-corpus",
    "weave-character",
    "weave-tabletop",
    "tree-sitter-weave",
)

MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)\s]+)(?:\s+[^)]*)?\)")
INCLUDE_DIRECTIVE = re.compile(r"\{\{#include\s+([^}:]+)(?::[^}]*)?\}\}")
WEAVE_FENCE = re.compile(r"```weave\s*\n(.*?)```", re.DOTALL)


class DocsError(RuntimeError):
    """A documentation invariant failed."""


def run(
    command: list[str],
    *,
    capture: bool = False,
    environment: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run one checked command from the repository root."""

    print(f"+ {shlex.join(command)}", flush=True)
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=environment,
            check=False,
            text=True,
            capture_output=capture,
        )
    except OSError as error:
        raise DocsError(f"could not run {command[0]}: {error.strerror}") from None
    if result.returncode != 0:
        if capture:
            sys.stderr.write(result.stdout)
            sys.stderr.write(result.stderr)
        raise DocsError(
            f"command failed with status {result.returncode}: {shlex.join(command)}"
        )
    return result


def workspace_version() -> str:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return str(cargo["workspace"]["package"]["version"])


def cargo_environment() -> dict[str, str]:
    """Use one isolated target directory for a reproducible site build."""

    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(DOCS_CARGO_TARGET)
    return environment


def check_versions() -> None:
    """Keep the site chrome and normative language release aligned."""

    book = tomllib.loads((ROOT / "book.toml").read_text(encoding="utf-8"))
    guide = (DOCS / "language_guide.md").read_text(encoding="utf-8")
    guide_match = re.search(
        r"^Language specification version: `([^`]+)`\.$", guide, re.MULTILINE
    )
    versions = {
        "workspace": workspace_version(),
        "language guide": guide_match.group(1) if guide_match else None,
    }
    for name, version in versions.items():
        if version != SITE_VERSION:
            raise DocsError(f"{name} version is {version!r}, expected {SITE_VERSION!r}")

    title = str(book["book"]["title"])
    if SITE_VERSION not in title:
        raise DocsError(f"book title does not identify Weave {SITE_VERSION}")


def markdown_destination(source: Path, raw_target: str) -> Path | None:
    """Resolve a source Markdown target, or return None for external links."""

    target = urllib.parse.unquote(raw_target).split("#", maxsplit=1)[0]
    parsed = urllib.parse.urlsplit(target)
    if parsed.scheme or target.startswith("//") or not target:
        return None
    if target.startswith("api/"):
        return None
    if target.startswith("downloads/"):
        return ROOT / "schemas" / Path(target).name
    return (source.parent / target).resolve()


def check_source_links() -> None:
    """Reject missing Markdown chapters, includes, and local link targets."""

    summary = (DOCS / "SUMMARY.md").read_text(encoding="utf-8")
    chapters = {
        target.split("#", maxsplit=1)[0]
        for target in MARKDOWN_LINK.findall(summary)
        if not urllib.parse.urlsplit(target).scheme
    }
    missing_chapters = REQUIRED_CHAPTERS - chapters
    if missing_chapters:
        raise DocsError(f"SUMMARY.md is missing chapters: {sorted(missing_chapters)}")

    unlisted = {
        path.name
        for path in DOCS.glob("*.md")
        if path.name != "SUMMARY.md" and path.name not in chapters
    }
    if unlisted:
        raise DocsError(
            f"documentation pages are absent from SUMMARY.md: {sorted(unlisted)}"
        )

    failures: list[str] = []
    for source in sorted(DOCS.rglob("*.md")):
        text = source.read_text(encoding="utf-8")
        for raw_target in MARKDOWN_LINK.findall(text):
            destination = markdown_destination(source, raw_target)
            if destination is not None and not destination.exists():
                failures.append(f"{source.relative_to(ROOT)} -> {raw_target}")
        for raw_include in INCLUDE_DIRECTIVE.findall(text):
            destination = (source.parent / raw_include).resolve()
            if not destination.exists():
                failures.append(f"{source.relative_to(ROOT)} includes {raw_include}")

    if failures:
        raise DocsError(
            "broken source documentation links:\n  " + "\n  ".join(failures)
        )


def check_quickstart_source() -> None:
    """Ensure the visible first story is the checked-in runnable fixture."""

    page = (DOCS / "getting_started.md").read_text(encoding="utf-8")
    examples = [block.strip() for block in WEAVE_FENCE.findall(page)]
    expected = (
        (ROOT / "examples" / "stories" / "basic.weave")
        .read_text(encoding="utf-8")
        .strip()
    )
    if expected not in examples:
        raise DocsError(
            "the quickstart Weave block has drifted from examples/stories/basic.weave"
        )


def cargo_target_directory() -> Path:
    metadata = run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        capture=True,
        environment=cargo_environment(),
    )
    return Path(json.loads(metadata.stdout)["target_directory"])


def compiler_binary() -> Path:
    run(
        ["cargo", "build", "--locked", "-p", "weave-compiler", "--bin", "weavec"],
        environment=cargo_environment(),
    )
    suffix = ".exe" if os.name == "nt" else ""
    binary = cargo_target_directory() / "debug" / f"weavec{suffix}"
    if not binary.is_file():
        raise DocsError(f"compiler binary was not produced at {binary}")
    return binary


def pattern_tool_binary() -> Path:
    """Build and locate the community package command-line tool."""

    run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "weave-patterns",
            "--bin",
            "weave-pattern",
        ],
        environment=cargo_environment(),
    )
    suffix = ".exe" if os.name == "nt" else ""
    binary = cargo_target_directory() / "debug" / f"weave-pattern{suffix}"
    if not binary.is_file():
        raise DocsError(f"pattern tool binary was not produced at {binary}")
    return binary


def domain_tool_binary() -> Path:
    """Build and locate the domain-module contract command-line tool."""

    run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "weave-domain",
            "--bin",
            "weave-module",
        ],
        environment=cargo_environment(),
    )
    suffix = ".exe" if os.name == "nt" else ""
    binary = cargo_target_directory() / "debug" / f"weave-module{suffix}"
    if not binary.is_file():
        raise DocsError(f"domain module tool was not produced at {binary}")
    return binary


def world_corpus_tool_binary() -> Path:
    """Build and locate the offline Weave World corpus builder."""

    run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "weave-world-corpus",
            "--bin",
            "weave-world-corpus",
        ],
        environment=cargo_environment(),
    )
    suffix = ".exe" if os.name == "nt" else ""
    binary = cargo_target_directory() / "debug" / f"weave-world-corpus{suffix}"
    if not binary.is_file():
        raise DocsError(f"world corpus tool was not produced at {binary}")
    return binary


def world_tool_binary() -> Path:
    """Build and locate the portable Weave World composition tool."""

    run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "weave-world",
            "--bin",
            "weave-world",
        ],
        environment=cargo_environment(),
    )
    suffix = ".exe" if os.name == "nt" else ""
    binary = cargo_target_directory() / "debug" / f"weave-world{suffix}"
    if not binary.is_file():
        raise DocsError(f"world composition tool was not produced at {binary}")
    return binary


def character_tool_binary() -> Path:
    """Build and locate the portable Weave Character contract tool."""

    run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "weave-character",
            "--bin",
            "weave-character",
        ],
        environment=cargo_environment(),
    )
    suffix = ".exe" if os.name == "nt" else ""
    binary = cargo_target_directory() / "debug" / f"weave-character{suffix}"
    if not binary.is_file():
        raise DocsError(f"Character contract tool was not produced at {binary}")
    return binary


def tabletop_tool_binary() -> Path:
    """Build and locate the portable tabletop adapter contract tool."""

    run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "weave-tabletop",
            "--bin",
            "weave-tabletop",
        ],
        environment=cargo_environment(),
    )
    suffix = ".exe" if os.name == "nt" else ""
    binary = cargo_target_directory() / "debug" / f"weave-tabletop{suffix}"
    if not binary.is_file():
        raise DocsError("tabletop contract tool was not produced")
    return binary


def compile_examples() -> None:
    """Compile every public story and compare the browser JSON artifact."""

    compiler = compiler_binary()
    domain_tracer = ROOT / "examples/domain-modules/contract/tracer.weave"
    domain_tutorial = ROOT / "examples/domain-modules/third-party-tutorial/story.weave"
    domain_world_sources = {
        ROOT / "examples/domain-modules/weave-world/reference-place.weave",
        ROOT / "examples/domain-modules/weave-world/authored-setting.weave",
        ROOT / "examples/domain-modules/weave-world/composed-setting.weave",
    }
    domain_world_corpus = set(
        (ROOT / "examples/domain-modules/weave-world/corpus/stories").glob("*.weave")
    )
    domain_character_sources = {
        ROOT / "examples/domain-modules/weave-character/ari-vale.weave",
        ROOT
        / "examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.weave",
    }
    domain_tabletop_sources = {
        ROOT / "examples/tabletop-adapters/contract/runtime/lantern-trail.weave",
    }
    sources = [
        source
        for source in sorted((ROOT / "examples").rglob("*.weave"))
        if source not in {domain_tracer, domain_tutorial}
        and source not in domain_world_sources
        and source not in domain_world_corpus
        and source not in domain_character_sources
        and source not in domain_tabletop_sources
    ]
    readme_fixture = (
        ROOT / "crates" / "weave-core" / "tests" / "fixtures" / "fortune_teller.weave"
    )
    sources.append(readme_fixture)
    if not sources:
        raise DocsError("no public Weave examples were found")

    with tempfile.TemporaryDirectory(prefix="weave-docs-") as temporary:
        output_dir = Path(temporary)
        for index, source in enumerate(sources):
            output = output_dir / f"example-{index}.ron"
            run(
                [
                    str(compiler),
                    str(source.relative_to(ROOT)),
                    "--output",
                    str(output),
                ],
                capture=True,
            )

        web_output = output_dir / "story.json"
        run(
            [
                str(compiler),
                "examples/web-player/story.weave",
                "--format",
                "json",
                "--output",
                str(web_output),
            ],
            capture=True,
        )
        checked_in = ROOT / "examples" / "web-player" / "story.json"
        if web_output.read_bytes() != checked_in.read_bytes():
            raise DocsError(
                "examples/web-player/story.json is stale; rebuild the web player"
            )

    print(f"verified {len(sources)} Weave source examples", flush=True)


def verify_community_package() -> None:
    """Exercise package schema, publication, installation, discovery, and story loading."""

    pattern_tool = pattern_tool_binary()
    compiler = compiler_binary()
    package = ROOT / "patterns/community/ember-omens/package.weave-pattern.json"
    story = ROOT / "patterns/community/ember-omens/example.weave"
    run([str(pattern_tool), "validate", str(package.relative_to(ROOT))], capture=True)

    with tempfile.TemporaryDirectory(prefix="weave-community-") as temporary:
        workspace = Path(temporary)
        publication = workspace / "publication"
        registry = workspace / "registry"
        generated_schema = workspace / "community-pattern-package-v1.schema.json"
        run(
            [
                str(pattern_tool),
                "schema",
                "--output",
                str(generated_schema),
            ],
            capture=True,
        )
        checked_schema = ROOT / "schemas/community-pattern-package-v1.schema.json"
        if generated_schema.read_bytes() != checked_schema.read_bytes():
            raise DocsError("the checked-in community package schema is stale")

        run(
            [
                str(pattern_tool),
                "publish",
                str(package.relative_to(ROOT)),
                "--output",
                str(publication),
            ],
            capture=True,
        )
        artifact = publication / "ember_omens-1.0.0.weave-pattern.json"
        run(
            [
                str(pattern_tool),
                "install",
                str(artifact),
                "--registry",
                str(registry),
            ],
            capture=True,
        )
        listing = run(
            [str(pattern_tool), "list", "--registry", str(registry)],
            capture=True,
        )
        if "ember_omens@1.0.0" not in listing.stdout or "MIT" not in listing.stdout:
            raise DocsError("installed community package is absent from discovery")

        index = workspace / "index.json"
        run(
            [
                str(pattern_tool),
                "index",
                "--registry",
                str(registry),
                "--output",
                str(index),
            ],
            capture=True,
        )
        index_data = json.loads(index.read_text(encoding="utf-8"))
        if index_data["packages"][0]["id"] != "ember_omens":
            raise DocsError("community registry index omitted the sample package")

        output = workspace / "story.json"
        run(
            [
                str(compiler),
                str(story.relative_to(ROOT)),
                "--pattern-registry",
                str(registry),
                "--pattern",
                "ember_omens@=1.0.0",
                "--format",
                "json",
                "--output",
                str(output),
            ],
            capture=True,
        )
        compiled = json.loads(output.read_text(encoding="utf-8"))
        elements = compiled["patterns"]["ember_omens"]["collections"]["elements"][
            "elements"
        ]
        if len(elements) != 4:
            raise DocsError("community package data was not embedded in story IR")

    print("verified community package publication and story loading", flush=True)


def verify_domain_contract() -> None:
    """Verify schemas, artifacts, and the compiled domain-module tracer."""

    domain_tool = domain_tool_binary()
    world_corpus_tool = world_corpus_tool_binary()
    world_tool = world_tool_binary()
    character_tool = character_tool_binary()
    compiler = compiler_binary()
    fixture = ROOT / "examples" / "domain-modules" / "contract"
    manifest_json = fixture / "module.weave-module.json"
    manifest_ron = fixture / "module.weave-module.ron"
    pack_json = fixture / "pack.weave-domain.json"
    pack_ron = fixture / "pack.weave-domain.ron"
    tracer = fixture / "tracer.weave"
    world_fixture = ROOT / "examples" / "domain-modules" / "weave-world"
    world_manifest = world_fixture / "module.weave-module.json"
    world_pack = world_fixture / "pack.weave-domain.json"
    world_source = world_fixture / "reference-place.weave"
    authored_world_source = world_fixture / "authored-setting.weave"
    authored_world_ron = world_fixture / "authored-setting.story.ron"
    authored_world_json = world_fixture / "authored-setting.story.json"
    composition_plan = world_fixture / "composition.weave-world.json"
    composition_receipt_json = world_fixture / "composition.receipt.json"
    composition_receipt_ron = world_fixture / "composition.receipt.ron"
    composed_world_pack = (
        world_fixture / "packs" / "glasswind_composed.weave-domain.json"
    )
    composed_world_source = world_fixture / "composed-setting.weave"
    composed_world_story_ron = world_fixture / "composed-setting.story.ron"
    composed_world_story_json = world_fixture / "composed-setting.story.json"
    full_world_json = world_fixture / "composed-world.full.json"
    compact_world_json = world_fixture / "composed-world.compact.json"
    naming_manifest = world_fixture / "naming" / "module.weave-module.json"
    naming_pack = world_fixture / "naming" / "glasswind.weave-domain.json"
    world_index = world_fixture / "corpus.weave-world.json"
    character_fixture = ROOT / "examples" / "domain-modules" / "weave-character"
    character_profile_json = character_fixture / "profile.character.json"
    character_profile_ron = character_fixture / "profile.character.ron"
    character_template_json = character_fixture / "template.character.json"
    character_template_ron = character_fixture / "template.character.ron"
    character_overlay_json = character_fixture / "overlay.character.json"
    character_overlay_ron = character_fixture / "overlay.character.ron"
    character_synthesis_json = character_fixture / "synthesis.character.json"
    character_synthesis_ron = character_fixture / "synthesis.character.ron"
    character_manifest_json = character_fixture / "module.weave-module.json"
    character_manifest_ron = character_fixture / "module.weave-module.ron"
    character_pack_json = character_fixture / "ari_vale.weave-domain.json"
    character_pack_ron = character_fixture / "ari_vale.weave-domain.ron"
    character_source = character_fixture / "ari-vale.weave"
    character_story_json = character_fixture / "ari-vale.story.json"
    character_story_ron = character_fixture / "ari-vale.story.ron"
    character_project = character_fixture / "weave.modules.json"
    character_authoring = character_fixture / "authoring"
    character_operations = character_fixture / "operations"
    character_presentation = character_fixture / "presentation"
    character_expression = character_fixture / "expression"
    character_collection_json = (
        character_operations / "collection.character-collection.json"
    )
    character_collection_ron = (
        character_operations / "collection.character-collection.ron"
    )
    character_request_json = character_operations / "rename.character-request.json"
    character_request_ron = character_operations / "rename.character-request.ron"
    character_proposal_json = character_operations / "rename.character-proposal.json"
    character_proposal_ron = character_operations / "rename.character-proposal.ron"
    character_review_json = character_operations / "rename.character-review.json"
    character_review_ron = character_operations / "rename.character-review.ron"
    character_progress_json = character_operations / "rename.character-progress.json"
    character_progress_ron = character_operations / "rename.character-progress.ron"
    renamed_character_collection_json = (
        character_operations / "renamed.character-collection.json"
    )
    renamed_character_collection_ron = (
        character_operations / "renamed.character-collection.ron"
    )
    presentation_input_json = (
        character_presentation / "input.character-collection.json"
    )
    presentation_input_ron = character_presentation / "input.character-collection.ron"
    presentation_catalog_json = (
        character_presentation / "glasswind.presentation-catalog.json"
    )
    presentation_catalog_ron = (
        character_presentation / "glasswind.presentation-catalog.ron"
    )
    presentation_request_json = (
        character_presentation / "allocation.presentation-request.json"
    )
    presentation_request_ron = (
        character_presentation / "allocation.presentation-request.ron"
    )
    presentation_proposal_json = (
        character_presentation / "proposal.presentation-proposal.json"
    )
    presentation_proposal_ron = (
        character_presentation / "proposal.presentation-proposal.ron"
    )
    presentation_decisions_json = (
        character_presentation / "decisions.presentation-review.json"
    )
    presentation_decisions_ron = (
        character_presentation / "decisions.presentation-review.ron"
    )
    presentation_review_json = (
        character_presentation / "review.presentation-review.json"
    )
    presentation_review_ron = (
        character_presentation / "review.presentation-review.ron"
    )
    presentation_receipt_json = (
        character_presentation / "receipt.presentation-receipt.json"
    )
    presentation_receipt_ron = (
        character_presentation / "receipt.presentation-receipt.ron"
    )
    presentation_applied_json = (
        character_presentation / "applied.character-collection.json"
    )
    presentation_applied_ron = (
        character_presentation / "applied.character-collection.ron"
    )
    presentation_lock_revision_json = (
        character_presentation / "unlock-avatar.presentation-lock-revision.json"
    )
    presentation_lock_revision_ron = (
        character_presentation / "unlock-avatar.presentation-lock-revision.ron"
    )
    presentation_unlocked_json = (
        character_presentation / "unlocked.character-collection.json"
    )
    presentation_unlocked_ron = (
        character_presentation / "unlocked.character-collection.ron"
    )
    character_alignment = character_fixture / "alignment"
    alignment_profile_json = character_alignment / "input.character.json"
    alignment_profile_ron = character_alignment / "input.character.ron"
    alignment_pack_json = (
        character_alignment / "wayfinder_compass.alignment-pack.json"
    )
    alignment_pack_ron = character_alignment / "wayfinder_compass.alignment-pack.ron"
    alignment_config_json = character_alignment / "selection.alignment-config.json"
    alignment_config_ron = character_alignment / "selection.alignment-config.ron"
    alignment_proposal_json = (
        character_alignment / "proposal.alignment-proposal.json"
    )
    alignment_proposal_ron = character_alignment / "proposal.alignment-proposal.ron"
    alignment_decisions_json = (
        character_alignment / "decisions.alignment-review.json"
    )
    alignment_decisions_ron = character_alignment / "decisions.alignment-review.ron"
    alignment_review_json = character_alignment / "review.alignment-review.json"
    alignment_review_ron = character_alignment / "review.alignment-review.ron"
    alignment_receipt_json = character_alignment / "receipt.alignment-receipt.json"
    alignment_receipt_ron = character_alignment / "receipt.alignment-receipt.ron"
    alignment_approved_json = character_alignment / "approved.character.json"
    alignment_approved_ron = character_alignment / "approved.character.ron"
    character_context = character_fixture / "context"
    temporal_profile_json = character_context / "input.character.json"
    temporal_profile_ron = character_context / "input.character.ron"
    temporal_pack_json = (
        character_context / "apollo_11.temporal-pack.json",
        character_context / "calendar.temporal-pack.json",
        character_context / "world.temporal-pack.json",
    )
    temporal_pack_ron = (
        character_context / "apollo_11.temporal-pack.ron",
        character_context / "calendar.temporal-pack.ron",
        character_context / "world.temporal-pack.ron",
    )
    temporal_config_json = character_context / "ranking.temporal-config.json"
    temporal_config_ron = character_context / "ranking.temporal-config.ron"
    temporal_proposal_json = character_context / "proposal.temporal-proposal.json"
    temporal_proposal_ron = character_context / "proposal.temporal-proposal.ron"
    temporal_decisions_json = character_context / "decisions.temporal-review.json"
    temporal_decisions_ron = character_context / "decisions.temporal-review.ron"
    temporal_review_json = character_context / "review.temporal-review.json"
    temporal_review_ron = character_context / "review.temporal-review.ron"
    temporal_receipt_json = character_context / "receipt.temporal-receipt.json"
    temporal_receipt_ron = character_context / "receipt.temporal-receipt.ron"
    temporal_enriched_json = character_context / "enriched.character.json"
    temporal_enriched_ron = character_context / "enriched.character.ron"
    temporal_runtime = character_context / "runtime"
    temporal_manifest_json = temporal_runtime / "module.weave-module.json"
    temporal_manifest_ron = temporal_runtime / "module.weave-module.ron"
    temporal_domain_pack_json = temporal_runtime / "ari_vale_temporal.weave-domain.json"
    temporal_domain_pack_ron = temporal_runtime / "ari_vale_temporal.weave-domain.ron"
    temporal_source = temporal_runtime / "ari-vale-temporal.weave"
    temporal_story_json = temporal_runtime / "ari-vale-temporal.story.json"
    temporal_story_ron = temporal_runtime / "ari-vale-temporal.story.ron"
    temporal_project = temporal_runtime / "weave.modules.json"
    world_packs = (
        world_pack,
        world_fixture / "packs" / "british_columbia_temperate_forest.weave-domain.json",
        world_fixture / "packs" / "hokkaido_japan.weave-domain.json",
        world_fixture / "packs" / "maldives.weave-domain.json",
    )
    world_story_fixtures = (
        (
            world_fixture
            / "corpus"
            / "stories"
            / "british-columbia-temperate-forest.weave",
            world_packs[1],
            world_fixture
            / "corpus"
            / "stories"
            / "british-columbia-temperate-forest.story.ron",
            world_fixture
            / "corpus"
            / "stories"
            / "british-columbia-temperate-forest.story.json",
            "british_columbia_temperate_forest",
            "ecosystem_scale_representative_point",
        ),
        (
            world_fixture / "corpus" / "stories" / "hokkaido-japan.weave",
            world_packs[2],
            world_fixture / "corpus" / "stories" / "hokkaido-japan.story.ron",
            world_fixture / "corpus" / "stories" / "hokkaido-japan.story.json",
            "hokkaido_japan",
            "region_scale_representative_point",
        ),
        (
            world_fixture / "corpus" / "stories" / "maldives.weave",
            world_packs[3],
            world_fixture / "corpus" / "stories" / "maldives.story.ron",
            world_fixture / "corpus" / "stories" / "maldives.story.json",
            "maldives",
            "country_scale_representative_point",
        ),
    )

    run(
        [
            str(domain_tool),
            "validate",
            str(manifest_json.relative_to(ROOT)),
            "--pack",
            str(pack_json.relative_to(ROOT)),
        ],
        capture=True,
    )
    run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "weave-character",
            "--example",
            "character_fixture",
            "--",
            "--check",
        ],
        capture=True,
        environment=cargo_environment(),
    )
    run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "weave-character",
            "--example",
            "expression_fixture",
            "--",
            "--check",
        ],
        capture=True,
        environment=cargo_environment(),
    )
    run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "weave-character",
            "--example",
            "guided_authoring_fixture",
            "--",
            "--check",
        ],
        capture=True,
        environment=cargo_environment(),
    )
    for manifest, pack in (
        (character_manifest_json, character_pack_json),
        (character_manifest_ron, character_pack_ron),
        (temporal_manifest_json, temporal_domain_pack_json),
        (temporal_manifest_ron, temporal_domain_pack_ron),
    ):
        run(
            [
                str(domain_tool),
                "validate",
                str(manifest.relative_to(ROOT)),
                "--pack",
                str(pack.relative_to(ROOT)),
            ],
            capture=True,
        )
    run(
        [
            str(domain_tool),
            "validate-project",
            str(character_project.relative_to(ROOT)),
        ],
        capture=True,
    )
    run(
        [
            str(domain_tool),
            "validate-project",
            str(temporal_project.relative_to(ROOT)),
        ],
        capture=True,
    )
    world_validation = [
        str(domain_tool),
        "validate",
        str(world_manifest.relative_to(ROOT)),
    ]
    for pack in (*world_packs, composed_world_pack):
        world_validation.extend(("--pack", str(pack.relative_to(ROOT))))
    run(world_validation, capture=True)
    run(
        [
            str(domain_tool),
            "validate",
            str(naming_manifest.relative_to(ROOT)),
            "--pack",
            str(naming_pack.relative_to(ROOT)),
        ],
        capture=True,
    )
    run(
        [
            str(world_corpus_tool),
            "check",
            str(world_index.relative_to(ROOT)),
            "--manifest",
            str(world_manifest.relative_to(ROOT)),
        ],
        capture=True,
    )
    run(
        [
            str(domain_tool),
            "validate",
            str(manifest_ron.relative_to(ROOT)),
            "--pack",
            str(pack_ron.relative_to(ROOT)),
        ],
        capture=True,
    )

    with tempfile.TemporaryDirectory(prefix="weave-domain-contract-") as temporary:
        workspace = Path(temporary)
        generated_manifest_schema = workspace / "domain-module-manifest-v1.schema.json"
        generated_pack_schema = workspace / "domain-pack-v1.schema.json"
        generated_project_schema = workspace / "domain-project-v1.schema.json"
        generated_lock_schema = workspace / "domain-lock-v1.schema.json"
        generated_registry_schema = workspace / "domain-registry-index-v1.schema.json"
        generated_world_index_schema = (
            workspace / "weave-world-corpus-index-v1.schema.json"
        )
        generated_world_preset_schema = (
            workspace / "weave-world-corpus-preset-v1.schema.json"
        )
        generated_world_composition_schema = (
            workspace / "weave-world-composition-v1.schema.json"
        )
        generated_world_full_schema = workspace / "weave-world-full-v1.schema.json"
        generated_world_compact_schema = (
            workspace / "weave-world-compact-v1.schema.json"
        )
        generated_character_profile_schema = (
            workspace / "weave-character-profile-v1.schema.json"
        )
        generated_character_template_schema = (
            workspace / "weave-character-template-v1.schema.json"
        )
        generated_character_overlay_schema = (
            workspace / "weave-character-overlay-v1.schema.json"
        )
        generated_character_synthesis_schema = (
            workspace / "weave-character-synthesis-v1.schema.json"
        )
        generated_character_diagnostic_schema = (
            workspace / "weave-character-diagnostic-v1.schema.json"
        )
        generated_character_collection_schema = (
            workspace / "weave-character-collection-v1.schema.json"
        )
        generated_character_request_schema = (
            workspace / "weave-character-operation-request-v1.schema.json"
        )
        generated_character_proposal_schema = (
            workspace / "weave-character-proposal-v1.schema.json"
        )
        generated_character_review_schema = (
            workspace / "weave-character-review-v1.schema.json"
        )
        generated_character_progress_schema = (
            workspace / "weave-character-progress-v1.schema.json"
        )
        generated_temporal_pack_schema = (
            workspace / "weave-character-temporal-pack-v1.schema.json"
        )
        generated_temporal_config_schema = (
            workspace / "weave-character-temporal-config-v1.schema.json"
        )
        generated_temporal_proposal_schema = (
            workspace / "weave-character-temporal-proposal-v1.schema.json"
        )
        generated_temporal_review_schema = (
            workspace / "weave-character-temporal-review-v1.schema.json"
        )
        generated_temporal_receipt_schema = (
            workspace / "weave-character-temporal-receipt-v1.schema.json"
        )
        generated_alignment_pack_schema = (
            workspace / "weave-character-alignment-pack-v1.schema.json"
        )
        generated_alignment_config_schema = (
            workspace / "weave-character-alignment-config-v1.schema.json"
        )
        generated_alignment_proposal_schema = (
            workspace / "weave-character-alignment-proposal-v1.schema.json"
        )
        generated_alignment_review_schema = (
            workspace / "weave-character-alignment-review-v1.schema.json"
        )
        generated_alignment_receipt_schema = (
            workspace / "weave-character-alignment-receipt-v1.schema.json"
        )
        generated_expression_schemas = {
            "expression-pack": workspace
            / "weave-character-expression-pack-v1.schema.json",
            "expression-revision": workspace
            / "weave-character-expression-revision-v1.schema.json",
            "expression-assignment-request": workspace
            / "weave-character-expression-assignment-request-v1.schema.json",
            "expression-assignment-receipt": workspace
            / "weave-character-expression-assignment-receipt-v1.schema.json",
            "expression-resolution-request": workspace
            / "weave-character-expression-resolution-request-v1.schema.json",
            "expression-resolution": workspace
            / "weave-character-expression-resolution-v1.schema.json",
            "expression-lint": workspace
            / "weave-character-expression-lint-v1.schema.json",
            "expression-coverage": workspace
            / "weave-character-expression-coverage-v1.schema.json",
        }
        generated_relationship_schemas = {
            "relationship-kind-pack": workspace
            / "weave-character-relationship-kind-pack-v1.schema.json",
            "relationship-policy": workspace
            / "weave-character-relationship-policy-v1.schema.json",
            "relationship-config": workspace
            / "weave-character-relationship-config-v1.schema.json",
            "relationship-proposal": workspace
            / "weave-character-relationship-proposal-v1.schema.json",
            "relationship-review": workspace
            / "weave-character-relationship-review-v1.schema.json",
            "relationship-receipt": workspace
            / "weave-character-relationship-receipt-v1.schema.json",
            "relationship-revision": workspace
            / "weave-character-relationship-revision-v1.schema.json",
            "relationship-reconciliation": workspace
            / "weave-character-relationship-reconciliation-v1.schema.json",
        }
        generated_presentation_schemas = {
            "presentation-catalog": workspace
            / "weave-character-presentation-catalog-v1.schema.json",
            "presentation-request": workspace
            / "weave-character-presentation-request-v1.schema.json",
            "presentation-proposal": workspace
            / "weave-character-presentation-proposal-v1.schema.json",
            "presentation-review": workspace
            / "weave-character-presentation-review-v1.schema.json",
            "presentation-receipt": workspace
            / "weave-character-presentation-receipt-v1.schema.json",
            "presentation-lock-revision": workspace
            / "weave-character-presentation-lock-revision-v1.schema.json",
        }
        generated_authoring_schemas = {
            "authoring-workspace": workspace
            / "weave-character-authoring-workspace-v1.schema.json",
            "authoring-revision": workspace
            / "weave-character-authoring-revision-v1.schema.json",
            "authoring-preview": workspace
            / "weave-character-authoring-preview-v1.schema.json",
            "questionnaire-pack": workspace
            / "weave-character-questionnaire-pack-v1.schema.json",
            "questionnaire-answers": workspace
            / "weave-character-questionnaire-answers-v1.schema.json",
            "questionnaire-proposal": workspace
            / "weave-character-questionnaire-proposal-v1.schema.json",
            "questionnaire-review": workspace
            / "weave-character-questionnaire-review-v1.schema.json",
            "questionnaire-receipt": workspace
            / "weave-character-questionnaire-receipt-v1.schema.json",
            "final-review": workspace / "weave-character-final-review-v1.schema.json",
        }
        normalized_manifest_json = workspace / "module.weave-module.json"
        normalized_manifest_ron = workspace / "module.weave-module.ron"
        normalized_pack_json = workspace / "pack.weave-domain.json"
        normalized_pack_ron = workspace / "pack.weave-domain.ron"
        normalized_world_manifest_json = workspace / "world-module.weave-module.json"
        normalized_naming_manifest_json = (
            workspace / "world-naming-module.weave-module.json"
        )
        normalized_naming_pack_json = workspace / "world-naming-pack.weave-domain.json"
        normalized_composed_world_pack = (
            workspace / "world-composed-pack.weave-domain.json"
        )
        normalized_world_packs = tuple(
            (workspace / f"world-pack-{index}.weave-domain.json", pack)
            for index, pack in enumerate(world_packs)
        )
        compiled_tracer_ron = workspace / "tracer.story.ron"
        compiled_tracer_json = workspace / "tracer.story.json"
        compiled_world_ron = workspace / "reference-place.story.ron"
        compiled_world_json = workspace / "reference-place.story.json"
        compiled_authored_world_ron = workspace / "authored-setting.story.ron"
        compiled_authored_world_json = workspace / "authored-setting.story.json"
        compiled_composed_world_ron = workspace / "composed-setting.story.ron"
        compiled_composed_world_json = workspace / "composed-setting.story.json"
        generated_composed_pack = workspace / "glasswind_composed.weave-domain.json"
        generated_composition_receipt_json = workspace / "composition.receipt.json"
        generated_composition_receipt_ron = workspace / "composition.receipt.ron"
        generated_character_synthesis_json = workspace / "synthesis.character.json"
        generated_character_synthesis_ron = workspace / "synthesis.character.ron"
        generated_character_manifest_json = (
            workspace / "character-module.weave-module.json"
        )
        generated_character_manifest_ron = (
            workspace / "character-module.weave-module.ron"
        )
        generated_character_pack_json = workspace / "ari_vale.weave-domain.json"
        generated_character_pack_ron = workspace / "ari_vale.weave-domain.ron"
        generated_temporal_manifest_json = (
            workspace / "temporal-module.weave-module.json"
        )
        generated_temporal_manifest_ron = workspace / "temporal-module.weave-module.ron"
        generated_temporal_domain_pack_json = (
            workspace / "ari_vale_temporal.weave-domain.json"
        )
        generated_temporal_domain_pack_ron = (
            workspace / "ari_vale_temporal.weave-domain.ron"
        )
        compiled_character_json = workspace / "ari-vale.story.json"
        compiled_character_ron = workspace / "ari-vale.story.ron"
        compiled_character_locked_json = workspace / "ari-vale.locked.story.json"
        compiled_temporal_json = workspace / "ari-vale-temporal.story.json"
        compiled_temporal_ron = workspace / "ari-vale-temporal.story.ron"
        compiled_temporal_locked_json = (
            workspace / "ari-vale-temporal.locked.story.json"
        )
        generated_character_proposal_json = workspace / "rename.character-proposal.json"
        generated_character_proposal_ron = workspace / "rename.character-proposal.ron"
        generated_character_review_json = workspace / "rename.character-review.json"
        generated_character_review_ron = workspace / "rename.character-review.ron"
        generated_character_progress_json = workspace / "rename.character-progress.json"
        generated_character_ready_progress_json = (
            workspace / "rename-ready.character-progress.json"
        )
        resumed_character_proposal_json = workspace / "resumed.character-proposal.json"
        applied_character_collection_json = (
            workspace / "renamed.character-collection.json"
        )
        applied_character_collection_ron = (
            workspace / "renamed.character-collection.ron"
        )
        generated_temporal_proposal_json = workspace / "proposal.temporal-proposal.json"
        generated_temporal_proposal_ron = workspace / "proposal.temporal-proposal.ron"
        generated_temporal_review_json = workspace / "review.temporal-review.json"
        generated_temporal_review_ron = workspace / "review.temporal-review.ron"
        generated_temporal_receipt_json = workspace / "receipt.temporal-receipt.json"
        generated_temporal_receipt_ron = workspace / "receipt.temporal-receipt.ron"
        generated_alignment_proposal_json = (
            workspace / "proposal.alignment-proposal.json"
        )
        generated_alignment_proposal_ron = (
            workspace / "proposal.alignment-proposal.ron"
        )
        generated_alignment_review_json = workspace / "review.alignment-review.json"
        generated_alignment_review_ron = workspace / "review.alignment-review.ron"
        generated_alignment_receipt_json = workspace / "receipt.alignment-receipt.json"
        generated_alignment_receipt_ron = workspace / "receipt.alignment-receipt.ron"
        generated_presentation_proposal_json = (
            workspace / "proposal.presentation-proposal.json"
        )
        generated_presentation_proposal_ron = (
            workspace / "proposal.presentation-proposal.ron"
        )
        generated_presentation_review_json = (
            workspace / "review.presentation-review.json"
        )
        generated_presentation_review_ron = (
            workspace / "review.presentation-review.ron"
        )
        generated_presentation_receipt_json = (
            workspace / "receipt.presentation-receipt.json"
        )
        generated_presentation_receipt_ron = (
            workspace / "receipt.presentation-receipt.ron"
        )
        generated_presentation_applied_json = (
            workspace / "applied.character-collection.json"
        )
        generated_presentation_applied_ron = (
            workspace / "applied.character-collection.ron"
        )
        generated_presentation_unlocked_json = (
            workspace / "unlocked.character-collection.json"
        )

        for kind, output in (
            ("manifest", generated_manifest_schema),
            ("pack", generated_pack_schema),
            ("project", generated_project_schema),
            ("lock", generated_lock_schema),
            ("registry", generated_registry_schema),
        ):
            run(
                [str(domain_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for kind, output in (
            ("index", generated_world_index_schema),
            ("preset", generated_world_preset_schema),
        ):
            run(
                [str(world_corpus_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for kind, output in (
            ("composition", generated_world_composition_schema),
            ("full", generated_world_full_schema),
            ("compact", generated_world_compact_schema),
        ):
            run(
                [str(world_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for kind, output in (
            ("profile", generated_character_profile_schema),
            ("template", generated_character_template_schema),
            ("overlay", generated_character_overlay_schema),
            ("synthesis", generated_character_synthesis_schema),
            ("diagnostic", generated_character_diagnostic_schema),
            ("collection", generated_character_collection_schema),
            ("request", generated_character_request_schema),
            ("proposal", generated_character_proposal_schema),
            ("review", generated_character_review_schema),
            ("progress", generated_character_progress_schema),
            ("temporal-pack", generated_temporal_pack_schema),
            ("temporal-config", generated_temporal_config_schema),
            ("temporal-proposal", generated_temporal_proposal_schema),
            ("temporal-review", generated_temporal_review_schema),
            ("temporal-receipt", generated_temporal_receipt_schema),
            ("alignment-pack", generated_alignment_pack_schema),
            ("alignment-config", generated_alignment_config_schema),
            ("alignment-proposal", generated_alignment_proposal_schema),
            ("alignment-review", generated_alignment_review_schema),
            ("alignment-receipt", generated_alignment_receipt_schema),
        ):
            run(
                [str(character_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for kind, output in generated_authoring_schemas.items():
            run(
                [str(character_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for kind, output in generated_expression_schemas.items():
            run(
                [str(character_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for kind, output in generated_relationship_schemas.items():
            run(
                [str(character_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for kind, output in generated_presentation_schemas.items():
            run(
                [str(character_tool), "schema", kind, "--output", str(output)],
                capture=True,
            )
        for generated, checked in (
            (
                generated_manifest_schema,
                ROOT / "schemas" / "domain-module-manifest-v1.schema.json",
            ),
            (generated_pack_schema, ROOT / "schemas" / "domain-pack-v1.schema.json"),
            (
                generated_project_schema,
                ROOT / "schemas" / "domain-project-v1.schema.json",
            ),
            (generated_lock_schema, ROOT / "schemas" / "domain-lock-v1.schema.json"),
            (
                generated_registry_schema,
                ROOT / "schemas" / "domain-registry-index-v1.schema.json",
            ),
            (
                generated_world_index_schema,
                ROOT / "schemas" / "weave-world-corpus-index-v1.schema.json",
            ),
            (
                generated_world_preset_schema,
                ROOT / "schemas" / "weave-world-corpus-preset-v1.schema.json",
            ),
            (
                generated_world_composition_schema,
                ROOT / "schemas" / "weave-world-composition-v1.schema.json",
            ),
            (
                generated_world_full_schema,
                ROOT / "schemas" / "weave-world-full-v1.schema.json",
            ),
            (
                generated_world_compact_schema,
                ROOT / "schemas" / "weave-world-compact-v1.schema.json",
            ),
            (
                generated_character_profile_schema,
                ROOT / "schemas" / "weave-character-profile-v1.schema.json",
            ),
            (
                generated_character_template_schema,
                ROOT / "schemas" / "weave-character-template-v1.schema.json",
            ),
            (
                generated_character_overlay_schema,
                ROOT / "schemas" / "weave-character-overlay-v1.schema.json",
            ),
            (
                generated_character_synthesis_schema,
                ROOT / "schemas" / "weave-character-synthesis-v1.schema.json",
            ),
            (
                generated_character_diagnostic_schema,
                ROOT / "schemas" / "weave-character-diagnostic-v1.schema.json",
            ),
            (
                generated_character_collection_schema,
                ROOT / "schemas" / "weave-character-collection-v1.schema.json",
            ),
            (
                generated_character_request_schema,
                ROOT / "schemas" / "weave-character-operation-request-v1.schema.json",
            ),
            (
                generated_character_proposal_schema,
                ROOT / "schemas" / "weave-character-proposal-v1.schema.json",
            ),
            (
                generated_character_review_schema,
                ROOT / "schemas" / "weave-character-review-v1.schema.json",
            ),
            (
                generated_character_progress_schema,
                ROOT / "schemas" / "weave-character-progress-v1.schema.json",
            ),
            (
                generated_temporal_pack_schema,
                ROOT / "schemas" / "weave-character-temporal-pack-v1.schema.json",
            ),
            (
                generated_temporal_config_schema,
                ROOT / "schemas" / "weave-character-temporal-config-v1.schema.json",
            ),
            (
                generated_temporal_proposal_schema,
                ROOT / "schemas" / "weave-character-temporal-proposal-v1.schema.json",
            ),
            (
                generated_temporal_review_schema,
                ROOT / "schemas" / "weave-character-temporal-review-v1.schema.json",
            ),
            (
                generated_temporal_receipt_schema,
                ROOT / "schemas" / "weave-character-temporal-receipt-v1.schema.json",
            ),
            (
                generated_alignment_pack_schema,
                ROOT / "schemas" / "weave-character-alignment-pack-v1.schema.json",
            ),
            (
                generated_alignment_config_schema,
                ROOT / "schemas" / "weave-character-alignment-config-v1.schema.json",
            ),
            (
                generated_alignment_proposal_schema,
                ROOT / "schemas" / "weave-character-alignment-proposal-v1.schema.json",
            ),
            (
                generated_alignment_review_schema,
                ROOT / "schemas" / "weave-character-alignment-review-v1.schema.json",
            ),
            (
                generated_alignment_receipt_schema,
                ROOT / "schemas" / "weave-character-alignment-receipt-v1.schema.json",
            ),
        ):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"checked-in domain schema is stale: {checked.name}")
        for generated in generated_authoring_schemas.values():
            checked = ROOT / "schemas" / generated.name
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"checked-in authoring schema is stale: {checked.name}")
        for generated in generated_expression_schemas.values():
            checked = ROOT / "schemas" / generated.name
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"checked-in expression schema is stale: {checked.name}")
        for generated in generated_relationship_schemas.values():
            checked = ROOT / "schemas" / generated.name
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(
                    f"checked-in relationship schema is stale: {checked.name}"
                )
        for generated in generated_presentation_schemas.values():
            checked = ROOT / "schemas" / generated.name
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(
                    f"checked-in presentation schema is stale: {checked.name}"
                )

        for kind, source in (
            (
                "authoring-workspace",
                character_authoring / "workspace.reviewed.authoring-workspace.json",
            ),
            (
                "authoring-workspace",
                character_authoring / "workspace.reviewed.authoring-workspace.ron",
            ),
            (
                "authoring-revision",
                character_authoring / "questionnaire.authoring-revision.json",
            ),
            (
                "authoring-revision",
                character_authoring / "questionnaire.authoring-revision.ron",
            ),
            (
                "questionnaire-pack",
                character_authoring / "lantern_choices.questionnaire-pack.json",
            ),
            (
                "questionnaire-pack",
                character_authoring / "lantern_choices.questionnaire-pack.ron",
            ),
            (
                "questionnaire-answers",
                character_authoring / "answers.questionnaire-answers.json",
            ),
            (
                "questionnaire-answers",
                character_authoring / "answers.questionnaire-answers.ron",
            ),
            (
                "questionnaire-proposal",
                character_authoring / "proposal.questionnaire-proposal.json",
            ),
            (
                "questionnaire-proposal",
                character_authoring / "proposal.questionnaire-proposal.ron",
            ),
            (
                "questionnaire-review",
                character_authoring / "review.questionnaire-review.json",
            ),
            (
                "questionnaire-review",
                character_authoring / "review.questionnaire-review.ron",
            ),
            (
                "questionnaire-receipt",
                character_authoring / "receipt.questionnaire-receipt.json",
            ),
            (
                "questionnaire-receipt",
                character_authoring / "receipt.questionnaire-receipt.ron",
            ),
        ):
            run(
                [
                    str(character_tool),
                    "validate",
                    kind,
                    str(source.relative_to(ROOT)),
                ],
                capture=True,
            )

        for kind, source in (
            ("profile", character_profile_json),
            ("profile", character_profile_ron),
            ("template", character_template_json),
            ("template", character_template_ron),
            ("overlay", character_overlay_json),
            ("overlay", character_overlay_ron),
            ("synthesis", character_synthesis_json),
            ("synthesis", character_synthesis_ron),
            ("collection", character_collection_json),
            ("collection", character_collection_ron),
            ("request", character_request_json),
            ("request", character_request_ron),
            ("proposal", character_proposal_json),
            ("proposal", character_proposal_ron),
            ("review", character_review_json),
            ("review", character_review_ron),
            ("progress", character_progress_json),
            ("progress", character_progress_ron),
            ("collection", presentation_input_json),
            ("collection", presentation_input_ron),
            ("collection", presentation_applied_json),
            ("collection", presentation_applied_ron),
            ("collection", presentation_unlocked_json),
            ("collection", presentation_unlocked_ron),
            ("presentation-catalog", presentation_catalog_json),
            ("presentation-catalog", presentation_catalog_ron),
            ("presentation-request", presentation_request_json),
            ("presentation-request", presentation_request_ron),
            ("presentation-proposal", presentation_proposal_json),
            ("presentation-proposal", presentation_proposal_ron),
            ("presentation-review", presentation_review_json),
            ("presentation-review", presentation_review_ron),
            ("presentation-receipt", presentation_receipt_json),
            ("presentation-receipt", presentation_receipt_ron),
            ("presentation-lock-revision", presentation_lock_revision_json),
            ("presentation-lock-revision", presentation_lock_revision_ron),
            ("profile", temporal_profile_json),
            ("profile", temporal_profile_ron),
            ("profile", temporal_enriched_json),
            ("profile", temporal_enriched_ron),
            ("temporal-pack", temporal_pack_json[0]),
            ("temporal-pack", temporal_pack_json[1]),
            ("temporal-pack", temporal_pack_json[2]),
            ("temporal-pack", temporal_pack_ron[0]),
            ("temporal-pack", temporal_pack_ron[1]),
            ("temporal-pack", temporal_pack_ron[2]),
            ("temporal-config", temporal_config_json),
            ("temporal-config", temporal_config_ron),
            ("temporal-proposal", temporal_proposal_json),
            ("temporal-proposal", temporal_proposal_ron),
            ("temporal-review", temporal_review_json),
            ("temporal-review", temporal_review_ron),
            ("temporal-receipt", temporal_receipt_json),
            ("temporal-receipt", temporal_receipt_ron),
            ("profile", alignment_profile_json),
            ("profile", alignment_profile_ron),
            ("profile", alignment_approved_json),
            ("profile", alignment_approved_ron),
            ("alignment-pack", alignment_pack_json),
            ("alignment-pack", alignment_pack_ron),
            ("alignment-config", alignment_config_json),
            ("alignment-config", alignment_config_ron),
            ("alignment-proposal", alignment_proposal_json),
            ("alignment-proposal", alignment_proposal_ron),
            ("alignment-review", alignment_review_json),
            ("alignment-review", alignment_review_ron),
            ("alignment-receipt", alignment_receipt_json),
            ("alignment-receipt", alignment_receipt_ron),
            ("profile", character_expression / "applied.character.json"),
            ("profile", character_expression / "applied.character.ron"),
            (
                "expression-pack",
                character_expression / "glasswind.expression-pack.json",
            ),
            (
                "expression-pack",
                character_expression / "glasswind.expression-pack.ron",
            ),
            (
                "expression-revision",
                character_expression / "normalized.expression-revision.json",
            ),
            (
                "expression-revision",
                character_expression / "normalized.expression-revision.ron",
            ),
            (
                "expression-assignment-request",
                character_expression / "assignment.expression-request.json",
            ),
            (
                "expression-assignment-request",
                character_expression / "assignment.expression-request.ron",
            ),
            (
                "expression-assignment-receipt",
                character_expression / "assignment.expression-receipt.json",
            ),
            (
                "expression-assignment-receipt",
                character_expression / "assignment.expression-receipt.ron",
            ),
            (
                "expression-resolution-request",
                character_expression / "contextual.expression-resolution-request.json",
            ),
            (
                "expression-resolution-request",
                character_expression / "contextual.expression-resolution-request.ron",
            ),
            (
                "expression-resolution",
                character_expression / "contextual.expression-resolution.json",
            ),
            (
                "expression-resolution",
                character_expression / "contextual.expression-resolution.ron",
            ),
            (
                "expression-lint",
                character_expression / "lint.expression-lint.json",
            ),
            (
                "expression-lint",
                character_expression / "lint.expression-lint.ron",
            ),
            (
                "expression-coverage",
                character_expression / "coverage.expression-coverage.json",
            ),
            (
                "expression-coverage",
                character_expression / "coverage.expression-coverage.ron",
            ),
        ):
            run(
                [
                    str(character_tool),
                    "validate",
                    kind,
                    str(source.relative_to(ROOT)),
                ],
                capture=True,
            )
        for encoding, output, checked in (
            ("json", generated_character_synthesis_json, character_synthesis_json),
            ("ron", generated_character_synthesis_ron, character_synthesis_ron),
        ):
            run(
                [
                    str(character_tool),
                    "synthesize",
                    str(character_overlay_json.relative_to(ROOT)),
                    "--template",
                    str(character_template_json.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
            if output.read_bytes() != checked.read_bytes():
                raise DocsError(
                    f"canonical Character synthesis is stale: {checked.name}"
                )

        for (
            encoding,
            collection,
            request,
            checked_proposal,
            generated_proposal,
            checked_review,
            generated_review,
            checked_applied,
            generated_applied,
        ) in (
            (
                "json",
                character_collection_json,
                character_request_json,
                character_proposal_json,
                generated_character_proposal_json,
                character_review_json,
                generated_character_review_json,
                renamed_character_collection_json,
                applied_character_collection_json,
            ),
            (
                "ron",
                character_collection_ron,
                character_request_ron,
                character_proposal_ron,
                generated_character_proposal_ron,
                character_review_ron,
                generated_character_review_ron,
                renamed_character_collection_ron,
                applied_character_collection_ron,
            ),
        ):
            run(
                [
                    str(character_tool),
                    "collection-propose",
                    str(collection.relative_to(ROOT)),
                    str(request.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(generated_proposal),
                ],
                capture=True,
            )
            if generated_proposal.read_bytes() != checked_proposal.read_bytes():
                raise DocsError(
                    f"canonical Character proposal is stale: {checked_proposal.name}"
                )
            run(
                [
                    str(character_tool),
                    "collection-review",
                    str(checked_proposal.relative_to(ROOT)),
                    "--decision",
                    "accepted",
                    "--reviewer",
                    "org.weave.reviewer.fixture",
                    "--rationale",
                    "Approve the complete synthetic reference-safe rename.",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_review),
                ],
                capture=True,
            )
            if generated_review.read_bytes() != checked_review.read_bytes():
                raise DocsError(
                    f"canonical Character review is stale: {checked_review.name}"
                )
            run(
                [
                    str(character_tool),
                    "collection-apply",
                    str(collection.relative_to(ROOT)),
                    str(checked_proposal.relative_to(ROOT)),
                    str(checked_review.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(generated_applied),
                ],
                capture=True,
            )
            if generated_applied.read_bytes() != checked_applied.read_bytes():
                raise DocsError(
                    f"canonical Character atomic apply is stale: {checked_applied.name}"
                )

        run(
            [
                str(character_tool),
                "collection-resume",
                str(character_collection_json.relative_to(ROOT)),
                str(character_request_json.relative_to(ROOT)),
                "--max-items",
                "1",
                "--progress-output",
                str(generated_character_progress_json),
            ],
            capture=True,
        )
        if (
            generated_character_progress_json.read_bytes()
            != character_progress_json.read_bytes()
        ):
            raise DocsError("canonical Character resumable progress is stale")
        run(
            [
                str(character_tool),
                "collection-resume",
                str(character_collection_json.relative_to(ROOT)),
                str(character_request_json.relative_to(ROOT)),
                "--progress",
                str(generated_character_progress_json),
                "--max-items",
                "1",
                "--progress-output",
                str(generated_character_ready_progress_json),
                "--proposal-output",
                str(resumed_character_proposal_json),
            ],
            capture=True,
        )
        if (
            resumed_character_proposal_json.read_bytes()
            != character_proposal_json.read_bytes()
        ):
            raise DocsError("resumed Character proposal differs from direct dry-run")
        dry_run_output = workspace / "forbidden-dry-run-output.json"
        run(
            [
                str(character_tool),
                "collection-apply",
                str(character_collection_json.relative_to(ROOT)),
                str(character_proposal_json.relative_to(ROOT)),
                str(character_review_json.relative_to(ROOT)),
                "--dry-run",
                "--output",
                str(dry_run_output),
            ],
            capture=True,
        )
        if dry_run_output.exists():
            raise DocsError("Character apply dry-run wrote a collection")

        presentation_encodings = (
            (
                "json",
                presentation_input_json,
                presentation_catalog_json,
                presentation_request_json,
                presentation_proposal_json,
                generated_presentation_proposal_json,
                presentation_decisions_json,
                presentation_review_json,
                generated_presentation_review_json,
                presentation_receipt_json,
                generated_presentation_receipt_json,
                presentation_applied_json,
                generated_presentation_applied_json,
            ),
            (
                "ron",
                presentation_input_ron,
                presentation_catalog_ron,
                presentation_request_ron,
                presentation_proposal_ron,
                generated_presentation_proposal_ron,
                presentation_decisions_ron,
                presentation_review_ron,
                generated_presentation_review_ron,
                presentation_receipt_ron,
                generated_presentation_receipt_ron,
                presentation_applied_ron,
                generated_presentation_applied_ron,
            ),
        )
        for (
            encoding,
            collection,
            catalog,
            request,
            checked_proposal,
            generated_proposal,
            decisions,
            checked_review,
            generated_review,
            checked_receipt,
            generated_receipt,
            checked_applied,
            generated_applied,
        ) in presentation_encodings:
            run(
                [
                    str(character_tool),
                    "presentation-propose",
                    str(collection.relative_to(ROOT)),
                    str(catalog.relative_to(ROOT)),
                    str(request.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(generated_proposal),
                ],
                capture=True,
            )
            if generated_proposal.read_bytes() != checked_proposal.read_bytes():
                raise DocsError(
                    f"canonical presentation proposal is stale: {checked_proposal.name}"
                )
            run(
                [
                    str(character_tool),
                    "presentation-review",
                    str(checked_proposal.relative_to(ROOT)),
                    str(decisions.relative_to(ROOT)),
                    "--reviewer",
                    "org.weave.reviewer.presentation_fixture",
                    "--rationale",
                    "Review every synthetic presentation allocation; retain catalog coordinates, balance traces, locks, and explicit override rationale.",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_review),
                ],
                capture=True,
            )
            if generated_review.read_bytes() != checked_review.read_bytes():
                raise DocsError(
                    f"canonical presentation review is stale: {checked_review.name}"
                )
            run(
                [
                    str(character_tool),
                    "presentation-apply",
                    str(collection.relative_to(ROOT)),
                    str(checked_proposal.relative_to(ROOT)),
                    str(checked_review.relative_to(ROOT)),
                    "--receipt-output",
                    str(generated_receipt),
                    "--collection-output",
                    str(generated_applied),
                    "--format",
                    encoding,
                ],
                capture=True,
            )
            if generated_receipt.read_bytes() != checked_receipt.read_bytes():
                raise DocsError(
                    f"canonical presentation receipt is stale: {checked_receipt.name}"
                )
            if generated_applied.read_bytes() != checked_applied.read_bytes():
                raise DocsError(
                    f"canonical presentation apply is stale: {checked_applied.name}"
                )

        presentation_dry_run_receipt = workspace / "dry-run.presentation-receipt.json"
        presentation_dry_run_collection = (
            workspace / "forbidden-presentation-dry-run-collection.json"
        )
        run(
            [
                str(character_tool),
                "presentation-apply",
                str(presentation_input_json.relative_to(ROOT)),
                str(presentation_proposal_json.relative_to(ROOT)),
                str(presentation_review_json.relative_to(ROOT)),
                "--dry-run",
                "--receipt-output",
                str(presentation_dry_run_receipt),
                "--collection-output",
                str(presentation_dry_run_collection),
            ],
            capture=True,
        )
        if presentation_dry_run_collection.exists():
            raise DocsError("presentation dry-run wrote a collection")
        if presentation_dry_run_receipt.read_bytes() != presentation_receipt_json.read_bytes():
            raise DocsError("presentation dry-run receipt differs from checked replay")

        run(
            [
                str(character_tool),
                "presentation-lock",
                str(presentation_applied_json.relative_to(ROOT)),
                str(presentation_lock_revision_json.relative_to(ROOT)),
                "--output",
                str(generated_presentation_unlocked_json),
            ],
            capture=True,
        )
        if (
            generated_presentation_unlocked_json.read_bytes()
            != presentation_unlocked_json.read_bytes()
        ):
            raise DocsError("canonical presentation unlock result is stale")
        forbidden_lock_output = workspace / "forbidden-presentation-lock-output.json"
        run(
            [
                str(character_tool),
                "presentation-lock",
                str(presentation_applied_json.relative_to(ROOT)),
                str(presentation_lock_revision_json.relative_to(ROOT)),
                "--dry-run",
                "--output",
                str(forbidden_lock_output),
            ],
            capture=True,
        )
        if forbidden_lock_output.exists():
            raise DocsError("presentation lock dry-run wrote a collection")

        presentation_proposal = json.loads(
            presentation_proposal_json.read_text(encoding="utf-8")
        )
        presentation_receipt = json.loads(
            presentation_receipt_json.read_text(encoding="utf-8")
        )
        presentation_input = json.loads(
            presentation_input_json.read_text(encoding="utf-8")
        )
        presentation_output = presentation_receipt.get("output_collection", {})
        for character_id, before in presentation_input.get("characters", {}).items():
            after = presentation_output.get("characters", {}).get(character_id, {})
            before_other_extensions = {
                key: value
                for key, value in before.get("extensions", {}).items()
                if key != "org.weave.character.identity_presentation"
            }
            after_other_extensions = {
                key: value
                for key, value in after.get("extensions", {}).items()
                if key != "org.weave.character.identity_presentation"
            }
            if (
                before.get("canon") != after.get("canon")
                or before.get("derived") != after.get("derived")
                or before.get("suggestions") != after.get("suggestions")
                or before_other_extensions != after_other_extensions
            ):
                raise DocsError(
                    "presentation allocation changed personality or another owned field"
                )
        allocations = [
            allocation
            for slots in presentation_proposal.get("allocations", {}).values()
            for allocation in slots.values()
        ]
        if (
            presentation_proposal.get("request", {}).get("seed") != 20_260_822
            or not allocations
            or any(not allocation.get("trace") for allocation in allocations)
            or any(
                sum(bool(candidate.get("selected")) for candidate in allocation["trace"])
                != 1
                for allocation in allocations
                if allocation.get("disposition") == "proposed"
            )
        ):
            raise DocsError(
                "presentation fixture omitted its pinned seed or transparent constraints"
            )

        alignment_encodings = (
            (
                "json",
                alignment_profile_json,
                alignment_pack_json,
                alignment_config_json,
                alignment_proposal_json,
                generated_alignment_proposal_json,
                alignment_decisions_json,
                alignment_review_json,
                generated_alignment_review_json,
                alignment_receipt_json,
                generated_alignment_receipt_json,
            ),
            (
                "ron",
                alignment_profile_ron,
                alignment_pack_ron,
                alignment_config_ron,
                alignment_proposal_ron,
                generated_alignment_proposal_ron,
                alignment_decisions_ron,
                alignment_review_ron,
                generated_alignment_review_ron,
                alignment_receipt_ron,
                generated_alignment_receipt_ron,
            ),
        )
        for (
            encoding,
            profile,
            pack,
            config,
            checked_proposal,
            generated_proposal,
            decisions,
            checked_review,
            generated_review,
            checked_receipt,
            generated_receipt,
        ) in alignment_encodings:
            run(
                [
                    str(character_tool),
                    "alignment-propose",
                    str(profile.relative_to(ROOT)),
                    "--pack",
                    str(pack.relative_to(ROOT)),
                    "--config",
                    str(config.relative_to(ROOT)),
                    "--seed",
                    "20260822",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_proposal),
                ],
                capture=True,
            )
            if generated_proposal.read_bytes() != checked_proposal.read_bytes():
                raise DocsError(
                    f"canonical alignment proposal is stale: {checked_proposal.name}"
                )
            run(
                [
                    str(character_tool),
                    "alignment-review",
                    str(checked_proposal.relative_to(ROOT)),
                    "--pack",
                    str(pack.relative_to(ROOT)),
                    str(decisions.relative_to(ROOT)),
                    "--reviewer",
                    "org.weave.reviewer.fixture",
                    "--rationale",
                    "Review every original Wayfinder Compass axis independently; publish only approved fictional shorthand and never treat a label as diagnosis, moral rank, canonical evidence, or runtime authority.",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_review),
                ],
                capture=True,
            )
            if generated_review.read_bytes() != checked_review.read_bytes():
                raise DocsError(
                    f"canonical alignment review is stale: {checked_review.name}"
                )
            run(
                [
                    str(character_tool),
                    "alignment-apply",
                    str(profile.relative_to(ROOT)),
                    "--pack",
                    str(pack.relative_to(ROOT)),
                    str(checked_proposal.relative_to(ROOT)),
                    str(checked_review.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(generated_receipt),
                ],
                capture=True,
            )
            if generated_receipt.read_bytes() != checked_receipt.read_bytes():
                raise DocsError(
                    f"canonical alignment receipt is stale: {checked_receipt.name}"
                )

        alignment_dry_run_output = workspace / "forbidden-alignment-dry-run-output.json"
        run(
            [
                str(character_tool),
                "alignment-apply",
                str(alignment_profile_json.relative_to(ROOT)),
                "--pack",
                str(alignment_pack_json.relative_to(ROOT)),
                str(alignment_proposal_json.relative_to(ROOT)),
                str(alignment_review_json.relative_to(ROOT)),
                "--dry-run",
                "--output",
                str(alignment_dry_run_output),
            ],
            capture=True,
        )
        if alignment_dry_run_output.exists():
            raise DocsError("alignment dry-run wrote a receipt")

        alignment_proposal = json.loads(
            alignment_proposal_json.read_text(encoding="utf-8")
        )
        alignment_review = json.loads(
            alignment_review_json.read_text(encoding="utf-8")
        )
        alignment_receipt = json.loads(
            alignment_receipt_json.read_text(encoding="utf-8")
        )
        alignment_approved = json.loads(
            alignment_approved_json.read_text(encoding="utf-8")
        )
        alignment_record = (
            alignment_receipt.get("output_profile", {})
            .get("extensions", {})
            .get("org.weave.character.alignment", {})
            .get("record", {})
        )
        alignment_public = alignment_record.get("value", {})
        alignment_decision_kinds = {
            decision.get("action", {}).get("decision")
            for decision in alignment_review.get("decisions", {}).values()
        }
        if (
            alignment_proposal.get("seed") != 20_260_822
            or list(alignment_proposal.get("values", {}))
            != ["horizon", "reciprocity", "signal", "structure", "tempo"]
            or any(
                not value.get("trace", {}).get("ordered_inputs")
                for value in alignment_proposal.get("values", {}).values()
            )
            or alignment_decision_kinds
            != {"accept", "edit", "reject", "override", "withhold"}
            or alignment_receipt.get("output_profile") != alignment_approved
            or alignment_receipt.get("input_profile", {}).get("canon")
            != alignment_receipt.get("output_profile", {}).get("canon")
            or list(alignment_public.get("values", {}))
            != ["horizon", "reciprocity", "structure"]
            or alignment_record.get("header", {}).get(
                "canonical_personality_write_back"
            )
            != "forbidden"
            or len(alignment_public.get("review_sha256", "")) != 64
            or len(alignment_public.get("applied_sha256", "")) != 64
        ):
            raise DocsError(
                "alignment fixture omitted exact traces, complete review, approved-only output, or immutable canon"
            )

        temporal_encodings = (
            (
                "json",
                temporal_profile_json,
                temporal_pack_json,
                temporal_config_json,
                temporal_proposal_json,
                generated_temporal_proposal_json,
                temporal_decisions_json,
                temporal_review_json,
                generated_temporal_review_json,
                temporal_receipt_json,
                generated_temporal_receipt_json,
            ),
            (
                "ron",
                temporal_profile_ron,
                temporal_pack_ron,
                temporal_config_ron,
                temporal_proposal_ron,
                generated_temporal_proposal_ron,
                temporal_decisions_ron,
                temporal_review_ron,
                generated_temporal_review_ron,
                temporal_receipt_ron,
                generated_temporal_receipt_ron,
            ),
        )
        for (
            encoding,
            profile,
            packs,
            config,
            checked_proposal,
            generated_proposal,
            decisions,
            checked_review,
            generated_review,
            checked_receipt,
            generated_receipt,
        ) in temporal_encodings:
            pack_arguments = [
                argument
                for pack in packs
                for argument in ("--pack", str(pack.relative_to(ROOT)))
            ]
            run(
                [
                    str(character_tool),
                    "context-propose",
                    str(profile.relative_to(ROOT)),
                    *pack_arguments,
                    "--config",
                    str(config.relative_to(ROOT)),
                    "--seed",
                    "19690720",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_proposal),
                ],
                capture=True,
            )
            if generated_proposal.read_bytes() != checked_proposal.read_bytes():
                raise DocsError(
                    f"canonical temporal proposal is stale: {checked_proposal.name}"
                )
            run(
                [
                    str(character_tool),
                    "context-review",
                    str(checked_proposal.relative_to(ROOT)),
                    str(decisions.relative_to(ROOT)),
                    "--reviewer",
                    "org.weave.reviewer.fixture",
                    "--rationale",
                    "Review every ranked temporal cue, preserve fact and fictional-cue lineage separately, and keep all accepted material outside canon.",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_review),
                ],
                capture=True,
            )
            if generated_review.read_bytes() != checked_review.read_bytes():
                raise DocsError(
                    f"canonical temporal review is stale: {checked_review.name}"
                )
            run(
                [
                    str(character_tool),
                    "context-apply",
                    str(profile.relative_to(ROOT)),
                    *pack_arguments,
                    str(checked_proposal.relative_to(ROOT)),
                    str(checked_review.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(generated_receipt),
                ],
                capture=True,
            )
            if generated_receipt.read_bytes() != checked_receipt.read_bytes():
                raise DocsError(
                    f"canonical temporal receipt is stale: {checked_receipt.name}"
                )

        temporal_dry_run_output = workspace / "forbidden-temporal-dry-run-output.json"
        temporal_pack_arguments = [
            argument
            for pack in temporal_pack_json
            for argument in ("--pack", str(pack.relative_to(ROOT)))
        ]
        run(
            [
                str(character_tool),
                "context-apply",
                str(temporal_profile_json.relative_to(ROOT)),
                *temporal_pack_arguments,
                str(temporal_proposal_json.relative_to(ROOT)),
                str(temporal_review_json.relative_to(ROOT)),
                "--dry-run",
                "--output",
                str(temporal_dry_run_output),
            ],
            capture=True,
        )
        if temporal_dry_run_output.exists():
            raise DocsError("temporal context dry-run wrote a receipt")

        temporal_proposal = json.loads(
            temporal_proposal_json.read_text(encoding="utf-8")
        )
        temporal_receipt = json.loads(temporal_receipt_json.read_text(encoding="utf-8"))
        temporal_enriched = json.loads(
            temporal_enriched_json.read_text(encoding="utf-8")
        )
        if (
            temporal_proposal.get("seed") != 19_690_720
            or len(temporal_proposal.get("packs", [])) != 3
            or len(temporal_proposal.get("candidates", [])) != 4
            or {
                entry.get("disposition")
                for entry in temporal_proposal.get("coverage", [])
            }
            != {"selected", "downgraded", "skipped"}
            or temporal_receipt.get("output_profile") != temporal_enriched
        ):
            raise DocsError(
                "temporal context fixture omitted coverage, exact inputs, or applied output"
            )

        for (
            encoding,
            generated_manifest,
            checked_manifest,
            generated_pack,
            checked_pack,
        ) in (
            (
                "json",
                generated_character_manifest_json,
                character_manifest_json,
                generated_character_pack_json,
                character_pack_json,
            ),
            (
                "ron",
                generated_character_manifest_ron,
                character_manifest_ron,
                generated_character_pack_ron,
                character_pack_ron,
            ),
        ):
            run(
                [
                    str(character_tool),
                    "module-manifest",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_manifest),
                ],
                capture=True,
            )
            run(
                [
                    str(character_tool),
                    "domain-pack",
                    str(character_profile_json.relative_to(ROOT)),
                    "--id",
                    "ari_vale",
                    "--version",
                    "1.0.0",
                    "--title",
                    "Ari Vale Synthetic Character",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_pack),
                ],
                capture=True,
            )
            if generated_manifest.read_bytes() != checked_manifest.read_bytes():
                raise DocsError(
                    f"canonical Character manifest is stale: {checked_manifest.name}"
                )
            if generated_pack.read_bytes() != checked_pack.read_bytes():
                raise DocsError(
                    f"canonical Character pack is stale: {checked_pack.name}"
                )

        for (
            encoding,
            generated_manifest,
            checked_manifest,
            generated_pack,
            checked_pack,
        ) in (
            (
                "json",
                generated_temporal_manifest_json,
                temporal_manifest_json,
                generated_temporal_domain_pack_json,
                temporal_domain_pack_json,
            ),
            (
                "ron",
                generated_temporal_manifest_ron,
                temporal_manifest_ron,
                generated_temporal_domain_pack_ron,
                temporal_domain_pack_ron,
            ),
        ):
            run(
                [
                    str(character_tool),
                    "module-manifest",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_manifest),
                ],
                capture=True,
            )
            run(
                [
                    str(character_tool),
                    "domain-pack",
                    str(temporal_enriched_json.relative_to(ROOT)),
                    "--id",
                    "ari_vale_temporal",
                    "--version",
                    "1.0.0",
                    "--title",
                    "Ari Vale Reviewed Temporal Context",
                    "--format",
                    encoding,
                    "--output",
                    str(generated_pack),
                ],
                capture=True,
            )
            if generated_manifest.read_bytes() != checked_manifest.read_bytes():
                raise DocsError(
                    f"canonical temporal Character manifest is stale: {checked_manifest.name}"
                )
            if generated_pack.read_bytes() != checked_pack.read_bytes():
                raise DocsError(
                    f"canonical temporal Character pack is stale: {checked_pack.name}"
                )

        normalization_jobs = (
            ("manifest", manifest_json, "json", normalized_manifest_json),
            ("manifest", manifest_json, "ron", normalized_manifest_ron),
            ("pack", pack_json, "json", normalized_pack_json),
            ("pack", pack_json, "ron", normalized_pack_ron),
            ("manifest", world_manifest, "json", normalized_world_manifest_json),
            ("manifest", naming_manifest, "json", normalized_naming_manifest_json),
            ("pack", naming_pack, "json", normalized_naming_pack_json),
            ("pack", composed_world_pack, "json", normalized_composed_world_pack),
        ) + tuple(
            ("pack", pack, "json", generated)
            for generated, pack in normalized_world_packs
        )
        for kind, source, encoding, output in normalization_jobs:
            run(
                [
                    str(domain_tool),
                    "normalize",
                    kind,
                    str(source.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
        normalized_checks = (
            (normalized_manifest_json, manifest_json),
            (normalized_manifest_ron, manifest_ron),
            (normalized_pack_json, pack_json),
            (normalized_pack_ron, pack_ron),
            (normalized_world_manifest_json, world_manifest),
            (normalized_naming_manifest_json, naming_manifest),
            (normalized_naming_pack_json, naming_pack),
            (normalized_composed_world_pack, composed_world_pack),
        ) + normalized_world_packs
        for generated, checked in normalized_checks:
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"canonical domain fixture is stale: {checked.name}")

        staged_world = workspace / "offline-world-corpus"
        shutil.copytree(world_fixture / "corpus", staged_world / "corpus")
        shutil.copy2(world_index, staged_world / world_index.name)
        shutil.copy2(world_manifest, staged_world / world_manifest.name)
        run(
            [
                str(world_corpus_tool),
                "build",
                str(staged_world / world_index.name),
                "--manifest",
                str(staged_world / world_manifest.name),
            ],
            capture=True,
        )
        staged_outputs = (
            staged_world / "pack.weave-domain.json",
            staged_world
            / "packs"
            / "british_columbia_temperate_forest.weave-domain.json",
            staged_world / "packs" / "hokkaido_japan.weave-domain.json",
            staged_world / "packs" / "maldives.weave-domain.json",
        )
        for generated, checked in zip(staged_outputs, world_packs, strict=True):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"offline world corpus output is stale: {checked.name}")

        composition_command = [
            str(world_tool),
            "compose",
            str(composition_plan.relative_to(ROOT)),
            "--manifest",
            str(world_manifest.relative_to(ROOT)),
            "--pack",
            str(world_pack.relative_to(ROOT)),
            "--pack",
            str(world_packs[2].relative_to(ROOT)),
            "--pack",
            str(world_packs[1].relative_to(ROOT)),
            "--output",
            str(generated_composed_pack),
        ]
        run(
            [
                *composition_command,
                "--receipt",
                str(generated_composition_receipt_json),
            ],
            capture=True,
        )
        if generated_composed_pack.read_bytes() != composed_world_pack.read_bytes():
            raise DocsError("checked composed World pack is stale")
        if (
            generated_composition_receipt_json.read_bytes()
            != composition_receipt_json.read_bytes()
        ):
            raise DocsError("checked JSON World composition receipt is stale")
        repeat_pack = workspace / "glasswind_composed-repeat.weave-domain.json"
        repeat_command = composition_command.copy()
        repeat_command[-1] = str(repeat_pack)
        run(
            [
                *repeat_command,
                "--receipt",
                str(generated_composition_receipt_ron),
            ],
            capture=True,
        )
        if repeat_pack.read_bytes() != composed_world_pack.read_bytes():
            raise DocsError("repeated World composition is not deterministic")
        if (
            generated_composition_receipt_ron.read_bytes()
            != composition_receipt_ron.read_bytes()
        ):
            raise DocsError("checked RON World composition receipt is stale")
        receipt = json.loads(composition_receipt_json.read_text(encoding="utf-8"))
        if (
            receipt.get("format_version") != 1
            or receipt.get("random_seed") != 4278421
            or len(receipt.get("layers", [])) != 3
            or any(not layer.get("candidates") for layer in receipt.get("layers", []))
        ):
            raise DocsError(
                "World composition receipt cannot reproduce layer selection"
            )

        for encoding, output in (
            ("ron", compiled_tracer_ron),
            ("json", compiled_tracer_json),
        ):
            run(
                [
                    str(compiler),
                    str(tracer.relative_to(ROOT)),
                    "--module-manifest",
                    str(manifest_json.relative_to(ROOT)),
                    "--module-pack",
                    str(pack_json.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
        for generated, checked in (
            (compiled_tracer_ron, fixture / "tracer.story.ron"),
            (compiled_tracer_json, fixture / "tracer.story.json"),
        ):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"compiled domain tracer is stale: {checked.name}")

        compiled = json.loads(compiled_tracer_json.read_text(encoding="utf-8"))
        module = compiled.get("modules", {}).get("constellation", {})
        if (
            compiled.get("version") != 4
            or module.get("id") != "org.weave.synthetic_constellation"
            or module.get("exports", {}).get("phase", {}).get("value", {}).get("value")
            != "twilight"
        ):
            raise DocsError("compiled domain tracer omitted its selected module data")

        for encoding, output, checked in (
            ("ron", compiled_character_ron, character_story_ron),
            ("json", compiled_character_json, character_story_json),
        ):
            run(
                [
                    str(compiler),
                    str(character_source.relative_to(ROOT)),
                    "--module-manifest",
                    str(character_manifest_json.relative_to(ROOT)),
                    "--module-pack",
                    str(character_pack_json.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
            if output.read_bytes() != checked.read_bytes():
                raise DocsError(f"compiled Character fixture is stale: {checked.name}")

        run(
            [
                str(compiler),
                str(character_source.relative_to(ROOT)),
                "--locked",
                "--format",
                "json",
                "--output",
                str(compiled_character_locked_json),
            ],
            capture=True,
        )
        if (
            compiled_character_locked_json.read_bytes()
            != character_story_json.read_bytes()
        ):
            raise DocsError(
                "locked Character project output differs from the checked Story IR"
            )
        character_story = json.loads(
            compiled_character_json.read_text(encoding="utf-8")
        )
        character_module = character_story.get("modules", {}).get("character", {})
        character_profile = (
            character_module.get("exports", {})
            .get("profile", {})
            .get("value", {})
            .get("value", {})
        )
        character_alignment = character_profile.get("alignment", {}).get("value", {})
        character_alignment_values = (
            character_alignment.get("values", {}).get("value", {})
        )
        character_relationships = character_profile.get("relationships", {}).get(
            "value", {}
        )
        character_mentor_edge = (
            character_relationships.get("edges", {})
            .get("value", {})
            .get("mentor_sable", {})
            .get("value", {})
        )
        character_relationship_pack = character_relationships.get(
            "kind_pack", {}
        ).get("value", {})
        character_expression = character_profile.get("expression", {}).get(
            "value", {}
        )
        character_expression_term = (
            character_expression.get("lexicon", {})
            .get("value", {})
            .get("trailmark", {})
            .get("value", {})
        )
        character_expression_assignment = (
            character_expression.get("template_assignments", {})
            .get("value", {})
            .get("arrival_greeting", {})
            .get("value", {})
        )
        character_presentation_value = (
            character_profile.get("presentation", {}).get("value", {})
        )
        character_presentation_avatar = (
            character_presentation_value.get("catalog_assignments", {})
            .get("value", {})
            .get("avatar", {})
            .get("value", {})
        )
        if (
            character_story.get("version") != 4
            or character_module.get("version") != "1.5.0"
            or character_profile.get("identity", {})
            .get("value", {})
            .get("id", {})
            .get("value")
            != "org.weave.character.ari_vale"
            or character_profile.get("hexaco", {})
            .get("value", {})
            .get("openness", {})
            .get("value", {})
            .get("creativity", {})
            .get("value", {})
            .get("projection_score", {})
            .get("value")
            != 0.86
            or character_profile.get("ocean", {})
            .get("value", {})
            .get("lossy", {})
            .get("value")
            is not True
            or character_profile.get("ocean", {})
            .get("value", {})
            .get("independent_evidence", {})
            .get("value")
            is not False
            or character_alignment.get(
                "canonical_personality_write_back", {}
            ).get("value")
            is not False
            or list(character_alignment_values)
            != ["horizon", "reciprocity", "structure"]
            or character_alignment_values.get("horizon", {})
            .get("value", {})
            .get("decision", {})
            .get("value")
            != "reviewed"
            or character_alignment_values.get("reciprocity", {})
            .get("value", {})
            .get("decision", {})
            .get("value")
            != "edited"
            or character_alignment_values.get("structure", {})
            .get("value", {})
            .get("decision", {})
            .get("value")
            != "overridden"
            or len(
                character_alignment.get("pack", {})
                .get("value", {})
                .get("sha256", {})
                .get("value", "")
            )
            != 64
            or len(character_alignment.get("review_sha256", {}).get("value", ""))
            != 64
            or len(character_alignment.get("applied_sha256", {}).get("value", ""))
            != 64
            or character_relationships.get(
                "canonical_personality_write_back", {}
            ).get("value")
            is not False
            or character_relationships.get("graph_format_version", {}).get("value")
            != 1.0
            or character_relationship_pack.get("id", {}).get("value")
            != "org.weave.relationship.reference"
            or character_relationship_pack.get("version", {}).get("value") != "1.0.0"
            or len(character_relationship_pack.get("sha256", {}).get("value", ""))
            != 64
            or character_mentor_edge.get("target_character_id", {}).get("value")
            != "org.weave.character.sable_reed"
            or character_mentor_edge.get("origin", {}).get("value") != "authored"
            or character_expression.get(
                "canonical_personality_write_back", {}
            ).get("value")
            is not False
            or character_expression_term.get("surface", {}).get("value")
            != "trailmark"
            or character_expression_term.get("origin", {}).get("value")
            != "pack_assigned"
            or character_expression_assignment.get("template_id", {}).get("value")
            != "arrival_greeting"
            or len(
                character_expression_assignment.get("pack", {})
                .get("value", {})
                .get("sha256", {})
                .get("value", "")
            )
            != 64
            or character_presentation_value.get(
                "canonical_personality_write_back", {}
            ).get("value")
            is not False
            or character_presentation_value.get("pronouns", {})
            .get("value", {})
            .get("subject", {})
            .get("value")
            != "they"
            or character_presentation_value.get("palette", {})
            .get("value", {})
            .get("colors", {})
            .get("value", {})
            .get("accent", {})
            .get("value")
            != "#D6A24A"
            or character_presentation_value.get("assets", {})
            .get("value", {})
            .get("authored_avatar", {})
            .get("value", {})
            .get("path", {})
            .get("value")
            != "presentation/assets/ari-vale-avatar.svg"
            or character_presentation_avatar.get("lock", {}).get("value")
            != "locked"
            or len(
                character_presentation_avatar.get("catalog", {})
                .get("value", {})
                .get("sha256", {})
                .get("value", "")
            )
            != 64
        ):
            raise DocsError(
                "compiled Character fixture omitted typed presentation, relationships, evidence, derivation labels, or approved alignment"
            )

        for encoding, output, checked in (
            ("ron", compiled_temporal_ron, temporal_story_ron),
            ("json", compiled_temporal_json, temporal_story_json),
        ):
            run(
                [
                    str(compiler),
                    str(temporal_source.relative_to(ROOT)),
                    "--module-manifest",
                    str(temporal_manifest_json.relative_to(ROOT)),
                    "--module-pack",
                    str(temporal_domain_pack_json.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
            if output.read_bytes() != checked.read_bytes():
                raise DocsError(
                    f"compiled temporal Character fixture is stale: {checked.name}"
                )
        run(
            [
                str(compiler),
                str(temporal_source.relative_to(ROOT)),
                "--locked",
                "--format",
                "json",
                "--output",
                str(compiled_temporal_locked_json),
            ],
            capture=True,
        )
        if (
            compiled_temporal_locked_json.read_bytes()
            != temporal_story_json.read_bytes()
        ):
            raise DocsError(
                "locked temporal Character project output differs from checked Story IR"
            )
        temporal_story = json.loads(compiled_temporal_json.read_text(encoding="utf-8"))
        temporal_module = temporal_story.get("modules", {}).get("character", {})
        temporal_profile = (
            temporal_module.get("exports", {})
            .get("profile", {})
            .get("value", {})
            .get("value", {})
        )
        temporal_context_value = temporal_profile.get("date_context", {}).get(
            "value", {}
        )
        accepted_record_ids = [
            item.get("value")
            for item in temporal_context_value.get("accepted_record_ids", {}).get(
                "value", []
            )
        ]
        temporal_cues = [
            item.get("value", {})
            for item in temporal_context_value.get("cues", {}).get("value", [])
        ]
        apollo_cues = [
            cue
            for cue in temporal_cues
            if cue.get("record_id", {}).get("value") == "apollo_11_lunar_landing"
        ]
        if (
            temporal_story.get("version") != 4
            or temporal_module.get("version") != "1.5.0"
            or temporal_module.get("pack_id") != "ari_vale_temporal"
            or accepted_record_ids
            != [
                "apollo_11_lunar_landing",
                "calendar_midsummer_period",
                "world_coastal_fog_cycle",
            ]
            or temporal_context_value.get("canonical_personality_write_back", {}).get(
                "value"
            )
            is not False
            or len(temporal_cues) != 3
            or len(apollo_cues) != 1
            or [
                item.get("value")
                for item in apollo_cues[0].get("fact_source_ids", {}).get("value", [])
            ]
            != ["apollo_11_wikidata"]
            or [
                item.get("value")
                for item in apollo_cues[0].get("cue_source_ids", {}).get("value", [])
            ]
            != ["weave_historical_cues"]
        ):
            raise DocsError(
                "compiled temporal Character fixture merged lineage or enabled write-back"
            )

        run(
            [
                str(domain_tool),
                "validate-project",
                str((world_fixture / "weave.modules.json").relative_to(ROOT)),
            ],
            capture=True,
        )
        for encoding, output in (
            ("ron", compiled_world_ron),
            ("json", compiled_world_json),
        ):
            run(
                [
                    str(compiler),
                    str(world_source.relative_to(ROOT)),
                    "--module-manifest",
                    str(world_manifest.relative_to(ROOT)),
                    "--module-pack",
                    str(world_pack.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
        for generated, checked in (
            (compiled_world_ron, world_fixture / "reference-place.story.ron"),
            (compiled_world_json, world_fixture / "reference-place.story.json"),
        ):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(
                    f"compiled Weave World fixture is stale: {checked.name}"
                )
        world_story = json.loads(compiled_world_json.read_text(encoding="utf-8"))
        world_seed = (
            world_story.get("modules", {})
            .get("world", {})
            .get("exports", {})
            .get("seed", {})
            .get("value", {})
            .get("value", {})
        )
        if (
            world_story.get("version") != 4
            or world_seed.get("primary_biome", {}).get("value")
            != "temperate_broadleaf_and_mixed_forest"
            or world_seed.get("identity", {})
            .get("value", {})
            .get("culture_included", {})
            .get("value")
            is not False
            or world_seed.get("context", {})
            .get("value", {})
            .get("geographic_resolution", {})
            .get("value")
            != "country_scale_selected_stations"
            or world_seed.get("climate", {})
            .get("value", {})
            .get("units", {})
            .get("value", {})
            .get("precipitation", {})
            .get("value")
            != "millimetres_per_year"
            or "daylight" not in world_seed
            or "hazards" not in world_seed
        ):
            raise DocsError(
                "compiled Weave World fixture omitted its environmental seed"
            )

        for encoding, output in (
            ("ron", compiled_authored_world_ron),
            ("json", compiled_authored_world_json),
        ):
            run(
                [
                    str(compiler),
                    str(authored_world_source.relative_to(ROOT)),
                    "--module-manifest",
                    str(world_manifest.relative_to(ROOT)),
                    "--module-pack",
                    str(world_pack.relative_to(ROOT)),
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
        for generated, checked in (
            (compiled_authored_world_ron, authored_world_ron),
            (compiled_authored_world_json, authored_world_json),
        ):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(
                    f"compiled authored World fixture is stale: {checked.name}"
                )
        authored_story = json.loads(
            compiled_authored_world_json.read_text(encoding="utf-8")
        )
        authored_module = authored_story.get("modules", {}).get("world", {})
        authored_exports = authored_module.get("exports", {})
        authored_places = (
            authored_exports.get("places", {}).get("value", {}).get("value", {})
        )
        authored_rules = (
            authored_exports.get("rules", {}).get("value", {}).get("value", {})
        )
        if (
            authored_module.get("version") != "1.1.0"
            or authored_module.get("pack_version") != "1.1.0"
            or len(authored_module.get("authored_overrides", [])) != 42
            or authored_rules.get("booleans", {})
            .get("value", {})
            .get("beacons_answer_storms", {})
            .get("value")
            is not True
            or authored_places.get("emberwake_harbor", {})
            .get("value", {})
            .get("parent_id", {})
            .get("value")
            != "glasswind_reach"
            or authored_places.get("lantern_road", {})
            .get("value", {})
            .get("name", {})
            .get("value")
            != "Lantern Road"
            or len(authored_story.get("modules", {})) != 1
        ):
            raise DocsError(
                "compiled authored World layer omitted rules, places, or lineage"
            )

        for encoding, output in (
            ("ron", compiled_composed_world_ron),
            ("json", compiled_composed_world_json),
        ):
            run(
                [
                    str(compiler),
                    str(composed_world_source.relative_to(ROOT)),
                    "--locked",
                    "--format",
                    encoding,
                    "--output",
                    str(output),
                ],
                capture=True,
            )
        for generated, checked in (
            (compiled_composed_world_ron, composed_world_story_ron),
            (compiled_composed_world_json, composed_world_story_json),
        ):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(
                    f"compiled composed World fixture is stale: {checked.name}"
                )
        composed_story = json.loads(
            compiled_composed_world_json.read_text(encoding="utf-8")
        )
        composed_module = composed_story.get("modules", {}).get("world", {})
        composed_seed = (
            composed_module.get("exports", {})
            .get("seed", {})
            .get("value", {})
            .get("value", {})
        )
        if (
            composed_module.get("pack_id") != "glasswind_composed"
            or composed_module.get("pack_version") != "1.0.0"
            or len(composed_module.get("authored_overrides", [])) != 46
            or composed_seed.get("primary_biome", {}).get("value")
            != "temperate_conifer_forest"
            or composed_seed.get("climate", {})
            .get("value", {})
            .get("band", {})
            .get("value")
            != "humid_continental"
            or composed_seed.get("identity", {})
            .get("value", {})
            .get("preset", {})
            .get("value")
            != "glasswind_composed"
        ):
            raise DocsError(
                "compiled composed World omitted layer or authored precedence"
            )

        full_export = json.loads(full_world_json.read_text(encoding="utf-8"))
        compact_export_text = compact_world_json.read_text(encoding="utf-8")
        compact_export = json.loads(compact_export_text)
        if (
            full_export.get("format_version") != 1
            or len(full_export.get("composition", {}).get("layers", [])) != 3
            or len(full_export.get("authored_override_paths", [])) != 46
            or compact_export.get("format_version") != 1
            or compact_export.get("pack_id") != "glasswind_composed"
            or compact_export.get("places", {})
            .get("emberwake_harbor", {})
            .get("environment", {})
            .get("primary_biome", {})
            .get("value")
            != "temperate_conifer_forest"
            or any(
                forbidden in compact_export_text
                for forbidden in (
                    '"authored_override_paths"',
                    '"composition"',
                    '"origins"',
                    '"source_path"',
                )
            )
        ):
            raise DocsError("portable World full/compact lineage boundary is invalid")

        observed_world_presets = {
            "aotearoa_new_zealand": "country_scale_selected_stations"
        }
        for (
            source,
            pack,
            checked_ron,
            checked_json,
            expected_preset,
            expected_resolution,
        ) in world_story_fixtures:
            generated_ron = workspace / checked_ron.name
            generated_json = workspace / checked_json.name
            for encoding, output in (("ron", generated_ron), ("json", generated_json)):
                run(
                    [
                        str(compiler),
                        str(source.relative_to(ROOT)),
                        "--module-manifest",
                        str(world_manifest.relative_to(ROOT)),
                        "--module-pack",
                        str(pack.relative_to(ROOT)),
                        "--format",
                        encoding,
                        "--output",
                        str(output),
                    ],
                    capture=True,
                )
            for generated, checked in (
                (generated_ron, checked_ron),
                (generated_json, checked_json),
            ):
                if generated.read_bytes() != checked.read_bytes():
                    raise DocsError(
                        f"compiled Weave World corpus story is stale: {checked.name}"
                    )
            story = json.loads(generated_json.read_text(encoding="utf-8"))
            seed = (
                story.get("modules", {})
                .get("world", {})
                .get("exports", {})
                .get("seed", {})
                .get("value", {})
                .get("value", {})
            )
            preset = (
                seed.get("identity", {}).get("value", {}).get("preset", {}).get("value")
            )
            resolution = (
                seed.get("context", {})
                .get("value", {})
                .get("geographic_resolution", {})
                .get("value")
            )
            culture_included = (
                seed.get("identity", {})
                .get("value", {})
                .get("culture_included", {})
                .get("value")
            )
            if (
                story.get("version") != 4
                or preset != expected_preset
                or resolution != expected_resolution
                or culture_included is not False
                or "daylight" not in seed
                or "hazards" not in seed
            ):
                raise DocsError(
                    f"compiled world corpus seed is incomplete: {expected_preset}"
                )
            observed_world_presets[preset] = resolution
        if (
            len(observed_world_presets) != 4
            or len(set(observed_world_presets.values())) != 4
        ):
            raise DocsError(
                "world corpus fixtures are not distinct across four geographic scales"
            )

        publication = workspace / "publication"
        registry = workspace / "registry"
        for command, destination in (("publish", publication), ("install", registry)):
            flag = "--output" if command == "publish" else "--registry"
            run(
                [
                    str(domain_tool),
                    command,
                    str(manifest_json.relative_to(ROOT)),
                    "--pack",
                    str(pack_json.relative_to(ROOT)),
                    flag,
                    str(destination),
                ],
                capture=True,
            )
        generated_index = workspace / "registry-index.json"
        run(
            [
                str(domain_tool),
                "index",
                "--registry",
                str(registry),
                "--output",
                str(generated_index),
            ],
            capture=True,
        )
        index = json.loads(generated_index.read_text(encoding="utf-8"))
        if index.get("schema_version") != 1 or len(index.get("modules", [])) != 1:
            raise DocsError("installed domain registry index is incomplete")

        tutorial_source = ROOT / "examples/domain-modules/third-party-tutorial"
        tutorial = workspace / "third-party-tutorial"
        shutil.copytree(tutorial_source, tutorial)
        run(
            [
                str(domain_tool),
                "validate-project",
                str(tutorial / "weave.modules.json"),
            ],
            capture=True,
        )
        tutorial_output = workspace / "tutorial.story.json"
        run(
            [
                str(compiler),
                str(tutorial / "story.weave"),
                "--format",
                "json",
                "--output",
                str(tutorial_output),
            ],
            capture=True,
        )
        if (tutorial / "weave.lock").read_bytes() != (
            tutorial_source / "weave.lock"
        ).read_bytes():
            raise DocsError("the checked-in third-party tutorial lock is stale")
        run(
            [
                str(compiler),
                str(tutorial / "story.weave"),
                "--locked",
                "--format",
                "json",
                "--output",
                str(tutorial_output),
            ],
            capture=True,
        )
        tutorial_story = json.loads(tutorial_output.read_text(encoding="utf-8"))
        if (
            tutorial_story.get("modules", {})
            .get("weather", {})
            .get("exports", {})
            .get("condition", {})
            .get("value", {})
            .get("value")
            != "rain"
        ):
            raise DocsError("third-party tutorial module data was not embedded")

    run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "weave-world",
            "--example",
            "world_fixture",
            "--",
            "--check",
        ],
        capture=True,
        environment=cargo_environment(),
    )
    print(
        "verified domain schemas, packaging, locks, tutorial, tracer, four-preset World corpus, deterministic World composition/exports, Character contract/synthesis/presentation catalogs/explainable alignment/reviewed temporal context/domain projection, authored hierarchy, and optional naming pack",
        flush=True,
    )


def verify_tabletop_contract() -> None:
    """Verify the adapter schemas, portable pairs, replay fixture, and license gate."""

    tool = tabletop_tool_binary()
    compiler = compiler_binary()
    fixture = ROOT / "examples/tabletop-adapters/contract"
    manifest_json = fixture / "synthetic.tabletop-adapter.json"
    manifest_ron = fixture / "synthetic.tabletop-adapter.ron"
    selection_json = fixture / "selection.tabletop-selection.json"
    selection_ron = fixture / "selection.tabletop-selection.ron"
    projection_json = fixture / "character.tabletop-projection.json"
    projection_ron = fixture / "character.tabletop-projection.ron"
    state_json = fixture / "state.tabletop-state.json"
    state_ron = fixture / "state.tabletop-state.ron"
    request_json = fixture / "request.tabletop-request.json"
    request_ron = fixture / "request.tabletop-request.ron"
    receipt_json = fixture / "receipt.tabletop-receipt.json"
    receipt_ron = fixture / "receipt.tabletop-receipt.ron"

    run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "weave-tabletop",
            "--example",
            "tabletop_fixture",
            "--",
            "--check",
        ],
        capture=True,
        environment=cargo_environment(),
    )
    for artifact in (manifest_json, manifest_ron):
        run([str(tool), "validate", "manifest", str(artifact.relative_to(ROOT))])
    for artifact in (selection_json, selection_ron):
        run(
            [
                str(tool),
                "validate",
                "selection",
                str(artifact.relative_to(ROOT)),
                "--manifest",
                str(manifest_json.relative_to(ROOT)),
            ]
        )
    for artifact in (projection_json, projection_ron):
        run(
            [
                str(tool),
                "validate",
                "projection",
                str(artifact.relative_to(ROOT)),
                "--manifest",
                str(manifest_json.relative_to(ROOT)),
            ]
        )
    for artifact in (state_json, state_ron):
        run(
            [
                str(tool),
                "validate",
                "state",
                str(artifact.relative_to(ROOT)),
                "--manifest",
                str(manifest_json.relative_to(ROOT)),
            ]
        )
    for artifact, state in ((request_json, state_json), (request_ron, state_ron)):
        run(
            [
                str(tool),
                "validate",
                "request",
                str(artifact.relative_to(ROOT)),
                "--manifest",
                str(manifest_json.relative_to(ROOT)),
                "--state",
                str(state.relative_to(ROOT)),
            ]
        )
    for artifact in (receipt_json, receipt_ron):
        run(
            [
                str(tool),
                "validate",
                "receipt",
                str(artifact.relative_to(ROOT)),
                "--manifest",
                str(manifest_json.relative_to(ROOT)),
                "--request",
                str(request_json.relative_to(ROOT)),
            ]
        )
    run(
        [
            str(tool),
            "license-gate",
            str(manifest_json.relative_to(ROOT)),
            "--source-artifact",
            str((fixture / "source/synthetic-rules.txt").relative_to(ROOT)),
            "--license-text",
            str((fixture / "LICENSE").relative_to(ROOT)),
        ]
    )

    schemas = {
        "manifest": "weave-tabletop-adapter-manifest-v1.schema.json",
        "selection": "weave-tabletop-adapter-selection-v1.schema.json",
        "projection": "weave-tabletop-character-projection-v1.schema.json",
        "state": "weave-tabletop-state-v1.schema.json",
        "request": "weave-tabletop-resolution-request-v1.schema.json",
        "receipt": "weave-tabletop-resolution-receipt-v1.schema.json",
    }
    with tempfile.TemporaryDirectory(prefix="weave-tabletop-contract-") as temporary:
        workspace = Path(temporary)
        for kind, name in schemas.items():
            generated = workspace / name
            run([str(tool), "schema", kind, "--output", str(generated)])
            if generated.read_bytes() != (ROOT / "schemas" / name).read_bytes():
                raise DocsError(f"checked tabletop schema is stale: {name}")
        runtime = workspace / "runtime.tabletop-receipt.json"
        run(
            [
                str(tool),
                "project-events",
                str(receipt_json.relative_to(ROOT)),
                "--manifest",
                str(manifest_json.relative_to(ROOT)),
                "--audience",
                "runtime",
                "--output",
                str(runtime),
            ]
        )
        if runtime.read_bytes() != (fixture / "runtime.tabletop-receipt.json").read_bytes():
            raise DocsError("checked tabletop runtime event projection is stale")
        compiled = workspace / "lantern-trail.story.json"
        run(
            [
                str(compiler),
                str((fixture / "runtime/lantern-trail.weave").relative_to(ROOT)),
                "--module-manifest",
                str((fixture / "runtime/module.weave-module.json").relative_to(ROOT)),
                "--module-pack",
                str((fixture / "runtime/lumen_reed.weave-domain.json").relative_to(ROOT)),
                "--format",
                "json",
                "--output",
                str(compiled),
            ]
        )
        if compiled.read_bytes() != (fixture / "runtime/lantern-trail.story.json").read_bytes():
            raise DocsError("checked tabletop source projection is stale")

    manifest = json.loads(manifest_json.read_text(encoding="utf-8"))
    selection = json.loads(selection_json.read_text(encoding="utf-8"))
    projection = json.loads(projection_json.read_text(encoding="utf-8"))
    runtime = json.loads(
        (fixture / "runtime.tabletop-receipt.json").read_text(encoding="utf-8")
    )
    if (
        manifest.get("extension_surface", {}).get("kind")
        != "declarative_data_with_registered_resolver"
        or manifest.get("provenance", {}).get("license") != "MIT"
        or len(selection.get("primary", [])) != 1
        or projection.get("canonical_character_write_back") is not False
        or sum(event.get("payload") is not None for event in runtime.get("events", []))
        != 1
    ):
        raise DocsError(
            "tabletop fixture omitted declarative isolation, exact selection, write-back prohibition, or runtime redaction"
        )
    print(
        "verified tabletop adapter selection, schemas, isolated state, deterministic replay fixture, typed event visibility, and source-license gate",
        flush=True,
    )


def run_finite_examples() -> None:
    """Exercise the non-interactive Rust example programs advertised by the site."""

    examples = (
        ("weave-example-standalone", []),
        ("weave-example-bevy-dialogue", ["--", "--smoke-test"]),
        ("weave-example-domain-module-bevy", []),
    )
    for package, arguments in examples:
        run(
            ["cargo", "run", "--locked", "--quiet", "-p", package, *arguments],
            capture=True,
            environment=cargo_environment(),
        )


def verify_pixijs_domain_consumer() -> None:
    """Install, test, and bundle the portable PixiJS module consumer."""

    example = "examples/domain-module-pixijs"
    run(["npm", "--prefix", example, "ci"], capture=True)
    run(["npm", "--prefix", example, "test"], capture=True)
    run(["npm", "--prefix", example, "run", "build"], capture=True)
    print("verified the PixiJS domain-module consumer", flush=True)


def build_rustdoc() -> Path:
    """Generate public API documentation with warnings denied."""

    command = ["cargo", "doc", "--locked", "--no-deps"]
    for package in RUSTDOC_PACKAGES:
        command.extend(("-p", package))
    environment = cargo_environment()
    existing_flags = environment.get("RUSTDOCFLAGS", "").strip()
    environment["RUSTDOCFLAGS"] = f"{existing_flags} -D warnings".strip()
    run(command, environment=environment)
    return cargo_target_directory() / "doc"


def mdbook_binary() -> str:
    binary = os.environ.get("MDBOOK", "mdbook")
    result = run([binary, "--version"], capture=True)
    match = re.search(r"(\d+\.\d+\.\d+)", result.stdout)
    version = match.group(1) if match else None
    if version != MDBOOK_VERSION:
        raise DocsError(
            f"mdBook {MDBOOK_VERSION} is required, found {version!r}; "
            f"install it with `cargo install mdbook --version {MDBOOK_VERSION} --locked`"
        )
    return binary


def build_site(rustdoc: Path, mdbook: str) -> None:
    """Render mdBook, then add generated API docs and downloadable schema."""

    run([mdbook, "build", str(ROOT)])
    api_destination = BOOK / "api"
    if api_destination.exists():
        shutil.rmtree(api_destination)
    shutil.copytree(rustdoc, api_destination)

    downloads = BOOK / "downloads"
    downloads.mkdir(parents=True, exist_ok=True)
    for schema in (
        "weave-story-ir-v2.schema.json",
        "weave-story-ir-v3.schema.json",
        "weave-story-ir-v4.schema.json",
        "community-pattern-package-v1.schema.json",
        "domain-module-manifest-v1.schema.json",
        "domain-pack-v1.schema.json",
        "domain-project-v1.schema.json",
        "domain-lock-v1.schema.json",
        "domain-registry-index-v1.schema.json",
        "weave-world-corpus-index-v1.schema.json",
        "weave-world-corpus-preset-v1.schema.json",
        "weave-world-composition-v1.schema.json",
        "weave-world-full-v1.schema.json",
        "weave-world-compact-v1.schema.json",
        "weave-character-profile-v1.schema.json",
        "weave-character-template-v1.schema.json",
        "weave-character-overlay-v1.schema.json",
        "weave-character-synthesis-v1.schema.json",
        "weave-character-diagnostic-v1.schema.json",
        "weave-character-collection-v1.schema.json",
        "weave-character-operation-request-v1.schema.json",
        "weave-character-proposal-v1.schema.json",
        "weave-character-review-v1.schema.json",
        "weave-character-progress-v1.schema.json",
        "weave-character-temporal-pack-v1.schema.json",
        "weave-character-temporal-config-v1.schema.json",
        "weave-character-temporal-proposal-v1.schema.json",
        "weave-character-temporal-review-v1.schema.json",
        "weave-character-temporal-receipt-v1.schema.json",
        "weave-character-alignment-pack-v1.schema.json",
        "weave-character-alignment-config-v1.schema.json",
        "weave-character-alignment-proposal-v1.schema.json",
        "weave-character-alignment-review-v1.schema.json",
        "weave-character-alignment-receipt-v1.schema.json",
        "weave-character-expression-pack-v1.schema.json",
        "weave-character-expression-revision-v1.schema.json",
        "weave-character-expression-assignment-request-v1.schema.json",
        "weave-character-expression-assignment-receipt-v1.schema.json",
        "weave-character-expression-resolution-request-v1.schema.json",
        "weave-character-expression-resolution-v1.schema.json",
        "weave-character-expression-lint-v1.schema.json",
        "weave-character-expression-coverage-v1.schema.json",
        "weave-character-relationship-kind-pack-v1.schema.json",
        "weave-character-relationship-policy-v1.schema.json",
        "weave-character-relationship-config-v1.schema.json",
        "weave-character-relationship-proposal-v1.schema.json",
        "weave-character-relationship-review-v1.schema.json",
        "weave-character-relationship-receipt-v1.schema.json",
        "weave-character-relationship-revision-v1.schema.json",
        "weave-character-relationship-reconciliation-v1.schema.json",
        "weave-character-presentation-catalog-v1.schema.json",
        "weave-character-presentation-request-v1.schema.json",
        "weave-character-presentation-proposal-v1.schema.json",
        "weave-character-presentation-review-v1.schema.json",
        "weave-character-presentation-receipt-v1.schema.json",
        "weave-character-presentation-lock-revision-v1.schema.json",
        "weave-character-authoring-workspace-v1.schema.json",
        "weave-character-authoring-revision-v1.schema.json",
        "weave-character-authoring-preview-v1.schema.json",
        "weave-character-questionnaire-pack-v1.schema.json",
        "weave-character-questionnaire-answers-v1.schema.json",
        "weave-character-questionnaire-proposal-v1.schema.json",
        "weave-character-questionnaire-review-v1.schema.json",
        "weave-character-questionnaire-receipt-v1.schema.json",
        "weave-character-final-review-v1.schema.json",
        "weave-tabletop-adapter-manifest-v1.schema.json",
        "weave-tabletop-adapter-selection-v1.schema.json",
        "weave-tabletop-character-projection-v1.schema.json",
        "weave-tabletop-state-v1.schema.json",
        "weave-tabletop-resolution-request-v1.schema.json",
        "weave-tabletop-resolution-receipt-v1.schema.json",
    ):
        shutil.copy2(ROOT / "schemas" / schema, downloads / schema)
    (BOOK / ".nojekyll").write_text("", encoding="utf-8")


@dataclass
class HtmlPage:
    """Relevant accessibility and link facts from one rendered page."""

    language: str | None = None
    title: str = ""
    in_title: bool = False
    main_count: int = 0
    ids: set[str] = field(default_factory=set)
    links: list[str] = field(default_factory=list)
    images_without_alt: int = 0


class PageParser(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.page = HtmlPage()

    def handle_starttag(
        self, tag: str, attributes: list[tuple[str, str | None]]
    ) -> None:
        values = dict(attributes)
        if tag == "html":
            self.page.language = values.get("lang")
        if tag == "title":
            self.page.in_title = True
        if tag == "main" or values.get("role") == "main":
            self.page.main_count += 1
        if identifier := values.get("id"):
            self.page.ids.add(identifier)
        if tag == "a" and (href := values.get("href")):
            self.page.links.append(href)
        if tag == "img" and "alt" not in values:
            self.page.images_without_alt += 1

    def handle_endtag(self, tag: str) -> None:
        if tag == "title":
            self.page.in_title = False

    def handle_data(self, data: str) -> None:
        if self.page.in_title:
            self.page.title += data


def parse_site_pages() -> dict[Path, HtmlPage]:
    pages: dict[Path, HtmlPage] = {}
    for path in sorted(BOOK.rglob("*.html")):
        relative = path.relative_to(BOOK)
        if (
            relative.parts and relative.parts[0] == "api"
        ) or relative.name == "toc.html":
            continue
        parser = PageParser()
        parser.feed(path.read_text(encoding="utf-8"))
        pages[path.resolve()] = parser.page
    return pages


def local_html_target(page: Path, raw_href: str) -> tuple[Path, str] | None:
    parsed = urllib.parse.urlsplit(raw_href)
    if parsed.scheme or raw_href.startswith("//"):
        return None
    link_path = urllib.parse.unquote(parsed.path)
    if link_path.startswith("/weave/"):
        target = BOOK / link_path.removeprefix("/weave/")
    elif link_path == "/weave":
        target = BOOK / "index.html"
    elif link_path.startswith("/"):
        raise DocsError(f"unexpected root-relative site link in {page}: {raw_href}")
    elif not link_path:
        target = page
    else:
        target = page.parent / link_path
    if target.is_dir() or link_path.endswith("/"):
        target /= "index.html"
    return target.resolve(), urllib.parse.unquote(parsed.fragment)


def check_rendered_site() -> None:
    """Check local HTML links plus baseline search and accessibility structure."""

    pages = parse_site_pages()
    if not pages:
        raise DocsError("mdBook produced no HTML pages")

    failures: list[str] = []
    saw_skip_link = False
    for path, page in pages.items():
        relative = path.relative_to(BOOK.resolve())
        if page.language != "en":
            failures.append(f"{relative}: expected html lang=en")
        if not page.title.strip():
            failures.append(f"{relative}: missing document title")
        if page.main_count != 1:
            failures.append(
                f"{relative}: expected one main landmark, found {page.main_count}"
            )
        if page.images_without_alt:
            failures.append(
                f"{relative}: {page.images_without_alt} image(s) have no alt attribute"
            )

        for href in page.links:
            if href in {"#main", "#mdbook-content"}:
                saw_skip_link = True
            if "/docs/docs/" in href:
                failures.append(
                    f"{relative}: duplicated documentation path in link: {href}"
                )
            resolved = local_html_target(path, href)
            if resolved is None:
                continue
            target, fragment = resolved
            if target.suffix == ".md":
                failures.append(
                    f"{relative}: rendered link still targets Markdown: {href}"
                )
                continue
            if not target.exists():
                failures.append(f"{relative}: missing local target {href}")
                continue
            target_page = pages.get(target)
            if fragment and target_page is not None and fragment not in target_page.ids:
                failures.append(f"{relative}: missing fragment {href}")

    if not saw_skip_link:
        failures.append("site has no skip link to the main content")

    search_indexes = [
        path
        for path in BOOK.glob("searchindex*")
        if path.is_file() and path.stat().st_size > 0
    ]
    if not search_indexes:
        failures.append("site search index is missing or empty")

    index = (BOOK / "index.html").read_text(encoding="utf-8")
    if "search" not in index.lower():
        failures.append("site navigation does not expose search")

    rendered_text = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (BOOK / "contributing.html", BOOK / "security.html")
    )
    for required in (
        "CONTRIBUTING.md",
        "SECURITY.md",
        "security/advisories/new",
        "[REDACTED]",
    ):
        if required not in rendered_text:
            failures.append(f"project guidance is missing {required!r}")

    for package in RUSTDOC_PACKAGES:
        rustdoc_name = package.replace("-", "_")
        entry = BOOK / "api" / rustdoc_name / "index.html"
        if not entry.is_file():
            failures.append(
                f"generated API entry point is missing: {entry.relative_to(BOOK)}"
            )

    if failures:
        raise DocsError(
            "documentation site verification failed:\n  " + "\n  ".join(failures)
        )
    print(f"verified {len(pages)} rendered documentation pages", flush=True)


def main() -> None:
    mdbook = mdbook_binary()
    check_versions()
    check_source_links()
    check_quickstart_source()
    compile_examples()
    verify_community_package()
    verify_domain_contract()
    verify_tabletop_contract()
    run_finite_examples()
    verify_pixijs_domain_consumer()
    rustdoc = build_rustdoc()
    build_site(rustdoc, mdbook)
    check_rendered_site()
    print(f"documentation site ready at {BOOK.relative_to(ROOT)}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except DocsError as error:
        print(f"docs: {error}", file=sys.stderr)
        raise SystemExit(1) from None
