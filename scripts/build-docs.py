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
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        check=False,
        text=True,
        capture_output=capture,
    )
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
        return ROOT / "schemas" / "weave-story-ir-v2.schema.json"
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


def compile_examples() -> None:
    """Compile every public story and compare the browser JSON artifact."""

    compiler = compiler_binary()
    sources = sorted((ROOT / "examples").rglob("*.weave"))
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


def run_finite_examples() -> None:
    """Exercise the two non-interactive example programs advertised by the site."""

    for package in ("weave-example-standalone", "weave-example-bevy-dialogue"):
        run(
            ["cargo", "run", "--locked", "--quiet", "-p", package],
            capture=True,
            environment=cargo_environment(),
        )


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
    shutil.copy2(
        ROOT / "schemas" / "weave-story-ir-v2.schema.json",
        downloads / "weave-story-ir-v2.schema.json",
    )
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
    run_finite_examples()
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
