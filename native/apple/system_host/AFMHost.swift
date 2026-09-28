// AFMHost.swift — the Apple system-model adapter over Harbor's
// system-host C ABI (native/apple/include/harbor_system_host.h).
//
// Implements HBR-033's provider half: availability, sessions, guided
// output and cancellation over FoundationModels (the on-device system
// model). Policy truth stays in Rust (decision 0009): this host reports
// on-device execution, never decides routing, and claims only the
// capabilities it actually serves.
//
// Guardrails: sessions run with .permissiveContentTransformations.
// Harbor processes the user's own local documents; a guardrail that
// silently transforms content would corrupt a read or an edit. Safety
// refusals still apply — a refusal is returned as a typed backend
// error, never silently altered content.

import Foundation
import FoundationModels

// MARK: - Availability

private func availabilityReason(_ a: SystemLanguageModel.Availability)
    -> (available: Bool, reason: String?) {
    switch a {
    case .available:
        return (true, nil)
    case .unavailable(.deviceNotEligible):
        return (false, "deviceNotEligible")
    case .unavailable(.appleIntelligenceNotEnabled):
        return (false, "appleIntelligenceNotEnabled")
    case .unavailable(.modelNotReady):
        return (false, "modelNotReady")
    case .unavailable(let other):
        return (false, String(describing: other))
    }
}

private func descriptorJSON() -> [String: Any] {
    let (available, reason) = availabilityReason(SystemLanguageModel.default.availability)
    let proc = ProcessInfo.processInfo
    let model = SystemLanguageModel.default
    let languages = model.supportedLanguages
        .map { lang -> String in
            let region = lang.region?.identifier ?? ""
            let code = lang.languageCode?.identifier ?? ""
            return region.isEmpty ? code : "\(code)-\(region)"
        }
        .sorted()
    return [
        "schema": "harbor.system_host/v1",
        "provider_id": "apple-system",
        "model_id": "foundation-model/default",
        "available": available,
        "unavailable_reason": reason ?? NSNull(),
        // Served capabilities only: AFM here is a chat model with
        // schema-guided decoding. No embeddings, no vision, no tools
        // (Harbor's graphs make tool decisions in Rust). The language
        // list is the OS's own declaration: content outside it is
        // refused with unsupportedLanguageOrLocale, and routing must
        // send those runs to a qualified GGUF package instead.
        "capabilities": ["chat", "structured_output"],
        "supported_languages": languages,
        "execution_location": "on_device",
        "identity": [
            "os": "macOS \(proc.operatingSystemVersionString)",
            "arch": hwModel(),
            "runtime": "FoundationModels/macOS-\(proc.operatingSystemVersion.majorVersion).\(proc.operatingSystemVersion.minorVersion)",
        ],
    ]
}

private func hwModel() -> String {
    var size = 0
    sysctlbyname("hw.machine", nil, &size, nil, 0)
    var buf = [CChar](repeating: 0, count: size)
    sysctlbyname("hw.machine", &buf, &size, nil, 0)
    return String(cString: buf)
}

// MARK: - JSON Schema -> DynamicGenerationSchema
//
// Translates the subset Harbor's graphs declare (03 §3: object/array/
// string/integer/number/boolean with enum, pattern, min/max bounds).
// Constructs the schema cannot express degrade to the closest weaker
// constraint and are reported in the response's host_metadata; the
// executor still validates the parsed output below the model.

private final class SchemaTranslator {
    var degradations: [String] = []

    func translate(_ node: [String: Any], name: String) -> DynamicGenerationSchema? {
        // A bare enum (no "type") is a closed choice of strings; the
        // graphs use it for kind/principle/op fields.
        if let values = enumValues(node) {
            return DynamicGenerationSchema(
                name: name, description: nodeDescription(node), anyOf: values)
        }
        let types = typeNames(node)
        // Optional scalars arrive as ["string", "null"] etc.; nullability
        // is handled by the enclosing Property (isOptional), so strip it.
        let effective = types.filter { $0 != "null" }
        if effective.isEmpty { return nil }
        if effective.count > 1 {
            // A union (e.g. cell values ["string", "number", "null"])
            // becomes anyOf of the scalar schemas; what cannot be
            // expressed is still validated below the model.
            var choices: [DynamicGenerationSchema] = []
            for t in effective {
                if let s = scalarSchema(t, node: node, name: name) { choices.append(s) }
            }
            guard !choices.isEmpty else { return nil }
            degradations.append("\(name): union of \(types) guided as anyOf(\(effective))")
            return DynamicGenerationSchema(name: name, anyOf: choices)
        }
        switch effective[0] {
        case "object":
            return translateObject(node, name: name)
        case "array":
            return translateArray(node, name: name)
        default:
            return scalarSchema(effective[0], node: node, name: name)
        }
    }

