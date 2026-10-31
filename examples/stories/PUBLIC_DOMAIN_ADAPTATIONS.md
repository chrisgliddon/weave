# Public-domain adaptations

Three branching stories under `examples/stories/` adapt public-domain works.
Two draw on texts already committed to this repository; the third cites an
external edition because its source is not part of the repository corpus. Each
is an original adaptation: the branching structure, connective narration, and
dialogue are newly written, and only short phrases are echoed from the source
text.

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

## `earnest-act-one.weave`

- Source work: *The Importance of Being Earnest*, Act I, opening scene
- Author: Oscar Wilde
- First published: 1899 (the play premiered in 1895)
- Standard Ebooks edition:
  <https://standardebooks.org/ebooks/oscar-wilde/the-importance-of-being-earnest>
- In-repository source directory: none. The `examples/manuscripts/` corpus does
  not contain this play, so the fixture cites the Standard Ebooks edition URL
  above instead of a local directory.
- Provenance: the underlying work is in the US public domain (first published
  before 1931), and the Standard Ebooks edition is released under CC0 1.0.
- Echoed phrases (short quotations from the source):
  - "Did you hear what I was playing, Lane?"
  - "I didn't think it polite to listen, sir."
  - "Anyone can play accurately" and "I play with wonderful expression."
  - "Have you got the cucumber sandwiches cut for Lady Bracknell?"
  - "eight bottles and a pint"
  - "Why is it that at a bachelor's establishment the servants invariably drink
    the champagne?"
  - "I attribute it to the superior quality of the wine, sir."
  - "Is marriage so demoralising as that?"
  - "It is not a very interesting subject. I never think of it myself."
  - "That will do, Lane, thank you."
- Newly written: all narration and stage business, the account-book and
  sandwich business, every choice, both endings, and the closing paraphrase
  about the lower orders setting an example. No long passage is reproduced.
- Presentation: one `scene` with a background path and two `stage`
  declarations (Algernon and Lane) with names, plus `@scene`, `@show`, `@hide`,
  and `@speaker` cues. Asset paths are relative `art/...` references; the image
  files are not part of the repository and are not needed to compile.

None of these stories performs network access or includes binary files. Each
compiles to runtime IR with `weave-compiler`.
