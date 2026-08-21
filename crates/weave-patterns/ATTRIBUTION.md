# Pattern data provenance

This file records the public sources used to validate canonical names, ordering, and symbols in `weave-patterns`. The crate uses cited factual data and original project semantics; it does not redistribute card artwork, book passages, commentary, fonts, or third-party implementations.

## Tarot

- Source consulted: Arthur Edward Waite, *The Pictorial Key to the Tarot* (1910/1922), [public-domain scan and transcription on Wikisource](https://en.wikisource.org/wiki/Pictorial_Key_to_the_Tarot). The scan's [file page](https://en.wikisource.org/wiki/File:The_Pictorial_Key_to_the_Tarot.pdf) records its public-domain status.
- Used to validate the 22-major/56-minor structure and conventional card names.
- Not copied: artwork, prose descriptions, spreads, or divinatory passages.
- Weave's semantic keywords, suit/rank composition, weighting, and reversal summaries are original project data released under Weave's MIT license.

## I-Ching

- Source consulted: James Legge (translator), *Sacred Books of the East, Volume XVI: The Yî King* (Clarendon Press, 1882), [public-domain scan index on Wikisource](https://en.wikisource.org/wiki/Index:Sacred_Books_of_the_East_-_Volume_16.djvu). Its [first hexagram page](https://en.wikisource.org/wiki/Sacred_Books_of_the_East/Volume_16/Hexagram_1) documents the six-line, bottom-first convention and changing-line values.
- Used to validate the 64-entry King Wen order, trigram composition, yin/yang line structure, and transformation model.
- Display names combine standard Hanyu Pinyin names with short, original English glosses. Semantic keywords are original summaries; no translation or commentary is copied.
- The three-coin distribution follows direct enumeration of three fair binary values. The yarrow option explicitly implements the conventional software probability model `6:7:8:9 = 1:5:7:3`; it does not claim to simulate physical stalk splitting.

## Elder Futhark

- Source consulted: the Unicode Character Database [`NamesList.txt`](https://www.unicode.org/Public/UCD/latest/ucd/NamesList.txt) and its [Runic names-list view](https://www.unicode.org/charts/nameslist/n_16A0.html), used to validate encoded rune characters and transliterations.
- Unicode data files are distributed under the OSI-approved [Unicode License v3 (`Unicode-3.0`)](https://www.unicode.org/license.txt). Copyright © 1991–present Unicode, Inc.
- The 24-rune selection follows the traditional Elder Futhark order. Weave's concise meaning and explicit reversal fields are original project data; no font, glyph outline, chart image, or prose annotation is redistributed.

## Maintenance rule

Changes to built-in pattern data must update this file, cite a public and license-compatible source for canonical factual data, and keep all interpretive summaries original unless their compatible license and required attribution are recorded here.
