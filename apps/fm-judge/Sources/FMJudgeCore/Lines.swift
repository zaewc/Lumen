import Foundation
import FoundationModels

/// The pinned model name Lumen accepts answers from: Apple's on-device system
/// language model, which has no public name or version of its own; the OS
/// build identifies it ([`modelVersion`]).
public let modelName = "apple.foundation-models"

/// Why no answer was given, as a closed set of codes. Each line names one;
/// `lumen-jev-eval` decides from it whether to retry, skip the case or stop.
public enum ErrorCode: String, CaseIterable, Sendable {
  // The model is not available: nothing more can be answered.
  case apple_intelligence_not_enabled
  case model_not_ready
  case device_not_eligible
  case unavailable
  case assets_unavailable
  // Worth another try later.
  case rate_limited
  case concurrent_requests
  // This request only.
  case context_exceeded
  case guardrail
  case refusal
  case decoding_failure
  case unsupported_guide
  case unsupported_language
  case bad_request
  case failed

  /// The exit status of `fm-judge --check` for an unavailable model, so
  /// scripts can tell the reasons apart without reading the line.
  public var exitCode: Int32 {
    switch self {
    case .apple_intelligence_not_enabled: 3
    case .model_not_ready: 4
    case .device_not_eligible: 5
    default: 6
    }
  }

  /// For people.
  public var message: String {
    switch self {
    case .apple_intelligence_not_enabled:
      "Apple Intelligence is not enabled (System Settings > Apple Intelligence & Siri)"
    case .model_not_ready: "the on-device model is not ready yet (it may still be downloading)"
    case .device_not_eligible: "this Mac is not eligible for Apple Intelligence"
    case .unavailable: "the on-device model is not available"
    case .assets_unavailable: "the on-device model's assets are not available"
    case .rate_limited: "the on-device model is rate limited"
    case .concurrent_requests: "the on-device model is busy"
    case .context_exceeded: "the item does not fit the on-device model's context"
    case .guardrail: "the on-device model's guardrails stopped the answer"
    case .refusal: "the on-device model refused to answer"
    case .decoding_failure: "the on-device model's answer could not be decoded"
    case .unsupported_guide: "the on-device model does not support the answer's schema"
    case .unsupported_language: "the on-device model took the item for an unsupported language"
    case .bad_request: "the line is not a jev.request/1"
    case .failed: "the on-device model failed"
    }
  }

  /// The code for an unavailable model, or `nil` when it is available.
  public init?(_ availability: SystemLanguageModel.Availability) {
    switch availability {
    case .available: return nil
    case .unavailable(.appleIntelligenceNotEnabled): self = .apple_intelligence_not_enabled
    case .unavailable(.modelNotReady): self = .model_not_ready
    case .unavailable(.deviceNotEligible): self = .device_not_eligible
    case .unavailable: self = .unavailable
    }
  }

  /// The code for an error thrown while generating.
  public init(_ error: any Error) {
    guard let error = error as? LanguageModelSession.GenerationError else {
      self = .failed
      return
    }
    switch error {
    case .exceededContextWindowSize: self = .context_exceeded
    case .assetsUnavailable: self = .assets_unavailable
    case .guardrailViolation: self = .guardrail
    case .unsupportedGuide: self = .unsupported_guide
    case .unsupportedLanguageOrLocale: self = .unsupported_language
    case .decodingFailure: self = .decoding_failure
    case .rateLimited: self = .rate_limited
    case .concurrentRequests: self = .concurrent_requests
    case .refusal: self = .refusal
    @unknown default: self = .failed
    }
  }
}

/// One answer line (`fm.answer/1`).
public struct AnswerLine: Encodable, Equatable, Sendable {
  public struct Answers: Encodable, Equatable, Sendable {
    public let assessment: String
    public let reason: String
    public let injection_suspected: Bool
  }

  public var schema = "fm.answer/1"
  public let item_ref: String
  public var prompt_version = Prompt.version
  public var model = modelName
  public let model_version: String
  public let answers: Answers
  public let elapsed_ms: UInt64

  public init(itemRef: String, modelVersion: String, judgment: Judgment, elapsedMs: UInt64) {
    item_ref = itemRef
    model_version = modelVersion
    answers = .init(
      assessment: judgment.assessment.key,
      reason: judgment.reason.key,
      injection_suspected: judgment.injection_suspected
    )
    elapsed_ms = elapsedMs
  }
}

/// One error line (`fm.error/1`): never an answer.
public struct ErrorLine: Encodable, Equatable, Sendable {
  public var schema = "fm.error/1"
  public let item_ref: String?
  public let error: String

  public init(itemRef: String?, code: ErrorCode) {
    item_ref = itemRef
    error = code.rawValue
  }
}

/// The status line of `fm-judge --check` (`fm.status/1`).
public struct StatusLine: Encodable, Equatable, Sendable {
  public var schema = "fm.status/1"
  public let available: Bool
  public let error: String?
  public var model = modelName
  public let model_version: String
  public var prompt_version = Prompt.version
  public let context_size: Int

  public init(unavailable: ErrorCode?, modelVersion: String, contextSize: Int) {
    available = unavailable == nil
    error = unavailable?.rawValue
    model_version = modelVersion
    context_size = contextSize
  }
}

/// One line of JSON: sorted keys, no newline inside.
public func line(_ value: some Encodable) -> String {
  let encoder = JSONEncoder()
  encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
  // These line types hold only strings, booleans and integers.
  let data = (try? encoder.encode(value)) ?? Data(#"{"schema":"fm.error/1","error":"failed"}"#.utf8)
  return String(decoding: data, as: UTF8.self)
}

/// The on-device model ships with the OS, so the OS version and build stand
/// for the model's version, e.g. `macOS 26.6.2 (25G83)`.
public func modelVersion(
  _ version: OperatingSystemVersion = ProcessInfo.processInfo.operatingSystemVersion,
  build: String = osBuild()
) -> String {
  "macOS \(version.majorVersion).\(version.minorVersion).\(version.patchVersion) (\(build))"
}

/// The OS build (`kern.osversion`), or `unknown`.
public func osBuild() -> String {
  var size = 0
  guard sysctlbyname("kern.osversion", nil, &size, nil, 0) == 0, size > 0, size < 64 else {
    return "unknown"
  }
  var bytes = [CChar](repeating: 0, count: size)
  guard sysctlbyname("kern.osversion", &bytes, &size, nil, 0) == 0 else { return "unknown" }
  let build = String(
    decoding: bytes.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
  return build.allSatisfy { $0.isLetter || $0.isNumber } && !build.isEmpty ? build : "unknown"
}
