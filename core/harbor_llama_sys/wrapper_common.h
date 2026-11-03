#pragma once

// Exception-safe C shims over the parts of llama.cpp Harbor calls that can
// throw C++ exceptions (adapted from llama-cpp-sys-2 0.1.156; the fit /
// memory-breakdown / MTP-speculative / lazy-grammar shims are dropped).

#include "llama.h"

#include <stdbool.h>
#include <stddef.h>

struct llama_sampler;
struct llama_vocab;

#include "wrapper_utils.h"

#ifdef __cplusplus
extern "C" {
#endif

// JSON Schema -> GBNF via llama.cpp common/json-schema-to-grammar.
// On success *out_grammar is a malloc'd string; free it with llama_rs_string_free.
llama_rs_status llama_rs_json_schema_to_grammar(
    const char * schema_json,
    bool force_gbnf,
    char ** out_grammar);

struct llama_sampler * llama_rs_sampler_init_grammar(
    const struct llama_vocab * vocab,
    const char * grammar_str,
    const char * grammar_root);

void llama_rs_string_free(char * ptr);

#ifdef __cplusplus
}
#endif
