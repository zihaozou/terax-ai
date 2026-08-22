// anemll-serverd — OpenAI-compatible local completion server on the Apple
// Neural Engine, powered by Anemll's Swift inference engine (AnemllCore).
//
// Serves the same surface as the earlier Python server so Terax's
// `openai-compatible` provider config works unchanged:
//   GET  /v1/models
//   POST /v1/chat/completions   (non-stream and SSE stream)
//
// Requests shaped like Terax's autocomplete prompt (PREFIX/SUFFIX blocks) are
// served via raw FIM continuation when the model has FIM tokens; everything
// else goes through the chat template.

import AnemllCore
import ArgumentParser
import FlyingFox
import Foundation

// MARK: - OpenAI wire types

struct ChatMessage: Codable {
    let role: String
    let content: MessageContent
}

enum MessageContent: Codable {
    case text(String)
    case parts([Part])

    struct Part: Codable {
        let type: String?
        let text: String?
    }

    var plainText: String {
        switch self {
        case .text(let value): return value
        case .parts(let parts): return parts.compactMap { $0.text }.joined()
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let value = try? container.decode(String.self) {
            self = .text(value)
        } else {
            self = .parts(try container.decode([Part].self))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .text(let value): try container.encode(value)
        case .parts(let parts): try container.encode(parts)
        }
    }
}

struct ChatRequest: Codable {
    let model: String?
    let messages: [ChatMessage]
    let temperature: Float?
    let maxTokens: Int?
    let maxCompletionTokens: Int?
    let stream: Bool?

    enum CodingKeys: String, CodingKey {
        case model, messages, temperature, stream
        case maxTokens = "max_tokens"
        case maxCompletionTokens = "max_completion_tokens"
    }
}

// MARK: - FIM prompt detection (mirror of the Python server / Terax prompt.ts)

struct FimExtractor {
    private let regex: NSRegularExpression

    init() throws {
        regex = try NSRegularExpression(
            pattern: "PREFIX:\\n<<<\\n(.*)\\n>>>\\n\\nSUFFIX:\\n<<<\\n(.*)\\n>>>",
            options: [.dotMatchesLineSeparators]
        )
    }

