#!/usr/bin/env python3
"""Build and verify the public-domain example manuscript corpus.

The committed corpus is plain data. Building it needs network access once;
checking it does not, so the repository gate stays local.

Usage:
    python3 scripts/build-corpus.py --check
    python3 scripts/build-corpus.py --fetch
    python3 scripts/build-corpus.py --write

Every source is a Standard Ebooks edition. Standard Ebooks publishes works
that are in the United States public domain and dedicates its own editorial
work under CC0 1.0, so the text and the markup may both be redistributed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import ssl
import sys
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "examples" / "manuscripts"
DEFAULT_WORK = ROOT / "target" / "corpus-src"
OPS = "{http://www.idpf.org/2007/ops}"
USER_AGENT = "weave-corpus-builder/0.1 (+https://github.com/chrisgliddon/weave)"

STATUSES = ((1, "Draft"), (2, "Revised"), (3, "Done"))
LABELS = ((1, "POV"),)
ORDER_GAP = 1024

PROJECT_STUB = """// Manuscript project shell.
//
// The prose lives in weave.manuscript.ron and weave.manuscript/ beside this
// file. This story exists so the editor can open the project; it is not part
// of the public-domain text.

=== start ===
-> END
"""

COMMON_TRANSFORMATIONS = (
    "XHTML parsed and markup removed; paragraphs kept in document order",
    "Zero-width no-break space (U+FEFF) removed",
    "Soft hyphen (U+00AD) removed",
    "Whitespace collapsed within each paragraph",
    "Illustrations, figures, and captions excluded",
)


@dataclass(frozen=True)
class Work:
    """One corpus entry and how to slice it."""

    key: str
    title: str
    author: str
    published: int
    url: str
    artifact: str
    subset: str
    transformations: tuple[str, ...] = ()


WORKS = (
    Work(
        key="alice-adventures-in-wonderland",
        title="Alice's Adventures in Wonderland",
        author="Lewis Carroll",
        published=1865,
        url=(
            "https://standardebooks.org/ebooks/lewis-carroll/"
            "alices-adventures-in-wonderland/john-tenniel"
        ),
        artifact="lewis-carroll_alices-adventures-in-wonderland_john-tenniel.epub",
        subset="Chapters I to XII, complete",
    ),
    Work(
        key="the-time-machine",
        title="The Time Machine",
        author="H. G. Wells",
        published=1895,
        url="https://standardebooks.org/ebooks/h-g-wells/the-time-machine",
        artifact="h-g-wells_the-time-machine.epub",
        subset="Chapters I to XII and the epilogue, complete",
    ),
    Work(
        key="the-adventures-of-sherlock-holmes",
        title="The Adventures of Sherlock Holmes",
        author="Arthur Conan Doyle",
        published=1892,
        url=(
            "https://standardebooks.org/ebooks/arthur-conan-doyle/"
            "the-adventures-of-sherlock-holmes"
        ),
        artifact="arthur-conan-doyle_the-adventures-of-sherlock-holmes.epub",
        subset="Two stories, each split into its numbered parts",
        transformations=(
            "Each story is a chapter; each numbered part is a scene",
        ),
    ),
    Work(
        key="aesops-fables",
        title="Aesop's Fables",
        author="Aesop",
        published=1912,
        url="https://standardebooks.org/ebooks/aesop/fables/v-s-vernon-jones",
        artifact="aesop_fables_v-s-vernon-jones.epub",
        subset="The first 20 fables",
        transformations=(
            "Translated by V. S. Vernon Jones, 1912",
            "Each fable is a scene; its moral is the final paragraph",
        ),
    ),
)

SHERLOCK_STORIES = ("a-scandal-in-bohemia", "the-redheaded-league")
AESOP_LIMIT = 20

ROMAN = (
    "",
    "I",
    "II",
    "III",
    "IV",
    "V",
    "VI",
    "VII",
    "VIII",
    "IX",
    "X",
    "XI",
    "XII",
    "XIII",
    "XIV",
    "XV",
    "XVI",
)


class CorpusError(RuntimeError):
    """Raised when the corpus cannot be built or verified."""


@dataclass
class Chapter:
    """One chapter or scene before it is written out."""

    title: str
    source: str
    paragraphs: list[str]
    children: list["Chapter"]


# --------------------------------------------------------------------------
# Source handling
# --------------------------------------------------------------------------


def ssl_context() -> ssl.SSLContext:
    """Prefer a bundled certificate store when the platform one is incomplete."""

    try:
        import certifi
    except ImportError:
        return ssl.create_default_context()
    return ssl.create_default_context(cafile=certifi.where())


def download(work: Work, directory: Path) -> Path:
    """Download and extract one edition, returning its extracted root."""

    directory.mkdir(parents=True, exist_ok=True)
    epub = directory / work.artifact
    if not epub.is_file():
        request = urllib.request.Request(
            f"{work.url}/downloads/{work.artifact}?source=download",
            headers={"User-Agent": USER_AGENT},
        )
        with urllib.request.urlopen(request, context=ssl_context()) as response:
            epub.write_bytes(response.read())
    if not zipfile.is_zipfile(epub):
        raise CorpusError(
            f"{work.artifact} is not an epub; remove it and fetch again"
        )
    extracted = directory / work.key
    if not extracted.is_dir():
        with zipfile.ZipFile(epub) as archive:
            archive.extractall(extracted)
    text = extracted / "epub" / "text"
    if not text.is_dir():
        raise CorpusError(f"{work.key} has no epub/text directory")
    return extracted


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def digest_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


def text_of(element: ElementTree.Element) -> str:
    """Extract and normalize the text of one element."""

    parts: list[str] = []

    def walk(node: ElementTree.Element) -> None:
        if isinstance(node.tag, str) and local_name(node.tag) == "br":
            parts.append(" ")
        if node.text:
            parts.append(node.text)
        for child in node:
            walk(child)
            if child.tail:
                parts.append(child.tail)

    walk(element)
    text = "".join(parts).replace("\ufeff", "").replace("\u00ad", "")
    return " ".join(text.split())


def parse(path: Path) -> ElementTree.Element:
    try:
        return ElementTree.parse(path).getroot()
    except ElementTree.ParseError as error:
        raise CorpusError(f"{path.name} is not valid XHTML: {error}") from error


def type_tokens(element: ElementTree.Element) -> list[str]:
    return element.get(f"{OPS}type", "").split()


def paragraphs(root: ElementTree.Element) -> list[str]:
    """Every body paragraph under a node, in document order."""

    found: list[str] = []
    for element in root.iter():
        if not isinstance(element.tag, str) or local_name(element.tag) != "p":
            continue
        if "title" in type_tokens(element):
            continue
        text = text_of(element)
        if text:
            found.append(text)
    return found


def title_of(root: ElementTree.Element) -> str:
    """The edition's own title for a chapter, when it has one."""

    for element in root.iter():
        if not isinstance(element.tag, str):
            continue
        if "title" not in type_tokens(element):
            continue
        text = text_of(element)
        if text:
            return text
    return ""


