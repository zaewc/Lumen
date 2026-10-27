import Foundation
import FoundationModels

/// Asking the on-device model about one request line.
public enum Judge {
  /// Greedy sampling, so a recording is as reproducible as the model allows,
  /// and a short answer: the three enumerated values need about 30 tokens.
  public static var options: GenerationOptions {
    #if compiler(>=6.4)
      GenerationOptions(samplingMode: .greedy, maximumResponseTokens: 128)
    #else
      GenerationOptions(sampling: .greedy, maximumResponseTokens: 128)
    #endif
  }

  /// The answer or error line for one request line. Each item gets a new
  /// session, so nothing of one item is in the context of the next.
  public static func answer(
    line text: String,
    model: SystemLanguageModel = .default,
    modelVersion: String
  ) async -> String {
    let request: JevRequest
    do {
      request = try JevRequest.decode(line: text)
    } catch {
      return line(ErrorLine(itemRef: nil, code: .bad_request))
    }
    if let code = ErrorCode(model.availability) {
      return line(ErrorLine(itemRef: request.itemRef, code: code))
    }
    let session = LanguageModelSession(model: model, instructions: Prompt.instructions)
    let clock = ContinuousClock()
    let start = clock.now
    do {
      let response = try await session.respond(
        to: Prompt.render(request),
        generating: Judgment.self,
        options: options
      )
      let ms = UInt64(max(0, (start.duration(to: clock.now) / .milliseconds(1)).rounded()))
      return line(
        AnswerLine(
          itemRef: request.itemRef,
          modelVersion: modelVersion,
          judgment: response.content,
          elapsedMs: ms
        )
      )
    } catch {
      return line(ErrorLine(itemRef: request.itemRef, code: ErrorCode(error)))
    }
  }

  /// The size of one request line, without the model: bytes of the
  /// instructions, of the schema as JSON and of the rendered item.
  public static func render(line text: String) -> String {
    guard let request = try? JevRequest.decode(line: text) else {
      return line(ErrorLine(itemRef: nil, code: .bad_request))
    }
    let schema = (try? JSONEncoder().encode(Judgment.generationSchema))?.count ?? 0
    return line(
      Sizes(
        item_ref: request.itemRef,
        instructions: Prompt.instructions.utf8.count,
        schema: schema,
        prompt: Prompt.render(request).utf8.count
      )
    )
  }

  /// Exact token counts of one request line (macOS 26.4 or later, and the
  /// model available).
  @available(macOS 26.4, *)
  public static func measure(line text: String, model: SystemLanguageModel = .default) async
    -> String
  {
    guard let request = try? JevRequest.decode(line: text) else {
      return line(ErrorLine(itemRef: nil, code: .bad_request))
    }
    if let code = ErrorCode(model.availability) {
      return line(ErrorLine(itemRef: request.itemRef, code: code))
    }
    do {
      let instructions = try await model.tokenCount(for: Instructions(Prompt.instructions))
      let schema = try await model.tokenCount(for: Judgment.generationSchema)
      let prompt = try await model.tokenCount(for: Prompt.render(request))
      return line(
        Sizes(
          item_ref: request.itemRef,
          instructions: instructions,
          schema: schema,
          prompt: prompt,
          context_size: model.contextSize
        )
      )
    } catch {
      return line(ErrorLine(itemRef: request.itemRef, code: ErrorCode(error)))
    }
  }

  struct Sizes: Encodable {
    let item_ref: String
    let instructions: Int
    let schema: Int
    let prompt: Int
    var context_size: Int?
    var total: Int { instructions + schema + prompt }

    enum CodingKeys: String, CodingKey {
      case item_ref, instructions, schema, prompt, context_size, total
    }

    func encode(to encoder: any Encoder) throws {
      var c = encoder.container(keyedBy: CodingKeys.self)
      try c.encode(item_ref, forKey: .item_ref)
      try c.encode(instructions, forKey: .instructions)
      try c.encode(schema, forKey: .schema)
      try c.encode(prompt, forKey: .prompt)
      try c.encode(total, forKey: .total)
      try c.encodeIfPresent(context_size, forKey: .context_size)
    }
  }
}
