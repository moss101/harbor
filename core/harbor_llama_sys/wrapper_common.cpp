// Adapted from llama-cpp-sys-2 0.1.156 (MIT OR Apache-2.0, Dial AI).
// Harbor changes: json_schema_to_grammar now takes a common_json (llama.cpp
// moved off nlohmann in common/), and only the shims Harbor uses are kept.

#include "wrapper_common.h"

#include <cstdlib>
#include <exception>
#include <string>

#include "json-schema-to-grammar.h"
#include "json.h"
#include "llama.h"
#include "wrapper_utils.h"

extern "C" llama_rs_status llama_rs_json_schema_to_grammar(
    const char * schema_json,
    bool force_gbnf,
    char ** out_grammar) {
    if (!schema_json || !out_grammar) {
        return LLAMA_RS_STATUS_INVALID_ARGUMENT;
    }

    *out_grammar = nullptr;
    try {
        const common_json schema = common_json::parse(schema_json);
        const std::string grammar = json_schema_to_grammar(schema, force_gbnf);
        *out_grammar = llama_rs_dup_string(grammar);
        return *out_grammar ? LLAMA_RS_STATUS_OK : LLAMA_RS_STATUS_ALLOCATION_FAILED;
    } catch (const common_json_error &) {
        return LLAMA_RS_STATUS_EXCEPTION;
    } catch (const std::exception &) {
        return LLAMA_RS_STATUS_EXCEPTION;
    } catch (...) {
        return LLAMA_RS_STATUS_EXCEPTION;
    }
}

extern "C" void llama_rs_string_free(char * ptr) {
    if (ptr) {
        std::free(ptr);
    }
}

extern "C" struct llama_sampler * llama_rs_sampler_init_grammar(
    const struct llama_vocab * vocab,
    const char * grammar_str,
    const char * grammar_root) {
    try {
        return llama_sampler_init_grammar(vocab, grammar_str, grammar_root);
    } catch (...) {
        return nullptr;
    }
}