def heading(root: ElementTree.Element, level: str) -> str:
    for element in root.iter():
        if isinstance(element.tag, str) and local_name(element.tag) == level:
            title = text_of(element)
            if title:
                return title
    return ""


def is_chapter_section(element: ElementTree.Element) -> bool:
    kind = element.get(f"{OPS}type", "")
    return "chapter" in kind.split()


# --------------------------------------------------------------------------
# Per-work slicing
# --------------------------------------------------------------------------


def slice_alice(text: Path) -> list[Chapter]:
    chapters: list[Chapter] = []
    for index in range(1, 13):
        source = text / f"chapter-{index}.xhtml"
        if not source.is_file():
            raise CorpusError(f"missing {source.name}")
        root = parse(source)
        chapters.append(
            Chapter(
                title=title_of(root) or f"Chapter {ROMAN[index]}",
                source=source.name,
                paragraphs=paragraphs(root),
                children=[],
            )
        )
    return chapters


def slice_time_machine(text: Path) -> list[Chapter]:
    chapters: list[Chapter] = []
    for index in range(1, 13):
        source = text / f"chapter-{index}.xhtml"
        if not source.is_file():
            raise CorpusError(f"missing {source.name}")
        chapters.append(
            Chapter(
                title=title_of(parse(source)) or f"Chapter {ROMAN[index]}",
                source=source.name,
                paragraphs=paragraphs(parse(source)),
                children=[],
            )
        )
    epilogue = text / "epilogue.xhtml"
    if epilogue.is_file():
        chapters.append(
            Chapter(
                title=title_of(parse(epilogue)) or "Epilogue",
                source=epilogue.name,
                paragraphs=paragraphs(parse(epilogue)),
                children=[],
            )
        )
    return chapters


