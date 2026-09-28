/*
 * harbor_system_host.h — the C ABI a platform system-model adapter
 * implements to serve generation into Harbor's Rust runtime
 * (core/harbor_inference/src/system.rs mirrors this definition; the
 * two must stay byte-compatible).
 *
 * Trust direction: the host owns availability and execution, Rust owns
 * policy truth. A host that reports any execution location other than
 * "on_device" is rejected by the Rust bridge, and the sanctioned remote
 * path is RemoteEndpoint behind the egress broker — never this ABI.
 *
 * Strings crossing the boundary are allocated by the host and must be
 * freed only by harbor_system_host_free_string.
 */
#ifndef HARBOR_SYSTEM_HOST_H
#define HARBOR_SYSTEM_HOST_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Returns a freshly allocated JSON descriptor:
 * {
 *   "schema": "harbor.system_host/v1",
 *   "provider_id": "apple-system",
 *   "model_id": "foundation-model/default",
 *   "available": true,
 *   "unavailable_reason": null | "deviceNotEligible" |
 *                          "appleIntelligenceNotEnabled" | "modelNotReady",
 *   "capabilities": ["chat", "structured_output"],
 *   "execution_location": "on_device",
 *   "identity": { "os": "...", "arch": "...", "runtime": "..." }
 * }
 */
char *harbor_system_host_descriptor(void);

/*
 * Synchronous generation. request_json is canonical
 * harbor.system_request/v1 JSON (numbers are integers; temperature
 * crosses as thousandths in "temperature_milli"). cancel points at a
 * Rust AtomicBool (repr(transparent) over bool) the host should poll
 * and abort generation when it becomes true. out receives the response
 * (or {"error": "..."}) as a host-allocated string.
 *
 * Returns one of the status codes below.
 */
int harbor_system_host_generate(const char *request_json, const bool *cancel,
                                char **out);

/* Status codes returned by harbor_system_host_generate. */
enum {
    HARBOR_SYSTEM_HOST_OK = 0,
    HARBOR_SYSTEM_HOST_UNSUPPORTED_CAPABILITY = 1,
    HARBOR_SYSTEM_HOST_UNAVAILABLE = 2,
    HARBOR_SYSTEM_HOST_CANCELLED = 3,
    HARBOR_SYSTEM_HOST_BACKEND = 4,
    HARBOR_SYSTEM_HOST_POLICY = 5
};

/* Frees a string this host allocated. NULL is a no-op. */
void harbor_system_host_free_string(char *s);

#ifdef __cplusplus
}
#endif

#endif /* HARBOR_SYSTEM_HOST_H */
