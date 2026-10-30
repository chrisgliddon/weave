# Example manuscripts

Real public-domain prose as Weave manuscript projects. Synthetic fixtures stay
for precise assertions; this corpus exists so the chapter tree, card board,
table, word count, and manuscript format can be exercised against real books.

| Directory | Work | Structure |
|---|---|---|
| `alice-adventures-in-wonderland` | *Alice's Adventures in Wonderland*, Lewis Carroll, 1865 | 12 chapters, one level |
| `the-time-machine` | *The Time Machine*, H. G. Wells, 1895 | 12 chapters and an epilogue |
| `the-adventures-of-sherlock-holmes` | *The Adventures of Sherlock Holmes*, Arthur Conan Doyle, 1892 | 2 stories, each split into numbered scenes |
| `aesops-fables` | *Aesop's Fables*, translated by V. S. Vernon Jones, 1912 | 20 fables as leaf scenes |

## Licensing

Every work was first published before 1931, so it is in the United States
public domain. The text and the transcription come from
[Standard Ebooks](https://standardebooks.org), which publishes works it believes
are free of United States copyright and dedicates its own editorial work under
[CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).

`Aesop's Fables` is included under the Vernon Jones translation of 1912. A
translation carries its own copyright, so only translations published before
1931 may be added.

No Project Gutenberg material is included. Project Gutenberg's licence and
trademark cover its ebook wrapper, so its texts are not used here even where the
underlying work is public domain.

Illustrations, figures, and captions are excluded. Every work is stored as text
only.

## Layout

Each directory is a manuscript project:

```text
alice-adventures-in-wonderland/
├── weave.manuscript.ron        # versioned chapter tree
├── weave.manuscript/
│   ├── styles.ron              # project style sheet
│   ├── 2.ron                   # chapter 2 body
│   └── ...
└── provenance.json             # source, licence, hashes, counts
```

`provenance.json` records the source URL, the licence, the retrieval date, the
SHA-256 of the downloaded edition, and per chapter the source file, paragraph
count, word count, text hash, and body file hash.

## Verify

```bash
python3 scripts/build-corpus.py --check
```

The check is offline. It fails when a committed body no longer matches its
recorded hash, which is what catches a hand edit that skipped provenance.

## Regenerate

Regeneration needs network access and is not part of the build:

```bash
python3 scripts/build-corpus.py --fetch
python3 scripts/build-corpus.py --write
```

Sources and extracted editions land under `target/corpus-src/`, which is
ignored by Git. `--only <work-key>` limits the work to one entry.

## Transformations

Applied to every work: markup removed, paragraphs kept in document order,
zero-width no-break spaces and soft hyphens removed, whitespace collapsed,
illustrations and captions dropped.

Per work, also recorded in `provenance.json`:

- *Sherlock Holmes*: each story is a chapter; each numbered part is a scene.
- *Aesop's Fables*: each fable is a scene; its moral is the final paragraph.