    func extract(messages: [ChatMessage]) -> (prefix: String, suffix: String)? {
        guard let user = messages.last(where: { $0.role == "user" }) else { return nil }
        let text = user.content.plainText
        let range = NSRange(text.startIndex..., in: text)
        guard let match = regex.firstMatch(in: text, options: [], range: range),
              match.numberOfRanges == 3,
              let prefixRange = Range(match.range(at: 1), in: text),
              let suffixRange = Range(match.range(at: 2), in: text)
        else { return nil }
        return (String(text[prefixRange]), String(text[suffixRange]))
    }
}

func logLine(_ message: String) {
    FileHandle.standardError.write((message + "\n").data(using: .utf8) ?? Data())
}

// MARK: - Generation engine

actor Engine {
    private let inferenceManager: InferenceManager
    private let tokenizer: AnemllCore.Tokenizer
    private let contextLength: Int
    private let maxTokensCap: Int
    private let maxNewlines: Int
    private let fimExtractor: FimExtractor
    let modelName: String
    let fimCapable: Bool
    private let fimPrefixIds: [Int]
    private let fimSuffixIds: [Int]
    private let fimMiddleIds: [Int]

    // Serializes generation. InferenceManager is a single mutable resource
    // (a plain class, not actor-isolated), and this actor's own suspension
    // points make it reentrant: two overlapping requests would otherwise both
    // enter InferenceManager.generateResponse concurrently, and the second
    // would hit its internal "busy" guard and get back a fabricated
    // near-empty completion instead of a real one. Callers queue here and are
    // served strictly one at a time.
    private var isGenerating = false
    private var pendingWaiters: [CheckedContinuation<Void, Never>] = []

    init(modelDir: String, maxTokensCap: Int, maxNewlines: Int) async throws {
        let metaPath = (modelDir as NSString).appendingPathComponent("meta.yaml")
        let config = try YAMLConfig.load(from: metaPath)
        self.contextLength = config.contextLength
        self.maxTokensCap = maxTokensCap
        self.maxNewlines = maxNewlines
        self.modelName = (modelDir as NSString).lastPathComponent
        self.fimExtractor = try FimExtractor()

        let prefix = config.modelPrefix.lowercased()
        let template: String
        if prefix.contains("gemma") { template = "gemma3" }
        else if prefix.contains("llama") { template = "llama" }
        else if prefix.contains("qwen") { template = "qwen" }
        else { template = "default" }

        let tokenizer = try await AnemllCore.Tokenizer(
            modelPath: config.tokenizerModel,
            template: template
        )
        self.tokenizer = tokenizer

        let models = try await ModelLoader(progressDelegate: nil).loadModel(from: config)
        self.inferenceManager = try InferenceManager(
            models: models,
            contextLength: config.contextLength,
            batchSize: config.batchSize,
            splitLMHead: config.splitLMHead,
            v110: config.configVersion == "0.1.1",
            argmaxInModel: config.argmaxInModel,
            slidingWindow: config.slidingWindow,
            updateMaskPrefill: config.updateMaskPrefill,
            prefillDynamicSlice: config.prefillDynamicSlice,
            modelPrefix: config.modelPrefix,
            vocabSize: config.vocabSize,
            lmHeadChunkSizes: config.lmHeadChunkSizes
        )

        // FIM support probe: real FIM special tokens encode to exactly one id.
        let singleId: (String) -> [Int] = { token in
            let ids = tokenizer.tokenize(token)
            return ids.count == 1 ? ids : []
        }
        let prefixIds = singleId("<|fim_prefix|>")
        let suffixIds = singleId("<|fim_suffix|>")
        let middleIds = singleId("<|fim_middle|>")
        self.fimPrefixIds = prefixIds
        self.fimSuffixIds = suffixIds
        self.fimMiddleIds = middleIds
        self.fimCapable = !prefixIds.isEmpty && !suffixIds.isEmpty && !middleIds.isEmpty
        logLine("FIM capable: \(self.fimCapable)")
    }

    struct GenerationResult {
        let text: String
        let stopReason: String
        let promptTokens: Int
        let completionTokens: Int
    }

    /// Tracks emitted tokens inside the onToken callback for early aborts.
    private final class TokenBudget {
        var generated: [Int] = []
        var aborted = false
    }

    private func buildTokens(request: ChatRequest, fim: (prefix: String, suffix: String)?) -> [Int] {
        if let fim {
            // Raw continuation: <|fim_prefix|>P<|fim_suffix|>S<|fim_middle|>
            return fimPrefixIds
                + tokenizer.tokenize(fim.prefix)
                + fimSuffixIds
                + tokenizer.tokenize(fim.suffix)
                + fimMiddleIds
        }
        let chat: [AnemllCore.Tokenizer.ChatMessage] = request.messages.map { message in
            switch message.role {
            case "system": return .system(message.content.plainText)
            case "assistant": return .assistant(message.content.plainText)
            default: return .user(message.content.plainText)
            }
        }
        return tokenizer.applyChatTemplate(input: chat, addGenerationPrompt: true)
    }

    /// Public entry point: waits for any in-flight generation to finish,
    /// runs this request, then hands the slot to the next waiter (if any).
    func generate(request: ChatRequest) async throws -> GenerationResult {
        await acquireSlot()
        defer { releaseSlot() }
        return try await performGenerate(request: request)
    }

    private func acquireSlot() async {
        guard isGenerating else {
            isGenerating = true
            return
        }
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            pendingWaiters.append(continuation)
        }
    }

    private func releaseSlot() {
        if pendingWaiters.isEmpty {
            isGenerating = false
        } else {
            // Hand the slot directly to the next waiter; isGenerating stays
            // true so a request arriving in this instant still queues behind
            // it instead of racing it for the slot.
            let next = pendingWaiters.removeFirst()
            next.resume()
        }
    }

