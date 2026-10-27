import Foundation

/// Text from an untrusted source, as `lumen_domain::UntrustedText` serializes
/// it: capped at 1,024 bytes by Lumen, and marked when it was cut.
public struct UntrustedText: Decodable, Equatable, Sendable {
  public let text: String
  public let truncated: Bool

  public init(_ text: String, truncated: Bool = false) {
    self.text = text
    self.truncated = truncated
  }
}

/// The redacted request Lumen sends every judge (`jev.request/1`,
/// docs/ai/model-contract.md): metadata only, never file contents. The helper
/// receives exactly what the cloud judge receives as its `state`.
public struct JevRequest: Decodable, Equatable, Sendable {
  public struct Candidate: Decodable, Equatable, Sendable {
    public let category: String
    public let pathDisplay: UntrustedText
    public let sizeBytes: String
    public let reclaimableBytes: String

    enum CodingKeys: String, CodingKey {
      case category
      case pathDisplay = "path_display"
      case sizeBytes = "size_bytes"
      case reclaimableBytes = "reclaimable_bytes"
    }
  }

  public struct Evidence: Decodable, Equatable, Sendable {
    public let id: String
    public let kind: String
    public let value: UntrustedText
  }

  public struct Coverage: Decodable, Equatable, Sendable {
    public let complete: Bool
    public let notes: [String]
  }

  public let schemaVersion: String
  public let itemRef: String
  public let candidate: Candidate
  public let evidence: [Evidence]
  public let coverage: Coverage
  public let untrustedStrings: [String: UntrustedText]

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case itemRef = "item_ref"
    case candidate, evidence, coverage
    case untrustedStrings = "untrusted_strings"
  }

  /// The longest request line read (Lumen's largest is a few KiB).
  public static let maxLineBytes = 64 * 1024

  /// Why a line is not a request this helper answers.
  public enum DecodeError: Error, Equatable {
    case tooLong
    case notARequest
    case otherSchema
  }

  /// Decodes one request line, refusing other schema versions.
  public static func decode(line: String) throws(DecodeError) -> JevRequest {
    guard line.utf8.count <= maxLineBytes else { throw .tooLong }
    let request: JevRequest
    do {
      request = try JSONDecoder().decode(JevRequest.self, from: Data(line.utf8))
    } catch {
      throw .notARequest
    }
    guard request.schemaVersion.hasPrefix("jev.request/1.") else { throw .otherSchema }
    return request
  }
}
