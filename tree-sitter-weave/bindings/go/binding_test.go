package tree_sitter_weave_test

import (
	"testing"

	tree_sitter_weave "github.com/chrisgliddon/weave/tree-sitter-weave/bindings/go"
	tree_sitter "github.com/tree-sitter/go-tree-sitter"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_weave.Language())
	if language == nil {
		t.Errorf("Error loading Weave grammar")
	}
}