    private func performGenerate(request: ChatRequest) async throws -> GenerationResult {
        let maxTokens = min(request.maxTokens ?? request.maxCompletionTokens ?? 256, maxTokensCap)
        let fim = fimCapable ? fimExtractor.extract(messages: request.messages) : nil
        let fimMode = fim != nil

        var tokens = buildTokens(request: request, fim: fim)

        // Left-truncate so prompt + generation fit the context window.
        let promptBudget = contextLength - maxTokens - 8
        if tokens.count > promptBudget {
            tokens = Array(tokens.suffix(promptBudget))
        }

        let start = CFAbsoluteTimeGetCurrent()
        let manager = inferenceManager
        let tok = tokenizer
        let newlineBudget = maxNewlines
        let budget = TokenBudget()

        let (generated, prefillTime, stopReason) = try await manager.generateResponse(
            initialTokens: tokens,
            temperature: request.temperature ?? 0.0,
            maxTokens: maxTokens,
            eosTokens: tok.eosTokenIds,
            tokenizer: tok,
            onToken: { token in
                budget.generated.append(token)
                if budget.aborted { return }
                let text = tok.decode(tokens: budget.generated)
                let newlines = text.filter { $0 == "\n" }.count
                let fenceClosed = !fimMode && text.components(separatedBy: "```").count > 2
                if newlines >= newlineBudget || fenceClosed {
                    budget.aborted = true
                    manager.AbortGeneration(Code: 2)
                }
            }
        )

        let text = tokenizer.decode(tokens: generated)
        let total = CFAbsoluteTimeGetCurrent() - start
        let decode = total - prefillTime
        let tps = decode > 0 ? Double(generated.count) / decode : 0
        let reason = budget.aborted ? "lines" : stopReason
        logLine(String(
            format: "[gen] mode=%@ prompt=%dtok prefill=%.2fs decode=%.2fs (%dtok, %.1ft/s) stop=%@ total=%.2fs",
            fimMode ? "fim" : "chat", tokens.count, prefillTime, decode,
            generated.count, tps, reason, total
        ))
        // Upstream stop strings: "eos_token", "max_tokens", "abort_generation<code>",
        // "sliding_window_requires_rotate"; ours: "lines". Only a genuine
        // token-budget exhaustion maps to OpenAI's "length".
        return GenerationResult(
            text: text,
            stopReason: reason == "max_tokens" ? "length" : "stop",
            promptTokens: tokens.count,
            completionTokens: generated.count
        )
    }
}

// MARK: - Response building

enum ResponseBuilder {
    static func json(_ object: [String: Any], status: HTTPStatusCode = .ok) -> HTTPResponse {
        let data = (try? JSONSerialization.data(withJSONObject: object)) ?? Data()
        return HTTPResponse(
            statusCode: status,
            headers: [.contentType: "application/json"],
            body: data
        )
    }

    static func modelsList(name: String) -> HTTPResponse {
        json([
            "object": "list",
            "data": [[
                "id": name,
                "object": "model",
                "created": Int(Date().timeIntervalSince1970),
                "owned_by": "anemll",
            ]],
        ])
    }

