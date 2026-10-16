/*
 * harbor_ocr_host.h - the C ABI a platform OCR adapter implements
 * (core/harbor_inference/src/ocr.rs mirrors this; keep them byte-compatible).
 *
 * The host owns the engine (Apple Vision here); Rust owns policy. Execution
 * is on-device only: a descriptor reporting any other location is rejected.
 * Strings are host-allocated and freed only by harbor_ocr_host_free_string.
 */
#ifndef HARBOR_OCR_HOST_H
#define HARBOR_OCR_HOST_H

#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * {"schema":"harbor.ocr_host/v1","engine":"apple-vision",
 *  "execution_location":"on_device","languages":["en-US","ar-SA",...],
 *  "identity":{"os":"...","arch":"..."}}
 */
char *harbor_ocr_host_descriptor(void);

/*
 * request_json: {"kind":"pdf"|"image","languages":["en-US",...]|[],
 *                "max_pages":N}
 * out: {"pages":[{"page":1,"text":"...","source":"text_layer"|"ocr",
 *                 "confidence":0.0-1.0}], "engine":"..."}  or {"error":"..."}
 * PDF pages that already carry a text layer are NOT re-recognised.
 * Returns 0 on success, 1 unsupported, 3 cancelled, 4 failure.
 */
int harbor_ocr_host_recognize(const char *request_json, const unsigned char *data,
                              size_t len, const bool *cancel, char **out);

void harbor_ocr_host_free_string(char *s);

#ifdef __cplusplus
}
#endif

#endif
