#!/usr/bin/env python3
"""Verify declared presentation assets and their licence/provenance records.

Every scene background, stage image, and variant image referenced by a story
under ``examples/stories`` must have one entry in the presentation asset
manifest. The checker is offline and uses only the Python standard library.

Usage:
    python3 scripts/check-assets.py
    python3 scripts/check-assets.py --manifest path/to/assets.weave-assets.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_STORIES = ROOT / "examples" / "stories"
DEFAULT_MANIFEST = DEFAULT_STORIES / "assets.weave-assets.json"
MANIFEST_VERSION = 1

PROVENANCE_VALUES = ("original", "bundled")
ENTRY_FIELDS = {
    "path",
    "licence",
    "provenance",
    "author",
    "source",
    "sha256",
    "note",
}

SPDX_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9.:+-]*$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
PLACEHOLDER_LICENCES = {
    "",
    "-",
    "all rights reserved",
    "commercial",
    "custom",
    "n/a",
    "na",
    "no licence",
    "no license",
    "none",
    "other",
    "proprietary",
    "public domain",
    "see licence",
    "see license",
    "tbd",
    "todo",
    "unknown",
    "unlicensed",
}

WINDOWS_DEVICE_NAMES = {
    "con",
    "prn",
    "aux",
    "nul",
    *(f"com{number}" for number in range(1, 10)),
    *(f"lpt{number}" for number in range(1, 10)),
}


class LicenceSyntaxError(ValueError):
    """A licence expression is not a syntactically valid SPDX expression."""


@dataclass(frozen=True)
class Token:
    """One lexical token with its 1-based source line."""

    kind: str
    value: str
    line: int


@dataclass(frozen=True)
class AssetReference:
    """One image path declared in a story."""

    path: str
    kind: str
    source: Path
    line: int
    owner: str


def display_path(path: Path) -> str:
    """Render a path relative to the repository when it is inside it."""

    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def decode_string(raw: str) -> str:
    """Decode the escape sequences the Weave string lexer accepts."""

    body = raw[1:-1]
    decoded: list[str] = []
    index = 0
    while index < len(body):
        character = body[index]
        if character == "\\" and index + 1 < len(body):
            following = body[index + 1]
            decoded.append({"n": "\n", "r": "\r", "t": "\t"}.get(following, following))
            index += 2
            continue
        decoded.append(character)
        index += 1
    return "".join(decoded)


def tokenize(text: str) -> list[Token]:
    """Tokenize enough Weave syntax to find presentation declarations."""

    tokens: list[Token] = []
    index = 0
    line = 1
    length = len(text)
    punctuation = {
        "{": "lbrace",
        "}": "rbrace",
        ":": "colon",
        ",": "comma",
    }
    while index < length:
        character = text[index]
        if character == "\n":
            line += 1
            index += 1
            continue
        if character in " \t\r":
            index += 1
            continue
        if text.startswith("//", index):
            newline = text.find("\n", index)
            index = length if newline < 0 else newline
            continue
        if text.startswith("/*", index):
            close = text.find("*/", index + 2)
            end = length if close < 0 else close + 2
            line += text.count("\n", index, end)
            index = end
            continue
        if character == '"':
            start = index
            index += 1
            while index < length and text[index] != '"':
                if text[index] == "\\" and index + 1 < length:
                    index += 2
                else:
                    index += 1
            index = min(index + 1, length)
            tokens.append(Token("string", decode_string(text[start:index]), line))
            continue
        if character in punctuation:
            tokens.append(Token(punctuation[character], character, line))
            index += 1
            continue
        if character.isalpha() or character == "_":
            start = index
            while index < length and (text[index].isalnum() or text[index] == "_"):
                index += 1
            tokens.append(Token("ident", text[start:index], line))
            continue
        tokens.append(Token("other", character, line))
        index += 1
    return tokens


def collect_block(tokens: list[Token], open_index: int) -> tuple[list[Token], int]:
    """Return the tokens inside a brace block and the index after its close."""

    depth = 0
    interior: list[Token] = []
    index = open_index
    while index < len(tokens):
        token = tokens[index]
        if token.kind == "lbrace":
            depth += 1
            if depth > 1:
                interior.append(token)
        elif token.kind == "rbrace":
            depth -= 1
            if depth == 0:
                return interior, index + 1
            interior.append(token)
        else:
            interior.append(token)
        index += 1
    return interior, index


def string_setting(
    tokens: list[Token],
    index: int,
    *,
    kind: str,
    source: Path,
    owner: str,
) -> AssetReference | None:
    """Read a ``name: "value"`` setting at ``index`` when it names a string."""

    if (
        index + 2 < len(tokens)
        and tokens[index + 1].kind == "colon"
        and tokens[index + 2].kind == "string"
    ):
        value = tokens[index + 2]
        return AssetReference(value.value, kind, source, value.line, owner)
    return None


def extract_from_block(
    tokens: list[Token],
    *,
    declaration: str,
    owner: str,
    source: Path,
) -> list[AssetReference]:
    """Extract scene, stage, and variant image paths from one block body."""

    references: list[AssetReference] = []
    index = 0
    depth = 0
    while index < len(tokens):
        token = tokens[index]
        if token.kind == "lbrace":
            depth += 1
            index += 1
            continue
        if token.kind == "rbrace":
            depth -= 1
            index += 1
            continue
        if depth == 0 and token.kind == "ident":
            if declaration == "scene" and token.value == "background":
                reference = string_setting(
                    tokens,
                    index,
                    kind="scene-background",
                    source=source,
                    owner=owner,
                )
                if reference is not None:
                    references.append(reference)
            elif declaration == "stage" and token.value == "image":
                reference = string_setting(
                    tokens,
                    index,
                    kind="stage-image",
                    source=source,
                    owner=owner,
                )
                if reference is not None:
                    references.append(reference)
            elif declaration == "stage" and token.value == "variant":
                if (
                    index + 2 < len(tokens)
                    and tokens[index + 1].kind == "ident"
                    and tokens[index + 2].kind == "lbrace"
                ):
                    variant = tokens[index + 1].value
                    nested, end = collect_block(tokens, index + 2)
                    nested_index = 0
                    nested_depth = 0
                    while nested_index < len(nested):
                        nested_token = nested[nested_index]
                        if nested_token.kind == "lbrace":
                            nested_depth += 1
                        elif nested_token.kind == "rbrace":
                            nested_depth -= 1
                        elif (
                            nested_depth == 0
                            and nested_token.kind == "ident"
                            and nested_token.value == "image"
                        ):
                            reference = string_setting(
                                nested,
                                nested_index,
                                kind="variant-image",
                                source=source,
                                owner=f"{owner}.{variant}",
                            )
                            if reference is not None:
                                references.append(reference)
                        nested_index += 1
                    index = end
                    continue
        index += 1
    return references


def extract_references(source: Path) -> list[AssetReference]:
    """Extract every scene background, stage image, and variant image path."""

    tokens = tokenize(source.read_text(encoding="utf-8"))
    references: list[AssetReference] = []
    index = 0
    while index < len(tokens):
        token = tokens[index]
        if token.kind == "ident" and token.value in {"scene", "stage"}:
            if (
                index + 2 < len(tokens)
                and tokens[index + 1].kind == "ident"
                and tokens[index + 2].kind == "lbrace"
            ):
                owner = tokens[index + 1].value
                interior, end = collect_block(tokens, index + 2)
                references.extend(
                    extract_from_block(
                        interior,
                        declaration=token.value,
                        owner=owner,
                        source=source,
                    )
                )
                index = end
                continue
        index += 1
    return references


def path_problem(path: object) -> str | None:
    """Describe the first way an asset path is not project-relative and safe."""

    if not isinstance(path, str):
        return "path is not a string"
    if not path:
        return "path is empty"
    if path.startswith("~"):
        return "path cannot start with a `~` home-directory prefix"
    if path.startswith("/"):
        return "absolute path escapes the project root"
    if "\\" in path:
        return "path must use `/` separators, not `\\`"
    if ":" in path:
        return "path cannot contain a `:` scheme"
    if "%" in path:
        return "path cannot contain percent-encoded sequences"
    if path.endswith("/"):
        return "path cannot end with `/`"
    if not path.isascii():
        return "path must contain only ASCII characters"
    if any(ord(character) < 32 or ord(character) == 127 for character in path):
        return "path cannot contain control characters"
    for segment in path.split("/"):
        if segment == "":
            return "path cannot contain an empty segment"
        if segment == ".":
            return "path cannot contain a `.` segment"
        if segment == "..":
            return "path escapes the project root with a `..` segment"
        if segment.endswith("."):
            return "no path segment may end with `.`"
        if segment.endswith(" "):
            return "no path segment may end with a space"
        if segment.split(".")[0].lower() in WINDOWS_DEVICE_NAMES:
            return "path cannot use a Windows reserved device name"
    if not re.fullmatch(r"[A-Za-z0-9_./-]+", path):
        return "path may contain only ASCII letters, digits, `_`, `-`, `.`, and `/`"
    return None


def escapes_project(project_root: Path, path: str) -> bool:
    """Return whether a safe-looking path still resolves outside the project."""

    root = project_root.resolve()
    candidate = (root / path).resolve()
    return candidate != root and root not in candidate.parents


def licence_tokens(expression: str) -> list[str]:
    """Split an SPDX expression into identifiers, parentheses, and operators."""

    return re.findall(r"\(|\)|[^\s()]+", expression)


def parse_spdx(expression: str) -> None:
    """Accept the small SPDX expression grammar used by asset licences."""

    tokens = licence_tokens(expression)
    position = 0

    def peek() -> str | None:
        return tokens[position] if position < len(tokens) else None

    def take() -> str:
        nonlocal position
        if position >= len(tokens):
            raise LicenceSyntaxError("unexpected end of expression")
        token = tokens[position]
        position += 1
        return token

    def identifier() -> None:
        token = take()
        if token in {"AND", "OR", "WITH", "(", ")"}:
            raise LicenceSyntaxError(f"unexpected operator or parenthesis {token!r}")
        if not SPDX_ID.fullmatch(token):
            raise LicenceSyntaxError(f"invalid licence identifier {token!r}")

    def factor() -> None:
        if peek() == "(":
            take()
            disjunction()
            if take() != ")":
                raise LicenceSyntaxError("missing closing parenthesis")
            return
        identifier()
        if peek() == "WITH":
            take()
            identifier()

    def conjunction() -> None:
        factor()
        while peek() == "AND":
            take()
            factor()

    def disjunction() -> None:
        conjunction()
        while peek() == "OR":
            take()
            conjunction()

    disjunction()
    if position != len(tokens):
        raise LicenceSyntaxError(f"unexpected token {tokens[position]!r}")


def licence_problem(value: object) -> str | None:
    """Reject empty, placeholder, and obviously malformed licence values."""

    if value is None:
        return "is missing"
    if not isinstance(value, str):
        return "is not a string"
    expression = value.strip()
    if not expression:
        return "is empty"
    if expression.lower() in PLACEHOLDER_LICENCES:
        return f"{expression!r} is a placeholder, not an SPDX expression"
    if any(character in expression for character in "\n\r\t"):
        return "contains a line break or tab"
    try:
        parse_spdx(expression)
    except LicenceSyntaxError as error:
        return f"{expression!r} is not a valid SPDX expression: {error}"
    return None


def sha256_file(path: Path) -> str:
    """Hash one file without loading it all into memory."""

    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_url_problem(value: object) -> str | None:
    """Require an absolute HTTP or HTTPS provenance URL."""

    if not isinstance(value, str) or not value.strip():
        return "source is missing"
    parsed = urlsplit(value)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        return f"source {value!r} is not an absolute http or https URL"
    return None


def manifest_entries(
    data: object, manifest: Path, failures: list[str]
) -> list[dict[str, object]]:
    """Validate the manifest envelope and return its entry objects."""

    if not isinstance(data, dict):
        failures.append(f"{display_path(manifest)}: manifest must be a JSON object")
        return []
    if data.get("version") != MANIFEST_VERSION:
        failures.append(
            f"{display_path(manifest)}: version must be {MANIFEST_VERSION}, "
            f"found {data.get('version')!r}"
        )
    assets = data.get("assets")
    if not isinstance(assets, list):
        failures.append(f"{display_path(manifest)}: assets must be a JSON array")
        return []

    entries: list[dict[str, object]] = []
    for index, entry in enumerate(assets):
        if not isinstance(entry, dict):
            failures.append(
                f"{display_path(manifest)}: assets[{index}]: entry must be an object"
            )
            continue
        unknown = sorted(set(entry) - ENTRY_FIELDS)
        if unknown:
            failures.append(
                f"{display_path(manifest)}: assets[{index}]: unknown fields {unknown}"
            )
        entries.append(entry)
    return entries


def check_manifest(
    *,
    manifest: Path,
    stories: Path,
    project_root: Path,
) -> int:
    """Run every presentation asset check and report all failures."""

    failures: list[str] = []
    manifest_name = display_path(manifest)

    if not stories.is_dir():
        print(f"asset check: stories directory is missing: {display_path(stories)}", file=sys.stderr)
        return 2
    if not manifest.is_file():
        print(f"asset check: manifest is missing: {manifest_name}", file=sys.stderr)
        return 1
    try:
        data = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"asset check: could not read {manifest_name}: {error}", file=sys.stderr)
        return 2

    entries = manifest_entries(data, manifest, failures)
    sources = sorted(stories.rglob("*.weave"))
    if not sources:
        failures.append(f"{display_path(stories)}: no .weave stories were found")

    references: list[AssetReference] = []
    for source in sources:
        try:
            found = extract_references(source)
        except (OSError, UnicodeDecodeError) as error:
            failures.append(f"{display_path(source)}: could not read story: {error}")
            continue
        for reference in found:
            problem = path_problem(reference.path)
            if problem is not None:
                failures.append(
                    f"{display_path(reference.source)}:{reference.line}: "
                    f"{reference.path}: {problem}"
                )
                continue
            if escapes_project(project_root, reference.path):
                failures.append(
                    f"{display_path(reference.source)}:{reference.line}: "
                    f"{reference.path}: path escapes the project root"
                )
                continue
            references.append(reference)

    entries_by_path: dict[str, dict[str, object]] = {}
    for entry in entries:
        path_value = entry.get("path")
        problem = path_problem(path_value)
        if problem is not None:
            shown = path_value if isinstance(path_value, str) else repr(path_value)
            failures.append(f"{manifest_name}: {shown}: {problem}")
            continue
        path = str(path_value)
        if escapes_project(project_root, path):
            failures.append(f"{manifest_name}: {path}: path escapes the project root")
            continue
        if path in entries_by_path:
            failures.append(f"{manifest_name}: {path}: duplicate manifest entry")
            continue
        entries_by_path[path] = entry

    referenced_paths = {reference.path for reference in references}
    for reference in references:
        if reference.path not in entries_by_path:
            failures.append(
                f"{display_path(reference.source)}:{reference.line}: "
                f"{reference.path}: referenced asset has no manifest entry"
            )
    for path in entries_by_path:
        if path not in referenced_paths:
            failures.append(
                f"{manifest_name}: {path}: manifest entry is not referenced by any story"
            )

    placeholders = 0
    for path, entry in entries_by_path.items():
        licence = entry.get("licence")
        problem = licence_problem(licence)
        if problem is not None:
            failures.append(f"{manifest_name}: {path}: licence {problem}")

        provenance = entry.get("provenance")
        if provenance not in PROVENANCE_VALUES:
            failures.append(
                f"{manifest_name}: {path}: provenance must be one of "
                f"{list(PROVENANCE_VALUES)}, found {provenance!r}"
            )
            continue

        asset_file = project_root / path
        if provenance == "original":
            author = entry.get("author")
            if not isinstance(author, str) or not author.strip():
                failures.append(
                    f"{manifest_name}: {path}: original asset must record an author"
                )
            if not asset_file.is_file():
                placeholders += 1
            continue

        source = entry.get("source")
        problem = source_url_problem(source)
        if problem is not None:
            failures.append(f"{manifest_name}: {path}: {problem}")
        digest = entry.get("sha256")
        if not isinstance(digest, str) or not SHA256.fullmatch(digest):
            failures.append(
                f"{manifest_name}: {path}: bundled asset must record a "
                "64-character lowercase sha256"
            )
            continue
        if not asset_file.is_file():
            failures.append(
                f"{manifest_name}: {path}: bundled asset file is missing at "
                f"{display_path(asset_file)}"
            )
            continue
        actual = sha256_file(asset_file)
        if actual != digest:
            failures.append(
                f"{manifest_name}: {path}: bundled sha256 mismatch: "
                f"recorded {digest}, computed {actual}"
            )

    if failures:
        print("asset check failed:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1

    print(
        f"asset check passed: {len(references)} reference(s) across "
        f"{len(sources)} stories, {len(entries_by_path)} manifest entries, "
        f"{placeholders} original placeholder(s) without a file"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=DEFAULT_MANIFEST,
        help="presentation asset manifest (default: %(default)s)",
    )
    parser.add_argument(
        "--stories",
        type=Path,
        default=DEFAULT_STORIES,
        help="directory searched recursively for .weave stories (default: %(default)s)",
    )
    parser.add_argument(
        "--project-root",
        type=Path,
        default=None,
        help="asset root for bundled files (default: the manifest directory)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the manifest and stories (the default action)",
    )
    arguments = parser.parse_args()
    del arguments.check

    manifest = arguments.manifest.resolve()
    project_root = (
        arguments.project_root.resolve()
        if arguments.project_root is not None
        else manifest.parent
    )
    return check_manifest(
        manifest=manifest,
        stories=arguments.stories.resolve(),
        project_root=project_root,
    )


if __name__ == "__main__":
    raise SystemExit(main())
