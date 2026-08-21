#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

enum TokenType {
  NEWLINE,
  INDENT,
  DEDENT,
};

typedef struct {
  uint16_t *levels;
  uint32_t length;
  uint32_t capacity;
  bool at_line_start;
} Scanner;

static bool push_level(Scanner *scanner, uint16_t level) {
  if (scanner->length == scanner->capacity) {
    uint32_t capacity = scanner->capacity == 0 ? 8 : scanner->capacity * 2;
    uint16_t *levels = realloc(scanner->levels, capacity * sizeof(uint16_t));
    if (levels == NULL) {
      return false;
    }
    scanner->levels = levels;
    scanner->capacity = capacity;
  }
  scanner->levels[scanner->length++] = level;
  return true;
}

void *tree_sitter_weave_external_scanner_create(void) {
  Scanner *scanner = calloc(1, sizeof(Scanner));
  if (scanner == NULL || !push_level(scanner, 0)) {
    free(scanner);
    return NULL;
  }
  scanner->at_line_start = true;
  return scanner;
}

void tree_sitter_weave_external_scanner_destroy(void *payload) {
  Scanner *scanner = payload;
  if (scanner != NULL) {
    free(scanner->levels);
    free(scanner);
  }
}

unsigned tree_sitter_weave_external_scanner_serialize(void *payload, char *buffer) {
  Scanner *scanner = payload;
  if (scanner == NULL) {
    return 0;
  }

  unsigned size = 1;
  buffer[0] = scanner->at_line_start ? 1 : 0;
  for (uint32_t index = 1;
       index < scanner->length && size + 2 <= TREE_SITTER_SERIALIZATION_BUFFER_SIZE;
       index++) {
    uint16_t level = scanner->levels[index];
    buffer[size++] = (char)(level & 0xff);
    buffer[size++] = (char)(level >> 8);
  }
  return size;
}

void tree_sitter_weave_external_scanner_deserialize(
    void *payload,
    const char *buffer,
    unsigned length) {
  Scanner *scanner = payload;
  if (scanner == NULL) {
    return;
  }
  scanner->length = 0;
  scanner->at_line_start = length == 0 || buffer[0] != 0;
  if (!push_level(scanner, 0)) {
    return;
  }
  for (unsigned index = 1; index + 1 < length; index += 2) {
    uint16_t level = (uint8_t)buffer[index] | ((uint16_t)(uint8_t)buffer[index + 1] << 8);
    if (!push_level(scanner, level)) {
      return;
    }
  }
}

bool tree_sitter_weave_external_scanner_scan(
    void *payload,
    TSLexer *lexer,
    const bool *valid_symbols) {
  Scanner *scanner = payload;
  if (scanner == NULL) {
    return false;
  }

  if (valid_symbols[NEWLINE]
      && (lexer->lookahead == '\n' || lexer->lookahead == '\r')) {
    if (lexer->lookahead == '\r') {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead == '\n') {
      lexer->advance(lexer, false);
    }
    lexer->mark_end(lexer);
    scanner->at_line_start = true;
    lexer->result_symbol = NEWLINE;
    return true;
  }

  if (lexer->eof(lexer)) {
    if (valid_symbols[DEDENT] && scanner->length > 1) {
      scanner->length--;
      lexer->result_symbol = DEDENT;
      return true;
    }
    return false;
  }

  if (!scanner->at_line_start || (!valid_symbols[INDENT] && !valid_symbols[DEDENT])) {
    return false;
  }

  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
    lexer->advance(lexer, true);
  }

  if (lexer->lookahead == '\n' || lexer->lookahead == '\r') {
    if (!valid_symbols[NEWLINE]) {
      return false;
    }
    if (lexer->lookahead == '\r') {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead == '\n') {
      lexer->advance(lexer, false);
    }
    lexer->mark_end(lexer);
    scanner->at_line_start = true;
    lexer->result_symbol = NEWLINE;
    return true;
  }

  if (lexer->eof(lexer)) {
    return false;
  }

  uint32_t column = lexer->get_column(lexer);
  uint16_t indentation = column > UINT16_MAX ? UINT16_MAX : (uint16_t)column;
  uint16_t current = scanner->levels[scanner->length - 1];
  lexer->mark_end(lexer);

  if (valid_symbols[INDENT] && indentation > current) {
    if (!push_level(scanner, indentation)) {
      return false;
    }
    scanner->at_line_start = false;
    lexer->result_symbol = INDENT;
    return true;
  }

  if (valid_symbols[DEDENT] && indentation < current) {
    scanner->length--;
    scanner->at_line_start = scanner->levels[scanner->length - 1] > indentation;
    lexer->result_symbol = DEDENT;
    return true;
  }

  scanner->at_line_start = false;
  return false;
}
