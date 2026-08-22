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
        raise DocsError(f"command failed with status {result.returncode}: {shlex.join(command)}")
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
    guide_match = re.search(r"^Language specification version: `([^`]+)`\.$", guide, re.MULTILINE)
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
        raise DocsError(f"documentation pages are absent from SUMMARY.md: {sorted(unlisted)}")

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
        raise DocsError("broken source documentation links:\n  " + "\n  ".join(failures))


def check_quickstart_source() -> None:
    """Ensure the visible first story is the checked-in runnable fixture."""

    page = (DOCS / "getting_started.md").read_text(encoding="utf-8")
    examples = [block.strip() for block in WEAVE_FENCE.findall(page)]
    expected = (ROOT / "examples" / "stories" / "basic.weave").read_text(
        encoding="utf-8"
    ).strip()
    if expected not in examples:
        raise DocsError("the quickstart Weave block has drifted from examples/stories/basic.weave")


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


def compile_examples() -> None:
    """Compile every public story and compare the browser JSON artifact."""

    compiler = compiler_binary()
    domain_tracer = ROOT / "examples/domain-modules/contract/tracer.weave"
    domain_tutorial = ROOT / "examples/domain-modules/third-party-tutorial/story.weave"
    domain_world = ROOT / "examples/domain-modules/weave-world/reference-place.weave"
    sources = [
        source
        for source in sorted((ROOT / "examples").rglob("*.weave"))
        if source not in {domain_tracer, domain_tutorial, domain_world}
    ]
    readme_fixture = ROOT / "crates" / "weave-core" / "tests" / "fixtures" / "fortune_teller.weave"
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
            raise DocsError("examples/web-player/story.json is stale; rebuild the web player")

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
            str(domain_tool),
            "validate",
            str(world_manifest.relative_to(ROOT)),
            "--pack",
            str(world_pack.relative_to(ROOT)),
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
        normalized_manifest_json = workspace / "module.weave-module.json"
        normalized_manifest_ron = workspace / "module.weave-module.ron"
        normalized_pack_json = workspace / "pack.weave-domain.json"
        normalized_pack_ron = workspace / "pack.weave-domain.ron"
        normalized_world_manifest_json = workspace / "world-module.weave-module.json"
        normalized_world_pack_json = workspace / "world-pack.weave-domain.json"
        compiled_tracer_ron = workspace / "tracer.story.ron"
        compiled_tracer_json = workspace / "tracer.story.json"
        compiled_world_ron = workspace / "reference-place.story.ron"
        compiled_world_json = workspace / "reference-place.story.json"

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
        for generated, checked in (
            (
                generated_manifest_schema,
                ROOT / "schemas" / "domain-module-manifest-v1.schema.json",
            ),
            (generated_pack_schema, ROOT / "schemas" / "domain-pack-v1.schema.json"),
            (generated_project_schema, ROOT / "schemas" / "domain-project-v1.schema.json"),
            (generated_lock_schema, ROOT / "schemas" / "domain-lock-v1.schema.json"),
            (
                generated_registry_schema,
                ROOT / "schemas" / "domain-registry-index-v1.schema.json",
            ),
        ):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"checked-in domain schema is stale: {checked.name}")

        normalization_jobs = (
            ("manifest", manifest_json, "json", normalized_manifest_json),
            ("manifest", manifest_json, "ron", normalized_manifest_ron),
            ("pack", pack_json, "json", normalized_pack_json),
            ("pack", pack_json, "ron", normalized_pack_ron),
            ("manifest", world_manifest, "json", normalized_world_manifest_json),
            ("pack", world_pack, "json", normalized_world_pack_json),
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
        for generated, checked in (
            (normalized_manifest_json, manifest_json),
            (normalized_manifest_ron, manifest_ron),
            (normalized_pack_json, pack_json),
            (normalized_pack_ron, pack_ron),
            (normalized_world_manifest_json, world_manifest),
            (normalized_world_pack_json, world_pack),
        ):
            if generated.read_bytes() != checked.read_bytes():
                raise DocsError(f"canonical domain fixture is stale: {checked.name}")

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
            compiled.get("version") != 3
            or module.get("id") != "org.weave.synthetic_constellation"
            or module.get("exports", {}).get("phase", {}).get("value", {}).get("value")
            != "twilight"
        ):
            raise DocsError("compiled domain tracer omitted its selected module data")

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
                    "--locked",
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
                raise DocsError(f"compiled Weave World fixture is stale: {checked.name}")
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
            world_story.get("version") != 3
            or world_seed.get("primary_biome", {}).get("value")
            != "temperate_broadleaf_and_mixed_forest"
            or world_seed.get("identity", {})
            .get("value", {})
            .get("culture_included", {})
            .get("value")
            is not False
        ):
            raise DocsError("compiled Weave World fixture omitted its environmental seed")

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

    print(
        "verified domain module schemas, packaging, locks, tutorial, tracer, and Weave World seed",
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
        "community-pattern-package-v1.schema.json",
        "domain-module-manifest-v1.schema.json",
        "domain-pack-v1.schema.json",
        "domain-project-v1.schema.json",
        "domain-lock-v1.schema.json",
        "domain-registry-index-v1.schema.json",
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

    def handle_starttag(self, tag: str, attributes: list[tuple[str, str | None]]) -> None:
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
        if (relative.parts and relative.parts[0] == "api") or relative.name == "toc.html":
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
            failures.append(f"{relative}: expected one main landmark, found {page.main_count}")
        if page.images_without_alt:
            failures.append(f"{relative}: {page.images_without_alt} image(s) have no alt attribute")

        for href in page.links:
            if href in {"#main", "#mdbook-content"}:
                saw_skip_link = True
            if "/docs/docs/" in href:
                failures.append(f"{relative}: duplicated documentation path in link: {href}")
            resolved = local_html_target(path, href)
            if resolved is None:
                continue
            target, fragment = resolved
            if target.suffix == ".md":
                failures.append(f"{relative}: rendered link still targets Markdown: {href}")
                continue
            if not target.exists():
                failures.append(f"{relative}: missing local target {href}")
                continue
            target_page = pages.get(target)
            if fragment and target_page is not None and fragment not in target_page.ids:
                failures.append(f"{relative}: missing fragment {href}")

    if not saw_skip_link:
        failures.append("site has no skip link to the main content")

    search_indexes = [path for path in BOOK.glob("searchindex*") if path.is_file() and path.stat().st_size > 0]
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
            failures.append(f"generated API entry point is missing: {entry.relative_to(BOOK)}")

    if failures:
        raise DocsError("documentation site verification failed:\n  " + "\n  ".join(failures))
    print(f"verified {len(pages)} rendered documentation pages", flush=True)


def main() -> None:
    mdbook = mdbook_binary()
    check_versions()
    check_source_links()
    check_quickstart_source()
    compile_examples()
    verify_community_package()
    verify_domain_contract()
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
