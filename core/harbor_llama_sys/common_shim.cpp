// The three string helpers from llama.cpp common/common.cpp that
// json-schema-to-grammar.cpp and json-schema.cpp call. Harbor compiles only
// the JSON-schema -> GBNF slice of common/ (see build.rs COMMON_SOURCES);
// common.cpp itself drags in sampling, speculative decoding, logging,
// fit and build-info, none of which Harbor uses.
//
// Bodies copied verbatim from llama.cpp v0.8.0 (8642541b25) common/common.cpp
// (MIT, Copyright (c) 2023-2026 The ggml authors). Declarations come from the
// vendored common.h, so a signature change upstream fails the build here. A
// NEW dependency of the JSON-schema files fails the link (undefined symbol):
// add it here or switch build.rs to the full llama-common library.

#include "common.h"

#include <sstream>
#include <string>
#include <vector>

std::string string_join(const std::vector<std::string> & values, const std::string & separator) {
    std::ostringstream result;
    for (size_t i = 0; i < values.size(); ++i) {
        if (i > 0) {
            result << separator;
        }
        result << values[i];
    }
    return result.str();
}

std::vector<std::string> string_split(const std::string & str, const std::string & delimiter) {
    std::vector<std::string> parts;
    size_t start = 0;
    size_t end = str.find(delimiter);

    while (end != std::string::npos) {
        parts.push_back(str.substr(start, end - start));
        start = end + delimiter.length();
        end = str.find(delimiter, start);
    }

    parts.push_back(str.substr(start));

    return parts;
}

std::string string_repeat(const std::string & str, size_t n) {
    if (n == 0) {
        return "";
    }

    std::string result;
    result.reserve(str.length() * n);

    for (size_t i = 0; i < n; ++i) {
        result += str;
    }

    return result;
}