    static func completion(
        result: Engine.GenerationResult, model: String, stream: Bool
    ) -> HTTPResponse {
        let cid = "chatcmpl-" + UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(24)
        let created = Int(Date().timeIntervalSince1970)

        if stream {
            // Whole-response SSE: role chunk, one content chunk, stop, DONE.
            func chunk(_ delta: [String: Any], _ finish: Any) -> [String: Any] {
                [
                    "id": String(cid), "object": "chat.completion.chunk",
                    "created": created, "model": model,
                    "choices": [["index": 0, "delta": delta, "finish_reason": finish]],
                ]
            }
            func sse(_ object: [String: Any]) -> String {
                guard let data = try? JSONSerialization.data(withJSONObject: object),
                      let str = String(data: data, encoding: .utf8) else { return "" }
                return "data: " + str + "\n\n"
            }
            var payload = ""
            payload += sse(chunk(["role": "assistant"], NSNull()))
            payload += sse(chunk(["content": result.text], NSNull()))
            payload += sse(chunk([:], "stop"))
            payload += "data: [DONE]\n\n"
            return HTTPResponse(
                statusCode: .ok,
                headers: [.contentType: "text/event-stream"],
                body: payload.data(using: .utf8) ?? Data()
            )
        }

        return json([
            "id": String(cid), "object": "chat.completion",
            "created": created, "model": model,
            "choices": [[
                "index": 0,
                "message": ["role": "assistant", "content": result.text],
                "finish_reason": result.stopReason,
            ]],
            "usage": [
                "prompt_tokens": result.promptTokens,
                "completion_tokens": result.completionTokens,
                "total_tokens": result.promptTokens + result.completionTokens,
            ],
        ])
    }
}

// MARK: - Entry point

@main
struct AnemllServerd: AsyncParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "anemll-serverd",
        abstract: "OpenAI-compatible completion server for Anemll CoreML models (ANE)."
    )

    @Option(name: .customLong("model-dir"), help: "Directory containing meta.yaml and .mlmodelc bundles.")
    var modelDir: String

    @Option(help: "Port to bind on 127.0.0.1.")
    var port: UInt16 = 8100

    @Option(name: .customLong("max-tokens"), help: "Hard cap on generated tokens per request.")
    var maxTokens: Int = 48

    @Option(name: .customLong("max-newlines"), help: "Stop generation after this many newlines.")
    var maxNewlines: Int = 5

    mutating func run() async throws {
        let engine = try await Engine(
            modelDir: (modelDir as NSString).expandingTildeInPath,
            maxTokensCap: maxTokens,
            maxNewlines: maxNewlines
        )
        try await warmup(engine: engine)
        try await serve(engine: engine)
    }

    private func warmup(engine: Engine) async throws {
        // Primes the ANE compilation cache before accepting traffic.
        let warm = ChatRequest(
            model: nil,
            messages: [ChatMessage(role: "user", content: .text(
                "PREFIX:\n<<<\ndef add(a, b):\n    \n>>>\n\nSUFFIX:\n<<<\n\n>>>"
            ))],
            temperature: 0, maxTokens: 3, maxCompletionTokens: nil, stream: false
        )
        let start = CFAbsoluteTimeGetCurrent()
        _ = try await engine.generate(request: warm)
        logLine(String(format: "Warmup done in %.1fs", CFAbsoluteTimeGetCurrent() - start))
    }

    private func serve(engine: Engine) async throws {
        // Bind IPv4 loopback explicitly — Terax's proxy dials 127.0.0.1, and
        // FlyingFox's .loopback resolves to IPv6 ::1 on this platform.
        let server = try HTTPServer(address: .inet(ip4: "127.0.0.1", port: port))
        let modelName = await engine.modelName

        await server.appendRoute("GET /v1/models") { _ in
            ResponseBuilder.modelsList(name: modelName)
        }

        await server.appendRoute("POST /v1/chat/completions") { req in
            let bodyData = try await req.bodyData
            let request: ChatRequest
            do {
                request = try JSONDecoder().decode(ChatRequest.self, from: bodyData)
            } catch {
                return ResponseBuilder.json(
                    ["error": ["message": "invalid request: \(error)"]],
                    status: .badRequest
                )
            }
            do {
                let result = try await engine.generate(request: request)
                return ResponseBuilder.completion(
                    result: result,
                    model: request.model ?? modelName,
                    stream: request.stream == true
                )
            } catch {
                return ResponseBuilder.json(
                    ["error": ["message": "generation failed: \(error)"]],
                    status: .internalServerError
                )
            }
        }

        logLine("Listening on 127.0.0.1:\(port)")
        try await server.run()
    }
}