def slice_sherlock(text: Path) -> list[Chapter]:
    chapters: list[Chapter] = []
    for story in SHERLOCK_STORIES:
        source = text / f"{story}.xhtml"
        if not source.is_file():
            raise CorpusError(f"missing {source.name}")
        root = parse(source)
        sections: list[Chapter] = []
        for element in root.iter():
            if not isinstance(element.tag, str) or local_name(element.tag) != "section":
                continue
            if not is_chapter_section(element):
                continue
            sections.append(
                Chapter(
                    title=title_of(element)
                    or heading(element, "h3")
                    or f"Part {len(sections) + 1}",
                    source=source.name,
                    paragraphs=paragraphs(element),
                    children=[],
                )
            )
        chapters.append(
            Chapter(
                title=title_of(root)
                or heading(root, "h2")
                or story.replace("-", " ").title(),
                source=source.name,
                paragraphs=[] if sections else paragraphs(root),
                children=sections,
            )
        )
    return chapters


def slice_aesop(text: Path) -> list[Chapter]:
    source = text / "fables.xhtml"
    if not source.is_file():
        raise CorpusError("missing fables.xhtml")
    root = parse(source)
    chapters: list[Chapter] = []
    for element in root.iter():
        if not isinstance(element.tag, str) or local_name(element.tag) != "article":
            continue
        title = title_of(element) or heading(element, "h2")
        if not title:
            continue
        chapters.append(
            Chapter(
                title=title,
                source=source.name,
                paragraphs=paragraphs(element),
                children=[],
            )
        )
        if len(chapters) == AESOP_LIMIT:
            break
    if len(chapters) != AESOP_LIMIT:
        raise CorpusError(f"expected {AESOP_LIMIT} fables, found {len(chapters)}")
    return chapters


SLICERS = {
    "alice-adventures-in-wonderland": slice_alice,
    "the-time-machine": slice_time_machine,
    "the-adventures-of-sherlock-holmes": slice_sherlock,
    "aesops-fables": slice_aesop,
}


# --------------------------------------------------------------------------
# Manuscript serialization
# --------------------------------------------------------------------------


def ron_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    escaped = escaped.replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")
    return f'"{escaped}"'


def body_ron(paragraphs_text: list[str]) -> str:
    lines = ["(", "    version: 1,", "    paragraphs: ["]
    for paragraph in paragraphs_text:
        lines.extend(
            [
                "        (",
                "            runs: [",
                "                (",
                f"                    text: {ron_string(paragraph)},",
                "                    character_style: None,",
                "                    bold: false,",
                "                    italic: false,",
                "                    underline: false,",
                "                ),",
                "            ],",
                "            style: None,",
                "        ),",
            ]
        )
    lines.extend(["    ],", ")"])
    return "\n".join(lines) + "\n"


def styles_ron() -> str:
    return "\n".join(
        [
            "(",
            "    version: 1,",
            "    paragraph_styles: [",
            '        (name: "Body", size: 12.0, bold: false, italic: false),',
            "    ],",
            "    character_styles: [",
            '        (name: "Emphasis", size: 12.0, bold: false, italic: true),',
            "    ],",
            ")",
            "",
        ]
    )


def chapter_record(
    identifier: int,
    parent: int,
    order: int,
    kind: str,
    title: str,
) -> str:
    return "\n".join(
        [
            "        (",
            f"            id: ChapterId({identifier}),",
            f"            parent_id: ChapterId({parent}),",
            f"            order: {order},",
            f"            kind: {kind},",
            f"            title: {ron_string(title)},",
            "            status_id: Some(1),",
            "            label_id: None,",
            f"            synopsis: {ron_string('')},",
            "        ),",
        ]
    )