    private func scalarSchema(_ type: String, node: [String: Any], name: String)
        -> DynamicGenerationSchema? {
        switch type {
        case "string":
            return translateString(node, name: name)
        case "integer":
            return translateNumber(node, name: name, isDouble: false)
        case "number":
            return translateNumber(node, name: name, isDouble: true)
        case "boolean":
            return DynamicGenerationSchema(type: Bool.self)
        default:
            return nil
        }
    }

    private func typeNames(_ node: [String: Any]) -> [String] {
        if let one = node["type"] as? String { return [one] }
        if let many = node["type"] as? [String] { return many }
        return []
    }

    private func translateObject(_ node: [String: Any], name: String) -> DynamicGenerationSchema? {
        guard let props = node["properties"] as? [String: Any] else { return nil }
        let required = (node["required"] as? [String]) ?? []
        var properties: [DynamicGenerationSchema.Property] = []
        for (key, raw) in props.sorted(by: { $0.key < $1.key }) {
            let child = raw as? [String: Any]
            let childSchema = child.flatMap { translate($0, name: "\(name).\(key)") }
            if childSchema == nil {
                degradations.append("\(name).\(key): unsupported schema; not guided")
            }
            let description = child?["description"] as? String
            properties.append(.init(
                name: key,
                description: description,
                schema: childSchema ?? DynamicGenerationSchema(type: String.self),
                isOptional: !required.contains(key)))
        }
        if let extra = node["additionalProperties"] as? Bool, extra == true {
            degradations.append("\(name): additionalProperties is always closed in guided output")
        }
        return DynamicGenerationSchema(
            name: name, description: nodeDescription(node), properties: properties)
    }

    private func translateArray(_ node: [String: Any], name: String) -> DynamicGenerationSchema? {
        guard let items = node["items"] as? [String: Any],
              let itemSchema = translate(items, name: "\(name)[]") else { return nil }
        let minimum = (node["minItems"] as? Int).map { max(0, $0) }
        let maximum = (node["maxItems"] as? Int).map { max(0, $0) }
        return DynamicGenerationSchema(
            arrayOf: itemSchema, minimumElements: minimum, maximumElements: maximum)
    }

    private func translateString(_ node: [String: Any], name: String) -> DynamicGenerationSchema {
        // Prohibited guides (probed on macOS 26.5.1, FoundationModels):
        // every Regex pattern guide — including bounded-length forms —
        // is rejected at generation time with GenerativeError 1020000.
        // String patterns and length bounds therefore degrade to a free
        // string, reported here and validated below the model.
        if node["pattern"] != nil {
            degradations.append("\(name): pattern guides rejected by this OS build; free string")
        } else if node["minLength"] != nil || node["maxLength"] != nil {
            degradations.append("\(name): length bounds not guidable on this OS build; validated below the model")
        }
        return DynamicGenerationSchema(type: String.self)
    }

    private func translateNumber(_ node: [String: Any], name: String, isDouble: Bool)
        -> DynamicGenerationSchema {
        var guides: [Any] = []
        if let minimum = node["minimum"] as? NSNumber {
            guides.append(isDouble ? GenerationGuide<Double>.minimum(minimum.doubleValue)
                                   : GenerationGuide<Int>.minimum(minimum.intValue))
        }
        if let maximum = node["maximum"] as? NSNumber {
            guides.append(isDouble ? GenerationGuide<Double>.maximum(maximum.doubleValue)
                                   : GenerationGuide<Int>.maximum(maximum.intValue))
        }
        if isDouble {
            var g = [GenerationGuide<Double>]()
            for guide in guides { if let d = guide as? GenerationGuide<Double> { g.append(d) } }
            return DynamicGenerationSchema(type: Double.self, guides: g)
        } else {
            var g = [GenerationGuide<Int>]()
            for guide in guides { if let i = guide as? GenerationGuide<Int> { g.append(i) } }
            return DynamicGenerationSchema(type: Int.self, guides: g)
        }
    }

    private func enumValues(_ node: [String: Any]) -> [String]? {
        if let values = node["enum"] as? [String], !values.isEmpty { return values }
        if let one = node["const"] as? String { return [one] }
        return nil
    }

    private func nodeDescription(_ node: [String: Any]) -> String? {
        node["description"] as? String
    }
}

// MARK: - Request execution

private struct HostRequest {
    var messages: [(role: String, content: String)]
    var maxTokens: Int
    var temperature: Double
    var requires: [String]
    var responseSchema: [String: Any]?
    var traceKey: String?
}

private enum HostOutcome {
    case ok(content: String, guided: Bool, degradations: [String],
            promptTokens: Int, completionTokens: Int, estimatedUsage: Bool)
    case failure(status: Int32, message: String)
}

