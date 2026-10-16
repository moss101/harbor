/*
 * harbor_asr_host.h - the C ABI a platform speech-recognition adapter
 * implements (core/harbor_inference/src/asr.rs mirrors this).
 *
 * The OS owns the model (Apple's on-device speech model); Harbor owns policy.
 * Execution is on-device only; a descriptor reporting any other location is
 * refused. Strings are host-allocated, freed only by harbor_asr_host_free_string.
 */
#ifndef HARBOR_ASR_HOST_H
#define HARBOR_ASR_HOST_H

#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * {"schema":"harbor.asr_host/v1","engine":"apple-speech-transcriber",
 *  "execution_location":"on_device","supported_locales":["en_US",...],
 *  "installed_locales":["en_US",...],"identity":{"os":"...","arch":"..."}}
 */
char *harbor_asr_host_descriptor(void);

/*
 * request_json: {"locale":"en_US"}. data: an audio file (WAV/AIFF/CAF/M4A...).
 * out: {"engine":"...","locale":"en_US","text":"...","segments":[{"start":0.0,
 *       "end":1.2,"text":"..."}]}  or {"error":"..."}.
 * Returns 0 ok, 1 unsupported (locale), 3 cancelled, 4 failure.
 */
int harbor_asr_host_transcribe(const char *request_json, const unsigned char *data,
                               size_t len, const bool *cancel, char **out);

void harbor_asr_host_free_string(char *s);

#ifdef __cplusplus
}
#endif

#endif