def write_work(work: Work, chapters: list[Chapter], source: Path, target: Path) -> None:
    if target.exists():
        shutil.rmtree(target)
    bodies = target / "weave.manuscript"
    bodies.mkdir(parents=True)

    records: list[str] = []
    provenance_chapters: list[dict[str, object]] = []
    next_id = 2
    total_words = 0
    total_paragraphs = 0

    def emit(chapter: Chapter, parent: int, order: int, kind: str) -> int:
        nonlocal next_id, total_words, total_paragraphs
        identifier = next_id
        next_id += 1
        records.append(chapter_record(identifier, parent, order, kind, chapter.title))
        text = "\n".join(chapter.paragraphs)
        words = len(text.split())
        body_file = bodies / f"{identifier}.ron"
        body_file.write_text(body_ron(chapter.paragraphs), encoding="utf-8")
        total_words += words
        total_paragraphs += len(chapter.paragraphs)
        provenance_chapters.append(
            {
                "id": identifier,
                "title": chapter.title,
                "source": chapter.source,
                "paragraphs": len(chapter.paragraphs),
                "words": words,
                "text_sha256": digest_text(text),
                "body_file": f"weave.manuscript/{identifier}.ron",
                "body_sha256": digest(body_file),
            }
        )
        for index, child in enumerate(chapter.children):
            emit(child, identifier, (index + 1) * ORDER_GAP, "scene")
        return identifier

    for index, chapter in enumerate(chapters):
        emit(chapter, 0, (index + 1) * ORDER_GAP, "chapter")

    (bodies / "styles.ron").write_text(styles_ron(), encoding="utf-8")
    (target / "story.weave").write_text(PROJECT_STUB, encoding="utf-8")

    tree = ["(", "    version: 1,", "    chapters: ["]
    tree.append(chapter_record(0, 0, 0, "chapter", "Manuscript"))
    tree.append(chapter_record(1, 0, 4_294_967_295, "chapter", "Trash"))
    tree.extend(records)
    tree.append("    ],")
    tree.append("    statuses: [")
    for identifier, name in STATUSES:
        tree.append(f'        (id: {identifier}, name: {ron_string(name)}),')
    tree.append("    ],")
    tree.append("    labels: [")
    for identifier, name in LABELS:
        tree.append(
            f"        (id: {identifier}, name: {ron_string(name)}, "
            "color: (0.55, 0.36, 0.96)),"
        )
    tree.append("    ],")
    tree.append(f"    next_id: {next_id},")
    tree.append(")")
    (target / "weave.manuscript.ron").write_text("\n".join(tree) + "\n", encoding="utf-8")

    artifact = source / work.artifact
    provenance = {
        "schema_version": 1,
        "work": {
            "title": work.title,
            "author": work.author,
            "published": work.published,
            "language": "en",
        },
        "source": {
            "publisher": "Standard Ebooks",
            "url": work.url,
            "license": "CC0-1.0",
            "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
            "public_domain": "United States; first published before 1931",
            "retrieved": "2026-10-30",
            "artifact": work.artifact,
            "artifact_sha256": digest(artifact),
            "artifact_bytes": artifact.stat().st_size,
        },
        "subset": work.subset,
        "transformations": list(COMMON_TRANSFORMATIONS + work.transformations),
        "totals": {
            "chapters": len(provenance_chapters),
            "paragraphs": total_paragraphs,
            "words": total_words,
        },
        "chapters": provenance_chapters,
    }
    (target / "provenance.json").write_text(
        json.dumps(provenance, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


# --------------------------------------------------------------------------
# Commands
# --------------------------------------------------------------------------


def fetch_all(directory: Path) -> None:
    for work in WORKS:
        print(f"fetch {work.key}", flush=True)
        download(work, directory)


def write_all(directory: Path, only: str | None) -> None:
    for work in WORKS:
        if only and work.key != only:
            continue
        extracted = directory / work.key
        if not extracted.is_dir():
            raise CorpusError(
                f"{work.key} is not extracted under {directory}; run --fetch first"
            )
        chapters = SLICERS[work.key](extracted / "epub" / "text")
        target = CORPUS / work.key
        write_work(work, chapters, directory, target)
        print(f"wrote {target.relative_to(ROOT)}", flush=True)


def check_all() -> None:
    failures: list[str] = []
    checked = 0
    for work in WORKS:
        target = CORPUS / work.key
        provenance_path = target / "provenance.json"
        if not provenance_path.is_file():
            failures.append(f"{work.key}: missing provenance.json")
            continue
        provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
        tree_path = target / "weave.manuscript.ron"
        if not tree_path.is_file():
            failures.append(f"{work.key}: missing weave.manuscript.ron")
            continue
        for entry in provenance["chapters"]:
            body = target / entry["body_file"]
            if not body.is_file():
                failures.append(f"{work.key}: missing {entry['body_file']}")
                continue
            checked += 1
            if digest(body) != entry["body_sha256"]:
                failures.append(
                    f"{work.key}: {entry['body_file']} does not match its recorded hash"
                )
    if failures:
        raise CorpusError("corpus verification failed:\n  " + "\n  ".join(failures))
    print(f"verified {checked} committed bodies across {len(WORKS)} works", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true", help="download and extract sources")
    parser.add_argument("--write", action="store_true", help="regenerate the committed corpus")
    parser.add_argument("--check", action="store_true", help="verify committed bodies")
    parser.add_argument("--source", type=Path, default=DEFAULT_WORK)
    parser.add_argument("--only", help="restrict to one work key")
    arguments = parser.parse_args()

    try:
        if arguments.fetch:
            fetch_all(arguments.source)
        if arguments.write:
            write_all(arguments.source, arguments.only)
        if arguments.check or not (arguments.fetch or arguments.write):
            check_all()
    except CorpusError as error:
        print(f"corpus: {error}", file=sys.stderr)
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