private func parseRequest(_ data: Data) -> HostRequest? {
    guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
        return nil
    }
    var messages: [(String, String)] = []
    if let arr = obj["messages"] as? [[String: Any]] {
        for m in arr {
            messages.append((m["role"] as? String ?? "user",
                             m["content"] as? String ?? ""))
        }
    }
    return HostRequest(
        messages: messages,
        maxTokens: obj["max_tokens"] as? Int ?? 1024,
        temperature: Double(obj["temperature_milli"] as? Int ?? 0) / 1000.0,
        requires: obj["requires"] as? [String] ?? [],
        responseSchema: obj["response_schema"] as? [String: Any],
        traceKey: obj["trace_key"] as? String)
}

/// Render the conversation for a fresh session: system messages become
/// the session instructions; the rest become one tagged prompt.
private func splitConversation(_ messages: [(role: String, content: String)])
    -> (instructions: String, prompt: String) {
    var instructions: [String] = []
    var turns: [String] = []
    for m in messages {
        switch m.role {
        case "system":
            if !m.content.isEmpty { instructions.append(m.content) }
        default:
            if !m.content.isEmpty {
                turns.append("\(m.role.uppercased()):\n\(m.content)")
            }
        }
    }
    return (instructions.joined(separator: "\n\n"), turns.joined(separator: "\n\n"))
}

private func execute(_ req: HostRequest, cancel: UnsafePointer<Bool>?) async -> HostOutcome {
    let model = SystemLanguageModel(guardrails: .permissiveContentTransformations)
    let (available, reason) = availabilityReason(model.availability)
    guard available else {
        return .failure(status: 2, message: "system model unavailable: \(reason ?? "unknown")")
    }
    for need in req.requires where need != "chat" && need != "structured_output" {
        return .failure(status: 1, message: "capability not served: \(need)")
    }
    let (instructions, prompt) = splitConversation(req.messages)
    let session = LanguageModelSession(model: model, instructions: instructions.isEmpty ? nil : instructions)

    var options = GenerationOptions()
    // Decoding policy: AFM under greedy sampling (temperature 0) falls
    // into verbatim repetition loops on long structured outputs — the
    // qualification runs show runaway repeated array items that consume
    // the token cap and end in decodingFailure. The system provider
    // therefore samples at a low temperature instead; run-to-run
    // determinism is the replay tier's job (cassettes), not the live
    // tier's. The GGUF path stays greedy: that engine does not loop.
    options.temperature = req.temperature > 0 ? req.temperature : 0.7
    options.maximumResponseTokens = req.maxTokens

    var guided = false
    var degradations: [String] = []
    var schema: GenerationSchema?
        if let node = req.responseSchema {
            let translator = SchemaTranslator()
            if let root = translator.translate(node, name: "root") {
                do {
                    schema = try GenerationSchema(root: root, dependencies: [])
                    guided = true
                    degradations = translator.degradations
                } catch {
                    degradations = translator.degradations + [
                        "schema rejected by FoundationModels: \(error.localizedDescription); unguided"
                    ]
                }
            } else {
                degradations = ["schema not translatable; unguided"]
            }
        }

    func isCancelled() -> Bool {
        cancel?.pointee ?? false
    }

    do {
        var content = ""
        if let schema {
            // Guided: stream snapshot by snapshot so the cancel flag is
            // honoured between generations, and keep the completed
            // content the last snapshot carries.
            let stream = session.streamResponse(to: prompt, schema: schema, options: options)
            var lastJSON = ""
            for try await snapshot in stream {
                if isCancelled() {
                    return .failure(status: 3, message: "cancelled")
                }
                lastJSON = snapshot.rawContent.jsonString
            }
            content = lastJSON
        } else {
            let response = try await session.respond(to: prompt, options: options)
            content = response.content
        }
        // Usage: real token counts from the system model where the OS
        // offers them (macOS 26.4+); below that a conservative chars/4
        // estimate, flagged in host_metadata, so context budgets still
        // bound the run.
        var promptTokens = 0
        var completionTokens = 0
        var estimatedUsage = false
        if #available(macOS 26.4, *) {
            if let p = try? await model.tokenCount(for: prompt) { promptTokens = p }
            if let c = try? await model.tokenCount(for: content) { completionTokens = c }
        }
        if promptTokens == 0 && !prompt.isEmpty {
            promptTokens = (prompt.utf8.count + 3) / 4
            estimatedUsage = true
        }
        if completionTokens == 0 && !content.isEmpty {
            completionTokens = (content.utf8.count + 3) / 4
            estimatedUsage = true
        }
        return .ok(content: content, guided: guided, degradations: degradations,
                   promptTokens: promptTokens, completionTokens: completionTokens,
                   estimatedUsage: estimatedUsage)
    } catch let error as LanguageModelSession.GenerationError {
        // Content the OS model will not serve in this configuration is
        // UNAVAILABLE, not a backend failure: the provider is absent for
        // that content and a substituting router may fall back to a
        // qualified GGUF package (visibly).
        if case .unsupportedLanguageOrLocale = error {
            return .failure(status: 2, message: describe(error))
        }
        return .failure(status: 4, message: describe(error))
    } catch {
        return .failure(status: 4, message: "\(error)")
    }
}

