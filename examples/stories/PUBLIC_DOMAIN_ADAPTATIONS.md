# Public-domain adaptations

Two branching stories under `examples/stories/` adapt situations from
public-domain works already committed to this repository. Each is an original
adaptation: the branching structure, connective narration, and dialogue are
newly written, and only short phrases are echoed from the source text.

## `alice-caucus.weave`

- Source work: *Alice's Adventures in Wonderland*
- Author: Lewis Carroll
- First published: 1865
- Standard Ebooks source directory in this repository:
  `examples/manuscripts/alice-adventures-in-wonderland/`
- Provenance: the underlying work is in the US public domain (first published
  before 1931), and the Standard Ebooks edition is released under CC0 1.0.
- Text used: only short phrases from the Caucus-race and croquet-ground
  chapters are echoed; all connective prose, choices, and story logic are
  newly written for this adaptation.

## `sherlock-bohemia.weave`

- Source work: *The Adventures of Sherlock Holmes*, "A Scandal in Bohemia"
- Author: Arthur Conan Doyle
- First published: 1892
- Standard Ebooks source directory in this repository:
  `examples/manuscripts/the-adventures-of-sherlock-holmes/`
- Provenance: the underlying work is in the US public domain (first published
  before 1931), and the Standard Ebooks edition is released under CC0 1.0.
- Text used: only short phrases from the Bohemia story are echoed; all
  connective prose, choices, and story logic are newly written for this
  adaptation.

Neither story loads external assets, performs network access, or includes
binary files. Each compiles to runtime IR with `weave-compiler`.