private func describe(_ error: LanguageModelSession.GenerationError) -> String {
    switch error {
    case .exceededContextWindowSize(let ctx):
        return "exceededContextWindowSize: \(ctx.debugDescription)"
    case .assetsUnavailable(let ctx):
        return "assetsUnavailable: \(ctx.debugDescription)"
    case .guardrailViolation(let ctx):
        return "guardrailViolation: \(ctx.debugDescription)"
    case .unsupportedGuide(let ctx):
        return "unsupportedGuide: \(ctx.debugDescription)"
    case .unsupportedLanguageOrLocale(let ctx):
        return "unsupportedLanguageOrLocale: \(ctx.debugDescription)"
    case .decodingFailure(let ctx):
        return "decodingFailure: \(ctx.debugDescription)"
    case .rateLimited(let ctx):
        return "rateLimited: \(ctx.debugDescription)"
    case .concurrentRequests(let ctx):
        return "concurrentRequests: \(ctx.debugDescription)"
    case .refusal(_, let ctx):
        return "refusal: \(ctx.debugDescription)"
    @unknown default:
        return "unrecognized GenerationError: \(error)"
    }
}

// MARK: - C ABI

private func withCString(_ s: String) -> UnsafeMutablePointer<CChar> {
    strdup(s)!
}

@_cdecl("harbor_system_host_descriptor")
public func harborSystemHostDescriptor() -> UnsafeMutablePointer<CChar> {
    let json = descriptorJSON()
    let data = (try? JSONSerialization.data(withJSONObject: json)) ?? Data("{}".utf8)
    return withCString(String(decoding: data, as: UTF8.self))
}

@_cdecl("harbor_system_host_generate")
public func harborSystemHostGenerate(
    requestJson: UnsafePointer<CChar>,
    cancel: UnsafePointer<Bool>,
    out: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> CInt {
    guard let req = parseRequest(Data(String(cString: requestJson).utf8)) else {
        out.pointee = withCString(#"{"error":"unparseable request"}"#)
        return 4
    }
    let sem = DispatchSemaphore(value: 0)
    let box = ResultBox()
    Task.detached {
        defer { sem.signal() }
        let outcome = await execute(req, cancel: cancel)
        box.set(outcome)
    }
    sem.wait()
    func log(_ line: String) {
        FileHandle.standardError.write(Data(line.utf8))
    }
    switch box.get()! {
    case .ok(let content, let guided, let degradations, let promptTokens,
             let completionTokens, let estimatedUsage):
        let body: [String: Any] = [
            "content": content,
            "prompt_tokens": promptTokens,
            "completion_tokens": completionTokens,
            "executed_on": "apple-system/foundation-model/default",
            "execution_location": "on_device",
            "host_metadata": [
                "guided": guided,
                "degradations": degradations,
                "estimated_usage": estimatedUsage,
            ] as [String: Any],
        ]
        let data = (try? JSONSerialization.data(withJSONObject: body)) ?? Data()
        out.pointee = withCString(String(decoding: data, as: UTF8.self))
        log("[afm-host] \(req.traceKey ?? "-") guided=\(guided) degradations=\(degradations.joined(separator: "; "))\n")
        return 0
    case .failure(let status, let message):
        let body: [String: Any] = ["error": message]
        let data = (try? JSONSerialization.data(withJSONObject: body)) ?? Data()
        out.pointee = withCString(String(decoding: data, as: UTF8.self))
        log("[afm-host] \(req.traceKey ?? "-") failure(\(status)): \(message.prefix(200))\n")
        return status
    }
}

@_cdecl("harbor_system_host_free_string")
public func harborSystemHostFreeString(_ s: UnsafeMutablePointer<CChar>?) {
    if let s = s { free(s) }
}

/// Thread-safe single-assignment box bridging the detached generation
/// task back to the blocked C-ABI caller.
private final class ResultBox: @unchecked Sendable {
    private let lock = NSLock()
    private var value: HostOutcome?
    func set(_ v: HostOutcome) {
        lock.lock(); value = v; lock.unlock()
    }
    func get() -> HostOutcome? {
        lock.lock(); defer { lock.unlock() }
        return value
    }
}
